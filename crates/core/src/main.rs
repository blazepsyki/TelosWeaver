//! TelosWeaver Core Daemon 진입점 (REQ-011, G1).
//!
//! `--policy <file>`이 없으면 Local API의 인증·Health만 제공하고 Run을 허용하지 않습니다.
//! 있으면 운영자 설정으로 Run을 승인하고 탐지한 Provider CLI로 실행합니다 (`telos_core::integration`).
use std::{collections::HashMap, path::PathBuf, process::ExitCode, time::Duration};
use telos_core::{
    api::{ApiServer, ApiServerConfig},
    integration::Phase0Execution,
};
use telos_schema::CliDiscoveryOutcome;

const USAGE: &str = "usage: telos-weaver serve --project-root <path> --project-id <id> --host-id <id> [--policy <file>]";

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("telos-weaver: {message}");
            ExitCode::FAILURE
        }
    }
}
async fn run() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    match args.next().as_deref().and_then(|arg| arg.to_str()) {
        Some("--version" | "-V") => {
            println!("{} {}", telos_core::SERVICE_ID, env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some("serve") => {}
        _ => return Err(USAGE.into()),
    }
    let mut options = HashMap::new();
    while let Some(flag) = args.next() {
        let flag = flag.to_str().ok_or("option name must be UTF-8")?;
        if !["--project-root", "--project-id", "--host-id", "--policy"].contains(&flag) {
            return Err("unknown option".into());
        }
        let value = args.next().ok_or("missing option value")?;
        if options.insert(flag.to_owned(), value).is_some() {
            return Err("duplicate option".into());
        }
    }
    let project_root = PathBuf::from(
        options
            .remove("--project-root")
            .ok_or("project root is required")?,
    );
    let project_id = options
        .remove("--project-id")
        .and_then(|value| value.into_string().ok())
        .filter(|value| !value.is_empty())
        .ok_or("project ID is required")?;
    let host_id = options
        .remove("--host-id")
        .and_then(|value| value.into_string().ok())
        .filter(|value| !value.is_empty())
        .ok_or("host ID is required")?;
    let policy = options.remove("--policy").map(PathBuf::from);
    let execution = Phase0Execution::build(project_id, host_id, policy.as_deref()).await?;
    for discovery in &execution.discoveries {
        match &discovery.outcome {
            CliDiscoveryOutcome::Found { cli } => println!(
                "{}: {} {} ({:?})",
                telos_core::SERVICE_ID,
                discovery.provider_id,
                cli.cli_version,
                cli.source
            ),
            CliDiscoveryOutcome::Unavailable { failure, .. } => println!(
                "{}: {} unavailable ({failure:?})",
                telos_core::SERVICE_ID,
                discovery.provider_id
            ),
        }
    }
    for note in &execution.notes {
        println!("{}: {note}", telos_core::SERVICE_ID);
    }
    let server = ApiServer::start(
        ApiServerConfig::new(project_root),
        execution.supervisor.clone(),
    )
    .await
    .map_err(|_| "Local API startup failed")?;
    // RN Fork (Spec v1.5.1 §12.5): optional; Core runs either way and reports it as `rn` in core.health.
    let rn = execution.start_rn(server.token_path()).await;
    let rn_state = rn.component();
    println!(
        "{}: rn {:?} ({})",
        telos_core::SERVICE_ID,
        rn_state.status,
        rn_state.detail.unwrap_or_default()
    );
    match &execution.policy_revision {
        Some(revision) => println!(
            "{}: Local API ready (policy {revision})",
            telos_core::SERVICE_ID
        ),
        None => println!(
            "{}: Local API ready (execution unavailable)",
            telos_core::SERVICE_ID
        ),
    }
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .map_err(|_| "shutdown signal setup failed")?;
        tokio::select! {result=tokio::signal::ctrl_c()=>result.map_err(|_|"shutdown signal failed")?,_=terminate.recv()=>{}}
    }
    #[cfg(windows)]
    tokio::signal::ctrl_c()
        .await
        .map_err(|_| "shutdown signal failed")?;
    // 연결부터 닫아 새 Run을 받지 않은 뒤, 활성 Run의 취소와 실제 종료를 확인합니다.
    let closed = server.shutdown().await;
    let report = execution.shutdown(Duration::from_secs(15)).await;
    rn.shutdown().await;
    if !report.unconfirmed.is_empty() {
        return Err(format!(
            "{} run(s) did not confirm termination",
            report.unconfirmed.len()
        ));
    }
    closed.map_err(|_| "Local API shutdown failed".into())
}

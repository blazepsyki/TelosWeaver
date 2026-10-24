//! TelosWeaver Core Daemon 진입점 (REQ-011). Local API의 인증·Health만 제공하며 정책이 연결되기 전에는 Run을 허용하지 않습니다.
use std::{collections::HashMap, path::PathBuf, process::ExitCode, sync::Arc};
use telos_core::{
    api::{ApiServer, ApiServerConfig},
    execution::{DenyRuns, ExecutionSupervisor},
};

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
        _ => {
            return Err(
                "usage: telos-weaver serve --project-root <path> --project-id <id> --host-id <id>"
                    .into(),
            );
        }
    }
    let mut options = HashMap::new();
    while let Some(flag) = args.next() {
        let flag = flag.to_str().ok_or("option name must be UTF-8")?;
        if !["--project-root", "--project-id", "--host-id"].contains(&flag) {
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
    // Policy and snapshot services are not configured here. No Run is authorized.
    let execution = Arc::new(ExecutionSupervisor::new(
        project_id,
        host_id,
        vec![],
        Arc::new(DenyRuns),
    ));
    let server = ApiServer::start(ApiServerConfig::new(project_root), execution)
        .await
        .map_err(|_| "Local API startup failed")?;
    println!(
        "{}: Local API ready (execution unavailable)",
        telos_core::SERVICE_ID
    );
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
    server
        .shutdown()
        .await
        .map_err(|_| "Local API shutdown failed".into())
}

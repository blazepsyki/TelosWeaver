//! Antigravity (`agy`) 실환경 시험 (PH0-AGY, PROV-01~09·FORK-POC-01·AGY-DATA-01~05).
//!
//! 실제 CLI를 실행하고 사용자 계정의 사용량을 쓰므로 기본 실행에서 제외합니다(`#[ignore]`).
//! 실행 조건(배정표 §6): 모델 `gemini-3.8-flash-low`, `--effort low`, 저장소 밖 일회용 폴더,
//! 시험이 만든 세션 폴더(`brain/<session_id>`)는 각 시험 끝에서 즉시 정리합니다.
//!
//! ```text
//! cargo test -p telos-adapters --test agy_environment -- --ignored --test-threads=1 --nocapture
//! ```

use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use futures::StreamExt;
use telos_adapters::{AdapterError, AgentAdapter, AgyAdapter, runtime::NativeRuntime};
use telos_schema::{
    FailureKind, ProcessTermination, ProviderEvent, ProviderEventKind, ProviderEventPayload,
    RunContext, RunTermination, SessionMode, ToolOutcome, TurnStatus, check_run_events, fixtures,
};

fn home_dir() -> PathBuf {
    PathBuf::from(
        std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .expect("home directory"),
    )
}

fn executable() -> PathBuf {
    if let Some(explicit) = std::env::var_os("TELOS_AGY_CLI") {
        return PathBuf::from(explicit);
    }
    let bin = if cfg!(windows) { "agy.exe" } else { "agy" };
    home_dir().join(".gemini/bin").join(bin)
}

fn workdir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("telos-agy-env").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn adapter(name: &str, prompt: &str, mode: Option<&str>) -> AgyAdapter {
    let host_id = fixtures::run_context(&fixtures::task_spec()).host_id;
    AgyAdapter::configured(
        executable(),
        "1.4.7".into(),
        host_id,
        Arc::new(NativeRuntime),
        Duration::from_secs(2),
        workdir(name),
        prompt.into(),
        Some("gemini-3.8-flash-low".into()),
        Some("low".into()),
        mode.map(str::to_owned),
    )
}

fn context(run_id: &str) -> RunContext {
    let mut context = fixtures::run_context(&fixtures::task_spec());
    context.run_id = run_id.into();
    context.idempotency_key = format!("{run_id}:start");
    context
}

/// 시험이 생성한 임시 대화 폴더만 정리합니다. 기존 사용자 대화는 건드리지 않습니다.
fn cleanup_session(session_id: &str) {
    if session_id.is_empty() {
        return;
    }
    let home = home_dir();
    for dir_name in ["antigravity", "antigravity-cli"] {
        let brain = home
            .join(".gemini")
            .join(dir_name)
            .join("brain")
            .join(session_id);
        if brain.exists() {
            let _ = std::fs::remove_dir_all(&brain);
        }
        let conv_pb = home
            .join(".gemini")
            .join(dir_name)
            .join("conversations")
            .join(format!("{session_id}.pb"));
        if conv_pb.exists() {
            let _ = std::fs::remove_file(&conv_pb);
        }
    }
}

async fn run(adapter: &AgyAdapter, context: &RunContext) -> Vec<ProviderEvent> {
    adapter
        .start_run(&fixtures::task_spec(), context)
        .await
        .unwrap();
    let events: Vec<_> = tokio::time::timeout(
        Duration::from_secs(180),
        adapter
            .events(&context.run_id, 0)
            .map(Result::unwrap)
            .collect(),
    )
    .await
    .expect("run finishes");
    report(&events);
    events
}

fn report(events: &[ProviderEvent]) {
    if let Some(id) = session_id(events) {
        println!("agy-session: {id}");
    }
    for event in events {
        println!(
            "  {:>2} {:?} {}",
            event.sequence,
            event.kind,
            serde_json::Value::Object(event.payload.clone())
        );
    }
}

fn session_id(events: &[ProviderEvent]) -> Option<String> {
    events.iter().find_map(|e| e.native_session_id.clone())
}

fn payloads(events: &[ProviderEvent]) -> Vec<ProviderEventPayload> {
    events
        .iter()
        .map(|e| ProviderEventPayload::parse(e.kind, &e.payload).unwrap())
        .collect()
}

fn text(events: &[ProviderEvent]) -> String {
    payloads(events)
        .into_iter()
        .filter_map(|p| match p {
            ProviderEventPayload::TextDelta(t) => Some(t.text),
            _ => None,
        })
        .collect()
}

fn turn_status(events: &[ProviderEvent]) -> Option<TurnStatus> {
    payloads(events).into_iter().find_map(|p| match p {
        ProviderEventPayload::TurnFinished(t) => Some(t.status),
        _ => None,
    })
}

fn exit(events: &[ProviderEvent]) -> telos_schema::ProcessExitedPayload {
    match payloads(events).pop() {
        Some(ProviderEventPayload::ProcessExited(exit)) => exit,
        other => panic!("last event is not process_exited: {other:?}"),
    }
}

/// PROV-01·AGY-DATA-01: 설치된 VS Code 확장에서 공통 탐지로 CLI를 찾고 `--app_data_dir=antigravity` 수용을 확인합니다.
#[tokio::test]
#[ignore = "reads the local VS Code extensions folder and runs agy --version"]
async fn env_discovery_and_data_dir_option() {
    use telos_adapters::{CliDiscoverer, DiscoveryRequest, NativeCliDiscoverer};
    use telos_schema::{BinaryLocation, CliSource, ExtensionResolution};

    let home = home_dir();
    let (platform, roots) = if cfg!(windows) {
        ("win32-x64", vec![home.join(".vscode/extensions")])
    } else {
        (
            "linux-x64",
            vec![
                home.join(".vscode-server/extensions"),
                home.join(".vscode/extensions"),
            ],
        )
    };
    let rule = AgyAdapter::cli_rule(platform).unwrap();
    let request = DiscoveryRequest {
        host_id: "windows-local".into(),
        target_platform: platform.into(),
        explicit_path: None,
        extension_roots: roots,
        home_dir: home,
        path_env: None,
        checked_at: "2026-10-24T00:00:00Z".into(),
    };
    let runtime = Arc::new(NativeRuntime);
    let discovery = NativeCliDiscoverer::new(runtime.clone())
        .discover("agy", &rule, &request)
        .await;
    println!("{}", serde_json::to_string_pretty(&discovery).unwrap());
    let cli = discovery.found().expect("Antigravity CLI is discovered");
    assert_eq!(cli.source, CliSource::Extension);
    assert_eq!(cli.location, BinaryLocation::ManagedPath);
    assert!(!cli.cli_version.is_empty());
    let extension = cli.extension.as_ref().unwrap();
    assert_eq!(extension.extension_id, "google.google-antigravity");
    assert!(matches!(
        extension.resolution,
        ExtensionResolution::Metadata | ExtensionResolution::FolderScan
    ));

    let version = AgyAdapter::verify_extension_data_dir(
        runtime.as_ref(),
        std::path::Path::new(&cli.executable_path),
        &workdir("version-probe"),
    )
    .await
    .expect("--app_data_dir=antigravity --version succeeds");
    assert_eq!(version, cli.cli_version);
}

/// PROV-02·03·04·06·AGY-DATA-02~05: 확장 데이터 디렉터리(`--app_data_dir=antigravity`)와 기본 디렉터리(`antigravity-cli`) 비교 실행.
#[tokio::test]
#[ignore = "runs the real Antigravity CLI"]
async fn env_completed_run_and_data_dir_comparison() {
    let home = home_dir();

    // 1. 확장 공유 디렉터리 (--app_data_dir=antigravity)
    let shared_adapter = adapter(
        "completed-shared",
        "Reply with only the token AGY_PONG and do not call tools.",
        Some("plan"),
    );
    let shared_events = run(&shared_adapter, &context("env-completed-shared")).await;
    let shared_id = session_id(&shared_events).expect("session id");
    let shared_brain = home.join(".gemini/antigravity/brain").join(&shared_id);
    let cli_brain_for_shared = home.join(".gemini/antigravity-cli/brain").join(&shared_id);
    let shared_exists = shared_brain.exists();
    let cli_exists_for_shared = cli_brain_for_shared.exists();
    cleanup_session(&shared_id);

    assert_eq!(
        check_run_events(&shared_events),
        Ok(RunTermination::ProcessExited)
    );
    assert_eq!(shared_events[0].kind, ProviderEventKind::SessionStarted);
    assert!(text(&shared_events).contains("AGY_PONG"));
    assert_eq!(turn_status(&shared_events), Some(TurnStatus::Completed));
    assert!(
        shared_exists && !cli_exists_for_shared,
        "ExtensionShared must create brain/<id> under ~/.gemini/antigravity"
    );
    let exit_shared = exit(&shared_events);
    assert_eq!(
        (exit_shared.exit_code, exit_shared.termination),
        (Some(0), ProcessTermination::Exited)
    );
    assert_eq!(exit_shared.children_reaped, Some(true));

    // 2. CLI 기본 디렉터리 (옵션 생략 -> ~/.gemini/antigravity-cli)
    let default_adapter = adapter(
        "completed-default",
        "Reply with only the token CLI_PONG and do not call tools.",
        Some("plan"),
    )
    .with_default_cli_data_dir();
    let default_events = run(&default_adapter, &context("env-completed-default")).await;
    let default_id = session_id(&default_events).expect("session id");
    let default_cli_brain = home.join(".gemini/antigravity-cli/brain").join(&default_id);
    let shared_brain_for_default = home.join(".gemini/antigravity/brain").join(&default_id);
    let default_cli_exists = default_cli_brain.exists();
    let shared_exists_for_default = shared_brain_for_default.exists();
    cleanup_session(&default_id);

    assert_eq!(
        check_run_events(&default_events),
        Ok(RunTermination::ProcessExited)
    );
    assert!(text(&default_events).contains("CLI_PONG"));
    assert!(
        default_cli_exists && !shared_exists_for_default,
        "DefaultCli must create brain/<id> under ~/.gemini/antigravity-cli"
    );
}

/// PROV-07: Headless 기본 권한(`request-review`)에서 명령 실행 도구는 소프트 거부되고 `ToolDenied`로 기록됩니다.
#[tokio::test]
#[ignore = "runs the real Antigravity CLI"]
async fn env_tool_denied() {
    let adapter = adapter(
        "denied",
        "Call run_command to execute 'whoami' and do nothing else.",
        None,
    );
    let events = run(&adapter, &context("env-denied")).await;
    if let Some(id) = session_id(&events) {
        cleanup_session(&id);
    }
    assert_eq!(check_run_events(&events), Ok(RunTermination::ProcessExited));
    let p = payloads(&events);
    let denied = p.iter().any(|item| {
        matches!(item, ProviderEventPayload::ToolCompleted(t)
            if t.name == "run_command" && t.outcome == ToolOutcome::Denied)
    });
    assert!(denied, "run_command must be reported as denied");
    assert_eq!(turn_status(&events), Some(TurnStatus::Failed));
}

/// PROV-06: 존재하지 않는 모델 지정은 Configuration 오류와 실패한 Turn으로 끝납니다.
#[tokio::test]
#[ignore = "runs the real Antigravity CLI"]
async fn env_invalid_model_fails() {
    let host_id = fixtures::run_context(&fixtures::task_spec()).host_id;
    let adapter = AgyAdapter::configured(
        executable(),
        "1.4.7".into(),
        host_id,
        Arc::new(NativeRuntime),
        Duration::from_secs(2),
        workdir("invalid-model"),
        "hi".into(),
        Some("no-such-model-xyz".into()),
        Some("low".into()),
        Some("plan".into()),
    );
    let events = run(&adapter, &context("env-invalid-model")).await;
    if let Some(id) = session_id(&events) {
        cleanup_session(&id);
    }
    assert_eq!(check_run_events(&events), Ok(RunTermination::ProcessExited));
    assert!(payloads(&events).iter().any(
        |p| matches!(p, ProviderEventPayload::Error(e) if e.kind == FailureKind::Configuration)
    ));
    assert_eq!(turn_status(&events), Some(TurnStatus::Failed));
    assert_ne!(exit(&events).exit_code, Some(0));
}

/// PROV-08: 취소 접수 뒤 실제 종료와 자식 회수를 확인합니다.
#[tokio::test]
#[ignore = "runs the real Antigravity CLI"]
async fn env_cancel_terminates_process_tree() {
    let adapter = adapter(
        "cancel",
        "Write the numbers from 1 to 400, one per line, with no other text and no tools.",
        Some("plan"),
    );
    let context = context("env-cancel");
    adapter
        .start_run(&fixtures::task_spec(), &context)
        .await
        .unwrap();
    let mut stream = adapter.events(&context.run_id, 0);
    let first = tokio::time::timeout(Duration::from_secs(60), stream.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(first.kind, ProviderEventKind::SessionStarted);
    let requested = Instant::now();
    assert_eq!(adapter.cancel_run(&context.run_id).await, Ok(true));
    let events: Vec<_> = tokio::time::timeout(
        Duration::from_secs(30),
        adapter
            .events(&context.run_id, 0)
            .map(Result::unwrap)
            .collect(),
    )
    .await
    .expect("cancelled run finishes");
    println!("cancel-to-exit: {:?}", requested.elapsed());
    report(&events);
    if let Some(id) = session_id(&events) {
        cleanup_session(&id);
    }
    assert_eq!(check_run_events(&events), Ok(RunTermination::ProcessExited));
    let exit = exit(&events);
    if turn_status(&events) == Some(TurnStatus::Cancelled) {
        assert!(matches!(
            exit.termination,
            ProcessTermination::TerminatedOnRequest | ProcessTermination::Killed
        ));
    }
    assert_eq!(exit.children_reaped, Some(true));
}

/// PROV-05·FORK-POC-01: `--conversation <id>` Resume은 같은 대화를 잇고, `NativeFork`는 명시적으로 거부합니다.
#[tokio::test]
#[ignore = "runs the real Antigravity CLI"]
async fn env_resume_and_fork_rejection() {
    let base = adapter(
        "resume",
        "Remember this codeword: birch-42. Reply only: ok",
        Some("plan"),
    );
    let first = run(&base, &context("env-resume-base")).await;
    let parent = session_id(&first).expect("session id");

    let mut resume_ctx = context("env-resume");
    resume_ctx.session.mode = SessionMode::Resume;
    resume_ctx.session.source_session_id = Some(parent.clone());

    let ask = adapter(
        "resume",
        "What was the codeword? Reply only the codeword.",
        Some("plan"),
    );
    let resumed = run(&ask, &resume_ctx).await;
    let resumed_id = session_id(&resumed);
    println!("resume: parent={parent} resumed={resumed_id:?}");

    let mut fork_ctx = context("env-fork");
    fork_ctx.session.mode = SessionMode::NativeFork;
    fork_ctx.session.source_session_id = Some(parent.clone());
    let fork_result = ask.start_run(&fixtures::task_spec(), &fork_ctx).await;

    cleanup_session(&parent);
    if let Some(id) = &resumed_id {
        cleanup_session(id);
    }

    assert_eq!(
        check_run_events(&resumed),
        Ok(RunTermination::ProcessExited)
    );
    assert!(text(&resumed).to_lowercase().contains("birch-42"));
    assert_eq!(resumed_id.as_deref(), Some(parent.as_str()));
    assert!(matches!(
        fork_result,
        Err(AdapterError::UnsupportedCapability {
            ref capability,
            ..
        }) if capability == "native_fork"
    ));
}

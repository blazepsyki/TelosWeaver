//! Antigravity (`agy`) 실환경 시험 (PH0-AGY, PROV-01~09·FORK-POC-01·AGY-DATA-01~05·DEC-08·AGENT-05).
//!
//! 실제 CLI를 실행하고 사용자 계정의 사용량을 쓰므로 기본 실행에서 제외합니다(`#[ignore]`).
//! 실행 조건(배정표 §6): 모델 `gemini-3.8-flash-low`, `--effort low`, 저장소 밖 일회용 폴더,
//! 시험이 만든 세션 폴더(`brain/<session_id>`)와 대화 DB(`conversations/<session_id>.db*`)는 각 시험 끝에서 즉시 정리합니다.
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
use telos_adapters::{
    AdapterError, AgentAdapter, AgyAdapter, agy::AgyVsCodeListing, runtime::NativeRuntime,
};
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

/// 확장 데이터 디렉터리(`~/.gemini/antigravity`)에 Headless CLI 인증 상태가 있는지 확인합니다.
///
/// Windows 로컬에서는 OS 자격 증명 저장소와 `antigravity/jetski_state.pbtxt`가 함께 존재하지만,
/// Linux Remote SSH(`psyki-nas`)에서는 `agy`가 SSH 세션을 감지해 디렉터리별 파일 토큰
/// (`<app_data_dir>/antigravity-oauth-token` + `jetski_state.pbtxt`)을 쓰며,
/// VS Code 원격 확장은 토큰을 메모리 gRPC로만 넘겨 `antigravity/`에는 CLI 토큰 파일이 없고
/// `antigravity-cli/`에만 CLI 로그인 토큰이 존재합니다 (AGY-DATA-02 Linux 관찰).
fn extension_dir_has_cli_auth() -> bool {
    let ext_dir = home_dir().join(".gemini/antigravity");
    let has_state = ext_dir.join("jetski_state.pbtxt").exists();
    if cfg!(windows) {
        has_state
    } else {
        has_state && ext_dir.join("antigravity-oauth-token").exists()
    }
}

fn adapter(name: &str, prompt: &str, mode: Option<&str>) -> AgyAdapter {
    let host_id = fixtures::run_context(&fixtures::task_spec()).host_id;
    let configured = AgyAdapter::configured(
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
    );
    if extension_dir_has_cli_auth() {
        configured
    } else {
        configured.with_default_cli_data_dir()
    }
}

fn context(run_id: &str) -> RunContext {
    let mut context = fixtures::run_context(&fixtures::task_spec());
    context.run_id = run_id.into();
    context.idempotency_key = format!("{run_id}:start");
    context
}

/// 시험이 생성한 임시 대화 폴더(`brain/<id>`)와 대화 DB(`conversations/<id>.db*`)만 정리합니다.
/// 기존 사용자 대화는 건드리지 않습니다 (Spec §7.5, INTEGRATION 증거 §8.4).
fn cleanup_session(session_id: &str) {
    if session_id.is_empty() || session_id.contains(['/', '\\', '.']) {
        return;
    }
    let home = home_dir();
    let mut removed = Vec::new();
    for dir_name in ["antigravity", "antigravity-cli"] {
        let base = home.join(".gemini").join(dir_name);
        let brain = base.join("brain").join(session_id);
        if brain.exists() && std::fs::remove_dir_all(&brain).is_ok() {
            removed.push(format!("{dir_name}/brain/{session_id}"));
        }
        for ext in ["db", "db-wal", "db-shm", "pb"] {
            let conv = base
                .join("conversations")
                .join(format!("{session_id}.{ext}"));
            if conv.exists() && std::fs::remove_file(&conv).is_ok() {
                removed.push(format!("{dir_name}/conversations/{session_id}.{ext}"));
            }
        }
    }
    println!("agy-cleanup: {session_id} removed={removed:?}");
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
    let (host_id, platform, roots) = if cfg!(windows) {
        (
            "windows-local",
            "win32-x64",
            vec![home.join(".vscode/extensions")],
        )
    } else {
        (
            "psyki-nas",
            "linux-x64",
            vec![
                home.join(".vscode-server/extensions"),
                home.join(".vscode/extensions"),
            ],
        )
    };
    let rule = AgyAdapter::cli_rule(platform).unwrap();
    let request = DiscoveryRequest {
        host_id: host_id.into(),
        target_platform: platform.into(),
        explicit_path: None,
        extension_roots: roots,
        home_dir: home,
        path_env: None,
        checked_at: "2026-10-25T00:00:00Z".into(),
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
    assert_eq!(extension.resolution, ExtensionResolution::Metadata);

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
    let host_id = fixtures::run_context(&fixtures::task_spec()).host_id;
    let shared_adapter = AgyAdapter::configured(
        executable(),
        "1.4.7".into(),
        host_id,
        Arc::new(NativeRuntime),
        Duration::from_secs(2),
        workdir("completed-shared"),
        "Reply with only the token AGY_PONG and do not call tools.".into(),
        Some("gemini-3.8-flash-low".into()),
        Some("low".into()),
        Some("plan".into()),
    );
    let shared_events = run(&shared_adapter, &context("env-completed-shared")).await;
    assert_eq!(
        check_run_events(&shared_events),
        Ok(RunTermination::ProcessExited)
    );
    if extension_dir_has_cli_auth() {
        let shared_id = session_id(&shared_events).expect("session id");
        let shared_brain = home.join(".gemini/antigravity/brain").join(&shared_id);
        let shared_conv = home
            .join(".gemini/antigravity/conversations")
            .join(format!("{shared_id}.db"));
        let cli_brain_for_shared = home.join(".gemini/antigravity-cli/brain").join(&shared_id);
        let shared_exists = shared_brain.exists();
        let shared_conv_exists = shared_conv.exists();
        let cli_exists_for_shared = cli_brain_for_shared.exists();
        cleanup_session(&shared_id);

        assert_eq!(shared_events[0].kind, ProviderEventKind::SessionStarted);
        assert!(text(&shared_events).contains("AGY_PONG"));
        assert_eq!(turn_status(&shared_events), Some(TurnStatus::Completed));
        assert!(
            shared_exists && shared_conv_exists && !cli_exists_for_shared,
            "ExtensionShared must create brain/<id> and conversations/<id>.db under ~/.gemini/antigravity"
        );
        let exit_shared = exit(&shared_events);
        assert_eq!(
            (exit_shared.exit_code, exit_shared.termination),
            (Some(0), ProcessTermination::Exited)
        );
        assert_eq!(exit_shared.children_reaped, Some(true));
    } else {
        // Linux Remote SSH(`psyki-nas`)처럼 확장 디렉터리에 별도 CLI 토큰 파일이 없는 경우:
        // 다른 디렉터리(`antigravity-cli`)로 조용히 대체하지 않고 `~/.gemini/antigravity`를 검사해
        // Authentication 실패로 보고함을 검증합니다 (Spec §7.3, §7.5).
        assert!(
            home.join(".gemini/antigravity/cli.log").exists(),
            "--app_data_dir=antigravity must target ~/.gemini/antigravity"
        );
        assert!(payloads(&shared_events).iter().any(
            |p| matches!(p, ProviderEventPayload::Error(e) if e.kind == FailureKind::Authentication)
        ));
        assert_eq!(turn_status(&shared_events), Some(TurnStatus::Failed));
        assert_eq!(exit(&shared_events).exit_code, Some(1));
    }

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
    let default_cli_conv = home
        .join(".gemini/antigravity-cli/conversations")
        .join(format!("{default_id}.db"));
    let shared_brain_for_default = home.join(".gemini/antigravity/brain").join(&default_id);
    let default_cli_exists = default_cli_brain.exists();
    let default_cli_conv_exists = default_cli_conv.exists();
    let shared_exists_for_default = shared_brain_for_default.exists();
    cleanup_session(&default_id);

    assert_eq!(
        check_run_events(&default_events),
        Ok(RunTermination::ProcessExited)
    );
    assert!(text(&default_events).contains("CLI_PONG"));
    assert!(
        default_cli_exists && default_cli_conv_exists && !shared_exists_for_default,
        "DefaultCli must create brain/<id> and conversations/<id>.db under ~/.gemini/antigravity-cli"
    );
}

/// PROV-02·07·AGY-DATA-03: `accept-edits` 모드와 정규화된 Workspace(`--add-dir`)에서
/// Antigravity가 자체 scratch 폴더가 아닌 실제 Workspace 안에 파일을 생성하는지 확인합니다 (Spec §7.2, §7.5).
#[tokio::test]
#[ignore = "runs the real Antigravity CLI"]
async fn env_workspace_file_write() {
    let raw_dir = workdir("workspace-write");
    let canonical = raw_dir.canonicalize().unwrap();
    if cfg!(unix) {
        assert!(
            !canonical.to_string_lossy().starts_with(r"\\?\"),
            "Unix canonicalize must not produce Windows extended-length prefix"
        );
    }
    let cwd = match canonical.to_str().and_then(|s| s.strip_prefix(r"\\?\")) {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
        _ => canonical,
    };
    let target = cwd.join("result-agy.txt");
    let _ = std::fs::remove_file(&target);

    let host_id = fixtures::run_context(&fixtures::task_spec()).host_id;
    let configured = AgyAdapter::with_disable_slash_commands(
        executable(),
        "1.4.7".into(),
        host_id,
        Arc::new(NativeRuntime),
        Duration::from_secs(2),
        cwd.clone(),
        "Create a file named result-agy.txt in the current working directory whose entire content is: pong. Do not run shell commands. Then reply done.".into(),
        Some("gemini-3.8-flash-low".into()),
        Some("low".into()),
        Some("accept-edits".into()),
        true,
        vec![cwd.clone()],
        vec![],
    );
    let adapter = if extension_dir_has_cli_auth() {
        configured
    } else {
        configured.with_default_cli_data_dir()
    };
    let events = run(&adapter, &context("env-workspace-write")).await;
    let id = session_id(&events).expect("session id");
    let active_dir = if extension_dir_has_cli_auth() {
        "antigravity"
    } else {
        "antigravity-cli"
    };
    let scratch_file = home_dir()
        .join(".gemini")
        .join(active_dir)
        .join("brain")
        .join(&id)
        .join("scratch/result-agy.txt");
    let scratch_root_file = home_dir()
        .join(".gemini")
        .join(active_dir)
        .join("scratch/result-agy.txt");
    let scratch_exists = scratch_file.exists() || scratch_root_file.exists();
    let content = std::fs::read_to_string(&target);
    let _ = std::fs::remove_file(&target);
    cleanup_session(&id);

    assert_eq!(check_run_events(&events), Ok(RunTermination::ProcessExited));
    assert_eq!(turn_status(&events), Some(TurnStatus::Completed));
    assert!(
        !scratch_exists,
        "file must not be written to Antigravity scratch directory"
    );
    let content = content.expect("result-agy.txt must be written inside workspace cwd");
    assert_eq!(content.trim(), "pong");
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
    let configured = AgyAdapter::configured(
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
    let adapter = if extension_dir_has_cli_auth() {
        configured
    } else {
        configured.with_default_cli_data_dir()
    };
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
    assert_eq!(turn_status(&events), Some(TurnStatus::Cancelled));
    assert!(matches!(
        exit.termination,
        ProcessTermination::TerminatedOnRequest | ProcessTermination::Killed
    ));
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

fn normalized_workdir(name: &str) -> PathBuf {
    let raw = workdir(name);
    let canonical = raw.canonicalize().unwrap_or(raw);
    match canonical.to_str().and_then(|s| s.strip_prefix(r"\\?\")) {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
        _ => canonical,
    }
}

/// `<home>/.gemini/<dir_name>/conversation_summaries.db`에서指定한 `conversation_id` 행을 읽기 전용으로 조회합니다.
/// Provider 내부 DB는 절대 수정하지 않습니다 (배정표 공통 규칙 §1.4).
fn query_conversation_summary(dir_name: &str, conversation_id: &str) -> Option<serde_json::Value> {
    let db_path = home_dir()
        .join(".gemini")
        .join(dir_name)
        .join("conversation_summaries.db");
    if !db_path.exists() {
        return None;
    }
    let script = r#"
import json, pathlib, sqlite3, sys
db_path = pathlib.Path(sys.argv[1])
conv_id = sys.argv[2]
con = sqlite3.connect(f"file:{db_path.as_posix()}?mode=ro", uri=True)
con.row_factory = sqlite3.Row
row = con.execute(
    "SELECT conversation_id, title, step_count, workspace_uris, status, source, project_id, app_data_dir "
    "FROM conversation_summaries WHERE conversation_id = ?",
    (conv_id,),
).fetchone()
print(json.dumps(dict(row) if row else None))
"#;
    let python_bins = if cfg!(windows) {
        ["python", "python3"]
    } else {
        ["python3", "python"]
    };
    for bin in python_bins {
        if let Ok(output) = std::process::Command::new(bin)
            .args([
                "-c",
                script,
                db_path.to_str().expect("utf-8 db path"),
                conversation_id,
            ])
            .output()
            && output.status.success()
            && let Ok(val) = serde_json::from_slice::<serde_json::Value>(&output.stdout)
        {
            return if val.is_null() { None } else { Some(val) };
        }
    }
    panic!("python/python3 with sqlite3 is required to inspect conversation_summaries.db");
}

/// DEC-08·AGENT-05 (Spec §7.6, 배정표 §4 A3):
/// 1. `AgyVsCodeListing::Listed`(`--app_data_dir=antigravity`) 실행은 확장 대화 목록 인덱스
///    (`~/.gemini/antigravity/conversation_summaries.db`)에 기록되고 `antigravity-cli` 인덱스에는 기록되지 않음을 확인합니다.
/// 2. `--add-dir <cwd>`를 전달했을 때 `workspace_uris`에 작업 폴더 URI가 기록되고,
///    `--project`를 생략하면 `project_id`가 `"default-cli-project"`로 기록됨을 확인합니다.
/// 3. Resume(`--conversation <id>`) 실행 시 같은 대화 행이 유지되고 `step_count`가 증가함을 확인합니다.
/// 4. `AgyVsCodeListing::Hidden`(`DefaultCli`) 실행은 확장 인덱스(`~/.gemini/antigravity/conversation_summaries.db`)에서
///    제외되고 `~/.gemini/antigravity-cli/conversation_summaries.db`에만 기록됨을 확인합니다.
#[tokio::test]
#[ignore = "runs the real Antigravity CLI and inspects conversation_summaries.db"]
async fn env_vscode_listing_summaries_and_workspace_filter() {
    let host_id = fixtures::run_context(&fixtures::task_spec()).host_id;
    let listed_cwd = normalized_workdir("listing-listed");
    let expected_uri_fragment = listed_cwd.to_string_lossy().replace('\\', "/");

    // 1. Listed (--app_data_dir=antigravity) + --add-dir <listed_cwd>
    let listed_adapter = AgyAdapter::with_disable_slash_commands(
        executable(),
        "1.4.7".into(),
        host_id.clone(),
        Arc::new(NativeRuntime),
        Duration::from_secs(2),
        listed_cwd.clone(),
        "Reply with only the token LISTED_OK and do not call tools.".into(),
        Some("gemini-3.8-flash-low".into()),
        Some("low".into()),
        Some("plan".into()),
        true,
        vec![listed_cwd.clone()],
        vec![],
    )
    .with_vscode_listing(AgyVsCodeListing::Listed);

    let listed_events = run(&listed_adapter, &context("env-listing-listed")).await;
    assert_eq!(
        check_run_events(&listed_events),
        Ok(RunTermination::ProcessExited)
    );

    if extension_dir_has_cli_auth() {
        let listed_id = session_id(&listed_events).expect("listed session id");
        let ext_summary = query_conversation_summary("antigravity", &listed_id);
        let cli_summary_for_listed = query_conversation_summary("antigravity-cli", &listed_id);
        println!("listed summary in antigravity: {ext_summary:?}");
        println!("listed summary in antigravity-cli: {cli_summary_for_listed:?}");

        let row = ext_summary.as_ref().expect(
            "listed run must be recorded in ~/.gemini/antigravity/conversation_summaries.db",
        );
        let initial_steps = row["step_count"].as_i64().unwrap_or(0);
        let workspace_uris = row["workspace_uris"].as_str().unwrap_or_default();
        assert_eq!(row["app_data_dir"].as_str(), Some("antigravity"));
        assert_eq!(
            row["source"].as_str(),
            Some(""),
            "listed run (--app_data_dir=antigravity) must be recorded as extension-native (source == \"\")"
        );
        assert_eq!(row["project_id"].as_str(), Some("default-cli-project"));
        assert!(
            initial_steps > 0,
            "listed summary must have positive step_count: {row:?}"
        );
        assert!(
            workspace_uris.contains(&expected_uri_fragment),
            "workspace_uris ({workspace_uris}) must contain {expected_uri_fragment}"
        );
        assert!(
            cli_summary_for_listed.is_none(),
            "listed run must not be recorded in antigravity-cli/conversation_summaries.db"
        );

        // Resume (--conversation <listed_id>) preserves the entry and advances step_count
        let mut resume_ctx = context("env-listing-listed-resume");
        resume_ctx.session.mode = SessionMode::Resume;
        resume_ctx.session.source_session_id = Some(listed_id.clone());
        let resumed_adapter = AgyAdapter::with_disable_slash_commands(
            executable(),
            "1.4.7".into(),
            host_id.clone(),
            Arc::new(NativeRuntime),
            Duration::from_secs(2),
            listed_cwd.clone(),
            "Reply with only the token RESUMED_OK and do not call tools.".into(),
            Some("gemini-3.8-flash-low".into()),
            Some("low".into()),
            Some("plan".into()),
            true,
            vec![listed_cwd.clone()],
            vec![],
        )
        .with_vscode_listing(AgyVsCodeListing::Listed);
        let resumed_events = run(&resumed_adapter, &resume_ctx).await;
        let resumed_row = query_conversation_summary("antigravity", &listed_id)
            .expect("resumed listed run remains in antigravity/conversation_summaries.db");
        let resumed_steps = resumed_row["step_count"].as_i64().unwrap_or(0);
        println!("resumed summary in antigravity: {resumed_row:?}");

        cleanup_session(&listed_id);
        let residual_after_cleanup = query_conversation_summary("antigravity", &listed_id);
        println!(
            "listed summary in antigravity after brain/conversations file cleanup: {}",
            residual_after_cleanup.is_some()
        );

        assert_eq!(
            check_run_events(&resumed_events),
            Ok(RunTermination::ProcessExited)
        );
        assert_eq!(
            session_id(&resumed_events).as_deref(),
            Some(listed_id.as_str())
        );
        assert!(
            resumed_steps > initial_steps,
            "resumed step_count ({resumed_steps}) must be greater than initial ({initial_steps})"
        );
    } else {
        assert!(payloads(&listed_events).iter().any(
            |p| matches!(p, ProviderEventPayload::Error(e) if e.kind == FailureKind::Authentication)
        ));
    }

    // 2. Hidden (DefaultCli, --app_data_dir 생략) + --add-dir <hidden_cwd>
    let hidden_cwd = normalized_workdir("listing-hidden");
    let hidden_uri_fragment = hidden_cwd.to_string_lossy().replace('\\', "/");
    let hidden_adapter = AgyAdapter::with_disable_slash_commands(
        executable(),
        "1.4.7".into(),
        host_id,
        Arc::new(NativeRuntime),
        Duration::from_secs(2),
        hidden_cwd.clone(),
        "Reply with only the token HIDDEN_OK and do not call tools.".into(),
        Some("gemini-3.8-flash-low".into()),
        Some("low".into()),
        Some("plan".into()),
        true,
        vec![hidden_cwd.clone()],
        vec![],
    )
    .with_vscode_listing(AgyVsCodeListing::Hidden);

    let hidden_events = run(&hidden_adapter, &context("env-listing-hidden")).await;
    let hidden_id = session_id(&hidden_events).expect("hidden session id");
    let home = home_dir();
    let hidden_in_ext_brain = home
        .join(".gemini/antigravity/brain")
        .join(&hidden_id)
        .exists();
    let hidden_in_ext_conv = home
        .join(".gemini/antigravity/conversations")
        .join(format!("{hidden_id}.db"))
        .exists();
    let hidden_in_cli_brain = home
        .join(".gemini/antigravity-cli/brain")
        .join(&hidden_id)
        .exists();
    let hidden_in_cli_conv = home
        .join(".gemini/antigravity-cli/conversations")
        .join(format!("{hidden_id}.db"))
        .exists();
    let ext_summary_for_hidden = query_conversation_summary("antigravity", &hidden_id);
    let cli_summary_for_hidden = query_conversation_summary("antigravity-cli", &hidden_id);
    println!("hidden summary in antigravity: {ext_summary_for_hidden:?}");
    println!("hidden summary in antigravity-cli: {cli_summary_for_hidden:?}");

    cleanup_session(&hidden_id);
    let hidden_residual_after_cleanup = query_conversation_summary("antigravity-cli", &hidden_id);
    println!(
        "hidden summary in antigravity-cli after brain/conversations file cleanup: {}",
        hidden_residual_after_cleanup.is_some()
    );

    assert_eq!(
        check_run_events(&hidden_events),
        Ok(RunTermination::ProcessExited)
    );
    assert!(text(&hidden_events).contains("HIDDEN_OK"));
    assert!(
        !hidden_in_ext_brain && !hidden_in_ext_conv && hidden_in_cli_brain && hidden_in_cli_conv,
        "hidden run must only create brain/conversations files under ~/.gemini/antigravity-cli"
    );
    // 2026-10-25 관찰(VSCODE_LISTING.md §3.3): 확장 허브(`agy --hub --app_data_dir=antigravity`)가 실행 중이면
    // DefaultCli 대화가 확장 인덱스(~/.gemini/antigravity/conversation_summaries.db)에도
    // source="CORTEX_TRAJECTORY_SOURCE_CLI"(제목 없음)로 나타났고, 대화 DB를 지운 뒤에는 사라졌습니다.
    // 허브 감시 원인은 추정으로 두며, 확장 UI 숨김은 사용자 화면 확인(`07b7210`, Windows 로컬)으로 확인했습니다.
    // 이 자동 시험은 파일 위치와 인덱스 출처만 확인하며, "hidden이 확장 인덱스에서 제외된다"나 UI 표시 자체를 자동으로 보장하지 않습니다.
    if let Some(mirrored) = &ext_summary_for_hidden {
        assert_eq!(
            mirrored["source"].as_str(),
            Some("CORTEX_TRAJECTORY_SOURCE_CLI"),
            "if hub AdditionalConvoDir watcher mirrors a DefaultCli session, it must be tagged CORTEX_TRAJECTORY_SOURCE_CLI: {mirrored:?}"
        );
    }
    let hidden_row = cli_summary_for_hidden.expect(
        "hidden run must be recorded in ~/.gemini/antigravity-cli/conversation_summaries.db",
    );
    assert_eq!(hidden_row["app_data_dir"].as_str(), Some("antigravity-cli"));
    assert_eq!(hidden_row["source"].as_str(), Some(""));
    assert!(hidden_row["step_count"].as_i64().unwrap_or(0) > 0);
    assert!(
        hidden_row["workspace_uris"]
            .as_str()
            .unwrap_or_default()
            .contains(&hidden_uri_fragment)
    );
}

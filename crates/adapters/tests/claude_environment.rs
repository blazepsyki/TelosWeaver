//! Claude Code 실환경 시험 (PH0-CLAUDE·PH1-CLAUDE, PROV-01~08·PROV-05·FORK-POC-01, 이월 P1-1~3).
//!
//! 실제 CLI를 실행하고 사용자 계정의 사용량을 쓰므로 기본 실행에서 제외합니다(`#[ignore]`).
//! 실행 조건(배정표 §6): 모델 `haiku`, 기본 설정 디렉터리, 저장소 밖 일회용 폴더, 허용 도구 명시.
//! 사용자 설정 파일은 읽기만 하며, 시험이 만든 세션은 `~/.claude/projects/<작업 폴더 이름>`에 남습니다(병합 때 정리).
//!
//! ```text
//! TELOS_CLAUDE_CLI=<확장의 resources/native-binary/claude.exe> \
//!   cargo test -p telos-adapters --test claude_environment -- --ignored --test-threads=1 --nocapture
//! ```
//!
//! 각 시험은 만든 Native Session ID를 `claude-session:` 줄로 출력합니다. 시험 뒤 Archive 대상입니다.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use futures::StreamExt;
use telos_adapters::{
    AdapterError, AgentAdapter, ClaudeCodeAdapter,
    claude::{
        ClaudeApprovals, ClaudeCodeConfig, ClaudeLaunch, ClaudeLaunchResolver, VsCodeListing,
    },
    runtime::NativeRuntime,
};
use telos_schema::{
    ApprovalAnswer, ApprovalRequiredPayload, CapabilityState, FailureKind, ProcessTermination,
    ProviderEvent, ProviderEventKind, ProviderEventPayload, RunContext, RunTermination,
    SessionMode, TaskMcpBinding, TaskSpec, ToolOutcome, TurnStatus, capability_keys as keys,
    check_run_events, fixtures,
};

struct Launch(ClaudeLaunch);

impl ClaudeLaunchResolver for Launch {
    fn resolve(&self, _: &TaskSpec, _: &RunContext) -> Result<ClaudeLaunch, AdapterError> {
        Ok(self.0.clone())
    }
}

fn executable() -> PathBuf {
    PathBuf::from(std::env::var_os("TELOS_CLAUDE_CLI").expect("set TELOS_CLAUDE_CLI"))
}

/// 저장소 밖 임시 폴더입니다. Windows의 짧은 이름(`PSYKI~1`)을 풀어 씁니다. 짧은 이름 경로는 CLI 2.1.325의
/// 쓰기 안전 검사("suspicious Windows path")에 걸려 권한 모드와 관계없이 승인을 요구합니다.
fn temp_root(folder: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(folder);
    std::fs::create_dir_all(&dir).unwrap();
    let full = std::fs::canonicalize(&dir).unwrap().display().to_string();
    PathBuf::from(full.strip_prefix(r"\\?\").unwrap_or(&full))
}

fn workdir(name: &str) -> PathBuf {
    let dir = temp_root("telos-claude-env").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Core 상태 디렉터리 역할을 하는 폴더입니다. Workspace(작업 폴더) 밖에 둡니다.
fn state_dir(name: &str) -> PathBuf {
    let dir = temp_root("telos-claude-env-state").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// 설치된 CLI가 지금 보고하는 버전입니다(모델 호출 없음). 시험이 버전 문자열을 고정하지 않습니다.
fn cli_version() -> String {
    static VERSION: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    VERSION
        .get_or_init(|| {
            let output = std::process::Command::new(executable())
                .arg("--version")
                .output()
                .expect("run --version");
            let version = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            println!("claude-cli-version: {version}");
            version
        })
        .clone()
}

fn launch(name: &str, prompt: &str) -> ClaudeLaunch {
    ClaudeLaunch {
        cwd: workdir(name),
        prompt: prompt.into(),
        model: Some("haiku".into()),
        permission_mode: "default".into(),
        allowed_tools: vec![],
        disallowed_tools: vec![],
        tools: Some(vec![]),
        setting_sources: Some("user".into()),
    }
}

fn adapter(launch: ClaudeLaunch) -> ClaudeCodeAdapter {
    let host_id = fixtures::run_context(&fixtures::task_spec()).host_id;
    ClaudeCodeAdapter::new(ClaudeCodeConfig {
        executable: executable(),
        cli_version: cli_version(),
        host_id,
        runtime: Arc::new(NativeRuntime),
        launcher: Arc::new(Launch(launch)),
        cancel_grace: Duration::from_secs(2),
    })
}

fn context(run_id: &str) -> RunContext {
    let mut context = fixtures::run_context(&fixtures::task_spec());
    context.run_id = run_id.into();
    context.idempotency_key = format!("{run_id}:start");
    context
}

async fn run(adapter: &ClaudeCodeAdapter, context: &RunContext) -> Vec<ProviderEvent> {
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
    if let Some(id) = events.iter().find_map(|e| e.native_session_id.clone()) {
        println!("claude-session: {id}");
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

/// PROV-02·03·04·06: 신규 Managed Run의 시작→스트림→완료→종료.
#[tokio::test]
#[ignore = "runs the real Claude Code CLI"]
async fn env_completed_run() {
    let adapter = adapter(launch("completed", "Reply with exactly: pong"));
    let events = run(&adapter, &context("env-completed")).await;
    assert_eq!(check_run_events(&events), Ok(RunTermination::ProcessExited));
    assert_eq!(events[0].kind, ProviderEventKind::SessionStarted);
    assert!(text(&events).to_lowercase().contains("pong"));
    assert_eq!(turn_status(&events), Some(TurnStatus::Completed));
    let exit = exit(&events);
    assert_eq!(
        (exit.exit_code, exit.termination),
        (Some(0), ProcessTermination::Exited)
    );
    assert_eq!(exit.children_reaped, Some(true));
}

/// §7.2: 백그라운드 작업을 요청해도 CLI가 첫 `result` 뒤에 Turn을 이어가지 않고 스스로 끝납니다
/// (`CLAUDE_CODE_DISABLE_BACKGROUND_TASKS`). 이어갔다면 Adapter가 프로세스를 끝내 요청 종료로 보고됩니다.
#[tokio::test]
#[ignore = "runs the real Claude Code CLI"]
async fn env_background_tasks_do_not_continue_the_run() {
    let mut launch = launch(
        "background",
        "Use the Bash tool with run_in_background set to true to run: sleep 5. Then reply 'started'. When the background command finishes, create a file named done.txt containing ok and reply done.",
    );
    launch.tools = None;
    launch.permission_mode = "acceptEdits".into();
    launch.allowed_tools = vec!["Bash(sleep:*)".into(), "Write".into()];
    let _ = std::fs::remove_file(launch.cwd.join("done.txt"));
    let adapter = adapter(launch);
    let events = run(&adapter, &context("env-background")).await;
    assert_eq!(check_run_events(&events), Ok(RunTermination::ProcessExited));
    let turns = payloads(&events)
        .into_iter()
        .filter(|p| matches!(p, ProviderEventPayload::TurnFinished(_)))
        .count();
    assert_eq!(turns, 1);
    assert_eq!(turn_status(&events), Some(TurnStatus::Completed));
    let exit = exit(&events);
    assert_eq!(
        (exit.exit_code, exit.termination),
        (Some(0), ProcessTermination::Exited),
        "the CLI must end by itself after the only turn"
    );
}

/// PROV-07: 시작 전 정책으로 허용하지 않은 도구는 거부되고 파일이 생기지 않습니다.
#[tokio::test]
#[ignore = "runs the real Claude Code CLI"]
async fn env_tool_denied() {
    let mut launch = launch(
        "denied",
        "Use the Write tool to create b.txt containing hello. Do not use any other tool. Then say done or failed.",
    );
    launch.tools = None;
    launch.allowed_tools = vec!["Read".into()];
    launch.disallowed_tools = vec!["Bash".into()];
    let target = launch.cwd.join("b.txt");
    let _ = std::fs::remove_file(&target);
    let adapter = adapter(launch);
    let events = run(&adapter, &context("env-denied")).await;
    assert_eq!(check_run_events(&events), Ok(RunTermination::ProcessExited));
    let denied = payloads(&events).into_iter().any(|p| {
        matches!(p, ProviderEventPayload::ToolCompleted(t) if t.name == "Write" && t.outcome == ToolOutcome::Denied)
    });
    assert!(denied, "Write must be reported as denied");
    assert!(!target.exists(), "denied write must not create the file");
}

/// PROV-06: Provider 오류는 실패한 Turn과 분류된 오류로 끝나고 성공으로 보지 않습니다.
#[tokio::test]
#[ignore = "runs the real Claude Code CLI"]
async fn env_invalid_model_fails() {
    let mut launch = launch("invalid-model", "hi");
    launch.model = Some("no-such-model-xyz".into());
    let adapter = adapter(launch);
    let events = run(&adapter, &context("env-invalid-model")).await;
    assert_eq!(check_run_events(&events), Ok(RunTermination::ProcessExited));
    assert!(payloads(&events).iter().any(
        |p| matches!(p, ProviderEventPayload::Error(e) if e.kind == FailureKind::Configuration)
    ));
    assert_eq!(turn_status(&events), Some(TurnStatus::Failed));
    assert_ne!(exit(&events).exit_code, Some(0));
}

/// PROV-08: 취소 접수 뒤 실제 종료와 자식 회수를 확인합니다.
#[tokio::test]
#[ignore = "runs the real Claude Code CLI"]
async fn env_cancel_terminates_process_tree() {
    let adapter = adapter(launch(
        "cancel",
        "Write the numbers from 1 to 400, one per line, with no other text.",
    ));
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
    assert_eq!(check_run_events(&events), Ok(RunTermination::ProcessExited));
    let exit = exit(&events);
    // 결과가 먼저 끝났으면 자연 종료가 실제 사건입니다. 그 경우도 그대로 기록합니다.
    if turn_status(&events) == Some(TurnStatus::Cancelled) {
        assert!(matches!(
            exit.termination,
            ProcessTermination::TerminatedOnRequest | ProcessTermination::Killed
        ));
    }
    assert_eq!(exit.children_reaped, Some(true));
}

/// PROV-05·FORK-POC-01: Resume은 같은 대화를 잇고, Fork는 새 Native ID로 이어 받습니다.
#[tokio::test]
#[ignore = "runs the real Claude Code CLI"]
async fn env_resume_and_fork() {
    let base = adapter(launch(
        "resume",
        "Remember this codeword: maple-17. Reply only: ok",
    ));
    let first = run(&base, &context("env-resume-base")).await;
    let parent = first
        .iter()
        .find_map(|e| e.native_session_id.clone())
        .expect("session id");

    let follow = |mode| {
        let mut context = context(match mode {
            SessionMode::Resume => "env-resume",
            _ => "env-fork",
        });
        context.session.mode = mode;
        context.session.source_session_id = Some(parent.clone());
        context
    };
    let ask = || {
        adapter(launch(
            "resume",
            "What was the codeword? Reply only the codeword.",
        ))
    };

    let resumed = run(&ask(), &follow(SessionMode::Resume)).await;
    assert_eq!(
        check_run_events(&resumed),
        Ok(RunTermination::ProcessExited)
    );
    assert!(text(&resumed).contains("maple-17"));
    let resumed_id = resumed.iter().find_map(|e| e.native_session_id.clone());
    println!("resume: parent={parent} resumed={resumed_id:?}");

    let forked = run(&ask(), &follow(SessionMode::NativeFork)).await;
    assert_eq!(check_run_events(&forked), Ok(RunTermination::ProcessExited));
    assert!(text(&forked).contains("maple-17"));
    let forked_id = forked.iter().find_map(|e| e.native_session_id.clone());
    println!("fork: parent={parent} forked={forked_id:?}");
    assert_ne!(
        forked_id.as_deref(),
        Some(parent.as_str()),
        "fork must get a new session id"
    );
}

/// 세션 파일 위치. 기본 설정 디렉터리(`CLAUDE_CONFIG_DIR` 또는 `~/.claude`)의 `projects/*/<id>.jsonl`입니다.
fn session_file(session_id: &str) -> PathBuf {
    let config = std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(
                std::env::var_os("USERPROFILE")
                    .or_else(|| std::env::var_os("HOME"))
                    .expect("home directory"),
            )
            .join(".claude")
        });
    let file = format!("{session_id}.jsonl");
    std::fs::read_dir(config.join("projects"))
        .unwrap()
        .map(|entry| entry.unwrap().path().join(&file))
        .find(|path| path.is_file())
        .unwrap_or_else(|| panic!("session file for {session_id}"))
}

/// 확장의 프로젝트 폴더 이름 규칙: 영숫자가 아닌 문자를 `-`로 바꿉니다(200자 이하 경로).
fn project_dir_name(cwd: &str) -> String {
    cwd.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

fn same_path(a: &std::path::Path, b: &std::path::Path) -> bool {
    let canonical = |p: &std::path::Path| {
        let text = std::fs::canonicalize(p).unwrap().display().to_string();
        let text = text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned();
        if cfg!(windows) {
            text.to_lowercase()
        } else {
            text
        }
    };
    canonical(a) == canonical(b)
}

/// 세션 파일이 확장 목록의 조회 범위(Workspace = cwd인 프로젝트 폴더)에 있고,
/// 모든 줄의 `entrypoint`가 기대 값인지 확인합니다. 확장 2.1.310은 첫 줄 부근 값으로 판정합니다 (§7.6).
fn assert_listing(session_id: &str, workspace: &std::path::Path, entrypoint: &str) {
    let path = session_file(session_id);
    println!("session-file: {}", path.display());
    let text = std::fs::read_to_string(&path).unwrap();
    let mut recorded_cwd = None;
    let mut entrypoints = Vec::new();
    for line in text.lines() {
        let value: serde_json::Value = serde_json::from_str(line).unwrap();
        if let Some(cwd) = value["cwd"].as_str() {
            recorded_cwd.get_or_insert_with(|| cwd.to_owned());
        }
        if let Some(e) = value["entrypoint"].as_str() {
            entrypoints.push(e.to_owned());
        }
    }
    let cwd = recorded_cwd.expect("session records its cwd");
    assert!(
        same_path(std::path::Path::new(&cwd), workspace),
        "cwd {cwd}"
    );
    let dir = path
        .parent()
        .unwrap()
        .file_name()
        .unwrap()
        .to_string_lossy();
    let expected_dir = project_dir_name(&cwd);
    assert!(
        dir.eq_ignore_ascii_case(&expected_dir),
        "project folder {dir} != {expected_dir}"
    );
    assert!(!entrypoints.is_empty());
    assert!(
        entrypoints.iter().all(|e| e == entrypoint),
        "entrypoints {entrypoints:?}"
    );
}

fn session_id(events: &[ProviderEvent]) -> String {
    events
        .iter()
        .find_map(|e| e.native_session_id.clone())
        .expect("session id")
}

/// §7.6·DEC-08·AGENT-05: 기본(`listed`) Run은 Fresh·Resume·Fork 모두 확장 목록 조회 대상으로 기록되고,
/// `hidden` Run은 자동화 출처로 기록됩니다. 시험 셸이 상속한 값(`claude-vscode` 등)과 관계없이 명시 값이 쓰입니다.
#[tokio::test]
#[ignore = "runs the real Claude Code CLI"]
async fn env_vscode_listing() {
    let listed = launch(
        "listing",
        "Remember this codeword: cedar-42. Reply only: ok",
    );
    let workspace = listed.cwd.clone();
    let first = run(&adapter(listed), &context("env-listing-base")).await;
    assert_eq!(check_run_events(&first), Ok(RunTermination::ProcessExited));
    let parent = session_id(&first);
    assert_listing(&parent, &workspace, "claude-vscode");

    let follow = |mode, run_id: &str| {
        let mut context = context(run_id);
        context.session.mode = mode;
        context.session.source_session_id = Some(parent.clone());
        context
    };
    let ask = || adapter(launch("listing", "What was the codeword? Reply only it."));
    let resumed = run(&ask(), &follow(SessionMode::Resume, "env-listing-resume")).await;
    assert!(text(&resumed).contains("cedar-42"));
    assert_eq!(session_id(&resumed), parent, "resume keeps the session");
    assert_listing(&parent, &workspace, "claude-vscode");

    let forked = run(&ask(), &follow(SessionMode::NativeFork, "env-listing-fork")).await;
    assert!(text(&forked).contains("cedar-42"));
    let fork_id = session_id(&forked);
    assert_ne!(fork_id, parent);
    assert_listing(&fork_id, &workspace, "claude-vscode");

    let hidden = adapter(launch("listing-hidden", "Reply with exactly: pong"))
        .with_vscode_listing(VsCodeListing::Hidden);
    let events = run(&hidden, &context("env-listing-hidden")).await;
    assert_eq!(check_run_events(&events), Ok(RunTermination::ProcessExited));
    assert_listing(&session_id(&events), &workdir("listing-hidden"), "sdk-cli");
}

/// PROV-01: 설치된 VS Code 확장에서 공통 탐지로 CLI를 찾습니다. 버전 명령만 실행하며 사용량을 쓰지 않습니다.
#[tokio::test]
#[ignore = "reads the local VS Code extensions folder"]
async fn env_discovery_finds_extension_cli() {
    use telos_adapters::{CliDiscoverer, DiscoveryRequest, NativeCliDiscoverer};
    use telos_schema::{BinaryLocation, CliSource, ExtensionResolution};

    let home = PathBuf::from(
        std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .expect("home directory"),
    );
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
    let rule = telos_adapters::claude::cli_rule(platform).unwrap();
    let request = DiscoveryRequest {
        host_id: std::env::var("TELOS_HOST_ID").unwrap_or_else(|_| "windows-local".into()),
        target_platform: platform.into(),
        explicit_path: None,
        extension_roots: roots,
        home_dir: home,
        path_env: None,
        checked_at: "2026-10-24T00:00:00Z".into(),
    };
    let discovery = NativeCliDiscoverer::new(Arc::new(NativeRuntime))
        .discover("claude", &rule, &request)
        .await;
    println!("{}", serde_json::to_string_pretty(&discovery).unwrap());
    let cli = discovery.found().expect("Claude Code CLI is discovered");
    assert_eq!(cli.source, CliSource::Extension);
    assert_eq!(cli.location, BinaryLocation::RelativePath);
    assert!(cli.cli_version.contains("Claude Code"));
    let extension = cli.extension.as_ref().unwrap();
    assert_eq!(extension.extension_id, "anthropic.claude-code");
    assert_eq!(extension.resolution, ExtensionResolution::Metadata);
}

/// Adapter를 거치지 않고 CLI를 Stream JSON 입출력으로 직접 띄우는 명령입니다. CLI 자체의 동작을 확인할 때만 씁니다.
/// 기본 옵션(`haiku`, `default` 권한, 사용자 설정만)에 `extra`를 붙입니다.
fn raw_command(dir: &Path, extra: &[&str]) -> std::process::Command {
    use std::process::{Command, Stdio};
    let mut command = Command::new(executable());
    command
        .args([
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            "--model",
            "haiku",
            "--permission-mode",
            "default",
            "--setting-sources",
            "user",
        ])
        .args(extra)
        .current_dir(dir)
        // Adapter와 같이 부모 세션 변수를 지우고 목록 밖 출처로 둡니다(§7.6).
        .env("CLAUDE_CODE_ENTRYPOINT", "sdk-cli")
        .env("CLAUDE_CODE_DISABLE_BACKGROUND_TASKS", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    for (key, _) in std::env::vars_os() {
        let key = key.to_string_lossy().into_owned();
        let parent = key == "CLAUDECODE" || key == "CLAUDE_PID" || key.starts_with("CLAUDE_CODE_");
        if parent
            && key != "CLAUDE_CODE_ENTRYPOINT"
            && key != "CLAUDE_CODE_DISABLE_BACKGROUND_TASKS"
        {
            command.env_remove(key);
        }
    }
    command
}

/// 승인 실험 한 번의 관찰 결과입니다.
struct ApprovalProbe {
    session_id: String,
    asked_tool: Option<String>,
    denials: usize,
    file_written: bool,
    exit_ok: bool,
}

/// CLI를 직접 실행해 `--permission-prompt-tool stdio`의 실시간 승인 요청에 `behavior`로 답합니다.
/// Adapter는 이 경로를 구현하지 않으므로(Phase 1) CLI 수단의 존재만 확인합니다 (G3 재검토 C4).
fn probe_live_approval(name: &str, behavior: &str) -> ApprovalProbe {
    use serde_json::{Value, json};
    use std::io::{BufRead, BufReader, Write};

    let dir = workdir(name);
    let _ = std::fs::remove_file(dir.join("probe.txt"));
    let mut child = raw_command(
        &dir,
        &["--permission-prompt-tool", "stdio", "--tools", "Write"],
    )
    .spawn()
    .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut send = |value: Value| {
        writeln!(stdin, "{value}").unwrap();
        stdin.flush().unwrap();
    };
    send(
        json!({"type":"control_request","request_id":"init-1","request":{"subtype":"initialize"}}),
    );
    send(json!({"type":"user","message":{"role":"user",
        "content":"Use the Write tool to create probe.txt containing OK. Do nothing else."}}));

    let stdout = child.stdout.take().unwrap();
    // 출력이 멈춰도 끝나도록 시간 제한이 지나면 이 시험의 프로세스만 종료합니다.
    let child = std::sync::Arc::new(std::sync::Mutex::new(child));
    let (done, finished) = std::sync::mpsc::channel::<()>();
    let killer = {
        let child = child.clone();
        std::thread::spawn(move || {
            if finished.recv_timeout(Duration::from_secs(120)).is_err() {
                let _ = child.lock().unwrap().kill();
            }
        })
    };
    let mut probe = ApprovalProbe {
        session_id: String::new(),
        asked_tool: None,
        denials: 0,
        file_written: false,
        exit_ok: false,
    };
    for line in BufReader::new(stdout).lines() {
        let event: Value = serde_json::from_str(&line.unwrap()).unwrap();
        if let Some(id) = event["session_id"].as_str() {
            probe.session_id = id.to_owned();
        }
        match event["type"].as_str() {
            Some("control_request") if event["request"]["subtype"] == "can_use_tool" => {
                probe.asked_tool = event["request"]["tool_name"].as_str().map(str::to_owned);
                let response = if behavior == "allow" {
                    json!({"behavior":"allow","updatedInput":event["request"]["input"]})
                } else {
                    json!({"behavior":"deny","message":"denied by TelosWeaver test"})
                };
                send(
                    json!({"type":"control_response","response":{"subtype":"success",
                    "request_id":event["request_id"],"response":response}}),
                );
            }
            Some("result") => {
                probe.denials = event["permission_denials"].as_array().map_or(0, Vec::len);
                break;
            }
            _ => {}
        }
    }
    drop(send);
    drop(stdin);
    let _ = done.send(());
    killer.join().unwrap();
    probe.exit_ok = child.lock().unwrap().wait().unwrap().success();
    probe.file_written = dir.join("probe.txt").is_file();
    println!("claude-session: {}", probe.session_id);
    probe
}

#[test]
#[ignore = "runs the real Claude Code CLI"]
fn env_permission_prompt_tool_offers_live_approval() {
    let allowed = probe_live_approval("approval-allow", "allow");
    assert_eq!(allowed.asked_tool.as_deref(), Some("Write"));
    assert!(allowed.file_written && allowed.denials == 0 && allowed.exit_ok);

    let denied = probe_live_approval("approval-deny", "deny");
    assert_eq!(denied.asked_tool.as_deref(), Some("Write"));
    assert!(!denied.file_written && denied.denials == 1 && denied.exit_ok);
}

// --- PH1-CLAUDE: 사전 점검·Task MCP·Capsule·실시간 승인·Turn 경계 ---

fn home() -> PathBuf {
    PathBuf::from(
        std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .expect("home directory"),
    )
}

/// P2·P3: 설치된 CLI에서 사전 점검이 모델 호출 없이 통과하고, 판정 기준대로 Capability가 정해집니다.
#[tokio::test]
#[ignore = "runs the real Claude Code CLI without a model call"]
async fn env_preflight_passes_and_capabilities_follow_the_evidence() {
    let adapter = adapter(launch("preflight", "unused"));
    let host = fixtures::run_context(&fixtures::task_spec()).host_id;
    let sessions =
        || std::fs::read_dir(home().join(".claude/projects")).map_or(0, |dir| dir.count());
    let before = sessions();
    let started = Instant::now();
    let report = adapter.preflight(&host).await.unwrap();
    println!(
        "preflight: {:?}, {} checks",
        started.elapsed(),
        report.results.len()
    );
    for result in &report.results {
        println!("  {} {:?} {:?}", result.passed, result.check, result.detail);
    }
    assert_eq!(report.cli_version, cli_version());
    assert!(report.passed(), "every option the adapter uses is accepted");
    println!("project-folders: before={before} after={}", sessions());

    let caps = adapter.detect(&host).await.unwrap();
    let mut names: Vec<_> = caps.keys().collect();
    names.sort();
    for name in names {
        let cap = &caps[name];
        println!(
            "  {name}: {:?} {:?} {:?}",
            cap.state, cap.evidence_level, cap.reason
        );
    }
    // 이 Host·플랫폼에는 같은 major.minor의 통과 증거가 있으므로 사전 점검 통과로 `supported`입니다.
    for key in [
        keys::MANAGED_RUN,
        keys::EVENT_STREAM,
        keys::CANCEL,
        keys::NATIVE_RESUME,
        keys::NATIVE_FORK,
        keys::VSCODE_LISTING,
    ] {
        assert_eq!(caps[key].state, CapabilityState::Supported, "{key}");
    }
    assert_eq!(caps[keys::SEND_INPUT].state, CapabilityState::Unsupported);
}

/// 사용자 설정에서 MCP와 관련된 부분입니다. 내용은 출력하지 않고 전후 비교에만 씁니다.
/// `~/.claude.json`은 CLI가 실행할 때마다 통계·프로젝트 항목을 고치므로 MCP 관련 키만 봅니다.
fn user_mcp_settings() -> String {
    let home = home();
    let read = |path: PathBuf| std::fs::read_to_string(path).unwrap_or_default();
    let state: serde_json::Value =
        serde_json::from_str(&read(home.join(".claude.json"))).unwrap_or_default();
    let mut parts = vec![
        format!("settings.json={}", read(home.join(".claude/settings.json"))),
        format!(
            "settings.local.json={}",
            read(home.join(".claude/settings.local.json"))
        ),
        format!("mcpServers={}", state["mcpServers"]),
    ];
    if let Some(projects) = state["projects"].as_object() {
        for (path, project) in projects {
            for key in [
                "mcpServers",
                "enabledMcpjsonServers",
                "disabledMcpjsonServers",
            ] {
                let value = &project[key];
                let empty = value.is_null()
                    || value.as_object().is_some_and(serde_json::Map::is_empty)
                    || value.as_array().is_some_and(Vec::is_empty);
                if !empty {
                    parts.push(format!("{path}.{key}={value}"));
                }
            }
        }
    }
    parts.join("\n")
}

/// 사용자 설정 어디에도 주입한 서버가 남지 않았는지 봅니다.
fn user_settings_mention(needle: &str) -> bool {
    let home = home();
    [
        ".claude.json",
        ".claude/settings.json",
        ".claude/settings.local.json",
    ]
    .iter()
    .any(|file| std::fs::read_to_string(home.join(file)).is_ok_and(|text| text.contains(needle)))
}

/// 시험용 stdio MCP 서버(`fixtures/claude/task_mcp_server.rs`)를 빌드합니다.
fn build_task_mcp_server(dir: &Path) -> PathBuf {
    let source = dir.join("task_mcp_server.rs");
    std::fs::write(&source, include_str!("fixtures/claude/task_mcp_server.rs")).unwrap();
    let binary = dir.join(if cfg!(windows) {
        "task_mcp_server.exe"
    } else {
        "task_mcp_server"
    });
    let status = std::process::Command::new("rustc")
        .args(["--edition", "2024", "-O", "-o"])
        .arg(&binary)
        .arg(&source)
        .current_dir(dir)
        .status()
        .expect("rustc is available");
    assert!(status.success(), "fixture MCP server builds");
    binary
}

const TASK_MCP_SERVER: &str = "telosweaver-ph1-probe";
const TASK_MCP_REPLY: &str = "PROBE_OK_PH1_CLAUDE";
const TASK_MCP_SECRET: &str = "SECRET-CREDENTIAL-7c1e9a";

/// P4·L1: `taskMcp`가 있는 Run에서만 Worker가 주입한 서버의 도구를 호출합니다. 사용자 설정은 바뀌지 않고,
/// 자격 원문은 이벤트·세션 기록에 남지 않으며, Run이 끝나면 주입 파일이 지워집니다.
#[tokio::test]
#[ignore = "runs the real Claude Code CLI"]
async fn env_task_mcp_tool_is_called_only_in_the_injected_run() {
    let state = state_dir("task-mcp");
    let server = build_task_mcp_server(&state);
    let log = state.join("calls.log");
    let credential = state.join("run-credential");
    std::fs::write(&credential, TASK_MCP_SECRET).unwrap();
    let binding = TaskMcpBinding {
        server_name: TASK_MCP_SERVER.into(),
        command: server.display().to_string(),
        args: vec![
            "--reply".into(),
            TASK_MCP_REPLY.into(),
            "--log".into(),
            log.display().to_string(),
        ],
        credential_file: credential.display().to_string(),
    };
    let prompt = "If a tool named telos_probe is available, call it once and reply with exactly the text it returns. If no such tool is available, reply with exactly: NO-TOOL";
    let calls = || {
        std::fs::read_to_string(&log)
            .unwrap_or_default()
            .lines()
            .filter(|line| line.starts_with("call "))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let tool = format!("mcp__{TASK_MCP_SERVER}__telos_probe");
    let before = user_mcp_settings();

    // 내장 도구는 모두 끄고(`--tools ""`) 허용 목록도 비웁니다. 주입한 서버의 도구만 쓸 수 있어야 합니다.
    let mut injected = context("env-task-mcp");
    injected.task_mcp = Some(binding);
    let events = run(&adapter(launch("task-mcp", prompt)), &injected).await;
    assert_eq!(check_run_events(&events), Ok(RunTermination::ProcessExited));
    assert_eq!(turn_status(&events), Some(TurnStatus::Completed));
    assert!(payloads(&events).iter().any(|p| matches!(p,
        ProviderEventPayload::ToolCompleted(t) if t.name == tool && t.outcome == ToolOutcome::Succeeded)));
    assert!(text(&events).contains(TASK_MCP_REPLY));
    let recorded = calls();
    println!("task-mcp-calls: {recorded:?}");
    assert_eq!(recorded.len(), 1);
    // 서버 프로세스가 환경 변수로 받은 경로의 자격 파일을 읽을 수 있었습니다.
    assert!(recorded[0].contains("tool=\"telos_probe\""));
    assert!(recorded[0].contains("credential_env=set credential_readable=true"));
    // Run이 끝난 뒤 주입 파일은 없고 자격 파일은 그대로입니다(폐기는 Core의 일).
    let leftovers: Vec<_> = std::fs::read_dir(&state)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("claude-mcp-"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
    assert_eq!(
        std::fs::read_to_string(&credential).unwrap(),
        TASK_MCP_SECRET
    );
    // 자격 원문은 이벤트와 세션 기록에 없습니다.
    let wire = serde_json::to_string(&events).unwrap();
    assert!(!wire.contains(TASK_MCP_SECRET));
    let transcript = std::fs::read_to_string(session_file(&session_id(&events))).unwrap();
    assert!(!transcript.contains(TASK_MCP_SECRET));
    // Workspace에는 MCP 설정을 쓰지 않습니다.
    assert!(!workdir("task-mcp").join(".mcp.json").exists());

    // `taskMcp`가 없는 Run에는 그 서버가 없습니다.
    let plain = run(
        &adapter(launch("task-mcp", prompt)),
        &context("env-task-mcp-none"),
    )
    .await;
    assert_eq!(check_run_events(&plain), Ok(RunTermination::ProcessExited));
    assert!(!payloads(&plain).iter().any(|p| matches!(p,
        ProviderEventPayload::ToolCompleted(t) if t.outcome == ToolOutcome::Succeeded)));
    assert!(text(&plain).contains("NO-TOOL"));
    assert_eq!(calls().len(), 1, "the server was not started again");

    // 사용자 설정의 MCP 관련 내용은 시험 전후가 같고, 주입한 서버 이름이 어디에도 없습니다.
    assert_eq!(user_mcp_settings(), before, "user MCP settings changed");
    assert!(!user_settings_mention(TASK_MCP_SERVER));
    println!(
        "user-mcp-settings: unchanged ({} bytes compared)",
        before.len()
    );
}

/// L1: `--strict-mcp-config`가 있으면 `--mcp-config`의 서버만 연결됩니다. 없으면 사용자 설정의 서버가 함께 연결될 수
/// 있습니다(개수만 출력). 모델을 호출하지 않고 제어 요청 `mcp_status`로 확인합니다.
#[test]
#[ignore = "runs the real Claude Code CLI without a model call"]
fn env_strict_mcp_config_connects_only_the_injected_server() {
    use serde_json::{Value, json};
    use std::io::{BufRead, BufReader, Write};

    let state = state_dir("strict-mcp");
    let server = build_task_mcp_server(&state);
    let config = state.join("mcp.json");
    std::fs::write(
        &config,
        json!({"mcpServers": {TASK_MCP_SERVER: {"type": "stdio", "command": server, "args": []}}})
            .to_string(),
    )
    .unwrap();
    let config = config.display().to_string();
    let before = user_mcp_settings();

    // (주입한 서버의 상태, 그 밖의 서버 수)
    let status = |strict: bool| -> (String, usize) {
        let mut args = vec!["--tools", "", "--mcp-config", config.as_str()];
        if strict {
            args.push("--strict-mcp-config");
        }
        let mut child = raw_command(&workdir("strict-mcp"), &args).spawn().unwrap();
        let mut stdin = child.stdin.take().unwrap();
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
        let mut observed = (String::from("missing"), 0);
        // 서버 연결이 끝날 때까지 몇 번 묻습니다. 사용자 메시지를 보내지 않으므로 Turn은 없습니다.
        for attempt in 0..8 {
            std::thread::sleep(Duration::from_secs(if attempt == 0 { 3 } else { 2 }));
            writeln!(
                stdin,
                "{}",
                json!({"type": "control_request", "request_id": format!("status-{attempt}"),
                    "request": {"subtype": "mcp_status"}})
            )
            .unwrap();
            stdin.flush().unwrap();
            let response = lines
                .by_ref()
                .map(|line| serde_json::from_str::<Value>(&line.unwrap()).unwrap())
                .find(|value| value["type"] == "control_response")
                .expect("mcp_status response");
            let servers = response["response"]["response"]["mcpServers"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let injected = servers.iter().find(|s| s["name"] == TASK_MCP_SERVER);
            observed = (
                injected.map_or("missing".into(), |s| {
                    s["status"].as_str().unwrap_or("?").into()
                }),
                servers.len() - usize::from(injected.is_some()),
            );
            let settled = servers.iter().all(|s| s["status"] != "pending");
            if observed.0 == "connected" && settled {
                break;
            }
        }
        drop(stdin);
        assert!(child.wait().unwrap().success());
        observed
    };

    let strict = status(true);
    let loose = status(false);
    println!(
        "mcp-servers strict: injected={} others={}",
        strict.0, strict.1
    );
    println!(
        "mcp-servers without strict: injected={} others={}",
        loose.0, loose.1
    );
    assert_eq!(strict, ("connected".into(), 0));
    assert_eq!(loose.0, "connected");
    assert_eq!(user_mcp_settings(), before, "user MCP settings changed");
    assert!(!user_settings_mention(TASK_MCP_SERVER));
}

/// P5·SB-11: `capsule` Run은 새 세션입니다. Core가 만든 본문은 전달되고, `sourceSessionId`의 이전 대화는 쓰이지 않습니다.
#[tokio::test]
#[ignore = "runs the real Claude Code CLI"]
async fn env_capsule_is_a_new_session_without_the_previous_conversation() {
    let base = adapter(launch(
        "capsule",
        "Remember this codeword: birch-63. Reply only: ok",
    ));
    let first = run(&base, &context("env-capsule-base")).await;
    let previous = session_id(&first);

    let capsule = adapter(launch(
        "capsule",
        "Context capsule handed over from an earlier session:\n- Goal: answer the question below.\n- Decision: the project color is teal.\n\nQuestion: reply with the project color on the first line. On the second line write the codeword from our earlier conversation if you have one in this conversation; otherwise write NO-CODEWORD.",
    ));
    let mut context = context("env-capsule");
    context.session.mode = SessionMode::Capsule;
    context.session.source_session_id = Some(previous.clone());
    context.session.capsule_version_id = Some("capv-env-1".into());
    let events = run(&capsule, &context).await;
    assert_eq!(check_run_events(&events), Ok(RunTermination::ProcessExited));
    assert_eq!(turn_status(&events), Some(TurnStatus::Completed));
    let answer = text(&events);
    println!("capsule: previous={previous} new={}", session_id(&events));
    assert_ne!(
        session_id(&events),
        previous,
        "capsule must not reuse the session"
    );
    assert!(
        answer.to_lowercase().contains("teal"),
        "capsule body was delivered"
    );
    assert!(answer.contains("NO-CODEWORD") && !answer.contains("birch-63"));
}

/// 실시간 승인 Run을 시작하고 승인 요청 이벤트까지 받습니다.
async fn start_until_approval(
    adapter: &ClaudeCodeAdapter,
    context: &RunContext,
) -> ApprovalRequiredPayload {
    adapter
        .start_run(&fixtures::task_spec(), context)
        .await
        .unwrap();
    let mut stream = adapter.events(&context.run_id, 0);
    tokio::time::timeout(Duration::from_secs(120), async {
        while let Some(event) = stream.next().await {
            let event = event.unwrap();
            if let Ok(ProviderEventPayload::ApprovalRequired(request)) =
                ProviderEventPayload::parse(event.kind, &event.payload)
            {
                return request;
            }
        }
        panic!("run ended without an approval request");
    })
    .await
    .expect("approval request arrives")
}

async fn finish(adapter: &ClaudeCodeAdapter, run_id: &str) -> Vec<ProviderEvent> {
    let events: Vec<_> = tokio::time::timeout(
        Duration::from_secs(180),
        adapter.events(run_id, 0).map(Result::unwrap).collect(),
    )
    .await
    .expect("run finishes");
    report(&events);
    events
}

fn approval_launch(name: &str) -> (ClaudeLaunch, PathBuf) {
    let mut launch = launch(
        name,
        "Use the Write tool to create probe.txt containing OK. Do nothing else.",
    );
    launch.tools = Some(vec!["Write".into()]);
    let target = launch.cwd.join("probe.txt");
    let _ = std::fs::remove_file(&target);
    (launch, target)
}

/// L2·P6·SB-07: Adapter 경로의 실시간 승인. 승인 요청이 대상·인자 Digest와 함께 이벤트로 나오고,
/// Allow는 도구를 실행시키고 Deny는 막습니다. 모르는 요청 ID와 중복 응답도 확인합니다.
#[tokio::test]
#[ignore = "runs the real Claude Code CLI"]
async fn env_live_approval_allow_and_deny() {
    let (launch, target) = approval_launch("live-approval-allow");
    let allow = adapter(launch).with_approvals(ClaudeApprovals::Prompt);
    let allow_run = context("env-live-approval-allow");
    let request = start_until_approval(&allow, &allow_run).await;
    println!(
        "approval-request: {}",
        serde_json::to_string(&request).unwrap()
    );
    assert_eq!(request.action, "file_change");
    assert!(request.answerable);
    assert!(
        request
            .target
            .as_deref()
            .is_some_and(|t| t.ends_with("probe.txt"))
    );
    let digest = request.args_digest.clone().expect("argsDigest");
    assert!(digest.starts_with("sha256:") && digest.len() == 7 + 64);
    assert!(!target.exists(), "nothing runs before the answer");
    assert!(matches!(
        allow
            .answer_approval(&allow_run.run_id, "no-such-request", ApprovalAnswer::Allow)
            .await,
        Err(AdapterError::Failure {
            kind: FailureKind::Configuration,
            ..
        })
    ));
    allow
        .answer_approval(
            &allow_run.run_id,
            &request.request_id,
            ApprovalAnswer::Allow,
        )
        .await
        .unwrap();
    assert_eq!(
        allow
            .answer_approval(
                &allow_run.run_id,
                &request.request_id,
                ApprovalAnswer::Allow
            )
            .await,
        Ok(())
    );
    assert!(matches!(
        allow
            .answer_approval(&allow_run.run_id, &request.request_id, ApprovalAnswer::Deny)
            .await,
        Err(AdapterError::IdempotencyConflict { .. })
    ));
    let events = finish(&allow, &allow_run.run_id).await;
    assert_eq!(check_run_events(&events), Ok(RunTermination::ProcessExited));
    assert_eq!(turn_status(&events), Some(TurnStatus::Completed));
    assert!(payloads(&events).iter().any(|p| matches!(p,
        ProviderEventPayload::ToolCompleted(t) if t.name == "Write" && t.outcome == ToolOutcome::Succeeded)));
    assert_eq!(std::fs::read_to_string(&target).unwrap().trim(), "OK");
    let exit = exit(&events);
    assert_eq!(
        (exit.exit_code, exit.termination),
        (Some(0), ProcessTermination::Exited),
        "the CLI ends by itself once stdin is closed after the result"
    );

    let (launch, target) = approval_launch("live-approval-deny");
    let adapter = adapter(launch).with_approvals(ClaudeApprovals::Prompt);
    let context = context("env-live-approval-deny");
    let request = start_until_approval(&adapter, &context).await;
    adapter
        .answer_approval(&context.run_id, &request.request_id, ApprovalAnswer::Deny)
        .await
        .unwrap();
    let events = finish(&adapter, &context.run_id).await;
    assert_eq!(check_run_events(&events), Ok(RunTermination::ProcessExited));
    assert!(payloads(&events).iter().any(|p| matches!(p,
        ProviderEventPayload::ToolCompleted(t) if t.name == "Write" && t.outcome == ToolOutcome::Denied)));
    assert!(!target.exists(), "denied write must not create the file");
    assert_eq!(turn_status(&events), Some(TurnStatus::Completed));
}

/// L2: 응답 전에 Run을 취소하면 도구는 실행되지 않고 프로세스가 끝나며, 그 뒤의 응답은 거부됩니다.
#[tokio::test]
#[ignore = "runs the real Claude Code CLI"]
async fn env_live_approval_cancel_before_answer() {
    let (launch, target) = approval_launch("live-approval-cancel");
    let adapter = adapter(launch).with_approvals(ClaudeApprovals::Prompt);
    let context = context("env-live-approval-cancel");
    let request = start_until_approval(&adapter, &context).await;
    let requested = Instant::now();
    assert_eq!(adapter.cancel_run(&context.run_id).await, Ok(true));
    let events = finish(&adapter, &context.run_id).await;
    println!("cancel-to-exit: {:?}", requested.elapsed());
    assert_eq!(check_run_events(&events), Ok(RunTermination::ProcessExited));
    assert!(!target.exists(), "the pending tool call never ran");
    assert!(!payloads(&events).iter().any(|p| matches!(p,
        ProviderEventPayload::ToolCompleted(t) if t.outcome == ToolOutcome::Succeeded)));
    // `interrupt`로 Turn을 끊으므로 모델을 다시 부르지 않고 취소로 끝납니다.
    assert_eq!(turn_status(&events), Some(TurnStatus::Cancelled));
    assert_eq!(exit(&events).children_reaped, Some(true));
    assert!(matches!(
        adapter
            .answer_approval(&context.run_id, &request.request_id, ApprovalAnswer::Allow)
            .await,
        Err(AdapterError::Failure {
            kind: FailureKind::Configuration,
            ..
        })
    ));
}

/// 실행 중에 보낸 두 번째 메시지가 어떻게 처리됐는지입니다.
#[derive(Debug)]
struct MidTurnMessage {
    session_id: String,
    /// `result` 수 = Turn 수
    turns: usize,
    /// 두 번째 메시지 처리 시작(`command_lifecycle` `started`)이 첫 `result`보다 먼저였는지
    joined_first_turn: bool,
}

/// CLI를 직접 실행해 첫 메시지의 Turn이 진행되는 동안(`trigger`가 참인 줄을 본 직후) 두 번째 메시지를 보냅니다.
fn probe_mid_turn_message(
    name: &str,
    extra: &[&str],
    first: &str,
    trigger: fn(&serde_json::Value) -> bool,
) -> MidTurnMessage {
    use serde_json::{Value, json};
    use std::io::{BufRead, BufReader, Write};

    const SECOND: &str = "11111111-2222-4333-8444-555555555555";
    let mut args = vec!["--replay-user-messages"];
    args.extend(extra);
    let mut child = raw_command(&workdir(name), &args).spawn().unwrap();
    let mut stdin = child.stdin.take();
    fn send(stdin: &mut Option<std::process::ChildStdin>, value: Value) {
        let pipe = stdin.as_mut().unwrap();
        writeln!(pipe, "{value}").unwrap();
        pipe.flush().unwrap();
    }
    send(
        &mut stdin,
        json!({"type": "user", "message": {"role": "user", "content": first}}),
    );
    let stdout = child.stdout.take().unwrap();
    let child = Arc::new(std::sync::Mutex::new(child));
    let (done, finished) = std::sync::mpsc::channel::<()>();
    let killer = {
        let child = child.clone();
        std::thread::spawn(move || {
            if finished.recv_timeout(Duration::from_secs(150)).is_err() {
                let _ = child.lock().unwrap().kill();
            }
        })
    };
    let mut observed = MidTurnMessage {
        session_id: String::new(),
        turns: 0,
        joined_first_turn: false,
    };
    let mut sent = false;
    for line in BufReader::new(stdout).lines() {
        let event: Value = serde_json::from_str(&line.unwrap()).unwrap();
        if let Some(id) = event["session_id"].as_str() {
            observed.session_id = id.to_owned();
        }
        if !sent && trigger(&event) {
            sent = true;
            send(
                &mut stdin,
                json!({"type": "user", "uuid": SECOND, "message": {"role": "user",
                    "content": "Additional instruction for the task in progress: end your final reply with the token ZETA-9."}}),
            );
        }
        if event["type"] == "command_lifecycle"
            && event["command_uuid"] == SECOND
            && event["state"] == "started"
        {
            observed.joined_first_turn = observed.turns == 0;
        }
        if event["type"] == "result" {
            observed.turns += 1;
            // 입력을 닫습니다. 이미 대기열에 있는 메시지가 새 Turn으로 실행되면 그 `result`까지 읽습니다.
            stdin = None;
        }
    }
    let _ = done.send(());
    killer.join().unwrap();
    let _ = child.lock().unwrap().wait();
    assert!(sent, "the second message was sent during the first turn");
    println!("claude-session: {}", observed.session_id);
    println!("{name}: {observed:?}");
    observed
}

/// L3·SB-06: 진행 중인 Turn에 보낸 메시지는 뒤에 도구 호출 경계가 남아 있을 때만 같은 Turn에 들어가고,
/// 아니면 CLI가 새 Turn을 시작합니다. 보내는 쪽이 정할 수 없으므로 `send_input`은 미지원으로 둡니다.
/// 이 판정을 다시 확인할 때 실행합니다(모델의 도구 사용 순서에 기대는 관찰 시험).
#[test]
#[ignore = "runs the real Claude Code CLI"]
fn env_mid_turn_message_joins_the_turn_only_at_a_tool_boundary() {
    let with_tools = probe_mid_turn_message(
        "mid-turn-tools",
        &["--tools", "Bash", "--allowedTools", "Bash(sleep:*)"],
        "Run the Bash command `sleep 3` two times, each as a separate tool call, one after another (not in parallel). After both complete, reply with exactly: DONE",
        |event| {
            event["type"] == "assistant"
                && event["message"]["content"]
                    .as_array()
                    .is_some_and(|content| content.iter().any(|c| c["type"] == "tool_use"))
        },
    );
    assert!(with_tools.joined_first_turn && with_tools.turns == 1);

    let without_tools = probe_mid_turn_message(
        "mid-turn-text",
        &["--tools", ""],
        "Write the numbers from 1 to 120, one per line, with no other text.",
        |event| event["type"] == "system" && event["subtype"] == "init",
    );
    assert!(!without_tools.joined_first_turn && without_tools.turns == 2);
}

/// `send_input`은 실환경에서도 프로세스를 건드리지 않고 거부됩니다 (SB-06).
#[tokio::test]
#[ignore = "runs the real Claude Code CLI"]
async fn env_send_input_is_rejected_while_the_run_continues() {
    let adapter = adapter(launch("send-input", "Reply with exactly: pong"));
    let context = context("env-send-input");
    adapter
        .start_run(&fixtures::task_spec(), &context)
        .await
        .unwrap();
    assert!(matches!(
        adapter.send_input(&context.run_id, "input-1", "also say ping").await,
        Err(AdapterError::UnsupportedCapability { capability, .. }) if capability == keys::SEND_INPUT
    ));
    let events = finish(&adapter, &context.run_id).await;
    assert_eq!(check_run_events(&events), Ok(RunTermination::ProcessExited));
    assert_eq!(turn_status(&events), Some(TurnStatus::Completed));
    assert!(!text(&events).to_lowercase().contains("ping"));
}

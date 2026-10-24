//! Claude Code 실환경 시험 (PH0-CLAUDE, PROV-01~08·PROV-05·FORK-POC-01).
//!
//! 실제 CLI를 실행하고 사용자 계정의 사용량을 쓰므로 기본 실행에서 제외합니다(`#[ignore]`).
//! 실행 조건(배정표 §6): 모델 `haiku`, 기본 설정 디렉터리, 저장소 밖 일회용 폴더, 허용 도구 명시.
//!
//! ```text
//! TELOS_CLAUDE_CLI=<확장의 resources/native-binary/claude.exe> \
//!   cargo test -p telos-adapters --test claude_environment -- --ignored --test-threads=1 --nocapture
//! ```
//!
//! 각 시험은 만든 Native Session ID를 `claude-session:` 줄로 출력합니다. 시험 뒤 Archive 대상입니다.

use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use futures::StreamExt;
use telos_adapters::{
    AdapterError, AgentAdapter, ClaudeCodeAdapter,
    claude::{ClaudeCodeConfig, ClaudeLaunch, ClaudeLaunchResolver},
    runtime::NativeRuntime,
};
use telos_schema::{
    FailureKind, ProcessTermination, ProviderEvent, ProviderEventKind, ProviderEventPayload,
    RunContext, RunTermination, SessionMode, TaskSpec, ToolOutcome, TurnStatus, check_run_events,
    fixtures,
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

fn workdir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("telos-claude-env").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    dir
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
        cli_version: "2.1.310 (Claude Code)".into(),
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

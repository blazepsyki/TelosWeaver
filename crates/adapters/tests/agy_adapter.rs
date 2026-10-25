//! Antigravity (`agy`) Adapter의 Mock·Contract 시험 (PH0-AGY, PROV-01·03·05·06·07·08·09, FORK-POC-01, AGY-DATA-01~05, DEC-08, AGENT-05).
//!
//! 실제 CLI 출력(`fixtures/agy/*.jsonl`, `agy` 1.4.7)을 가짜 프로세스 런타임으로 재생합니다.
//! 실제 CLI·프로세스 시험은 `agy_environment.rs`에 있습니다.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use futures::{StreamExt, stream::BoxStream};
use telos_adapters::{
    AdapterError, AgentAdapter, AgyAdapter, CliBinaryRule,
    agy::{AgyDataDirMode, AgyVsCodeListing},
    runtime::*,
};
use telos_schema::{
    ApprovalAnswer, CapabilityState, EvidenceLevel, FailureKind, ProviderEvent, ProviderEventKind,
    ProviderEventPayload, RunInspection, RunTermination, SessionMode, ToolOutcome, TurnStatus,
    check_run_events, fixtures,
};
use tokio::sync::watch;

// --- 가짜 런타임 ---

#[derive(Clone, Default)]
struct Script {
    lines: Vec<Vec<u8>>,
    stderr_lines: Vec<Vec<u8>>,
    /// 종료 신호(`terminate`) 수신 직후 닫히기 전에 내보낼 stdout 줄(Linux SIGTERM 직전 `result(ERROR)` 재현용).
    lines_on_exit: Vec<Vec<u8>>,
    exit_code: Option<i32>,
    /// 출력 뒤 종료 요청까지 멈춥니다(취소 시험).
    hang: bool,
}

impl Script {
    fn fixture(name: &str, exit_code: i32) -> Self {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/agy")
            .join(name);
        let text = std::fs::read_to_string(path).unwrap();
        Self {
            lines: text.lines().map(|l| l.as_bytes().to_vec()).collect(),
            stderr_lines: Vec::new(),
            lines_on_exit: Vec::new(),
            exit_code: Some(exit_code),
            hang: false,
        }
    }
}

#[derive(Default)]
struct FakeRuntime {
    script: Mutex<Script>,
    spawned: Mutex<Vec<CommandSpec>>,
    stdin: Arc<Mutex<Vec<u8>>>,
}

impl FakeRuntime {
    fn new(script: Script) -> Arc<Self> {
        Arc::new(Self {
            script: Mutex::new(script),
            ..Self::default()
        })
    }
}

struct FakeHandle {
    identity: ProcessIdentity,
    output: Mutex<Option<BoxStream<'static, OutputEvent>>>,
    exit: watch::Sender<Option<ExitStatus>>,
    stdin: Arc<Mutex<Vec<u8>>>,
}

#[async_trait]
impl ProcessRuntime for FakeRuntime {
    async fn spawn(&self, spec: CommandSpec) -> Result<Arc<dyn ProcessHandle>, RuntimeError> {
        self.spawned.lock().unwrap().push(spec);
        let script = self.script.lock().unwrap().clone();
        let natural = (!script.hang).then_some(ExitStatus {
            code: script.exit_code,
            cause: TerminationCause::Exited,
        });
        let (exit, mut exited) = watch::channel(natural);
        let stdout_events =
            script
                .lines
                .into_iter()
                .enumerate()
                .map(|(i, data)| OutputEvent::Line {
                    stream: OutputStream::Stdout,
                    sequence: i as u64 + 1,
                    data,
                });
        let stderr_events = script
            .stderr_lines
            .into_iter()
            .enumerate()
            .map(|(i, data)| OutputEvent::Line {
                stream: OutputStream::Stderr,
                sequence: i as u64 + 500,
                data,
            });
        let exit_lines = script.lines_on_exit;
        let tail = futures::stream::once(async move {
            let _ = exited.wait_for(Option::is_some).await;
            exit_lines
        })
        .flat_map(|exit_lines| {
            let trailing = exit_lines
                .into_iter()
                .enumerate()
                .map(|(i, data)| OutputEvent::Line {
                    stream: OutputStream::Stdout,
                    sequence: i as u64 + 800,
                    data,
                });
            futures::stream::iter(trailing.chain([
                OutputEvent::Closed {
                    stream: OutputStream::Stdout,
                    sequence: 1000,
                },
                OutputEvent::Closed {
                    stream: OutputStream::Stderr,
                    sequence: 1001,
                },
            ]))
        });
        Ok(Arc::new(FakeHandle {
            identity: ProcessIdentity {
                pid: 1,
                started_at: "2026-10-24T00:00:00Z".into(),
            },
            output: Mutex::new(Some(
                futures::stream::iter(stdout_events.chain(stderr_events))
                    .chain(tail)
                    .boxed(),
            )),
            exit,
            stdin: self.stdin.clone(),
        }))
    }
}

#[async_trait]
impl ProcessHandle for FakeHandle {
    fn identity(&self) -> &ProcessIdentity {
        &self.identity
    }
    fn take_output(&self) -> Result<BoxStream<'static, OutputEvent>, RuntimeError> {
        self.output
            .lock()
            .unwrap()
            .take()
            .ok_or(RuntimeError::OutputAlreadyTaken)
    }
    async fn write_stdin(&self, data: &[u8]) -> Result<(), RuntimeError> {
        self.stdin.lock().unwrap().extend_from_slice(data);
        Ok(())
    }
    async fn close_stdin(&self) -> Result<(), RuntimeError> {
        Ok(())
    }
    async fn wait(&self) -> Result<ExitStatus, RuntimeError> {
        let mut rx = self.exit.subscribe();
        let status = rx.wait_for(Option::is_some).await.unwrap();
        Ok((*status).unwrap())
    }
    async fn terminate(&self, _grace: Duration) -> Result<TerminationReport, RuntimeError> {
        self.exit.send_if_modified(|exit| {
            if exit.is_none() {
                *exit = Some(ExitStatus {
                    code: None,
                    cause: TerminationCause::TerminatedOnRequest,
                });
                true
            } else {
                false
            }
        });
        Ok(TerminationReport {
            status: self.exit.borrow().unwrap(),
            children: ChildrenReaped::All,
        })
    }
}

// --- 대상 구성 ---

fn adapter_with_version(runtime: Arc<FakeRuntime>, version: &str) -> AgyAdapter {
    let context = fixtures::run_context(&fixtures::task_spec());
    AgyAdapter::configured(
        PathBuf::from("C:/Users/user/.gemini/bin/agy.exe"),
        version.into(),
        context.host_id,
        runtime,
        Duration::from_millis(10),
        PathBuf::from("C:/work"),
        "Reply with exactly: pong".into(),
        Some("gemini-3.8-flash-low".into()),
        Some("low".into()),
        Some("plan".into()),
    )
}

fn adapter(runtime: Arc<FakeRuntime>) -> AgyAdapter {
    adapter_with_version(runtime, "1.4.7")
}

async fn run_to_end(adapter: &AgyAdapter) -> Vec<ProviderEvent> {
    let task = fixtures::task_spec();
    let context = fixtures::run_context(&task);
    adapter.start_run(&task, &context).await.unwrap();
    collect(adapter, &context.run_id, 0).await
}

async fn collect(adapter: &AgyAdapter, run_id: &str, after: u64) -> Vec<ProviderEvent> {
    tokio::time::timeout(
        Duration::from_secs(5),
        adapter.events(run_id, after).map(Result::unwrap).collect(),
    )
    .await
    .expect("event stream ends")
}

fn payloads(events: &[ProviderEvent]) -> Vec<ProviderEventPayload> {
    events
        .iter()
        .map(|e| ProviderEventPayload::parse(e.kind, &e.payload).unwrap())
        .collect()
}

fn kinds(events: &[ProviderEvent]) -> Vec<ProviderEventKind> {
    events.iter().map(|e| e.kind).collect()
}

// --- 시험 ---

#[tokio::test]
async fn completed_run_maps_stream_and_ends_with_process_exit() {
    let runtime = FakeRuntime::new(Script::fixture("success.jsonl", 0));
    let adapter = adapter(runtime.clone());
    let events = run_to_end(&adapter).await;

    assert_eq!(check_run_events(&events), Ok(RunTermination::ProcessExited));
    use ProviderEventKind::*;
    assert_eq!(
        kinds(&events),
        [
            SessionStarted,
            TextDelta,
            TextDelta,
            Usage,
            TurnFinished,
            ProcessExited
        ]
    );
    let p = payloads(&events);
    let ProviderEventPayload::SessionStarted(started) = &p[0] else {
        panic!()
    };
    assert_eq!(started.provider_version.as_deref(), Some("1.4.7"));
    assert_eq!(started.model.as_deref(), Some("gemini-3.8-flash-low"));
    let ProviderEventPayload::TextDelta(text) = &p[1] else {
        panic!()
    };
    assert_eq!(text.text, "pong");
    let ProviderEventPayload::Usage(usage) = &p[3] else {
        panic!()
    };
    assert_eq!(usage.input_tokens, Some(8102));
    assert_eq!(usage.output_tokens, Some(3));
    assert_eq!(usage.cache_included_in_input, Some(false));
    assert_eq!(usage.cost_usd, None);
    let ProviderEventPayload::TurnFinished(turn) = &p[4] else {
        panic!()
    };
    assert_eq!(turn.status, TurnStatus::Completed);
    let ProviderEventPayload::ProcessExited(exit) = &p[5] else {
        panic!()
    };
    assert_eq!(exit.exit_code, Some(0));
    assert_eq!(exit.children_reaped, Some(true));
    assert_eq!(
        events[0].native_session_id.as_deref(),
        Some("72037980-2e05-44b8-bfd5-17d21149859e")
    );
    assert_eq!(
        adapter.inspect_run("run-1").await,
        Ok(RunInspection::Finished)
    );

    let spec = &runtime.spawned.lock().unwrap()[0];
    let args: Vec<_> = spec.args.iter().map(|a| a.to_str().unwrap()).collect();
    for expected in [
        "--app_data_dir=antigravity",
        "--input-format",
        "stream-json",
        "--output-format",
        "gemini-3.8-flash-low",
        "low",
        "plan",
    ] {
        assert!(args.contains(&expected), "missing {expected}: {args:?}");
    }
    assert!(!args.iter().any(|a| a.contains("dangerously")));
    assert_eq!(spec.stdin, StdinMode::Piped);
    let stdin = String::from_utf8(runtime.stdin.lock().unwrap().clone()).unwrap();
    let message: serde_json::Value = serde_json::from_str(stdin.trim_end()).unwrap();
    assert_eq!(message["event"], "user");
    assert_eq!(message["message"]["content"], "Reply with exactly: pong");
}

#[tokio::test]
async fn permission_denial_and_soft_denied_actions_are_reported() {
    let adapter = adapter(FakeRuntime::new(Script::fixture(
        "permission-denied.jsonl",
        0,
    )));
    let events = run_to_end(&adapter).await;
    assert_eq!(check_run_events(&events), Ok(RunTermination::ProcessExited));
    let p = payloads(&events);

    let completed: Vec<_> = p
        .iter()
        .filter_map(|item| match item {
            ProviderEventPayload::ToolCompleted(t) => Some(t),
            _ => None,
        })
        .collect();
    assert_eq!(completed.len(), 2);
    assert_eq!(completed[0].name, "read_file");
    assert_eq!(completed[0].outcome, ToolOutcome::Succeeded);
    assert_eq!(completed[1].name, "run_command");
    assert_eq!(completed[1].outcome, ToolOutcome::Denied);

    // Spec §7.3·§7.4: 종료 코드 0, status "SUCCESS"여도 denied_actions로 응답 없이 끝났으면 ToolDenied 실패입니다.
    assert!(p.iter().any(
        |item| matches!(item, ProviderEventPayload::Error(e) if e.kind == FailureKind::ToolDenied)
    ));
    assert!(p.iter().any(|item| {
        matches!(item, ProviderEventPayload::TurnFinished(t)
            if t.status == TurnStatus::Failed && t.reason.as_deref() == Some("tool_denied"))
    }));
}

#[tokio::test]
async fn provider_error_is_classified_and_turn_fails() {
    let model_adapter = adapter(FakeRuntime::new(Script::fixture(
        "model-not-found.jsonl",
        1,
    )));
    let events = run_to_end(&model_adapter).await;
    assert_eq!(check_run_events(&events), Ok(RunTermination::ProcessExited));
    let p = payloads(&events);
    let error = p.iter().find_map(|item| match item {
        ProviderEventPayload::Error(e) => Some(e),
        _ => None,
    });
    assert_eq!(error.unwrap().kind, FailureKind::Configuration);
    assert!(
        !p.iter()
            .any(|item| matches!(item, ProviderEventPayload::Usage(_)))
    );
    assert!(p.iter().any(
        |item| matches!(item, ProviderEventPayload::TurnFinished(t) if t.status == TurnStatus::Failed)
    ));
    let ProviderEventPayload::ProcessExited(exit) = p.last().unwrap() else {
        panic!()
    };
    assert_eq!(exit.exit_code, Some(1));

    // Linux/SSH Headless 미로그인 시 반환되는 "authentication failed or timed out"은
    // Timeout이 아니라 Authentication으로 분류되어야 합니다 (Spec §7.3).
    let auth_script = Script {
        lines: vec![
            br#"{"event":"result","result":{"conversation_id":"","status":"ERROR","response":"","error":"authentication failed or timed out","duration_seconds":0,"num_turns":0,"usage":{"input_tokens":0,"output_tokens":0,"thinking_tokens":0,"cache_read_tokens":0,"total_tokens":0}}}"#.to_vec(),
        ],
        stderr_lines: vec![b"Error: authentication required. Run 'agy' to log in, then retry.".to_vec()],
        lines_on_exit: vec![],
        exit_code: Some(1),
        hang: false,
    };
    let auth_adapter = adapter(FakeRuntime::new(auth_script));
    let auth_events = run_to_end(&auth_adapter).await;
    assert_eq!(
        check_run_events(&auth_events),
        Ok(RunTermination::ProcessExited)
    );
    let auth_payloads = payloads(&auth_events);
    assert!(auth_payloads.iter().any(
        |item| matches!(item, ProviderEventPayload::Error(e) if e.kind == FailureKind::Authentication)
    ));
}

#[tokio::test]
async fn invalid_stream_line_is_a_protocol_error_not_a_message() {
    let mut script = Script::fixture("success.jsonl", 0);
    script.lines.insert(1, b"{not json".to_vec());
    let adapter = adapter(FakeRuntime::new(script));
    let events = run_to_end(&adapter).await;
    assert_eq!(check_run_events(&events), Ok(RunTermination::ProcessExited));
    let p = payloads(&events);
    assert!(
        matches!(&p[1], ProviderEventPayload::Error(e) if e.kind == FailureKind::ProtocolParse)
    );
    assert!(
        !p.iter()
            .any(|item| matches!(item, ProviderEventPayload::TextDelta(_)))
    );
    assert!(p.iter().any(
        |item| matches!(item, ProviderEventPayload::TurnFinished(t) if t.status == TurnStatus::Failed)
    ));
}

#[tokio::test]
async fn exit_without_result_is_a_failed_turn_and_uses_stderr_hint() {
    let mut script = Script::fixture("success.jsonl", 3);
    script.lines.truncate(1);
    script
        .stderr_lines
        .push(b"Authentication required. Please visit the URL to log in:".to_vec());
    let adapter = adapter(FakeRuntime::new(script));
    let events = run_to_end(&adapter).await;
    assert_eq!(check_run_events(&events), Ok(RunTermination::ProcessExited));
    let p = payloads(&events);
    assert!(
        matches!(&p[1], ProviderEventPayload::Error(e) if e.kind == FailureKind::Authentication)
    );
    assert!(
        matches!(&p[2], ProviderEventPayload::TurnFinished(t) if t.status == TurnStatus::Failed)
    );
}

#[tokio::test]
async fn cancel_terminates_the_process_and_reports_the_actual_exit() {
    let mut script = Script::fixture("success.jsonl", 0);
    script.lines.truncate(1);
    script.hang = true;
    let adapter = adapter(FakeRuntime::new(script));
    let task = fixtures::task_spec();
    let context = fixtures::run_context(&task);
    adapter.start_run(&task, &context).await.unwrap();

    let mut stream = adapter.events("run-1", 0);
    assert_eq!(
        stream.next().await.unwrap().unwrap().kind,
        ProviderEventKind::SessionStarted
    );
    assert_eq!(
        adapter.inspect_run("run-1").await,
        Ok(RunInspection::Running)
    );
    assert_eq!(adapter.cancel_run("run-1").await, Ok(true));

    let events = collect(&adapter, "run-1", 0).await;
    assert_eq!(check_run_events(&events), Ok(RunTermination::ProcessExited));
    let p = payloads(&events);
    assert!(
        matches!(&p[1], ProviderEventPayload::TurnFinished(t) if t.status == TurnStatus::Cancelled)
    );
    assert!(matches!(&p[2], ProviderEventPayload::ProcessExited(e)
        if e.termination == telos_schema::ProcessTermination::TerminatedOnRequest));
    assert_eq!(adapter.cancel_run("run-1").await, Ok(false));
}

/// PROV-08 (INTEGRATION §9.3 후속): Linux에서 SIGTERM 수신 시 내부 서버가 먼저 닫히며 종료 직전
/// `result(status: "ERROR")` 줄을 출력하더라도, 이미 취소가 요청된 Run이면 `Error` 이벤트를 억제하고
/// `TurnFinished(Cancelled)`로 정규화하며 종료 경로에서 중복 `TurnFinished`를 내지 않습니다.
#[tokio::test]
async fn cancel_normalizes_trailing_error_result_to_cancelled() {
    let mut script = Script::fixture("success.jsonl", 0);
    script.lines.truncate(1);
    script.lines_on_exit = vec![
        br#"{"event":"result","result":{"conversation_id":"72037980-2e05-44b8-bfd5-17d21149859e","status":"ERROR","response":"","error":"failed to send message: trajectory not found","duration_seconds":0.2,"num_turns":1,"usage":{"input_tokens":0,"output_tokens":0,"thinking_tokens":0,"cache_read_tokens":0,"total_tokens":0}}}"#.to_vec(),
    ];
    script.hang = true;
    let adapter = adapter(FakeRuntime::new(script));
    let task = fixtures::task_spec();
    let context = fixtures::run_context(&task);
    adapter.start_run(&task, &context).await.unwrap();

    let mut stream = adapter.events("run-1", 0);
    assert_eq!(
        stream.next().await.unwrap().unwrap().kind,
        ProviderEventKind::SessionStarted
    );
    assert_eq!(adapter.cancel_run("run-1").await, Ok(true));

    let events = collect(&adapter, "run-1", 0).await;
    assert_eq!(check_run_events(&events), Ok(RunTermination::ProcessExited));
    assert_eq!(
        kinds(&events),
        [
            ProviderEventKind::SessionStarted,
            ProviderEventKind::Usage,
            ProviderEventKind::TurnFinished,
            ProviderEventKind::ProcessExited,
        ]
    );
    let p = payloads(&events);
    assert!(
        !p.iter()
            .any(|item| matches!(item, ProviderEventPayload::Error(_))),
        "trailing result(ERROR) after cancel must not emit Error event"
    );
    assert!(matches!(
        &p[2],
        ProviderEventPayload::TurnFinished(t)
            if t.status == TurnStatus::Cancelled && t.reason.as_deref() == Some("cancelled")
    ));
    assert!(matches!(
        &p[3],
        ProviderEventPayload::ProcessExited(e)
            if e.termination == telos_schema::ProcessTermination::TerminatedOnRequest
    ));
}

#[tokio::test]
async fn idempotent_start_reuses_the_run_and_rejects_key_reuse() {
    let runtime = FakeRuntime::new(Script::fixture("success.jsonl", 0));
    let adapter = adapter(runtime.clone());
    let task = fixtures::task_spec();
    let context = fixtures::run_context(&task);
    let first = adapter.start_run(&task, &context).await.unwrap();
    let again = adapter.start_run(&task, &context).await.unwrap();
    assert_eq!(first, again);
    assert_eq!(runtime.spawned.lock().unwrap().len(), 1);

    let mut other = context.clone();
    other.run_id = "run-2".into();
    assert!(matches!(
        adapter.start_run(&task, &other).await,
        Err(AdapterError::IdempotencyConflict { .. })
    ));
}

#[tokio::test]
async fn replay_after_sequence_returns_only_later_events() {
    let adapter = adapter(FakeRuntime::new(Script::fixture("success.jsonl", 0)));
    let all = run_to_end(&adapter).await;
    let tail = collect(&adapter, "run-1", 3).await;
    assert_eq!(tail, all[3..]);
}

#[tokio::test]
async fn unsupported_controls_and_unknown_runs_are_explicit() {
    let adapter = adapter(FakeRuntime::new(Script::fixture("success.jsonl", 0)));
    assert!(matches!(
        adapter.send_input("run-1", "in-1", "more").await,
        Err(AdapterError::UnsupportedCapability { .. })
    ));
    assert!(matches!(
        adapter
            .answer_approval("run-1", "req-1", ApprovalAnswer::Allow)
            .await,
        Err(AdapterError::UnsupportedCapability { .. })
    ));
    let first = adapter.events("missing", 0).next().await.unwrap();
    assert!(matches!(first, Err(AdapterError::UnknownRun { .. })));

    let undiscovered = AgyAdapter::default();
    let task = fixtures::task_spec();
    assert!(matches!(
        undiscovered
            .start_run(&task, &fixtures::run_context(&task))
            .await,
        Err(AdapterError::UnsupportedCapability { .. })
    ));
    assert!(
        undiscovered
            .detect("windows-local")
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn dangerous_permission_flags_and_invalid_modes_are_rejected() {
    let runtime = FakeRuntime::new(Script::fixture("success.jsonl", 0));
    let task = fixtures::task_spec();
    let context = fixtures::run_context(&task);

    for bad_mode in [
        "dangerously-skip-permissions",
        "bypassPermissions",
        "unknown-mode",
    ] {
        let bad_adapter = AgyAdapter::configured(
            PathBuf::from("C:/Users/user/.gemini/bin/agy.exe"),
            "1.4.7".into(),
            context.host_id.clone(),
            runtime.clone(),
            Duration::from_millis(10),
            PathBuf::from("C:/work"),
            "hi".into(),
            None,
            None,
            Some(bad_mode.into()),
        );
        assert!(matches!(
            bad_adapter.start_run(&task, &context).await,
            Err(AdapterError::Failure {
                kind: FailureKind::Configuration,
                ..
            })
        ));
    }

    for bad_extra in ["--dangerously-skip-permissions", "--hub", "-i"] {
        let bad_adapter = AgyAdapter::with_disable_slash_commands(
            PathBuf::from("C:/Users/user/.gemini/bin/agy.exe"),
            "1.4.7".into(),
            context.host_id.clone(),
            runtime.clone(),
            Duration::from_millis(10),
            PathBuf::from("C:/work"),
            "hi".into(),
            None,
            None,
            None,
            true,
            vec![],
            vec![bad_extra.into()],
        );
        assert!(matches!(
            bad_adapter.start_run(&task, &context).await,
            Err(AdapterError::Failure {
                kind: FailureKind::Configuration,
                ..
            })
        ));
    }
    assert!(runtime.spawned.lock().unwrap().is_empty());
}

#[tokio::test]
async fn resume_builds_conversation_flag_and_native_fork_is_unsupported() {
    let runtime = FakeRuntime::new(Script::fixture("success.jsonl", 0));
    let adapter = adapter(runtime.clone());
    let task = fixtures::task_spec();

    let mut resume = fixtures::run_context(&task);
    resume.session.mode = SessionMode::Resume;
    resume.session.source_session_id = Some("session-parent".into());
    adapter.start_run(&task, &resume).await.unwrap();
    let _ = collect(&adapter, &resume.run_id, 0).await;

    let mut fork = fixtures::run_context(&task);
    fork.run_id = "run-fork".into();
    fork.idempotency_key = "run-fork:start".into();
    fork.session.mode = SessionMode::NativeFork;
    fork.session.source_session_id = Some("session-parent".into());
    assert!(matches!(
        adapter.start_run(&task, &fork).await,
        Err(AdapterError::UnsupportedCapability {
            ref capability,
            ..
        }) if capability == "native_fork"
    ));

    let mut missing = fixtures::run_context(&task);
    missing.run_id = "run-missing".into();
    missing.idempotency_key = "run-missing:start".into();
    missing.session.mode = SessionMode::Resume;
    missing.session.source_session_id = None;
    assert!(matches!(
        adapter.start_run(&task, &missing).await,
        Err(AdapterError::Failure {
            kind: FailureKind::Configuration,
            ..
        })
    ));

    let spawned = runtime.spawned.lock().unwrap();
    assert_eq!(spawned.len(), 1);
    let resume_args: Vec<_> = spawned[0]
        .args
        .iter()
        .map(|a| a.to_str().unwrap())
        .collect();
    assert!(
        resume_args
            .windows(2)
            .any(|w| w == ["--conversation", "session-parent"])
    );
}

#[tokio::test]
async fn data_dir_modes_and_option_rejection_follow_spec_7_5() {
    let task = fixtures::task_spec();
    let context = fixtures::run_context(&task);

    // 1. DefaultCli 모드는 --app_data_dir 인자를 붙이지 않습니다.
    let runtime_cli = FakeRuntime::new(Script::fixture("success.jsonl", 0));
    let cli_adapter = adapter(runtime_cli.clone()).with_default_cli_data_dir();
    let _ = run_to_end(&cli_adapter).await;
    let cli_args: Vec<_> = runtime_cli.spawned.lock().unwrap()[0]
        .args
        .iter()
        .map(|a| a.to_str().unwrap().to_owned())
        .collect();
    assert!(!cli_args.iter().any(|a| a.starts_with("--app_data_dir")));

    // 2. --app_data_dir 옵션이 CLI에서 거부된 경우 조용히 다른 디렉터리로 바꾸지 않고 unsupported로 거부합니다 (§7.5 규칙 5).
    let runtime_unsupported = FakeRuntime::new(Script::fixture("success.jsonl", 0));
    let rejected_adapter =
        adapter(runtime_unsupported.clone()).with_data_dir_option_supported(false);
    let caps = rejected_adapter.detect(&context.host_id).await.unwrap();
    assert_eq!(caps["managed_run"].state, CapabilityState::Unsupported);
    assert_eq!(caps["vscode_listing"].state, CapabilityState::Unsupported);
    assert!(matches!(
        rejected_adapter.start_run(&task, &context).await,
        Err(AdapterError::UnsupportedCapability {
            ref capability,
            ..
        }) if capability == "managed_run"
    ));
    assert!(runtime_unsupported.spawned.lock().unwrap().is_empty());

    // 3. verify_extension_data_dir는 --app_data_dir=antigravity --version을 호출해 확인합니다.
    let version_runtime = FakeRuntime::new(Script {
        lines: vec![b"1.4.7".to_vec()],
        stderr_lines: vec![],
        lines_on_exit: vec![],
        exit_code: Some(0),
        hang: false,
    });
    let verified = AgyAdapter::verify_extension_data_dir(
        version_runtime.as_ref(),
        std::path::Path::new("C:/Users/user/.gemini/bin/agy.exe"),
        std::path::Path::new("C:/work"),
    )
    .await
    .unwrap();
    assert_eq!(verified, "1.4.7");
    let probe_args: Vec<_> = version_runtime.spawned.lock().unwrap()[0]
        .args
        .iter()
        .map(|a| a.to_str().unwrap().to_owned())
        .collect();
    assert_eq!(probe_args, ["--app_data_dir=antigravity", "--version"]);
}

/// Spec §7.6 (DEC-08, AGENT-05): VS Code 확장 대화 목록 표시 모드(`listed` 기본 / `hidden`) 대응과
/// `vscode_listing` Capability 보고를 검증합니다.
#[tokio::test]
async fn vscode_listing_modes_and_capability_follow_spec_7_6() {
    let host_id = fixtures::run_context(&fixtures::task_spec()).host_id;

    // 1. 기본값은 Listed이며 ExtensionShared(--app_data_dir=antigravity)와 대응합니다.
    assert_eq!(AgyVsCodeListing::default(), AgyVsCodeListing::Listed);
    assert_eq!(
        AgyDataDirMode::from(AgyVsCodeListing::Listed),
        AgyDataDirMode::ExtensionShared
    );
    assert_eq!(
        AgyDataDirMode::from_vscode_listing(AgyVsCodeListing::Hidden),
        AgyDataDirMode::DefaultCli
    );
    assert_eq!(
        AgyDataDirMode::ExtensionShared.vscode_listing(),
        Some(AgyVsCodeListing::Listed)
    );
    assert_eq!(
        AgyDataDirMode::DefaultCli.vscode_listing(),
        Some(AgyVsCodeListing::Hidden)
    );
    assert_eq!(
        AgyDataDirMode::Custom("custom-dir".into()).vscode_listing(),
        None
    );

    // 2. Adapter 기본 설정은 Listed이고, Hidden으로 바꾸면 --app_data_dir 인자가 생략됩니다.
    let runtime_listed = FakeRuntime::new(Script::fixture("success.jsonl", 0));
    let listed_adapter = adapter(runtime_listed.clone());
    assert_eq!(
        listed_adapter.vscode_listing(),
        Some(AgyVsCodeListing::Listed)
    );
    let _ = run_to_end(&listed_adapter).await;
    let listed_args: Vec<_> = runtime_listed.spawned.lock().unwrap()[0]
        .args
        .iter()
        .map(|a| a.to_str().unwrap().to_owned())
        .collect();
    assert!(
        listed_args
            .iter()
            .any(|a| a == "--app_data_dir=antigravity")
    );

    let runtime_hidden = FakeRuntime::new(Script::fixture("success.jsonl", 0));
    let hidden_adapter =
        adapter(runtime_hidden.clone()).with_vscode_listing(AgyVsCodeListing::Hidden);
    assert_eq!(
        hidden_adapter.vscode_listing(),
        Some(AgyVsCodeListing::Hidden)
    );
    let _ = run_to_end(&hidden_adapter).await;
    let hidden_args: Vec<_> = runtime_hidden.spawned.lock().unwrap()[0]
        .args
        .iter()
        .map(|a| a.to_str().unwrap().to_owned())
        .collect();
    assert!(!hidden_args.iter().any(|a| a.starts_with("--app_data_dir")));

    // 3. 운영자 설정이 Hidden이어도 Capability는 수단의 지원 여부를 나타내므로 Supported입니다 (§7.6).
    let hidden_caps = hidden_adapter.detect(&host_id).await.unwrap();
    assert_eq!(
        hidden_caps["vscode_listing"].state,
        CapabilityState::Supported
    );
    assert_eq!(
        hidden_caps["vscode_listing"].evidence_level,
        EvidenceLevel::EnvironmentTested
    );
    let reason = hidden_caps["vscode_listing"]
        .reason
        .as_deref()
        .expect("vscode_listing reason");
    assert!(reason.contains("--app_data_dir=antigravity"));
    assert!(reason.contains("AGY-DATA-02"));

    // 4. Hidden 모드여도 --app_data_dir 옵션 자체가 CLI에서 거부된 경우 vscode_listing은 Unsupported입니다.
    let hidden_unsupported = hidden_adapter.with_data_dir_option_supported(false);
    let hidden_unsupported_caps = hidden_unsupported.detect(&host_id).await.unwrap();
    assert_eq!(
        hidden_unsupported_caps["vscode_listing"].state,
        CapabilityState::Unsupported
    );
}

#[tokio::test]
async fn detect_reports_only_verified_capabilities_and_host_mismatch() {
    let host_id = fixtures::run_context(&fixtures::task_spec()).host_id;
    let runtime = FakeRuntime::new(Script::default());
    let verified = adapter(runtime.clone());
    let caps = verified.detect(&host_id).await.unwrap();
    for name in ["managed_run", "event_stream", "cancel", "vscode_listing"] {
        assert_eq!(caps[name].state, CapabilityState::Supported);
        assert_eq!(caps[name].evidence_level, EvidenceLevel::EnvironmentTested);
    }
    assert_eq!(caps["native_resume"].state, CapabilityState::Unverified);
    assert_eq!(
        caps["native_resume"].evidence_level,
        EvidenceLevel::EnvironmentTested
    );
    assert_eq!(caps["send_input"].state, CapabilityState::Unsupported);
    assert_eq!(caps["answer_approval"].state, CapabilityState::Unsupported);
    assert_eq!(caps["native_fork"].state, CapabilityState::Unsupported);

    let unverified = adapter_with_version(runtime, "9.9.9");
    let caps = unverified.detect(&host_id).await.unwrap();
    assert_eq!(caps["managed_run"].state, CapabilityState::Unverified);
    assert_eq!(caps["vscode_listing"].state, CapabilityState::Unverified);
    assert_eq!(
        caps["vscode_listing"].evidence_level,
        EvidenceLevel::ContractTested
    );
    assert_eq!(caps["native_resume"].state, CapabilityState::Unverified);
    assert_eq!(caps["native_fork"].state, CapabilityState::Unsupported);

    let other_host = verified.detect("ssh-linux").await.unwrap();
    assert_eq!(
        other_host["managed_run"].state,
        CapabilityState::Unsupported
    );
}

#[test]
fn cli_rule_matches_supported_platforms() {
    let win = AgyAdapter::cli_rule("win32-x64").unwrap();
    assert_eq!(win.extension_id, "google.google-antigravity");
    assert_eq!(win.binary_name, "agy.exe");
    assert_eq!(
        win.binary,
        CliBinaryRule::Managed {
            home_relative_path: PathBuf::from(".gemini/bin/agy.exe"),
        }
    );

    let linux = AgyAdapter::cli_rule("linux-x64").unwrap();
    assert_eq!(linux.binary_name, "agy");
    assert_eq!(
        linux.binary,
        CliBinaryRule::Managed {
            home_relative_path: PathBuf::from(".gemini/bin/agy"),
        }
    );

    assert!(AgyAdapter::cli_rule("freebsd-x64").is_none());
}

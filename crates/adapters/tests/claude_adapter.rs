//! Claude Code Adapter의 Mock 시험 (PH0-CLAUDE, PROV-03·06·07·08).
//!
//! 실제 CLI 출력(`fixtures/claude/*.jsonl`, Claude Code 2.1.310)을 가짜 프로세스 런타임으로 재생합니다.
//! 실제 CLI·프로세스 시험은 `claude_environment.rs`에 있습니다.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use futures::{StreamExt, stream::BoxStream};
use telos_adapters::{
    AdapterError, AgentAdapter, ClaudeCodeAdapter,
    claude::{ClaudeCodeConfig, ClaudeLaunch, ClaudeLaunchResolver, cli_rule},
    runtime::*,
};
use telos_schema::{
    ApprovalAnswer, CapabilityState, FailureKind, ProviderEvent, ProviderEventKind,
    ProviderEventPayload, RunContext, RunInspection, RunTermination, SessionMode, TaskSpec,
    ToolOutcome, TurnStatus, check_run_events, fixtures,
};
use tokio::sync::watch;

// --- 가짜 런타임 ---

#[derive(Clone, Default)]
struct Script {
    lines: Vec<Vec<u8>>,
    exit_code: Option<i32>,
    /// 출력 뒤 종료 요청까지 멈춥니다(취소 시험).
    hang: bool,
}

impl Script {
    fn fixture(name: &str, exit_code: i32) -> Self {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/claude")
            .join(name);
        let text = std::fs::read_to_string(path).unwrap();
        Self {
            lines: text.lines().map(|l| l.as_bytes().to_vec()).collect(),
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
        let lines = script
            .lines
            .into_iter()
            .enumerate()
            .map(|(i, data)| OutputEvent::Line {
                stream: OutputStream::Stdout,
                sequence: i as u64 + 1,
                data,
            });
        let tail = futures::stream::once(async move {
            let _ = exited.wait_for(Option::is_some).await;
        })
        .flat_map(|()| {
            futures::stream::iter([
                OutputEvent::Closed {
                    stream: OutputStream::Stdout,
                    sequence: 1000,
                },
                OutputEvent::Closed {
                    stream: OutputStream::Stderr,
                    sequence: 1001,
                },
            ])
        });
        Ok(Arc::new(FakeHandle {
            identity: ProcessIdentity {
                pid: 1,
                started_at: "2026-10-24T00:00:00Z".into(),
            },
            output: Mutex::new(Some(futures::stream::iter(lines).chain(tail).boxed())),
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

struct Launch(ClaudeLaunch);

impl ClaudeLaunchResolver for Launch {
    fn resolve(&self, _: &TaskSpec, _: &RunContext) -> Result<ClaudeLaunch, AdapterError> {
        Ok(self.0.clone())
    }
}

fn launch() -> ClaudeLaunch {
    ClaudeLaunch {
        cwd: PathBuf::from("C:/work"),
        prompt: "Reply with exactly: pong".into(),
        model: Some("haiku".into()),
        permission_mode: "default".into(),
        allowed_tools: vec!["Read".into()],
        disallowed_tools: vec!["Bash".into()],
        tools: None,
        setting_sources: Some("user".into()),
    }
}

fn adapter_with(
    runtime: Arc<FakeRuntime>,
    launch: ClaudeLaunch,
    version: &str,
) -> ClaudeCodeAdapter {
    let context = fixtures::run_context(&fixtures::task_spec());
    ClaudeCodeAdapter::new(ClaudeCodeConfig {
        executable: PathBuf::from("C:/ext/resources/native-binary/claude.exe"),
        cli_version: version.into(),
        host_id: context.host_id,
        runtime,
        launcher: Arc::new(Launch(launch)),
        cancel_grace: Duration::from_millis(10),
    })
}

fn adapter(runtime: Arc<FakeRuntime>) -> ClaudeCodeAdapter {
    adapter_with(runtime, launch(), "2.1.310 (Claude Code)")
}

async fn run_to_end(adapter: &ClaudeCodeAdapter) -> Vec<ProviderEvent> {
    let task = fixtures::task_spec();
    let context = fixtures::run_context(&task);
    adapter.start_run(&task, &context).await.unwrap();
    collect(adapter, &context.run_id, 0).await
}

async fn collect(adapter: &ClaudeCodeAdapter, run_id: &str, after: u64) -> Vec<ProviderEvent> {
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
            Usage,
            TurnFinished,
            ProcessExited
        ]
    );
    let p = payloads(&events);
    let ProviderEventPayload::TextDelta(text) = &p[1] else {
        panic!()
    };
    assert_eq!(text.text, "pong");
    let ProviderEventPayload::Usage(usage) = &p[2] else {
        panic!()
    };
    assert_eq!(usage.cost_usd, None, "subscription usage is not a cost");
    let ProviderEventPayload::TurnFinished(turn) = &p[3] else {
        panic!()
    };
    assert_eq!(turn.status, TurnStatus::Completed);
    let ProviderEventPayload::ProcessExited(exit) = &p[4] else {
        panic!()
    };
    assert_eq!(exit.exit_code, Some(0));
    assert_eq!(exit.children_reaped, Some(true));
    assert!(events[1].native_session_id.is_some());
    assert_eq!(
        adapter.inspect_run("run-1").await,
        Ok(RunInspection::Finished)
    );

    let spec = &runtime.spawned.lock().unwrap()[0];
    let args: Vec<_> = spec.args.iter().map(|a| a.to_str().unwrap()).collect();
    for expected in [
        "-p",
        "stream-json",
        "--verbose",
        "--strict-mcp-config",
        "haiku",
        "Read",
        "Bash",
        "user",
    ] {
        assert!(args.contains(&expected), "missing {expected}: {args:?}");
    }
    assert!(!args.iter().any(|a| a.contains("dangerously")));
    assert_eq!(spec.stdin, StdinMode::Piped);
    let stdin = String::from_utf8(runtime.stdin.lock().unwrap().clone()).unwrap();
    let message: serde_json::Value = serde_json::from_str(stdin.trim_end()).unwrap();
    assert_eq!(message["message"]["content"], "Reply with exactly: pong");
}

#[tokio::test]
async fn permission_denial_is_reported_as_denied_tool() {
    let adapter = adapter(FakeRuntime::new(Script::fixture(
        "permission-denied.jsonl",
        0,
    )));
    let events = run_to_end(&adapter).await;
    assert_eq!(check_run_events(&events), Ok(RunTermination::ProcessExited));
    let denied = payloads(&events).into_iter().find_map(|p| match p {
        ProviderEventPayload::ToolCompleted(t) => Some(t),
        _ => None,
    });
    let denied = denied.expect("tool_completed");
    assert_eq!(denied.name, "Write");
    assert_eq!(denied.outcome, ToolOutcome::Denied);
}

#[tokio::test]
async fn provider_error_is_classified_and_turn_fails() {
    let adapter = adapter(FakeRuntime::new(Script::fixture(
        "model-not-found.jsonl",
        1,
    )));
    let events = run_to_end(&adapter).await;
    assert_eq!(check_run_events(&events), Ok(RunTermination::ProcessExited));
    let p = payloads(&events);
    let error = p.iter().find_map(|p| match p {
        ProviderEventPayload::Error(e) => Some(e),
        _ => None,
    });
    assert_eq!(error.unwrap().kind, FailureKind::Configuration);
    assert!(p.iter().any(
        |p| matches!(p, ProviderEventPayload::TurnFinished(t) if t.status == TurnStatus::Failed)
    ));
    let ProviderEventPayload::ProcessExited(exit) = p.last().unwrap() else {
        panic!()
    };
    assert_eq!(exit.exit_code, Some(1));
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
            .any(|p| matches!(p, ProviderEventPayload::TextDelta(_)))
    );
    assert!(p.iter().any(
        |p| matches!(p, ProviderEventPayload::TurnFinished(t) if t.status == TurnStatus::Failed)
    ));
}

#[tokio::test]
async fn exit_without_result_is_a_failed_turn() {
    let mut script = Script::fixture("success.jsonl", 3);
    script.lines.truncate(1);
    let adapter = adapter(FakeRuntime::new(script));
    let events = run_to_end(&adapter).await;
    assert_eq!(check_run_events(&events), Ok(RunTermination::ProcessExited));
    let p = payloads(&events);
    assert!(matches!(&p[1], ProviderEventPayload::Error(e) if e.kind == FailureKind::Provider));
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

    let undiscovered = ClaudeCodeAdapter::default();
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
async fn bypass_permissions_is_rejected_before_spawn() {
    let runtime = FakeRuntime::new(Script::fixture("success.jsonl", 0));
    let mut unsafe_launch = launch();
    unsafe_launch.permission_mode = "bypassPermissions".into();
    let adapter = adapter_with(runtime.clone(), unsafe_launch, "2.1.310");
    let task = fixtures::task_spec();
    let result = adapter
        .start_run(&task, &fixtures::run_context(&task))
        .await;
    assert!(matches!(
        result,
        Err(AdapterError::Failure {
            kind: FailureKind::Configuration,
            ..
        })
    ));
    assert!(runtime.spawned.lock().unwrap().is_empty());
    assert!(matches!(
        adapter.events("run-1", 0).next().await.unwrap(),
        Err(AdapterError::UnknownRun { .. })
    ));
}

#[tokio::test]
async fn native_fork_passes_resume_and_fork_flags() {
    let runtime = FakeRuntime::new(Script::fixture("success.jsonl", 0));
    let adapter = adapter(runtime.clone());
    let task = fixtures::task_spec();
    let mut context = fixtures::run_context(&task);
    context.session.mode = SessionMode::NativeFork;
    context.session.source_session_id = Some("parent-session".into());
    adapter.start_run(&task, &context).await.unwrap();
    let args: Vec<_> = runtime.spawned.lock().unwrap()[0]
        .args
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let at = args.iter().position(|a| a == "--resume").unwrap();
    assert_eq!(args[at + 1], "parent-session");
    assert!(args.contains(&"--fork-session".to_owned()));

    context.run_id = "run-2".into();
    context.idempotency_key = "run-2:start".into();
    context.session.source_session_id = None;
    assert!(matches!(
        adapter.start_run(&task, &context).await,
        Err(AdapterError::Failure {
            kind: FailureKind::Configuration,
            ..
        })
    ));
}

#[tokio::test]
async fn detect_reports_only_verified_versions_as_supported() {
    let host = fixtures::run_context(&fixtures::task_spec()).host_id;
    let verified = adapter(FakeRuntime::new(Script::default()));
    let caps = verified.detect(&host).await.unwrap();
    assert_eq!(caps["managed_run"].state, CapabilityState::Supported);
    assert_eq!(caps["answer_approval"].state, CapabilityState::Unsupported);
    assert_eq!(caps["native_fork"].state, CapabilityState::Unverified);

    let newer = adapter_with(
        FakeRuntime::new(Script::default()),
        launch(),
        "9.9.9 (Claude Code)",
    );
    assert_eq!(
        newer.detect(&host).await.unwrap()["managed_run"].state,
        CapabilityState::Unverified
    );
    assert_eq!(
        verified.detect("other-host").await.unwrap()["managed_run"].state,
        CapabilityState::Unsupported
    );
}

#[test]
fn cli_rule_follows_the_extension_layout() {
    let windows = cli_rule("win32-x64").unwrap();
    assert_eq!(windows.extension_id, "anthropic.claude-code");
    assert_eq!(windows.binary_name, "claude.exe");
    assert!(matches!(
        &windows.binary,
        telos_adapters::CliBinaryRule::InExtension { relative_path }
            if relative_path == &PathBuf::from("resources/native-binary/claude.exe")
    ));
    assert_eq!(cli_rule("linux-x64").unwrap().binary_name, "claude");
    assert!(cli_rule("web").is_none());
}

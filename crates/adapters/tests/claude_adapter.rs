//! Claude Code Adapter의 Mock 시험 (PH0-CLAUDE·PH1-CLAUDE, PROV-03·06·07·08).
//!
//! 실제 CLI 출력(`fixtures/claude/*.jsonl`, Claude Code 2.1.310·2.1.325)을 가짜 프로세스 런타임으로 재생합니다.
//! 사전 점검(`--version`·`--help`·옵션 수용)은 가짜 CLI가 실제 CLI의 응답 형식으로 답합니다.
//! 실제 CLI·프로세스 시험은 `claude_environment.rs`에 있습니다.

use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use futures::{StreamExt, stream::BoxStream};
use sha2::{Digest, Sha256};
use telos_adapters::{
    AdapterError, AgentAdapter, ClaudeCodeAdapter,
    claude::{
        ClaudeApprovals, ClaudeCodeConfig, ClaudeLaunch, ClaudeLaunchResolver, VsCodeListing,
        cli_rule, mcp::TASK_MCP_CREDENTIAL_FILE_ENV, preflight::host_platform,
    },
    runtime::*,
};
use telos_schema::{
    ApprovalAnswer, ApprovalRequiredPayload, CapabilityEvidence, CapabilityState, EvidenceLevel,
    FailureKind, PreflightCheck, ProcessTermination, ProviderEvent, ProviderEventKind,
    ProviderEventPayload, RunContext, RunInspection, RunTermination, SessionMode, TaskMcpBinding,
    TaskSpec, ToolOutcome, TurnStatus, capability_keys as keys, check_run_events, fixtures,
};
use tokio::sync::{mpsc, watch};

// --- 가짜 런타임 ---

/// 다음 줄을 내기 전에 기다리는 조건입니다. 실제 CLI가 입력을 기다리는 지점을 재현합니다.
#[derive(Clone)]
enum Wait {
    /// stdin에 이 문자열이 쓰일 때까지
    Stdin(&'static str),
    /// stdin이 닫힐 때까지
    StdinClosed,
}

#[derive(Clone, Default)]
struct Script {
    lines: Vec<Vec<u8>>,
    stderr: Vec<Vec<u8>>,
    exit_code: Option<i32>,
    /// 출력 뒤 종료 요청까지 멈춥니다(취소 시험).
    hang: bool,
    /// (줄 번호, 조건). 줄 수와 같은 번호는 모든 줄 뒤입니다.
    waits: Vec<(usize, Wait)>,
}

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/claude")
        .join(name)
}

impl Script {
    fn fixture(name: &str, exit_code: i32) -> Self {
        let text = std::fs::read_to_string(fixture_path(name)).unwrap();
        Self {
            lines: text.lines().map(|l| l.as_bytes().to_vec()).collect(),
            exit_code: Some(exit_code),
            ..Self::default()
        }
    }

    fn lines(lines: &[&str]) -> Self {
        Self {
            lines: lines.iter().map(|l| l.as_bytes().to_vec()).collect(),
            exit_code: Some(0),
            ..Self::default()
        }
    }

    /// `needle`이 들어 있는 첫 줄의 번호입니다.
    fn index_of(&self, needle: &str) -> usize {
        self.lines
            .iter()
            .position(|l| String::from_utf8_lossy(l).contains(needle))
            .unwrap()
    }
}

/// 사전 점검에 답하는 가짜 CLI입니다. 응답 형식은 Claude Code 2.1.325에서 관찰한 것입니다.
#[derive(Clone)]
struct FakeCli {
    version: String,
    /// 이 CLI가 더는 모르는 옵션
    removed: Vec<&'static str>,
    /// `--permission-mode default`를 받지 않는 CLI
    rejects_default_mode: bool,
}

impl FakeCli {
    fn respond(&self, args: &[String]) -> Script {
        let text = |lines: Vec<String>| lines.into_iter().map(String::into_bytes).collect();
        if args == ["--version"] {
            return Script {
                lines: text(vec![self.version.clone()]),
                exit_code: Some(0),
                ..Script::default()
            };
        }
        if args == ["--help"] {
            let help = std::fs::read_to_string(fixture_path("help-2.1.325.txt")).unwrap();
            let kept = help.lines().filter(|line| {
                !(line.starts_with("  -")
                    && line
                        .split_whitespace()
                        .any(|t| self.removed.contains(&t.trim_end_matches(','))))
            });
            return Script {
                lines: text(kept.map(str::to_owned).collect()),
                exit_code: Some(0),
                ..Script::default()
            };
        }
        let failure = |message: String| Script {
            stderr: text(vec![message]),
            exit_code: Some(1),
            ..Script::default()
        };
        if let Some(unknown) = args.iter().find(|a| self.removed.contains(&a.as_str())) {
            return failure(format!("error: unknown option '{unknown}'"));
        }
        if self.rejects_default_mode && args.iter().any(|a| a == "--permission-mode") {
            return failure(
                "error: option '--permission-mode <mode>' argument 'default' is invalid. Allowed choices are acceptEdits, auto, bypassPermissions, manual, dontAsk, plan."
                    .into(),
            );
        }
        if args.iter().any(|a| a == "--resume") {
            return Script {
                lines: text(vec![
                    r#"{"type":"result","subtype":"error_during_execution","is_error":true,"num_turns":0}"#.into(),
                ]),
                stderr: text(vec![
                    "No conversation found with session ID: 00000000-0000-4000-8000-000000000000"
                        .into(),
                ]),
                exit_code: Some(1),
                ..Script::default()
            };
        }
        Script {
            exit_code: Some(0),
            ..Script::default()
        }
    }
}

#[derive(Clone, Default)]
struct Stdin {
    text: String,
    closed: bool,
}

struct FakeRuntime {
    script: Mutex<Script>,
    cli: Mutex<FakeCli>,
    /// Run 실행
    spawned: Mutex<Vec<CommandSpec>>,
    /// 사전 점검 실행(stdin 없음)
    checks: Mutex<Vec<CommandSpec>>,
    stdin: watch::Sender<Stdin>,
    fail_spawn: AtomicBool,
}

impl FakeRuntime {
    fn new(script: Script) -> Arc<Self> {
        Arc::new(Self {
            script: Mutex::new(script),
            cli: Mutex::new(FakeCli {
                version: "2.1.310 (Claude Code)".into(),
                removed: Vec::new(),
                rejects_default_mode: false,
            }),
            spawned: Mutex::default(),
            checks: Mutex::default(),
            stdin: watch::channel(Stdin::default()).0,
            fail_spawn: AtomicBool::new(false),
        })
    }

    fn stdin_text(&self) -> String {
        self.stdin.borrow().text.clone()
    }

    fn stdin_closed(&self) -> bool {
        self.stdin.borrow().closed
    }

    fn run_args(&self, index: usize) -> Vec<String> {
        strings(&self.spawned.lock().unwrap()[index].args)
    }
}

fn strings(args: &[std::ffi::OsString]) -> Vec<String> {
    args.iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect()
}

struct FakeHandle {
    identity: ProcessIdentity,
    output: Mutex<Option<BoxStream<'static, OutputEvent>>>,
    exit: watch::Sender<Option<ExitStatus>>,
    stdin: watch::Sender<Stdin>,
    /// stdin이 닫히면 스스로 끝나는 프로세스인지
    exits_on_close: bool,
}

#[async_trait]
impl ProcessRuntime for FakeRuntime {
    async fn spawn(&self, spec: CommandSpec) -> Result<Arc<dyn ProcessHandle>, RuntimeError> {
        let script = if spec.stdin == StdinMode::Null {
            let script = self.cli.lock().unwrap().respond(&strings(&spec.args));
            self.checks.lock().unwrap().push(spec);
            script
        } else {
            if self.fail_spawn.load(Ordering::SeqCst) {
                return Err(RuntimeError::SpawnFailed {
                    program: spec.program,
                    message: "simulated".into(),
                });
            }
            self.spawned.lock().unwrap().push(spec);
            // Run마다 새 프로세스의 stdin입니다.
            self.stdin.send_replace(Stdin::default());
            self.script.lock().unwrap().clone()
        };
        let (exit, _) = watch::channel(None);
        let (tx, rx) = mpsc::unbounded_channel();
        let exits_on_close = script
            .waits
            .iter()
            .any(|(at, wait)| *at == script.lines.len() && matches!(wait, Wait::StdinClosed));
        tokio::spawn({
            let exit = exit.clone();
            let stdin = self.stdin.subscribe();
            async move {
                let mut stopped = exit.subscribe();
                let mut sequence = 0;
                let mut line = |stream, data: &Vec<u8>| {
                    sequence += 1;
                    let _ = tx.send(OutputEvent::Line {
                        stream,
                        sequence,
                        data: data.clone(),
                    });
                };
                let mut interrupted = false;
                'output: for index in 0..=script.lines.len() {
                    for (_, wait) in script.waits.iter().filter(|(at, _)| *at == index) {
                        let mut stdin = stdin.clone();
                        let met = async {
                            let _ = match wait {
                                Wait::Stdin(text) => {
                                    stdin.wait_for(|s| s.text.contains(text)).await.map(|_| ())
                                }
                                Wait::StdinClosed => stdin.wait_for(|s| s.closed).await.map(|_| ()),
                            };
                        };
                        tokio::select! {
                            () = met => {}
                            _ = stopped.wait_for(Option::is_some) => {
                                interrupted = true;
                                break 'output;
                            }
                        }
                    }
                    if let Some(data) = script.lines.get(index) {
                        line(OutputStream::Stdout, data);
                    }
                }
                if !interrupted {
                    for data in &script.stderr {
                        line(OutputStream::Stderr, data);
                    }
                    if !script.hang {
                        exit.send_if_modified(|status| {
                            status.is_none() && {
                                *status = Some(ExitStatus {
                                    code: script.exit_code,
                                    cause: TerminationCause::Exited,
                                });
                                true
                            }
                        });
                    }
                }
                let _ = stopped.wait_for(Option::is_some).await;
                for (stream, sequence) in
                    [(OutputStream::Stdout, 1000), (OutputStream::Stderr, 1001)]
                {
                    let _ = tx.send(OutputEvent::Closed { stream, sequence });
                }
            }
        });
        let output = futures::stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|event| (event, rx))
        });
        Ok(Arc::new(FakeHandle {
            identity: ProcessIdentity {
                pid: 1,
                started_at: "2026-10-24T00:00:00Z".into(),
            },
            output: Mutex::new(Some(output.boxed())),
            exit,
            stdin: self.stdin.clone(),
            exits_on_close,
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
        if self.exit.borrow().is_some() || self.stdin.borrow().closed {
            return Err(RuntimeError::StdinUnavailable);
        }
        self.stdin
            .send_modify(|s| s.text.push_str(&String::from_utf8_lossy(data)));
        Ok(())
    }
    async fn close_stdin(&self) -> Result<(), RuntimeError> {
        self.stdin.send_modify(|s| s.closed = true);
        Ok(())
    }
    async fn wait(&self) -> Result<ExitStatus, RuntimeError> {
        let mut rx = self.exit.subscribe();
        let status = rx.wait_for(Option::is_some).await.unwrap();
        Ok((*status).unwrap())
    }
    async fn terminate(&self, _grace: Duration) -> Result<TerminationReport, RuntimeError> {
        // 실제 런타임처럼 먼저 stdin을 닫고 스스로 끝나기를 기다립니다.
        if self.exits_on_close {
            self.stdin.send_modify(|s| s.closed = true);
            let mut exited = self.exit.subscribe();
            let _ = tokio::time::timeout(Duration::from_secs(2), exited.wait_for(Option::is_some))
                .await;
        }
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
    adapter_resolving(runtime, Arc::new(Launch(launch)), version)
}

/// 가짜 CLI도 같은 버전을 보고하게 합니다.
fn adapter_resolving(
    runtime: Arc<FakeRuntime>,
    launcher: Arc<dyn ClaudeLaunchResolver>,
    version: &str,
) -> ClaudeCodeAdapter {
    let context = fixtures::run_context(&fixtures::task_spec());
    runtime.cli.lock().unwrap().version = version.into();
    ClaudeCodeAdapter::new(ClaudeCodeConfig {
        executable: PathBuf::from("C:/ext/resources/native-binary/claude.exe"),
        cli_version: version.into(),
        host_id: context.host_id,
        runtime,
        launcher,
        cancel_grace: Duration::from_millis(10),
    })
}

fn context(run_id: &str) -> RunContext {
    let mut context = fixtures::run_context(&fixtures::task_spec());
    context.run_id = run_id.into();
    context.idempotency_key = format!("{run_id}:start");
    context
}

/// `kind` 이벤트가 나올 때까지 받은 이벤트입니다.
async fn events_until(
    adapter: &ClaudeCodeAdapter,
    run_id: &str,
    kind: ProviderEventKind,
) -> Vec<ProviderEvent> {
    let mut stream = adapter.events(run_id, 0);
    let mut seen = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(event) = stream.next().await {
            let event = event.unwrap();
            let done = event.kind == kind;
            seen.push(event);
            if done {
                return;
            }
        }
        panic!("stream ended before {kind:?}");
    })
    .await
    .expect("event arrives");
    seen
}

/// 시험마다 따로 쓰는 저장소 밖 폴더입니다.
fn state_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("telos-claude-mock-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn mcp_configs(dir: &std::path::Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("claude-mcp-"))
        })
        .collect()
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
    let message: serde_json::Value = serde_json::from_str(runtime.stdin_text().trim_end()).unwrap();
    assert_eq!(message["message"]["content"], "Reply with exactly: pong");
    // 기본값은 실시간 승인 없이 프롬프트만 보내고 stdin을 닫습니다.
    assert!(runtime.stdin_closed());
    assert!(!args.contains(&"--permission-prompt-tool") && !args.contains(&"--mcp-config"));
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
    run_to_end(&adapter).await;
    assert!(matches!(
        adapter.send_input("run-1", "in-1", "more").await,
        Err(AdapterError::UnsupportedCapability { capability, .. }) if capability == keys::SEND_INPUT
    ));
    // 실시간 승인을 켜지 않은 Run은 승인 응답을 받지 않습니다 (§7.1: 키 + 사유).
    assert!(matches!(
        adapter
            .answer_approval("run-1", "req-1", ApprovalAnswer::Allow)
            .await,
        Err(AdapterError::UnsupportedCapability { capability, reason })
            if capability == keys::ANSWER_APPROVAL && reason.contains("not enabled")
    ));
    assert!(matches!(
        adapter
            .answer_approval("missing", "req-1", ApprovalAnswer::Allow)
            .await,
        Err(AdapterError::UnknownRun { .. })
    ));
    let first = adapter.events("missing", 0).next().await.unwrap();
    assert!(matches!(first, Err(AdapterError::UnknownRun { .. })));

    // REQ-024: 공통 키를 씁니다(이전 `start_run`).
    let undiscovered = ClaudeCodeAdapter::default();
    let task = fixtures::task_spec();
    assert!(matches!(
        undiscovered
            .start_run(&task, &fixtures::run_context(&task))
            .await,
        Err(AdapterError::UnsupportedCapability { capability, .. }) if capability == keys::MANAGED_RUN
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
    let args = runtime.run_args(0);
    let at = args.iter().position(|a| a == "--resume").unwrap();
    assert_eq!(args[at + 1], "parent-session");
    assert!(args.contains(&"--fork-session".to_owned()));

    // 특정 Turn 기준 재개·분기는 CLI 수단이 없어 프로세스를 띄우기 전에 거부합니다 (Spec §2.1·§7.1).
    for (mode, key) in [
        (SessionMode::Resume, keys::NATIVE_RESUME),
        (SessionMode::NativeFork, keys::NATIVE_FORK),
    ] {
        let mut at_turn = context.clone();
        at_turn.run_id = format!("run-turn-{mode:?}");
        at_turn.idempotency_key = format!("{}:start", at_turn.run_id);
        at_turn.session.mode = mode;
        at_turn.session.source_turn_id = Some("turn-1".into());
        assert!(matches!(
            adapter.start_run(&task, &at_turn).await,
            Err(AdapterError::UnsupportedCapability { capability, .. }) if capability == key
        ));
    }
    assert_eq!(runtime.spawned.lock().unwrap().len(), 1);

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

fn spawned_env(runtime: &FakeRuntime) -> Vec<(String, Option<String>)> {
    runtime.spawned.lock().unwrap()[0]
        .env
        .iter()
        .map(|(k, v)| {
            (
                k.to_string_lossy().into_owned(),
                v.as_ref().map(|v| v.to_string_lossy().into_owned()),
            )
        })
        .collect()
}

/// §7.6·DEC-08: 기본은 확장 목록에 표시하고, 부모 Claude 세션 변수는 상속하지 않습니다.
#[tokio::test]
async fn runs_are_listed_in_the_extension_by_default() {
    let runtime = FakeRuntime::new(Script::fixture("success.jsonl", 0));
    let adapter = adapter(runtime.clone());
    assert_eq!(adapter.vscode_listing(), VsCodeListing::Listed);
    run_to_end(&adapter).await;
    let env = spawned_env(&runtime);
    assert!(env.contains(&(
        "CLAUDE_CODE_ENTRYPOINT".into(),
        Some("claude-vscode".into())
    )));
    assert!(env.contains(&(
        "CLAUDE_CODE_DISABLE_BACKGROUND_TASKS".into(),
        Some("1".into())
    )));
    for parent in [
        "CLAUDECODE",
        "CLAUDE_CODE_SESSION_ID",
        "CLAUDE_CODE_CHILD_SESSION",
        "CLAUDE_CODE_SESSION_ATTENDED",
        "CLAUDE_CODE_MESSAGING_SOCKET",
        "CLAUDE_CODE_MESSAGING_TOKEN",
        "CLAUDE_PID",
    ] {
        assert!(
            env.contains(&(parent.into(), None)),
            "{parent} must be removed"
        );
    }
}

/// §7.2: `result` 뒤에 CLI가 스스로 Turn을 이어가면(백그라운드 작업 완료) 그 출력은 Run에 넣지 않고
/// 프로세스를 끝냅니다. 요청 종료로 보고되어 성공으로 판정되지 않습니다.
#[tokio::test]
async fn continuation_after_result_is_not_recorded_and_stops_the_process() {
    let mut script = Script::fixture("background-continuation.jsonl", 0);
    script.hang = true;
    let adapter = adapter(FakeRuntime::new(script));
    let events = run_to_end(&adapter).await;

    assert_eq!(check_run_events(&events), Ok(RunTermination::ProcessExited));
    let p = payloads(&events);
    let turns = p
        .iter()
        .filter(|item| matches!(item, ProviderEventPayload::TurnFinished(_)))
        .count();
    assert_eq!(turns, 1, "only the first turn belongs to the run");
    assert!(matches!(
        &p[p.len() - 2],
        ProviderEventPayload::TurnFinished(t) if t.status == TurnStatus::Completed
    ));
    let ProviderEventPayload::ProcessExited(exit) = p.last().unwrap() else {
        panic!()
    };
    assert_eq!(exit.termination, ProcessTermination::TerminatedOnRequest);
    assert_eq!(exit.exit_code, None);
}

/// §7.6: 숨김 설정은 상속된 값과 관계없이 자동화 출처(`sdk-cli`)로 기록합니다. Resume·Fork에도 같은 값입니다.
#[tokio::test]
async fn hidden_runs_are_recorded_as_automation_for_every_session_mode() {
    for mode in [
        SessionMode::Fresh,
        SessionMode::Resume,
        SessionMode::NativeFork,
    ] {
        let runtime = FakeRuntime::new(Script::fixture("success.jsonl", 0));
        let adapter = adapter(runtime.clone()).with_vscode_listing(VsCodeListing::Hidden);
        let task = fixtures::task_spec();
        let mut context = fixtures::run_context(&task);
        context.session.mode = mode;
        context.session.source_session_id = Some("parent-session".into());
        adapter.start_run(&task, &context).await.unwrap();
        let env = spawned_env(&runtime);
        let entrypoints: Vec<_> = env
            .iter()
            .filter(|(k, _)| k == "CLAUDE_CODE_ENTRYPOINT")
            .collect();
        assert_eq!(
            entrypoints,
            [&(
                "CLAUDE_CODE_ENTRYPOINT".to_owned(),
                Some("sdk-cli".to_owned())
            )],
            "{mode:?}"
        );
    }
}

// --- Capability 판정과 사전 점검 (P2·P3·L4) ---

async fn detect_for(version: &str, configure: impl FnOnce(&mut FakeCli)) -> Caps {
    let runtime = FakeRuntime::new(Script::default());
    let adapter = adapter_with(runtime.clone(), launch(), version);
    configure(&mut runtime.cli.lock().unwrap());
    let host = fixtures::run_context(&fixtures::task_spec()).host_id;
    let caps = adapter.detect(&host).await.unwrap();
    // 점검은 모델을 호출하지 않습니다. Run을 시작하지 않고 stdin도 주지 않습니다.
    assert!(runtime.spawned.lock().unwrap().is_empty());
    assert!(runtime.stdin_text().is_empty());
    caps
}

type Caps = std::collections::HashMap<String, telos_schema::Capability>;

fn state(caps: &Caps, key: &str) -> CapabilityState {
    caps[key].state
}

fn reason<'a>(caps: &'a Caps, key: &str) -> &'a str {
    caps[key].reason.as_deref().unwrap_or("")
}

/// 내장 증거는 Windows와 Linux x64에만 있습니다.
fn has_builtin_evidence() -> bool {
    matches!(host_platform().as_str(), "win32-x64" | "linux-x64")
}

const BASIC_KEYS: [&str; 6] = [
    keys::MANAGED_RUN,
    keys::EVENT_STREAM,
    keys::CANCEL,
    keys::NATIVE_RESUME,
    keys::NATIVE_FORK,
    keys::VSCODE_LISTING,
];

/// §7.1 판정 기준: 같은 버전의 통과 증거가 있으면 `supported`, 세부 제한은 `reason`입니다 (H0 D11).
#[tokio::test]
async fn detect_reports_tested_versions_as_supported() {
    if !has_builtin_evidence() {
        return;
    }
    let caps = detect_for("2.1.310 (Claude Code)", |_| {}).await;
    // REQ-024: 보고하는 키는 공통 키 8개뿐입니다.
    let mut names: Vec<_> = caps.keys().map(String::as_str).collect();
    names.sort_unstable();
    let mut common = keys::ALL.to_vec();
    common.sort_unstable();
    assert_eq!(names, common);
    for key in BASIC_KEYS {
        assert_eq!(state(&caps, key), CapabilityState::Supported, "{key}");
        assert_eq!(caps[key].evidence_level, EvidenceLevel::EnvironmentTested);
        assert_eq!(caps[key].provider_version, "2.1.310 (Claude Code)");
    }
    assert_eq!(caps[keys::MANAGED_RUN].reason, None);
    // 실환경에서 통과한 Resume·Fork는 `supported`이고 키보다 좁은 제한은 사유로 남깁니다.
    for key in [keys::NATIVE_RESUME, keys::NATIVE_FORK] {
        assert!(reason(&caps, key).contains("at a specific turn is rejected before start"));
    }
    // 수단이 계약과 맞지 않는 기능은 계속 `unsupported`와 사유입니다 (SB-06).
    assert_eq!(state(&caps, keys::SEND_INPUT), CapabilityState::Unsupported);
    assert!(reason(&caps, keys::SEND_INPUT).contains("new resumed run"));
    // Adapter 승인 경로의 증거는 Windows 2.1.325뿐입니다. 2.1.310은 같은 major.minor라 사전 점검으로 인정됩니다.
    let approval = &caps[keys::ANSWER_APPROVAL];
    if cfg!(windows) {
        assert_eq!(approval.state, CapabilityState::Supported);
        assert!(reason(&caps, keys::ANSWER_APPROVAL).contains("passed the preflight checks"));
    } else {
        assert_eq!(approval.state, CapabilityState::Unverified);
        assert!(reason(&caps, keys::ANSWER_APPROVAL).contains("no environment test has passed"));
    }
    assert!(reason(&caps, keys::ANSWER_APPROVAL).contains("ClaudeApprovals::Prompt"));
}

/// H0 D14: 같은 major.minor의 새 버전은 사전 점검을 통과하면 `supported`, major.minor가 바뀌면 `unverified`입니다.
#[tokio::test]
async fn detect_accepts_the_same_minor_line_only_with_a_passing_preflight() {
    if !has_builtin_evidence() {
        return;
    }
    let patched = detect_for("2.1.999 (Claude Code)", |_| {}).await;
    for key in BASIC_KEYS {
        assert_eq!(state(&patched, key), CapabilityState::Supported, "{key}");
        assert!(reason(&patched, key).contains("2.1.999 is the same major.minor"));
        assert!(reason(&patched, key).contains("environment-tested on 2.1.310"));
    }
    for version in ["2.2.0 (Claude Code)", "9.9.9 (Claude Code)"] {
        let other = detect_for(version, |_| {}).await;
        for key in BASIC_KEYS.into_iter().chain([keys::ANSWER_APPROVAL]) {
            assert_eq!(state(&other, key), CapabilityState::Unverified, "{key}");
            // 검증하지 않은 버전은 실환경 근거를 주장하지 않습니다.
            assert_eq!(other[key].evidence_level, EvidenceLevel::ContractTested);
        }
        assert!(reason(&other, keys::MANAGED_RUN).contains("different major.minor"));
    }
}

/// P3: 옵션이 없어진 CLI에서는 그 옵션이 필요한 기능만 `unverified`로 떨어집니다.
#[tokio::test]
async fn removed_option_makes_dependent_capabilities_unverified() {
    if !has_builtin_evidence() {
        return;
    }
    let no_fork = detect_for("2.1.999 (Claude Code)", |cli| {
        cli.removed.push("--fork-session")
    })
    .await;
    assert_eq!(
        state(&no_fork, keys::NATIVE_FORK),
        CapabilityState::Unverified
    );
    assert!(reason(&no_fork, keys::NATIVE_FORK).contains("--fork-session not accepted"));
    assert!(reason(&no_fork, keys::NATIVE_FORK).contains("--fork-session not in --help"));
    for key in [keys::MANAGED_RUN, keys::NATIVE_RESUME, keys::VSCODE_LISTING] {
        assert_eq!(state(&no_fork, key), CapabilityState::Supported, "{key}");
    }

    // `--resume`이 없으면 Fork도 쓸 수 없습니다.
    let no_resume = detect_for("2.1.999 (Claude Code)", |cli| cli.removed.push("--resume")).await;
    for key in [keys::NATIVE_RESUME, keys::NATIVE_FORK] {
        assert_eq!(state(&no_resume, key), CapabilityState::Unverified, "{key}");
    }
    assert_eq!(
        state(&no_resume, keys::MANAGED_RUN),
        CapabilityState::Supported
    );

    // 도움말에 정의가 없는 `--permission-prompt-tool`은 실행 점검으로 잡습니다.
    let no_prompt = detect_for("2.1.999 (Claude Code)", |cli| {
        cli.removed.push("--permission-prompt-tool")
    })
    .await;
    assert_eq!(
        state(&no_prompt, keys::ANSWER_APPROVAL),
        CapabilityState::Unverified
    );
    assert_eq!(
        state(&no_prompt, keys::NATIVE_FORK),
        CapabilityState::Supported
    );

    // 모든 Run이 쓰는 옵션이 없어지거나 값을 받지 않으면 전부 `unverified`입니다.
    for configure in [
        (|cli: &mut FakeCli| cli.removed.push("--strict-mcp-config")) as fn(&mut FakeCli),
        |cli: &mut FakeCli| cli.rejects_default_mode = true,
    ] {
        let broken = detect_for("2.1.999 (Claude Code)", configure).await;
        for key in BASIC_KEYS {
            assert_eq!(state(&broken, key), CapabilityState::Unverified, "{key}");
        }
    }
    let mode = detect_for("2.1.999 (Claude Code)", |cli| {
        cli.rejects_default_mode = true
    })
    .await;
    assert!(reason(&mode, keys::CANCEL).contains("--permission-mode default not accepted"));

    // 같은 버전의 증거가 있으면 `judge`는 사전 점검과 관계없이 `supported`입니다(계약 그대로).
    let tested = detect_for("2.1.310 (Claude Code)", |cli| {
        cli.removed.push("--fork-session")
    })
    .await;
    assert_eq!(
        state(&tested, keys::NATIVE_FORK),
        CapabilityState::Supported
    );
}

/// 판정에 쓰는 현재 버전은 지금 `--version`이 보고한 값입니다. 읽지 못하면 어떤 증거와도 맞지 않습니다.
#[tokio::test]
async fn detect_uses_the_version_the_cli_reports_now() {
    if !has_builtin_evidence() {
        return;
    }
    // 탐지 뒤 실행 파일이 바뀌어 다른 major.minor를 보고하는 경우
    let replaced = detect_for("2.1.310 (Claude Code)", |cli| {
        cli.version = "3.0.0 (Claude Code)".into()
    })
    .await;
    assert_eq!(
        state(&replaced, keys::MANAGED_RUN),
        CapabilityState::Unverified
    );
    assert_eq!(
        replaced[keys::MANAGED_RUN].provider_version,
        "3.0.0 (Claude Code)"
    );

    let garbled = detect_for("2.1.310 (Claude Code)", |cli| cli.version = "claude".into()).await;
    for key in BASIC_KEYS {
        assert_eq!(state(&garbled, key), CapabilityState::Unverified, "{key}");
    }
    assert!(reason(&garbled, keys::MANAGED_RUN).contains("could not be read"));
    assert_eq!(
        garbled[keys::MANAGED_RUN].provider_version,
        "2.1.310 (Claude Code)"
    );
}

#[tokio::test]
async fn preflight_report_lists_every_check_without_calling_the_model() {
    let runtime = FakeRuntime::new(Script::default());
    let adapter = adapter_with(runtime.clone(), launch(), "2.1.325 (Claude Code)");
    let report = adapter.preflight("windows-local").await.unwrap();
    assert!(report.passed());
    assert_eq!(report.provider, "claude");
    assert_eq!(report.cli_version, "2.1.325 (Claude Code)");
    let has = |check: PreflightCheck| report.results.iter().any(|r| r.check == check && r.passed);
    assert!(has(PreflightCheck::VersionOutput));
    assert!(has(PreflightCheck::HelpContains {
        flag: "--strict-mcp-config".into()
    }));
    for option in [
        "-p",
        "--input-format stream-json",
        "--output-format stream-json",
        "--permission-mode default",
        "--mcp-config",
        "--permission-prompt-tool stdio",
        "--resume",
        "--fork-session",
    ] {
        assert!(
            has(PreflightCheck::OptionAccepted {
                option: option.into()
            }),
            "{option}"
        );
    }

    // 세 번의 실행 모두 stdin 없이 실행했고 프롬프트를 넘기지 않았습니다.
    {
        let checks = runtime.checks.lock().unwrap();
        assert_eq!(checks.len(), 3);
        assert!(checks.iter().all(|spec| spec.stdin == StdinMode::Null));
        assert_eq!(strings(&checks[0].args), ["--version"]);
        assert_eq!(strings(&checks[1].args), ["--help"]);
        let probe = strings(&checks[2].args);
        // 없는 세션을 가리켜 Turn 없이 끝나게 합니다.
        let at = probe.iter().position(|a| a == "--resume").unwrap();
        assert_eq!(probe[at + 1], "00000000-0000-4000-8000-000000000000");
        assert!(runtime.spawned.lock().unwrap().is_empty());
    }

    // 거부된 옵션은 이름과 함께 실패로 남고, 나머지는 다시 확인해 통과로 남습니다.
    runtime.cli.lock().unwrap().removed = vec!["--fork-session", "--verbose"];
    let report = adapter.preflight("windows-local").await.unwrap();
    assert!(!report.passed());
    let failed: Vec<_> = report
        .results
        .iter()
        .filter(|r| !r.passed)
        .map(|r| match &r.check {
            PreflightCheck::OptionAccepted { option } => format!("option {option}"),
            PreflightCheck::HelpContains { flag } => format!("help {flag}"),
            PreflightCheck::VersionOutput => "version".into(),
        })
        .collect();
    assert_eq!(
        failed,
        [
            "help --verbose",
            "help --fork-session",
            "option --verbose",
            "option --fork-session"
        ]
    );
    assert!(
        ClaudeCodeAdapter::default()
            .preflight("windows-local")
            .await
            .is_err()
    );
}

/// L4: Core가 기록한 증거(`CapabilityEvidence`)를 더하면 그 버전·플랫폼에서 같은 기준으로 판정합니다.
#[tokio::test]
async fn recorded_evidence_extends_the_builtin_evidence() {
    let host = fixtures::run_context(&fixtures::task_spec()).host_id;
    let evidence = |platform: String, passed| CapabilityEvidence {
        key: keys::ANSWER_APPROVAL.into(),
        provider: "claude".into(),
        host_id: host.clone(),
        platform,
        cli_version: "7.7.7".into(),
        extension_version: None,
        test: "env_live_approval_allow_and_deny".into(),
        passed,
        checked_at: "2026-11-16T00:00:00Z".into(),
        evidence_ref: "test".into(),
    };
    let detect = |evidence: CapabilityEvidence| {
        let host = host.clone();
        async move {
            let runtime = FakeRuntime::new(Script::default());
            adapter_with(runtime, launch(), "7.7.7 (Claude Code)")
                .with_evidence([evidence])
                .detect(&host)
                .await
                .unwrap()
        }
    };
    let caps = detect(evidence(host_platform(), true)).await;
    assert_eq!(
        state(&caps, keys::ANSWER_APPROVAL),
        CapabilityState::Supported
    );
    assert_eq!(state(&caps, keys::MANAGED_RUN), CapabilityState::Unverified);
    // 다른 플랫폼의 증거와 실패한 시험은 근거가 아닙니다.
    for other in [
        evidence("other-os".into(), true),
        evidence(host_platform(), false),
    ] {
        let caps = detect(other).await;
        assert_eq!(
            state(&caps, keys::ANSWER_APPROVAL),
            CapabilityState::Unverified
        );
    }
}

#[tokio::test]
async fn detect_on_another_host_or_without_a_cli() {
    let runtime = FakeRuntime::new(Script::default());
    let verified = adapter(runtime.clone());
    assert_eq!(
        verified.detect("other-host").await.unwrap()[keys::MANAGED_RUN].state,
        CapabilityState::Unsupported
    );
    assert!(runtime.checks.lock().unwrap().is_empty());
    // 숨김 설정이어도 Capability는 수단의 지원 여부입니다 (§7.6).
    if has_builtin_evidence() {
        let host = fixtures::run_context(&fixtures::task_spec()).host_id;
        let hidden =
            adapter(FakeRuntime::new(Script::default())).with_vscode_listing(VsCodeListing::Hidden);
        assert_eq!(
            hidden.detect(&host).await.unwrap()[keys::VSCODE_LISTING].state,
            CapabilityState::Supported
        );
    }
}

// --- Task MCP 주입 (P4·L1) ---

const SECRET: &str = "SECRET-RUN-TOKEN-0f3a";

fn binding(dir: &std::path::Path) -> TaskMcpBinding {
    let credential = dir.join("mcp-credential");
    std::fs::write(&credential, SECRET).unwrap();
    TaskMcpBinding {
        server_name: "telosweaver".into(),
        command: "telos-weaver".into(),
        args: vec!["mcp".into(), "--stdio".into()],
        credential_file: credential.display().to_string(),
    }
}

fn value_after<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == flag)
        .map(|at| args[at + 1].as_str())
}

/// §9.2: `taskMcp`가 있는 Run에만 연결하고, 자격 원문은 설정 파일·인자·환경에 넣지 않으며, Run이 끝나면 파일을 지웁니다.
#[tokio::test]
async fn task_mcp_is_injected_for_that_run_only_and_cleaned_up() {
    let dir = state_dir("inject");
    let mut script = Script::fixture("task-mcp-call.jsonl", 0);
    script.hang = true;
    let runtime = FakeRuntime::new(script);
    let adapter = adapter(runtime.clone());
    let task = fixtures::task_spec();
    let mut with_mcp = context("run-mcp");
    with_mcp.task_mcp = Some(binding(&dir));
    adapter.start_run(&task, &with_mcp).await.unwrap();

    let args = runtime.run_args(0);
    let config_path = PathBuf::from(value_after(&args, "--mcp-config").unwrap());
    assert!(args.contains(&"--strict-mcp-config".to_owned()));
    // 주입한 서버의 도구만 추가로 허용합니다.
    assert_eq!(
        value_after(&args, "--allowedTools"),
        Some("Read,mcp__telosweaver")
    );
    // Run 상태 파일은 자격 파일 옆(Core 상태 디렉터리)에 있고 Workspace에는 쓰지 않습니다.
    assert_eq!(config_path.parent(), Some(dir.as_path()));
    assert_eq!(mcp_configs(&dir), std::slice::from_ref(&config_path));
    let text = std::fs::read_to_string(&config_path).unwrap();
    let config: serde_json::Value = serde_json::from_str(&text).unwrap();
    let server = &config["mcpServers"]["telosweaver"];
    assert_eq!(config["mcpServers"].as_object().unwrap().len(), 1);
    assert_eq!(server["type"], "stdio");
    assert_eq!(server["command"], "telos-weaver");
    assert_eq!(server["args"], serde_json::json!(["mcp", "--stdio"]));
    assert_eq!(
        server["env"],
        serde_json::json!({ TASK_MCP_CREDENTIAL_FILE_ENV: dir.join("mcp-credential").display().to_string() })
    );
    // 경로만 넘깁니다. 자격 원문은 파일·인자·환경·stdin 어디에도 없습니다.
    let spec = runtime.spawned.lock().unwrap()[0].clone();
    let env: String = spec
        .env
        .iter()
        .map(|(k, v)| format!("{k:?}={v:?};"))
        .collect();
    for place in [&text, &args.join(" "), &env, &runtime.stdin_text()] {
        assert!(!place.contains(SECRET));
    }

    // 실제 출력 재생: 주입한 서버의 도구 호출이 도구 이벤트로 나옵니다.
    let events = events_until(&adapter, "run-mcp", ProviderEventKind::TurnFinished).await;
    assert!(payloads(&events).iter().any(|p| matches!(p,
        ProviderEventPayload::ToolCompleted(t)
            if t.name == "mcp__telosweaver__telos_probe" && t.outcome == ToolOutcome::Succeeded)));
    // Run이 끝나면(여기서는 Turn 뒤 프로세스 종료) 주입 파일을 지웁니다.
    assert!(config_path.is_file(), "kept while the process is alive");
    adapter.cancel_run("run-mcp").await.unwrap();
    let events = collect(&adapter, "run-mcp", 0).await;
    assert_eq!(check_run_events(&events), Ok(RunTermination::ProcessExited));
    assert!(mcp_configs(&dir).is_empty());
    assert!(
        dir.join("mcp-credential").is_file(),
        "the credential is Core's"
    );

    // `taskMcp`가 없는 Run은 이전과 같게 실행합니다.
    adapter
        .start_run(&task, &context("run-plain"))
        .await
        .unwrap();
    let plain = runtime.run_args(1);
    assert!(!plain.contains(&"--mcp-config".to_owned()));
    assert_eq!(value_after(&plain, "--allowedTools"), Some("Read"));
    assert!(mcp_configs(&dir).is_empty());
    adapter.cancel_run("run-plain").await.unwrap();
    collect(&adapter, "run-plain", 0).await;
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn task_mcp_file_is_removed_when_the_run_ends_or_fails_to_start() {
    let dir = state_dir("cleanup");
    let task = fixtures::task_spec();

    // 스스로 끝난 Run
    let runtime = FakeRuntime::new(Script::fixture("task-mcp-call.jsonl", 0));
    let adapter = adapter(runtime.clone());
    let mut finished = context("run-finished");
    finished.task_mcp = Some(binding(&dir));
    adapter.start_run(&task, &finished).await.unwrap();
    let events = collect(&adapter, "run-finished", 0).await;
    assert_eq!(check_run_events(&events), Ok(RunTermination::ProcessExited));
    assert!(mcp_configs(&dir).is_empty());

    // 프로세스 시작 실패
    runtime.fail_spawn.store(true, Ordering::SeqCst);
    let mut failed = context("run-spawn-failed");
    failed.task_mcp = Some(binding(&dir));
    assert!(matches!(
        adapter.start_run(&task, &failed).await,
        Err(AdapterError::Failure {
            kind: FailureKind::StartFailed,
            ..
        })
    ));
    assert!(mcp_configs(&dir).is_empty());
    runtime.fail_spawn.store(false, Ordering::SeqCst);

    // 잘못된 연결 정보는 프로세스를 띄우기 전에 거부하고 파일을 남기지 않습니다.
    let spawned = runtime.spawned.lock().unwrap().len();
    let invalid: [fn(&mut TaskMcpBinding); 4] = [
        |b| b.server_name = "bad name".into(),
        |b| b.server_name = "a__b".into(),
        |b| b.credential_file = "relative/credential".into(),
        |b| b.credential_file.push_str(".missing"),
    ];
    for (index, change) in invalid.into_iter().enumerate() {
        let mut rejected = context(&format!("run-invalid-{index}"));
        let mut bad = binding(&dir);
        change(&mut bad);
        rejected.task_mcp = Some(bad);
        assert!(
            matches!(
                adapter.start_run(&task, &rejected).await,
                Err(AdapterError::Failure {
                    kind: FailureKind::Configuration,
                    ..
                })
            ),
            "case {index}"
        );
    }
    assert_eq!(runtime.spawned.lock().unwrap().len(), spawned);
    assert!(mcp_configs(&dir).is_empty());
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Task MCP를 요청한 Run에서 서버가 연결되지 않으면 결과를 보고할 수 없으므로 Run을 실패로 끝냅니다.
#[tokio::test]
async fn task_mcp_server_that_is_not_connected_fails_the_run() {
    let dir = state_dir("failed-server");
    for (from, status) in [
        (r#""status":"connected""#, "failed"),
        (
            r#"{"name":"telosweaver","status":"connected","source":"dynamic"}"#,
            "missing",
        ),
    ] {
        let mut script = Script::fixture("task-mcp-call.jsonl", 0);
        let init = String::from_utf8(script.lines[0].clone()).unwrap();
        let to = if status == "failed" {
            r#""status":"failed""#
        } else {
            ""
        };
        script.lines[0] = init.replace(from, to).into_bytes();
        script.hang = true;
        let adapter = adapter(FakeRuntime::new(script));
        let run_id = format!("run-{status}");
        let mut context = context(&run_id);
        context.task_mcp = Some(binding(&dir));
        adapter
            .start_run(&fixtures::task_spec(), &context)
            .await
            .unwrap();
        let events = collect(&adapter, &run_id, 0).await;
        assert_eq!(check_run_events(&events), Ok(RunTermination::ProcessExited));
        let p = payloads(&events);
        assert!(matches!(&p[1], ProviderEventPayload::Error(e)
            if e.kind == FailureKind::Configuration
                && e.message.contains("telosweaver")
                && e.message.contains(status)));
        assert!(
            !p.iter()
                .any(|p| matches!(p, ProviderEventPayload::ToolStarted(_)))
        );
        assert!(p.iter().any(
            |p| matches!(p, ProviderEventPayload::TurnFinished(t) if t.status == TurnStatus::Failed)
        ));
        assert!(mcp_configs(&dir).is_empty());
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

// --- Capsule 입력 (P5, SB-11) ---

/// Core의 문맥 조립을 흉내 냅니다. Capsule Run이면 `capsuleVersionId`가 가리키는 본문을 프롬프트로 만듭니다.
struct ContextPrompt;

impl ClaudeLaunchResolver for ContextPrompt {
    fn resolve(&self, _: &TaskSpec, context: &RunContext) -> Result<ClaudeLaunch, AdapterError> {
        let mut resolved = launch();
        resolved.prompt = match (&context.session.mode, &context.session.capsule_version_id) {
            (SessionMode::Capsule, Some(id)) => {
                format!("[capsule {id}] goal: finish the report; decided: use the v2 outline")
            }
            _ => "fresh prompt".into(),
        };
        Ok(resolved)
    }
}

/// `capsule`은 새 세션입니다. 이전 세션을 쓰지 않고 Core가 만든 본문을 그대로 보냅니다. `fresh`와 달리 Capsule 참조가 필요합니다.
#[tokio::test]
async fn capsule_starts_a_new_session_with_the_capsule_body() {
    let prompt_of = |runtime: &FakeRuntime| {
        let message: serde_json::Value =
            serde_json::from_str(runtime.stdin_text().trim_end()).unwrap();
        message["message"]["content"].as_str().unwrap().to_owned()
    };
    let start = |mode, capsule: Option<&str>| {
        let capsule = capsule.map(str::to_owned);
        async move {
            let runtime = FakeRuntime::new(Script::fixture("success.jsonl", 0));
            let adapter = adapter_resolving(
                runtime.clone(),
                Arc::new(ContextPrompt),
                "2.1.310 (Claude Code)",
            );
            let mut context = context("run-capsule");
            context.session.mode = mode;
            // 인계 원본 세션이 적혀 있어도 Capsule·Fresh는 그 세션을 열지 않습니다.
            context.session.source_session_id = Some("previous-session".into());
            context.session.capsule_version_id = capsule;
            let result = adapter.start_run(&fixtures::task_spec(), &context).await;
            (runtime, result)
        }
    };

    let (runtime, result) = start(SessionMode::Capsule, Some("capv-7")).await;
    result.unwrap();
    let args = runtime.run_args(0);
    assert!(
        !args
            .iter()
            .any(|a| a == "--resume" || a == "--fork-session")
    );
    assert!(!args.iter().any(|a| a.contains("previous-session")));
    assert_eq!(
        prompt_of(&runtime),
        "[capsule capv-7] goal: finish the report; decided: use the v2 outline"
    );

    let (runtime, result) = start(SessionMode::Fresh, None).await;
    result.unwrap();
    assert!(!runtime.run_args(0).iter().any(|a| a == "--resume"));
    assert_eq!(prompt_of(&runtime), "fresh prompt");

    // Capsule 참조가 없는 Capsule 요청을 `fresh`처럼 실행하지 않습니다.
    let (runtime, result) = start(SessionMode::Capsule, None).await;
    assert!(matches!(
        result,
        Err(AdapterError::Failure { kind: FailureKind::Configuration, message })
            if message.contains("capsuleVersionId")
    ));
    assert!(runtime.spawned.lock().unwrap().is_empty());
}

// --- 실시간 승인 (L2·P6, SB-07) ---

fn approval_script(name: &str) -> Script {
    let mut script = Script::fixture(name, 0);
    // 실제 CLI처럼 승인 요청 뒤에는 응답을, `result` 뒤에는 stdin이 닫히기를 기다립니다.
    let request = script.index_of("can_use_tool");
    script.waits = vec![
        (request + 1, Wait::Stdin("control_response")),
        (script.lines.len(), Wait::StdinClosed),
    ];
    script
}

fn prompting(runtime: Arc<FakeRuntime>) -> ClaudeCodeAdapter {
    adapter(runtime).with_approvals(ClaudeApprovals::Prompt)
}

fn approval(events: &[ProviderEvent]) -> ApprovalRequiredPayload {
    match payloads(events).pop() {
        Some(ProviderEventPayload::ApprovalRequired(payload)) => payload,
        other => panic!("not an approval request: {other:?}"),
    }
}

fn control_responses(runtime: &FakeRuntime) -> Vec<serde_json::Value> {
    runtime
        .stdin_text()
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .filter(|v| v["type"] == "control_response")
        .collect()
}

#[tokio::test]
async fn live_approval_reports_target_and_digest_and_returns_allow() {
    let runtime = FakeRuntime::new(approval_script("approval-allow.jsonl"));
    let adapter = prompting(runtime.clone());
    assert_eq!(adapter.approvals(), ClaudeApprovals::Prompt);
    let task = fixtures::task_spec();
    adapter
        .start_run(&task, &context("run-allow"))
        .await
        .unwrap();
    let args = runtime.run_args(0);
    assert_eq!(
        value_after(&args, "--permission-prompt-tool"),
        Some("stdio")
    );

    let request =
        approval(&events_until(&adapter, "run-allow", ProviderEventKind::ApprovalRequired).await);
    assert_eq!(request.request_id, "0e24f100-04e9-4c66-beea-6b6ea5596162");
    assert_eq!(request.action, "file_change");
    assert!(request.answerable);
    assert!(request.summary.contains("Write"));
    // P6: 대상과 인자 Digest. Digest는 키 순서로 정렬한 압축 JSON의 SHA-256입니다.
    assert_eq!(request.target.as_deref(), Some(r"C:\work\probe.txt"));
    let canonical = br#"{"content":"OK","file_path":"C:\\work\\probe.txt"}"#;
    let hex: String = Sha256::digest(canonical)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(request.args_digest, Some(format!("sha256:{hex}")));
    // 답을 기다리는 동안 stdin은 열려 있고 Run은 진행 중입니다.
    assert!(!runtime.stdin_closed());
    assert_eq!(
        adapter.inspect_run("run-allow").await,
        Ok(RunInspection::Running)
    );

    // 모르는 요청 ID
    assert!(matches!(
        adapter.answer_approval("run-allow", "no-such-request", ApprovalAnswer::Allow).await,
        Err(AdapterError::Failure { kind: FailureKind::Configuration, message })
            if message.contains("unknown approval request")
    ));
    assert!(control_responses(&runtime).is_empty());

    adapter
        .answer_approval("run-allow", &request.request_id, ApprovalAnswer::Allow)
        .await
        .unwrap();
    // 같은 답의 반복은 다시 보내지 않고, 다른 답은 충돌입니다.
    assert_eq!(
        adapter
            .answer_approval("run-allow", &request.request_id, ApprovalAnswer::Allow)
            .await,
        Ok(())
    );
    assert!(matches!(
        adapter.answer_approval("run-allow", &request.request_id, ApprovalAnswer::Deny).await,
        Err(AdapterError::IdempotencyConflict { idempotency_key }) if idempotency_key == request.request_id
    ));

    let events = collect(&adapter, "run-allow", 0).await;
    assert_eq!(check_run_events(&events), Ok(RunTermination::ProcessExited));
    use ProviderEventKind::*;
    assert_eq!(
        kinds(&events),
        [
            SessionStarted,
            ToolStarted,
            ApprovalRequired,
            ToolCompleted,
            TextDelta,
            Usage,
            TurnFinished,
            ProcessExited
        ]
    );
    assert!(payloads(&events).iter().any(|p| matches!(p,
        ProviderEventPayload::ToolCompleted(t) if t.name == "Write" && t.outcome == ToolOutcome::Succeeded)));
    // 승인한 인자 그대로 실행하도록 돌려줍니다. 응답은 한 번만 썼습니다.
    let responses = control_responses(&runtime);
    assert_eq!(responses.len(), 1);
    assert_eq!(
        responses[0]["response"],
        serde_json::json!({
            "subtype": "success",
            "request_id": request.request_id,
            "response": {
                "behavior": "allow",
                "updatedInput": {"file_path": r"C:\work\probe.txt", "content": "OK"},
            },
        })
    );
    // `result` 뒤에 stdin을 닫아 CLI가 끝나게 합니다. 끝난 Run의 요청에는 같은 답만 재확인됩니다.
    assert!(runtime.stdin_closed());
    assert_eq!(
        adapter
            .answer_approval("run-allow", &request.request_id, ApprovalAnswer::Allow)
            .await,
        Ok(())
    );
}

#[tokio::test]
async fn live_approval_deny_is_returned_and_marks_the_tool_denied() {
    let runtime = FakeRuntime::new(approval_script("approval-deny.jsonl"));
    let adapter = prompting(runtime.clone());
    adapter
        .start_run(&fixtures::task_spec(), &context("run-deny"))
        .await
        .unwrap();
    let request =
        approval(&events_until(&adapter, "run-deny", ProviderEventKind::ApprovalRequired).await);
    adapter
        .answer_approval("run-deny", &request.request_id, ApprovalAnswer::Deny)
        .await
        .unwrap();
    let events = collect(&adapter, "run-deny", 0).await;
    assert_eq!(check_run_events(&events), Ok(RunTermination::ProcessExited));
    // CLI는 이 거부를 `is_error` 결과로만 알립니다. 승인 응답을 근거로 `denied`로 보고합니다.
    assert!(payloads(&events).iter().any(|p| matches!(p,
        ProviderEventPayload::ToolCompleted(t) if t.name == "Write" && t.outcome == ToolOutcome::Denied)));
    let responses = control_responses(&runtime);
    assert_eq!(responses.len(), 1);
    assert_eq!(
        responses[0]["response"]["request_id"],
        request.request_id.as_str()
    );
    assert_eq!(responses[0]["response"]["response"]["behavior"], "deny");
    assert!(
        responses[0]["response"]["response"]
            .get("updatedInput")
            .is_none()
    );
}

/// 답하기 전에 Run을 취소하면 요청은 닫히고, 그 뒤의 답은 CLI로 가지 않습니다.
#[tokio::test]
async fn cancel_before_the_answer_closes_the_approval_request() {
    let mut script = Script::fixture("approval-allow.jsonl", 0);
    script.lines.truncate(script.index_of("can_use_tool") + 1);
    script.hang = true;
    let runtime = FakeRuntime::new(script);
    let adapter = prompting(runtime.clone());
    adapter
        .start_run(&fixtures::task_spec(), &context("run-cancel"))
        .await
        .unwrap();
    let request =
        approval(&events_until(&adapter, "run-cancel", ProviderEventKind::ApprovalRequired).await);
    assert_eq!(adapter.cancel_run("run-cancel").await, Ok(true));
    let events = collect(&adapter, "run-cancel", 0).await;
    assert_eq!(check_run_events(&events), Ok(RunTermination::ProcessExited));
    let p = payloads(&events);
    assert!(matches!(&p[p.len() - 2],
        ProviderEventPayload::TurnFinished(t) if t.status == TurnStatus::Cancelled));
    assert!(
        !p.iter()
            .any(|p| matches!(p, ProviderEventPayload::ToolCompleted(_)))
    );
    for decision in [ApprovalAnswer::Allow, ApprovalAnswer::Deny] {
        assert!(matches!(
            adapter.answer_approval("run-cancel", &request.request_id, decision).await,
            Err(AdapterError::Failure { kind: FailureKind::Configuration, message })
                if message.contains("no longer pending")
        ));
    }
    assert!(control_responses(&runtime).is_empty());
    // 프로세스를 끝내기 전에 Turn을 끊는 제어 요청을 보냈습니다.
    assert!(runtime.stdin_text().contains(r#""subtype":"interrupt""#));
}

/// 승인 대기 중 취소: `interrupt`를 받은 CLI가 요청을 거두고 Turn을 끝냅니다(2.1.325 출력 재생).
/// 끊긴 Turn은 Provider 오류가 아니라 취소로, 실행되지 않은 도구는 `denied`로 보고합니다.
#[tokio::test]
async fn cancel_interrupts_the_turn_that_waits_for_an_approval() {
    let mut script = Script::fixture("approval-interrupt.jsonl", 1);
    script.waits = vec![
        (
            script.index_of("control_cancel_request"),
            Wait::Stdin("interrupt"),
        ),
        (script.lines.len(), Wait::StdinClosed),
    ];
    let runtime = FakeRuntime::new(script);
    let adapter = prompting(runtime.clone());
    adapter
        .start_run(&fixtures::task_spec(), &context("run-interrupt"))
        .await
        .unwrap();
    let request = approval(
        &events_until(
            &adapter,
            "run-interrupt",
            ProviderEventKind::ApprovalRequired,
        )
        .await,
    );
    assert_eq!(adapter.cancel_run("run-interrupt").await, Ok(true));
    let events = collect(&adapter, "run-interrupt", 0).await;
    assert_eq!(check_run_events(&events), Ok(RunTermination::ProcessExited));
    use ProviderEventKind::*;
    assert_eq!(
        kinds(&events),
        [
            SessionStarted,
            ToolStarted,
            ApprovalRequired,
            ToolCompleted,
            Usage,
            TurnFinished,
            ProcessExited
        ]
    );
    let p = payloads(&events);
    assert!(
        matches!(&p[3], ProviderEventPayload::ToolCompleted(t) if t.outcome == ToolOutcome::Denied)
    );
    assert!(matches!(&p[5], ProviderEventPayload::TurnFinished(t)
        if t.status == TurnStatus::Cancelled && t.reason.as_deref() == Some("aborted_tools")));
    assert!(matches!(&p[6], ProviderEventPayload::ProcessExited(e)
        if e.exit_code == Some(1) && e.termination == ProcessTermination::Exited));
    assert!(matches!(
        adapter
            .answer_approval("run-interrupt", &request.request_id, ApprovalAnswer::Allow)
            .await,
        Err(AdapterError::Failure {
            kind: FailureKind::Configuration,
            ..
        })
    ));
    assert!(control_responses(&runtime).is_empty());
}

/// CLI가 요청을 거두면 더 답할 수 없습니다. Adapter가 모르는 제어 요청에는 오류로 답해 CLI가 기다리지 않게 합니다.
#[tokio::test]
async fn withdrawn_and_unsupported_control_requests() {
    let mut script = Script::lines(&[
        r#"{"type":"system","subtype":"init","session_id":"s-1","claude_code_version":"2.1.325"}"#,
        r#"{"type":"control_request","request_id":"req-1","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"git push"},"tool_use_id":"toolu_1"}}"#,
        r#"{"type":"control_cancel_request","request_id":"req-1"}"#,
        r#"{"type":"control_request","request_id":"req-2","request":{"subtype":"hook_callback","callback_id":"x"}}"#,
        r#"{"type":"control_request","request_id":"req-3","request":{"subtype":"can_use_tool","tool_name":"mcp__other__send","input":{"b":1,"a":[true,null]},"tool_use_id":"toolu_3"}}"#,
        r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"waiting"}]}}"#,
    ]);
    script.hang = true;
    let runtime = FakeRuntime::new(script);
    let adapter = prompting(runtime.clone());
    adapter
        .start_run(&fixtures::task_spec(), &context("run-control"))
        .await
        .unwrap();
    let events = events_until(&adapter, "run-control", ProviderEventKind::TextDelta).await;
    let requests: Vec<_> = payloads(&events)
        .into_iter()
        .filter_map(|p| match p {
            ProviderEventPayload::ApprovalRequired(a) => Some(a),
            _ => None,
        })
        .collect();
    assert_eq!(requests.len(), 2);
    // 명령 실행은 명령이 대상이고, 대상 인자가 없는 MCP 도구는 도구 이름이 대상입니다.
    assert_eq!(
        (requests[0].action.as_str(), requests[0].target.as_deref()),
        ("command_execution", Some("git push"))
    );
    assert_eq!(
        (requests[1].action.as_str(), requests[1].target.as_deref()),
        ("mcp_tool_call", Some("mcp__other__send"))
    );
    let hex: String = Sha256::digest(br#"{"a":[true,null],"b":1}"#)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(requests[1].args_digest, Some(format!("sha256:{hex}")));

    assert!(matches!(
        adapter.answer_approval("run-control", "req-1", ApprovalAnswer::Allow).await,
        Err(AdapterError::Failure { kind: FailureKind::Configuration, message })
            if message.contains("no longer pending")
    ));
    let responses = control_responses(&runtime);
    assert_eq!(responses.len(), 1);
    assert_eq!(
        responses[0]["response"],
        serde_json::json!({"subtype": "error", "request_id": "req-2", "error": "unsupported control request"})
    );
    adapter.cancel_run("run-control").await.unwrap();
    assert_eq!(
        check_run_events(&collect(&adapter, "run-control", 0).await),
        Ok(RunTermination::ProcessExited)
    );
}

/// 승인 경로가 없는 Run에 승인 요청이 오면 답할 수 없으므로 정상 메시지로 넘기지 않고 Run을 끝냅니다.
#[tokio::test]
async fn approval_request_without_live_approval_ends_the_run() {
    let mut script = Script::fixture("approval-allow.jsonl", 0);
    script.hang = true;
    let runtime = FakeRuntime::new(script);
    let adapter = adapter(runtime.clone());
    let events = run_to_end(&adapter).await;
    assert_eq!(check_run_events(&events), Ok(RunTermination::ProcessExited));
    let p = payloads(&events);
    assert!(p.iter().any(
        |p| matches!(p, ProviderEventPayload::Error(e) if e.kind == FailureKind::ProtocolParse)
    ));
    assert!(
        !p.iter()
            .any(|p| matches!(p, ProviderEventPayload::ApprovalRequired(_)))
    );
    assert!(control_responses(&runtime).is_empty());
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

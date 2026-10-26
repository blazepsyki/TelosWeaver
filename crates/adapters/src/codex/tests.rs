//! Provider-local tests until REQ-012 exposes the configuration module (Spec §19.5).
use super::*;
use crate::runtime::*;
use std::{
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
};
use tokio::sync::mpsc;

#[derive(Clone, Copy, Default)]
enum Mode {
    #[default]
    Good,
    Hold,
    Approval,
    BadJson,
    StartError,
    ExitUnknown,
    IgnoreInterrupt,
    Steer,
    Permissions,
    NonInteractive,
}
#[derive(Default)]
struct MockRuntime {
    spawns: AtomicUsize,
    mode: Mode,
    requests: Arc<Mutex<Vec<Value>>>,
}
struct MockHandle {
    id: ProcessIdentity,
    output: Mutex<Option<mpsc::UnboundedReceiver<OutputEvent>>>,
    sender: mpsc::UnboundedSender<OutputEvent>,
    requests: Arc<Mutex<Vec<Value>>>,
    mode: Mode,
    thread: Mutex<String>,
    closed: AtomicUsize,
}
#[async_trait]
impl ProcessRuntime for MockRuntime {
    async fn spawn(&self, _: CommandSpec) -> Result<Arc<dyn ProcessHandle>, RuntimeError> {
        self.spawns.fetch_add(1, Ordering::Relaxed);
        let (sender, output) = mpsc::unbounded_channel();
        Ok(Arc::new(MockHandle {
            id: ProcessIdentity {
                pid: 1,
                started_at: "fixture".into(),
            },
            output: Mutex::new(Some(output)),
            sender,
            requests: self.requests.clone(),
            mode: self.mode,
            thread: Mutex::new("thread-fixture".into()),
            closed: AtomicUsize::new(0),
        }))
    }
}
impl MockHandle {
    fn emit(&self, value: Value) {
        let _ = self.sender.send(OutputEvent::Line {
            stream: OutputStream::Stdout,
            sequence: 1,
            data: serde_json::to_vec(&value).unwrap(),
        });
    }
    fn complete(&self, status: &str) {
        self.emit(json!({"method":"turn/completed","params":{"threadId":*self.thread.lock().unwrap(),"turn":{"id":"turn-fixture","status":status,"error":null}}}));
    }
    fn event(&self, method: &str, extra: Value) {
        let mut params = extra;
        params["threadId"] = json!(*self.thread.lock().unwrap());
        params["turnId"] = json!("turn-fixture");
        self.emit(json!({"method":method,"params":params}));
    }
}
#[async_trait]
impl ProcessHandle for MockHandle {
    fn identity(&self) -> &ProcessIdentity {
        &self.id
    }
    fn take_output(&self) -> Result<BoxStream<'static, OutputEvent>, RuntimeError> {
        let output = self
            .output
            .lock()
            .unwrap()
            .take()
            .ok_or(RuntimeError::OutputAlreadyTaken)?;
        Ok(futures::stream::unfold(output, |mut output| async move {
            output.recv().await.map(|event| (event, output))
        })
        .boxed())
    }
    async fn write_stdin(&self, data: &[u8]) -> Result<(), RuntimeError> {
        let request: Value = serde_json::from_slice(data).unwrap();
        self.requests.lock().unwrap().push(request.clone());
        let id = request["id"].clone();
        let params = &request["params"];
        match request["method"].as_str(){
            Some("initialize")=>self.emit(json!({"id":id,"result":{}})),
            Some("initialized")=>{},
            Some("config/read")=>self.emit(json!({"id":id,"result":{"config":{"mcp_servers":{"fixture":{"credential":"fixture-secret"}}}}})),
            Some("thread/start"|"thread/resume"|"thread/fork")=>{
                if matches!(self.mode,Mode::StartError){self.emit(json!({"id":id,"error":{"code":-32602,"message":"invalid model fixture-secret"}}));}
                else {let thread=match request["method"].as_str().unwrap(){"thread/resume"=>params["threadId"].as_str().unwrap().to_owned(),"thread/fork"=>"fork-fixture".into(),_=>"thread-fixture".into()};*self.thread.lock().unwrap()=thread.clone();let source=if matches!(self.mode,Mode::NonInteractive){"appServer"}else{"vscode"};self.emit(json!({"id":id,"result":{"thread":{"id":thread,"source":source}}}));}
            }
            Some("turn/start")=>{
                self.emit(json!({"id":id,"result":{"turn":{"id":"turn-fixture","status":"inProgress"}}}));
                match self.mode {
                    Mode::Hold|Mode::IgnoreInterrupt|Mode::Steer=>{},
                    Mode::BadJson=>{let _=self.sender.send(OutputEvent::Line {stream:OutputStream::Stdout,sequence:1,data:b"{".to_vec()});},
                    Mode::Approval|Mode::Permissions=>{
                        self.event("item/started",json!({"item":{"type":"fileChange","id":"tool-fixture"}}));
                        self.emit(json!({"id":900,"method":if matches!(self.mode,Mode::Permissions){"item/permissions/requestApproval"}else{"item/fileChange/requestApproval"},"params":{"threadId":*self.thread.lock().unwrap(),"turnId":"turn-fixture","itemId":"tool-fixture"}}));
                    }
                    _=>{
                        self.event("item/agentMessage/delta",json!({"itemId":"message-fixture","delta":"ok"}));
                        self.event("item/completed",json!({"item":{"type":"agentMessage","id":"message-fixture","text":"ok"}}));
                        self.event("thread/tokenUsage/updated",json!({"tokenUsage":{"last":{"inputTokens":5,"outputTokens":1,"cachedInputTokens":2},"total":{"inputTokens":5,"outputTokens":1,"cachedInputTokens":2}}}));self.complete("completed");
                    }
                }
            }
            Some("turn/interrupt")=>{self.emit(json!({"id":id,"result":{}}));if matches!(self.mode,Mode::Hold){self.complete("interrupted");}},
            Some("turn/steer")=>{self.emit(json!({"id":id,"result":{"turnId":"turn-fixture"}}));self.event("item/agentMessage/delta",json!({"itemId":"message-fixture","delta":"steered"}));self.complete("completed");},
            Some("thread/archive")=>self.emit(json!({"id":id,"result":{}})),
            None if id==900=>{let denied=params.is_null()&&request.pointer("/result/decision").and_then(Value::as_str)!=Some("accept");self.event("item/completed",json!({"item":{"type":"fileChange","id":"tool-fixture","status":if denied{"declined"}else{"completed"}}}));self.complete("completed");},
            _=>panic!("unexpected fixture request"),
        }
        Ok(())
    }
    async fn close_stdin(&self) -> Result<(), RuntimeError> {
        if self.closed.fetch_add(1, Ordering::Relaxed) == 0 {
            let _ = self.sender.send(OutputEvent::Closed {
                stream: OutputStream::Stdout,
                sequence: 2,
            });
        }
        Ok(())
    }
    async fn wait(&self) -> Result<ExitStatus, RuntimeError> {
        Ok(ExitStatus {
            code: Some(0),
            cause: TerminationCause::Exited,
        })
    }
    async fn terminate(&self, _: Duration) -> Result<TerminationReport, RuntimeError> {
        self.close_stdin().await?;
        if matches!(self.mode, Mode::ExitUnknown) {
            return Err(RuntimeError::TerminationUnconfirmed {
                message: "fixture-secret".into(),
            });
        }
        Ok(TerminationReport {
            status: self.wait().await?,
            children: ChildrenReaped::All,
        })
    }
}
struct Resolver {
    cwd: PathBuf,
    approvals: CodexApprovals,
}
impl CodexLaunchResolver for Resolver {
    fn resolve(&self, _: &TaskSpec, _: &RunContext) -> Result<CodexLaunch, AdapterError> {
        let mut launch = CodexLaunch::read_only(
            self.cwd.clone(),
            "fixture prompt".into(),
            "fixture-model".into(),
        );
        launch.approvals = self.approvals;
        Ok(launch)
    }
}
fn configured(mode: Mode, approvals: CodexApprovals) -> (CodexAdapter, Arc<MockRuntime>) {
    configured_listing(mode, approvals, CodexVscodeListing::default())
}
fn configured_listing(
    mode: Mode,
    approvals: CodexApprovals,
    listing: CodexVscodeListing,
) -> (CodexAdapter, Arc<MockRuntime>) {
    let runtime = Arc::new(MockRuntime {
        mode,
        ..Default::default()
    });
    (
        CodexAdapter::with_vscode_listing(
            CodexConfig {
                executable: "fixture.exe".into(),
                cli_version: "fixture-version".into(),
                host_id: fixtures::HOST_ID.into(),
                runtime: runtime.clone(),
                launcher: Arc::new(Resolver {
                    cwd: std::env::temp_dir(),
                    approvals,
                }),
                request_timeout: Duration::from_secs(1),
                cancel_grace: Duration::from_millis(20),
            },
            listing,
        ),
        runtime,
    )
}
fn inputs() -> (TaskSpec, RunContext) {
    let task = fixtures::task_spec();
    let context = fixtures::run_context(&task);
    (task, context)
}

// Spec §7.6, DEC-08: defaults preserve visible, persistent threads across session modes.
#[tokio::test]
async fn listed_defaults_persist_creation_and_fork_without_reclassifying_resume() {
    assert_eq!(CodexVscodeListing::default(), CodexVscodeListing::Listed);
    for mode in [
        SessionMode::Fresh,
        SessionMode::Capsule,
        SessionMode::Resume,
        SessionMode::NativeFork,
    ] {
        let (adapter, runtime) = configured(Mode::Good, CodexApprovals::Deny);
        let (task, mut context) = inputs();
        context.session.mode = mode;
        if matches!(mode, SessionMode::Resume | SessionMode::NativeFork) {
            context.session.source_session_id = Some("source-fixture".into());
        }
        adapter.start_run(&task, &context).await.unwrap();
        collect(&adapter, &context.run_id).await;
        let requests = runtime.requests.lock().unwrap();
        let method = match mode {
            SessionMode::Resume => "thread/resume",
            SessionMode::NativeFork => "thread/fork",
            _ => "thread/start",
        };
        let params = &requests.iter().find(|r| r["method"] == method).unwrap()["params"];
        if mode == SessionMode::Resume {
            assert!(params.get("threadSource").is_none());
            assert!(params.get("ephemeral").is_none());
        } else {
            assert_eq!(params["ephemeral"], false);
            assert_eq!(params["threadSource"], "vscode");
        }
        assert_eq!(params["cwd"], std::env::temp_dir().to_str().unwrap());
        assert_eq!(params["sandbox"], "read-only");
        assert_eq!(params["approvalPolicy"], "on-request");
    }
}

#[tokio::test]
async fn hidden_is_explicitly_unsupported_and_never_starts_a_process() {
    let (adapter, runtime) =
        configured_listing(Mode::Good, CodexApprovals::Deny, CodexVscodeListing::Hidden);
    let capability = adapter
        .detect(fixtures::HOST_ID)
        .await
        .unwrap()
        .remove(capability_keys::VSCODE_LISTING)
        .unwrap();
    assert_eq!(capability.state, CapabilityState::Unsupported);
    let (task, context) = inputs();
    assert!(matches!(
        adapter.start_run(&task, &context).await,
        Err(AdapterError::UnsupportedCapability { capability, .. }) if capability == capability_keys::VSCODE_LISTING
    ));
    assert_eq!(runtime.spawns.load(Ordering::SeqCst), 0);
    assert!(adapter.owned_thread_ids().is_empty());
}

#[tokio::test]
async fn unlisted_native_sources_fail_before_generation_and_preserve_source_ownership() {
    for mode in [SessionMode::Fresh, SessionMode::Resume] {
        let (adapter, runtime) = configured(Mode::NonInteractive, CodexApprovals::Deny);
        let (task, mut context) = inputs();
        context.session.mode = mode;
        context.session.source_session_id = Some("source-fixture".into());
        assert!(matches!(
            adapter.start_run(&task, &context).await,
            Err(AdapterError::UnsupportedCapability { capability, .. }) if capability == capability_keys::VSCODE_LISTING
        ));
        let requests = runtime.requests.lock().unwrap();
        assert!(!requests.iter().any(|r| r["method"] == "turn/start"));
        assert_eq!(
            requests.iter().any(|r| r["method"] == "thread/archive"),
            mode == SessionMode::Fresh
        );
    }
}

#[tokio::test]
async fn listing_capability_does_not_claim_an_unverified_cli_or_extension() {
    let (adapter, _) = configured(Mode::Good, CodexApprovals::Deny);
    let capabilities = adapter.detect(fixtures::HOST_ID).await.unwrap();
    assert_eq!(
        capabilities[capability_keys::VSCODE_LISTING].state,
        CapabilityState::Unverified
    );
    assert_eq!(
        capabilities[capability_keys::VSCODE_LISTING].evidence_level,
        EvidenceLevel::MockTested
    );
}

// Spec §7.1, REQ-024: key normalization must not promote an unverified configuration.
#[tokio::test]
async fn detect_reports_common_keys_and_preserves_fixture_judgements() {
    let (adapter, _) = configured(Mode::Good, CodexApprovals::Deny);
    let capabilities = adapter.detect(fixtures::HOST_ID).await.unwrap();
    let reported: std::collections::HashSet<_> = capabilities.keys().map(String::as_str).collect();
    assert_eq!(reported, capability_keys::ALL.iter().copied().collect());
    for capability in capabilities.values() {
        assert_eq!(capability.state, CapabilityState::Unverified);
        assert_eq!(capability.evidence_level, EvidenceLevel::MockTested);
        assert_eq!(capability.provider_version, "fixture-version");
        assert_eq!(capability.host_id, fixtures::HOST_ID);
    }
}

// Spec §7.1, C3-X1/X2: evidence is scoped to an exact platform/CLI/extension tuple.
#[tokio::test]
async fn capability_evidence_matrix_distinguishes_platform_version_and_listing_identity() {
    let (mut adapter, _) = configured(Mode::Good, CodexApprovals::Deny);
    Arc::get_mut(&mut adapter.inner)
        .unwrap()
        .config
        .as_mut()
        .unwrap()
        .cli_version = "codex-cli 0.164.0-alpha.11.1".into();
    let config = adapter.config().unwrap();
    for platform in ["win32-x64", "linux-x64", "darwin-arm64"] {
        let extension = capabilities::ExtensionIdentity {
            id: "openai.chatgpt".into(),
            version: "26.1021.22027".into(),
            platform: platform.into(),
        };
        let reported = capabilities::with_evidence(
            config,
            fixtures::HOST_ID,
            platform,
            CodexVscodeListing::Listed,
            Some(&extension),
        );
        let known = matches!(platform, "win32-x64" | "linux-x64");
        for key in [
            capability_keys::MANAGED_RUN,
            capability_keys::EVENT_STREAM,
            capability_keys::CANCEL,
            capability_keys::NATIVE_RESUME,
            capability_keys::NATIVE_FORK,
            capability_keys::VSCODE_LISTING,
        ] {
            assert_eq!(
                reported[key].state,
                if known {
                    CapabilityState::Supported
                } else {
                    CapabilityState::Unverified
                },
                "{platform} {key}"
            );
            assert_eq!(
                reported[key].evidence_level,
                if known {
                    EvidenceLevel::EnvironmentTested
                } else {
                    EvidenceLevel::MockTested
                }
            );
        }
        if known {
            let reason = reported[capability_keys::VSCODE_LISTING]
                .reason
                .as_deref()
                .unwrap();
            assert!(reason.contains(if platform == "win32-x64" {
                "user-confirmed"
            } else {
                "UI listing/opening and live refresh unverified"
            }));
        }
        for key in [
            capability_keys::SEND_INPUT,
            capability_keys::ANSWER_APPROVAL,
        ] {
            assert_eq!(
                reported[key].state,
                if platform == "win32-x64" {
                    CapabilityState::Supported
                } else {
                    CapabilityState::Unverified
                }
            );
        }
        for invalid in [
            None,
            Some(capabilities::ExtensionIdentity {
                id: "other.extension".into(),
                ..extension.clone()
            }),
            Some(capabilities::ExtensionIdentity {
                version: "unknown-version".into(),
                ..extension.clone()
            }),
            Some(capabilities::ExtensionIdentity {
                platform: "wrong-platform".into(),
                ..extension.clone()
            }),
        ] {
            let caps = capabilities::with_evidence(
                config,
                fixtures::HOST_ID,
                platform,
                CodexVscodeListing::Listed,
                invalid.as_ref(),
            );
            assert_eq!(
                caps[capability_keys::VSCODE_LISTING].state,
                CapabilityState::Unverified
            );
        }
    }
    let (adapter, _) = configured(Mode::Good, CodexApprovals::Deny);
    let unknown = capabilities::with_evidence(
        adapter.config().unwrap(),
        fixtures::HOST_ID,
        "win32-x64",
        CodexVscodeListing::Listed,
        None,
    );
    assert!(
        unknown
            .values()
            .all(|cap| cap.state == CapabilityState::Unverified)
    );
}

#[test]
fn listing_extension_identity_uses_manifest_instead_of_directory_spelling() {
    struct Temporary(PathBuf);
    impl Drop for Temporary {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let root = Temporary(std::env::temp_dir().join(format!(
            "telos-codex-manifest-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )));
    let binary = root.0.join("renamed-extension/bin/native/codex.exe");
    std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
    std::fs::write(&binary, []).unwrap();
    let package = root.0.join("renamed-extension/package.json");
    std::fs::write(&package, json!({"publisher":"openai","name":"chatgpt","version":"26.1021.22027","__metadata":{"targetPlatform":"win32-x64"}}).to_string()).unwrap();
    let identity = capabilities::extension_identity(&binary).unwrap();
    assert_eq!(
        (
            identity.id.as_str(),
            identity.version.as_str(),
            identity.platform.as_str()
        ),
        ("openai.chatgpt", "26.1021.22027", "win32-x64")
    );
    std::fs::write(&package, json!({"publisher":"other","name":"chatgpt","version":"26.1021.22027","__metadata":{"targetPlatform":"win32-x64"}}).to_string()).unwrap();
    assert!(capabilities::extension_identity(&binary).is_none());
    std::fs::write(
        &package,
        json!({"publisher":"openai","name":"chatgpt","version":"26.1021.22027"}).to_string(),
    )
    .unwrap();
    assert!(capabilities::extension_identity(&binary).is_none());
}

#[tokio::test]
async fn request_specific_refusals_use_common_keys_and_keep_their_reasons() {
    assert!(matches!(
        CodexAdapter::default().detect(fixtures::HOST_ID).await,
        Err(AdapterError::UnsupportedCapability { capability, .. }) if capability == capability_keys::MANAGED_RUN
    ));
    let (adapter, runtime) = configured(Mode::Good, CodexApprovals::Deny);
    let (task, mut context) = inputs();
    context.session.mode = SessionMode::Resume;
    context.session.source_session_id = Some("source-fixture".into());
    context.session.source_turn_id = Some("turn-fixture".into());
    assert!(matches!(
        adapter.start_run(&task, &context).await,
        Err(AdapterError::UnsupportedCapability { capability, reason })
            if capability == capability_keys::NATIVE_RESUME && reason.contains("specific turn")
    ));
    assert!(
        !runtime
            .requests
            .lock()
            .unwrap()
            .iter()
            .any(|r| r["method"] == "turn/start")
    );

    let (adapter, _) = configured(Mode::Hold, CodexApprovals::Deny);
    let (task, context) = inputs();
    adapter.start_run(&task, &context).await.unwrap();
    assert!(matches!(
        adapter.answer_approval(&context.run_id, "not-interactive", ApprovalAnswer::Allow).await,
        Err(AdapterError::UnsupportedCapability { capability, reason })
            if capability == capability_keys::ANSWER_APPROVAL && reason.contains("does not permit interactive approval")
    ));
    adapter.cancel_run(&context.run_id).await.unwrap();
    assert!(check_run_events(&collect(&adapter, &context.run_id).await).is_ok());
}

async fn collect(adapter: &CodexAdapter, id: &str) -> Vec<ProviderEvent> {
    tokio::time::timeout(
        Duration::from_secs(3),
        adapter.events(id, 0).map(Result::unwrap).collect(),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn managed_events_replay_and_start_are_idempotent() {
    let (adapter, runtime) = configured(Mode::Good, CodexApprovals::Deny);
    let (task, context) = inputs();
    let (first, second) = tokio::join!(
        adapter.start_run(&task, &context),
        adapter.start_run(&task, &context)
    );
    assert_eq!(first.unwrap(), second.unwrap());
    assert_eq!(runtime.spawns.load(Ordering::Relaxed), 1);
    let events = collect(&adapter, &context.run_id).await;
    assert_eq!(check_run_events(&events), Ok(RunTermination::ProcessExited));
    assert_eq!(
        events
            .iter()
            .filter(|e| e.kind == ProviderEventKind::TextDelta)
            .count(),
        1
    );
    assert!(
        events
            .iter()
            .all(|e| e.native_turn_id.as_deref() == Some("turn-fixture"))
    );
    let replay = adapter
        .events(&context.run_id, 2)
        .map(Result::unwrap)
        .collect::<Vec<_>>()
        .await;
    assert_eq!(replay, events[2..]);
    let mut changed = task.clone();
    changed.objective = "changed".into();
    assert!(matches!(
        adapter.start_run(&changed, &context).await,
        Err(AdapterError::IdempotencyConflict { .. })
    ));
    let requests = runtime.requests.lock().unwrap();
    let start = requests
        .iter()
        .find(|r| r["method"] == "thread/start")
        .unwrap();
    assert_eq!(
        start["params"]["config"]["mcp_servers.fixture.enabled"],
        false
    );
    assert!(!start.to_string().contains("fixture-secret"));
}
#[tokio::test]
async fn pre_start_error_has_no_events_and_does_not_leak_diagnostics() {
    let (adapter, _) = configured(Mode::StartError, CodexApprovals::Deny);
    let (task, context) = inputs();
    let error = adapter.start_run(&task, &context).await.unwrap_err();
    assert_eq!(error.failure_kind(), Some(FailureKind::Configuration));
    assert!(!error.to_string().contains("fixture-secret"));
    assert!(matches!(
        adapter.events(&context.run_id, 0).next().await,
        Some(Err(AdapterError::UnknownRun { .. }))
    ));
}
#[tokio::test]
async fn protocol_failure_and_unknown_exit_are_not_success() {
    for mode in [Mode::BadJson, Mode::ExitUnknown] {
        let (adapter, _) = configured(mode, CodexApprovals::Deny);
        let (task, context) = inputs();
        adapter.start_run(&task, &context).await.unwrap();
        let events = collect(&adapter, &context.run_id).await;
        assert!(check_run_events(&events).is_ok());
        assert!(events.iter().any(|e| e.kind == ProviderEventKind::Error));
        if matches!(mode, Mode::ExitUnknown) {
            assert_eq!(check_run_events(&events), Ok(RunTermination::TerminalError));
            assert_eq!(
                adapter.inspect_run(&context.run_id).await.unwrap(),
                RunInspection::Interrupted
            );
        }
    }
}
#[tokio::test]
async fn cancellation_is_shared_and_confirms_teardown() {
    for mode in [Mode::Hold, Mode::IgnoreInterrupt] {
        let (adapter, runtime) = configured(mode, CodexApprovals::Deny);
        let (task, context) = inputs();
        adapter.start_run(&task, &context).await.unwrap();
        let (a, b) = tokio::join!(
            adapter.cancel_run(&context.run_id),
            adapter.cancel_run(&context.run_id)
        );
        assert!(a.unwrap() && b.unwrap());
        let events = collect(&adapter, &context.run_id).await;
        assert_eq!(check_run_events(&events), Ok(RunTermination::ProcessExited));
        assert!(
            events
                .iter()
                .any(|e| e.payload.get("status") == Some(&json!("cancelled")))
        );
        assert!(!adapter.cancel_run(&context.run_id).await.unwrap());
        assert_eq!(
            runtime
                .requests
                .lock()
                .unwrap()
                .iter()
                .filter(|r| r["method"] == "turn/interrupt")
                .count(),
            1
        );
    }
}
#[tokio::test]
async fn denied_tools_and_permission_escalations_do_not_receive_grants() {
    for mode in [Mode::Approval, Mode::Permissions] {
        let (adapter, _) = configured(mode, CodexApprovals::Deny);
        let (task, context) = inputs();
        adapter.start_run(&task, &context).await.unwrap();
        let events = collect(&adapter, &context.run_id).await;
        assert_eq!(check_run_events(&events), Ok(RunTermination::ProcessExited));
        assert!(
            events
                .iter()
                .any(|e| e.payload.get("outcome") == Some(&json!("denied")))
        );
    }
}
#[tokio::test]
async fn interactive_approval_is_correlated_and_repeated_answers_are_idempotent() {
    let (adapter, _) = configured(Mode::Approval, CodexApprovals::Prompt);
    let (task, context) = inputs();
    adapter.start_run(&task, &context).await.unwrap();
    let mut stream = adapter.events(&context.run_id, 0);
    let request = loop {
        let event = stream.next().await.unwrap().unwrap();
        if event.kind == ProviderEventKind::ApprovalRequired {
            break event.payload["requestId"].as_str().unwrap().to_owned();
        }
    };
    assert!(matches!(
        adapter
            .answer_approval(&context.run_id, &request, ApprovalAnswer::Allow)
            .await,
        Err(AdapterError::UnsupportedCapability { capability, .. }) if capability == capability_keys::ANSWER_APPROVAL
    ));
    adapter
        .answer_approval(&context.run_id, &request, ApprovalAnswer::Deny)
        .await
        .unwrap();
    adapter
        .answer_approval(&context.run_id, &request, ApprovalAnswer::Deny)
        .await
        .unwrap();
    assert!(matches!(
        adapter
            .answer_approval(&context.run_id, &request, ApprovalAnswer::Allow)
            .await,
        Err(AdapterError::IdempotencyConflict { .. })
    ));
    assert!(check_run_events(&collect(&adapter, &context.run_id).await).is_ok());
}
#[tokio::test]
async fn steering_is_scoped_and_input_keys_do_not_duplicate_it() {
    let (adapter, runtime) = configured(Mode::Steer, CodexApprovals::Deny);
    let (task, context) = inputs();
    adapter.start_run(&task, &context).await.unwrap();
    let (a, b) = tokio::join!(
        adapter.send_input(&context.run_id, "input-1", "next"),
        adapter.send_input(&context.run_id, "input-1", "next")
    );
    a.unwrap();
    b.unwrap();
    assert!(matches!(
        adapter
            .send_input(&context.run_id, "input-1", "changed")
            .await,
        Err(AdapterError::IdempotencyConflict { .. })
    ));
    collect(&adapter, &context.run_id).await;
    assert_eq!(
        runtime
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r["method"] == "turn/steer")
            .count(),
        1
    );
}
#[tokio::test]
async fn resume_fork_and_archive_preserve_thread_ownership() {
    for mode in [SessionMode::Resume, SessionMode::NativeFork] {
        let (adapter, runtime) = configured(Mode::Good, CodexApprovals::Deny);
        let (task, mut context) = inputs();
        context.session.mode = mode;
        context.session.source_session_id = Some("source-fixture".into());
        if mode == SessionMode::NativeFork {
            context.session.source_turn_id = Some("parent-turn".into());
        }
        let started = adapter.start_run(&task, &context).await.unwrap();
        assert_eq!(
            started.native_session_id.as_deref(),
            Some(if mode == SessionMode::Resume {
                "source-fixture"
            } else {
                "fork-fixture"
            })
        );
        collect(&adapter, &context.run_id).await;
        if mode == SessionMode::NativeFork {
            {
                let requests = runtime.requests.lock().unwrap();
                assert_eq!(
                    requests
                        .iter()
                        .find(|r| r["method"] == "thread/fork")
                        .unwrap()["params"]["lastTurnId"],
                    "parent-turn"
                );
            }
            adapter
                .archive("fork-fixture", std::env::temp_dir())
                .await
                .unwrap();
            assert!(adapter.owned_thread_ids().is_empty());
        }
        assert!(
            adapter
                .archive("source-fixture", std::env::temp_dir())
                .await
                .is_err()
        );
    }
}
#[test]
fn platform_rules_follow_extension_layout() {
    assert!(
        matches!(cli_rule("win32-x64").unwrap().binary,CliBinaryRule::InExtension {relative_path} if relative_path==Path::new("bin/windows-x86_64/codex.exe"))
    );
    assert!(cli_rule("unknown").is_none());
}

#[path = "tests/environment.rs"]
mod environment;

#[test]
fn actual_multi_step_usage_is_cumulative_per_run_not_per_thread() {
    let frames = include_str!("../../tests/fixtures/codex/usage-multi-step.jsonl")
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    for offset in [0_u64, 100000] {
        let mut mapper = protocol::Mapper::new("thread-fixture".into(), "turn-fixture".into());
        let mut last = None;
        for original in &frames {
            let mut frame = original.clone();
            for key in [
                "inputTokens",
                "outputTokens",
                "cachedInputTokens",
                "cacheWriteInputTokens",
            ] {
                let count = frame["params"]["tokenUsage"]["total"][key]
                    .as_u64()
                    .unwrap();
                frame["params"]["tokenUsage"]["total"][key] = json!(count + offset);
            }
            let mapped = mapper
                .map(frame["method"].as_str().unwrap(), &frame["params"])
                .unwrap();
            last = Some(mapped.into_iter().last().unwrap().payload);
            assert!(
                mapper
                    .map(frame["method"].as_str().unwrap(), &frame["params"])
                    .unwrap()
                    .is_empty()
            );
        }
        let ProviderEventPayload::Usage(usage) = last.unwrap() else {
            panic!("usage expected")
        };
        assert_eq!(usage.input_tokens, Some(24000));
        assert_eq!(usage.output_tokens, Some(51));
        assert_eq!(usage.cache_read_tokens, Some(11776));
        assert_eq!(usage.cost_usd, None);
    }
}
#[test]
fn usage_regression_and_foreign_turns_fail_closed() {
    let mut mapper = protocol::Mapper::new("thread-fixture".into(), "turn-fixture".into());
    let mut frame: Value = serde_json::from_str(
        include_str!("../../tests/fixtures/codex/usage-multi-step.jsonl")
            .lines()
            .next()
            .unwrap(),
    )
    .unwrap();
    let mut foreign = frame["params"].clone();
    foreign["turnId"] = json!("unrelated");
    assert!(
        mapper
            .map("thread/tokenUsage/updated", &foreign)
            .unwrap()
            .is_empty()
    );
    mapper
        .map("thread/tokenUsage/updated", &frame["params"])
        .unwrap();
    frame["params"]["tokenUsage"]["total"]["inputTokens"] = json!(1);
    assert_eq!(
        mapper
            .map("thread/tokenUsage/updated", &frame["params"])
            .err()
            .unwrap()
            .failure_kind(),
        Some(FailureKind::ProtocolParse)
    );
}
#[tokio::test]
async fn cancel_racing_with_completion_preserves_native_result() {
    let (adapter, _) = configured(Mode::Good, CodexApprovals::Deny);
    let (task, context) = inputs();
    adapter.start_run(&task, &context).await.unwrap();
    let _ = adapter.cancel_run(&context.run_id).await;
    let events = collect(&adapter, &context.run_id).await;
    assert!(
        events.iter().any(
            |e| e.kind == ProviderEventKind::TurnFinished && e.payload["status"] == "completed"
        )
    );
}

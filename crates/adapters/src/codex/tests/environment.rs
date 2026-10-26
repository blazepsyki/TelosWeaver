//! Explicit approved account tests; credentials stay inside the native CLI (Spec §20.1).
use super::*;
use crate::{CliDiscoverer, DiscoveryRequest, NativeCliDiscoverer};
struct LiveResolver {
    cwd: PathBuf,
    model: String,
}
impl CodexLaunchResolver for LiveResolver {
    fn resolve(&self, task: &TaskSpec, _: &RunContext) -> Result<CodexLaunch, AdapterError> {
        let mut launch = CodexLaunch::read_only(
            self.cwd.clone(),
            task.objective.clone(),
            task.agent
                .model
                .clone()
                .unwrap_or_else(|| self.model.clone()),
        );
        launch.run_timeout = Duration::from_secs(45);
        if task.permission_profile_id == "phase0-workspace-write" {
            launch.sandbox = CodexSandbox::WorkspaceWrite;
        }
        Ok(launch)
    }
}
fn live() -> (CodexAdapter, PathBuf) {
    let executable = PathBuf::from(
        std::env::var_os("TELOS_CODEX_CLI")
            .expect("TELOS_CODEX_CLI must select the extension executable"),
    );
    let model = std::env::var("TELOS_CODEX_MODEL")
        .expect("TELOS_CODEX_MODEL must select a verified low-effort catalog entry");
    let cwd = std::env::temp_dir().canonicalize().unwrap().join(format!(
        "telos-codex-live-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&cwd).unwrap();
    eprintln!(
        "live cwd={} host={} model={model} effort=low",
        cwd.display(),
        live_host_id()
    );
    (
        CodexAdapter::new(CodexConfig {
            executable,
            cli_version: std::env::var("TELOS_CODEX_VERSION")
                .unwrap_or_else(|_| "unverified".into()),
            host_id: live_host_id(),
            runtime: Arc::new(NativeRuntime),
            launcher: Arc::new(LiveResolver {
                cwd: cwd.clone(),
                model,
            }),
            request_timeout: Duration::from_secs(30),
            cancel_grace: Duration::from_secs(1),
        }),
        cwd,
    )
}
fn live_host_id() -> String {
    std::env::var("TELOS_CODEX_HOST_ID").unwrap_or_else(|_| fixtures::HOST_ID.into())
}
fn case(id: &str, prompt: &str) -> (TaskSpec, RunContext) {
    let (mut task, _) = inputs();
    task.id = id.into();
    task.objective = prompt.into();
    task.workspace.host_id = live_host_id();
    let mut context = fixtures::run_context(&task);
    context.run_id = id.into();
    context.idempotency_key = format!("{id}:start");
    (task, context)
}
async fn execute(
    adapter: &CodexAdapter,
    task: &TaskSpec,
    context: &RunContext,
) -> Result<(RunStarted, Vec<ProviderEvent>, String), AdapterError> {
    let started = adapter.start_run(task, context).await?;
    let events = tokio::time::timeout(
        Duration::from_secs(50),
        adapter.events(&context.run_id, 0).collect::<Vec<_>>(),
    )
    .await
    .map_err(|_| error(FailureKind::Timeout, "live collection timed out"))?
    .into_iter()
    .collect::<Result<Vec<_>, _>>()?;
    if check_run_events(&events).is_err() {
        return Err(error(
            FailureKind::ProtocolParse,
            "invalid live event sequence",
        ));
    }
    let text = events
        .iter()
        .filter_map(|e| {
            if e.kind == ProviderEventKind::TextDelta {
                e.payload.get("text").and_then(Value::as_str)
            } else {
                None
            }
        })
        .collect::<String>();
    let status = events
        .iter()
        .find(|e| e.kind == ProviderEventKind::TurnFinished)
        .map(|e| e.payload["status"].clone());
    eprintln!(
        "run={} thread={} turn={} events={} status={status:?} reply={text:?}",
        started.run_id,
        started.native_session_id.as_deref().unwrap_or(""),
        started.native_turn_id.as_deref().unwrap_or(""),
        events.len()
    );
    for event in events.iter().filter(|e| {
        matches!(
            e.kind,
            ProviderEventKind::Usage
                | ProviderEventKind::ProcessExited
                | ProviderEventKind::Error
                | ProviderEventKind::ToolCompleted
        )
    }) {
        eprintln!(
            "{} {:?} {}",
            context.run_id,
            event.kind,
            Value::Object(event.payload.clone())
        );
    }
    Ok((started, events, text))
}
fn ensure(value: bool, message: &str) -> Result<(), AdapterError> {
    if value {
        Ok(())
    } else {
        Err(error(FailureKind::Provider, message))
    }
}
fn completed(events: &[ProviderEvent]) -> bool {
    events
        .iter()
        .any(|e| e.kind == ProviderEventKind::TurnFinished && e.payload["status"] == "completed")
}
async fn cleanup(adapter: &CodexAdapter, cwd: &std::path::Path) -> Result<(), AdapterError> {
    let active = adapter
        .inner
        .registry
        .lock()
        .unwrap()
        .runs
        .values()
        .cloned()
        .collect::<Vec<_>>();
    for run in active {
        if run.inspect() == RunInspection::Running {
            run.cancel(Duration::from_millis(100));
            let _ =
                tokio::time::timeout(Duration::from_secs(5), run.subscribe(0).collect::<Vec<_>>())
                    .await;
        }
    }
    let mut threads = adapter.owned_thread_ids();
    threads.sort();
    threads.reverse();
    let mut failure = None;
    for thread in threads {
        let archived = adapter.archive(&thread, cwd.to_path_buf()).await;
        eprintln!("archive {thread}: {}", archived.is_ok());
        if let Err(error) = archived {
            failure = Some(error);
        }
    }
    failure.map_or(Ok(()), Err)
}
#[tokio::test(flavor = "multi_thread")]
#[ignore = "explicit local extension discovery"]
async fn env_discovery() {
    let home = PathBuf::from(
        std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).unwrap(),
    );
    // Spec §7.5, PROV-09: inspect the native local and Remote SSH extension roots.
    let os = match std::env::consts::OS {
        "windows" => "win32",
        "macos" => "darwin",
        os => os,
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        arch => arch,
    };
    let target_platform = format!("{os}-{arch}");
    let request = DiscoveryRequest {
        host_id: live_host_id(),
        target_platform: target_platform.clone(),
        explicit_path: None,
        extension_roots: vec![
            home.join(".vscode/extensions"),
            home.join(".vscode-server/extensions"),
        ],
        home_dir: home,
        path_env: None,
        checked_at: timestamp(),
    };
    let record = NativeCliDiscoverer::new(Arc::new(NativeRuntime))
        .discover(PROVIDER_ID, &cli_rule(&target_platform).unwrap(), &request)
        .await;
    let cli = record.found().expect("extension discovery failed");
    assert_eq!(
        PathBuf::from(&cli.executable_path),
        PathBuf::from(std::env::var_os("TELOS_CODEX_CLI").unwrap())
            .canonicalize()
            .unwrap()
    );
    eprintln!(
        "discovery executable={} version={} sha256={} extension={:?}",
        cli.executable_path, cli.cli_version, cli.sha256, cli.extension
    );
}
#[tokio::test(flavor = "multi_thread")]
#[ignore = "uses the approved Codex account and model quota"]
async fn env_managed_smoke() {
    let (adapter, cwd) = live();
    let (task, context) = case("smoke", "Reply with exactly TELOS_PONG. Do not use tools.");
    let result = async {
        let (_, events, text) = execute(&adapter, &task, &context).await?;
        ensure(
            completed(&events) && text.contains("TELOS_PONG"),
            "live response did not match",
        )
    }
    .await;
    let cleaned = cleanup(&adapter, &cwd).await;
    result.unwrap();
    cleaned.unwrap();
}
#[tokio::test(flavor = "multi_thread")]
#[ignore = "uses the approved Codex account and new test sessions only"]
async fn env_resume_fork() {
    let (adapter, cwd) = live();
    let word = format!("TELOS_WORD_{}", std::process::id());
    let result=async {
        let(task,context)=case("parent",&format!("Remember this exact word for our next messages: {word}. Reply READY only. Do not use tools."));let(parent,events,_)=execute(&adapter,&task,&context).await?;ensure(completed(&events),"parent failed")?;
        let(task,mut context)=case("resume","What exact word did I ask you to remember? Reply with only that word. Do not use tools.");context.session.mode=SessionMode::Resume;context.session.source_session_id=parent.native_session_id.clone();let(resumed,events,text)=execute(&adapter,&task,&context).await?;ensure(completed(&events)&&text.contains(&word)&&resumed.native_session_id==parent.native_session_id,"resume did not preserve context")?;
        let(task,mut context)=case("fork","What exact word did I ask you to remember? Reply with only that word. Do not use tools.");context.session.mode=SessionMode::NativeFork;context.session.source_session_id=parent.native_session_id.clone();context.session.source_turn_id=parent.native_turn_id.clone();let(forked,events,text)=execute(&adapter,&task,&context).await?;ensure(completed(&events)&&text.contains(&word)&&forked.native_session_id!=parent.native_session_id,"fork did not preserve isolated identity")?;
        let client=connect(adapter.config()?,cwd.clone()).await?;let read=client.request("thread/read",json!({"threadId":parent.native_session_id,"includeTurns":true})).await;let _=client.close(Duration::from_secs(1)).await;
        let read=read?;let turns=read.pointer("/thread/turns").and_then(Value::as_array).ok_or_else(||error(FailureKind::ProtocolParse,"parent turns are missing"))?;ensure(turns.len()==2,"fork modified the parent thread")
    }.await;
    let cleaned = cleanup(&adapter, &cwd).await;
    result.unwrap();
    cleaned.unwrap();
}
#[tokio::test(flavor = "multi_thread")]
#[ignore = "uses the approved Codex account in a disposable workspace"]
async fn env_tool_denied() {
    let (adapter, cwd) = live();
    let (task, context) = case(
        "deny",
        "Use apply_patch once to create denied.txt containing BLOCKED in the current directory. Attempt the tool, do not assume permission. If permission is denied, stop and reply DENIED. Do not run shell commands or use other tools.",
    );
    let result = async {
        let (_, events, _) = execute(&adapter, &task, &context).await?;
        ensure(
            !cwd.join("denied.txt").exists(),
            "read-only test created a file",
        )?;
        ensure(
            events.iter().any(|e| {
                e.kind == ProviderEventKind::ToolCompleted
                    && e.payload.get("outcome") == Some(&json!("denied"))
            }),
            "no actual tool denial was reported",
        )
    }
    .await;
    let cleaned = cleanup(&adapter, &cwd).await;
    result.unwrap();
    cleaned.unwrap();
}
#[tokio::test(flavor = "multi_thread")]
#[ignore = "uses the approved Codex account and writes one disposable file"]
async fn env_tool_allowed() {
    let (adapter, cwd) = live();
    let (mut task, context) = case(
        "write",
        "Use apply_patch to create allowed.txt containing exactly TELOS_ALLOWED in the current directory. Do not use shell commands or any other tools. Reply DONE when finished.",
    );
    task.permission_profile_id = "phase0-workspace-write".into();
    let result = async {
        let (_, events, _) = execute(&adapter, &task, &context).await?;
        ensure(completed(&events), "write turn failed")?;
        let content = std::fs::read_to_string(cwd.join("allowed.txt"))
            .map_err(|_| error(FailureKind::Provider, "allowed file is missing"))?;
        ensure(
            content.trim() == "TELOS_ALLOWED",
            "allowed file content did not match",
        )
    }
    .await;
    let cleaned = cleanup(&adapter, &cwd).await;
    result.unwrap();
    cleaned.unwrap();
}
#[tokio::test(flavor = "multi_thread")]
#[ignore = "uses the approved Codex account and interrupts a short test immediately"]
async fn env_cancel() {
    let (adapter, cwd) = live();
    let (task, context) = case(
        "cancel",
        "Count from 1 to 100000 with one number per line. Do not use tools.",
    );
    let result = async {
        let started = adapter.start_run(&task, &context).await?;
        ensure(
            adapter.cancel_run(&context.run_id).await?,
            "cancel was not accepted",
        )?;
        let events = tokio::time::timeout(
            Duration::from_secs(10),
            adapter.events(&context.run_id, 0).collect::<Vec<_>>(),
        )
        .await
        .map_err(|_| error(FailureKind::Timeout, "cancel did not finish"))?
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
        eprintln!(
            "cancel thread={:?} elapsed event count={}",
            started.native_session_id,
            events.len()
        );
        ensure(
            check_run_events(&events) == Ok(RunTermination::ProcessExited),
            "cancel sequence failed",
        )?;
        ensure(
            events.iter().any(|e| {
                e.kind == ProviderEventKind::TurnFinished && e.payload["status"] == "cancelled"
            }),
            "actual cancelled turn was not reported",
        )?;
        ensure(
            events.last().unwrap().payload["childrenReaped"] == true,
            "child reaping was not confirmed",
        )
    }
    .await;
    let cleaned = cleanup(&adapter, &cwd).await;
    result.unwrap();
    cleaned.unwrap();
}
#[tokio::test(flavor = "multi_thread")]
#[ignore = "explicit invalid-model check using the approved account"]
async fn env_invalid_model() {
    let (adapter, cwd) = live();
    let (mut task, context) = case("bad-model", "Reply PONG. Do not use tools.");
    task.agent.model = Some("telos-invalid-model".into());
    let result = match execute(&adapter, &task, &context).await {
        Err(error) => {
            eprintln!("invalid model start error: {error}");
            // Spec §7, PROV-03: reject local setup failures as live model evidence.
            ensure(
                error.failure_kind() == Some(FailureKind::Configuration)
                    && !adapter.owned_thread_ids().is_empty(),
                "invalid model was not rejected by a native thread",
            )
        }
        Ok((_, events, _)) => ensure(
            events.iter().any(|e| {
                e.kind == ProviderEventKind::Error && e.payload["kind"] == "configuration"
            }) && !completed(&events),
            "invalid model was not rejected",
        ),
    };
    let cleaned = cleanup(&adapter, &cwd).await;
    result.unwrap();
    cleaned.unwrap();
}

// §7.6, DEC-08: check the native DB-backed filter; do not infer UI success from it.
async fn listing_in_default_catalog(
    adapter: &CodexAdapter,
    cwd: &std::path::Path,
    started: &RunStarted,
    label: &str,
) -> Result<(), AdapterError> {
    let config = adapter.config()?;
    let client = connect(config, cwd.to_path_buf()).await?;
    let result = async {
        let thread_id = started.native_session_id.as_deref().unwrap();
        let cwd_text = cwd.to_string_lossy();
        let query_cwd = if cfg!(windows) { cwd_text.strip_prefix(r"\\?\").unwrap_or(&cwd_text) } else { &cwd_text };
        // Query DB-only before thread/read, which must not repair the metadata for this assertion.
        for empty_sources in [false, true] {
            let mut params = json!({"cwd":query_cwd,"limit":100,"useStateDbOnly":true});
            if empty_sources { params["sourceKinds"] = json!([]); }
            let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
            loop {
                let list = client.request("thread/list", params.clone()).await?;
                let included = list["data"].as_array().is_some_and(|rows| rows.iter().any(|t| t["id"] == thread_id));
                if included { break; }
                ensure(tokio::time::Instant::now() < deadline, "persisted thread is missing from the default DB catalog")?;
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
        let read = client.request("thread/read", json!({"threadId":thread_id,"includeTurns":false})).await?;
        let source = read.pointer("/thread/source").and_then(Value::as_str).unwrap_or("");
        let native_cwd = read.pointer("/thread/cwd").and_then(Value::as_str).unwrap_or("");
        ensure(matches!(source, "cli" | "vscode"), "native source is outside the interactive filter")?;
        ensure(std::path::Path::new(native_cwd).canonicalize().ok().as_deref() == Some(cwd), "native cwd changed")?;
        eprintln!("listing {label}: thread={thread_id} source={source} cwd={native_cwd} default=true emptySourceKinds=true useStateDbOnly=true");
        Ok(())
    }.await;
    let closed = client.close(config.cancel_grace).await;
    result?;
    closed.map(|_| ())
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "uses the approved account for three short listing checks; archives only owned test threads"]
async fn env_vscode_listing_fresh_resume_and_fork() {
    let (adapter, cwd) = live();
    let result = async {
        let (task, context) = case("listing-fresh", "Reply only PONG. Do not use tools.");
        let (fresh, events, _) = execute(&adapter, &task, &context).await?;
        ensure(completed(&events), "fresh listing run failed")?;
        listing_in_default_catalog(&adapter, &cwd, &fresh, "fresh").await?;

        let (task, mut context) = case("listing-resume", "Reply only PONG. Do not use tools.");
        context.session.mode = SessionMode::Resume;
        context.session.source_session_id = fresh.native_session_id.clone();
        let (resumed, events, _) = execute(&adapter, &task, &context).await?;
        ensure(
            completed(&events) && resumed.native_session_id == fresh.native_session_id,
            "resume listing run failed",
        )?;
        listing_in_default_catalog(&adapter, &cwd, &resumed, "resume").await?;

        let (task, mut context) = case("listing-fork", "Reply only PONG. Do not use tools.");
        context.session.mode = SessionMode::NativeFork;
        context.session.source_session_id = fresh.native_session_id.clone();
        let (forked, events, _) = execute(&adapter, &task, &context).await?;
        ensure(
            completed(&events) && forked.native_session_id != fresh.native_session_id,
            "fork listing run failed",
        )?;
        listing_in_default_catalog(&adapter, &cwd, &forked, "fork").await?;
        Ok::<(), AdapterError>(())
    }
    .await;
    let cleaned = cleanup(&adapter, &cwd).await;
    result.unwrap();
    cleaned.unwrap();
}

// Spec §7.2/§7.3, C3-X3: steering must update this Turn, not create another one.
#[tokio::test(flavor = "multi_thread")]
#[cfg(windows)]
#[ignore = "uses the approved Codex account to steer a disposable foreground wait"]
async fn env_steering_updates_the_same_turn() {
    let (adapter, cwd) = live();
    let (task, context) = case(
        "steering",
        "Use exec_command once to run the PowerShell command `Start-Sleep -Seconds 10` in the current directory and wait for completion. Then reply only ORIGINAL. Do not write files or use any other tools.",
    );
    let result = async {
        let started = adapter.start_run(&task, &context).await?;
        eprintln!("steering thread={:?} turn={:?}", started.native_session_id, started.native_turn_id);
        let mut events = adapter.events(&context.run_id, 0);
        tokio::time::timeout(Duration::from_secs(35), async {
            while let Some(event) = events.next().await {
                let event = event?;
                if event.kind == ProviderEventKind::ToolStarted { return Ok::<(), AdapterError>(()); }
                ensure(event.kind != ProviderEventKind::TurnFinished, "Turn ended before the foreground wait")?;
            }
            Err(error(FailureKind::Provider, "no live tool event"))
        }).await.map_err(|_| error(FailureKind::Timeout, "no foreground wait within deadline"))??;
        adapter.send_input(&context.run_id, "steer-1", "After the pending command completes, disregard the previous final answer instruction and reply only C3_STEERED.").await?;
        // start_run replays the same idempotency key; execute collects the full retained stream.
        let (replayed, events, text) = execute(&adapter, &task, &context).await?;
        ensure(replayed == started && completed(&events) && text.contains("C3_STEERED"), "steering was not reflected in the same completed Run")?;
        ensure(events.iter().all(|e| e.native_turn_id == started.native_turn_id)
            && events.iter().filter(|e| e.kind == ProviderEventKind::TurnFinished).count() == 1,
            "steering created another Turn")?;
        Ok::<(), AdapterError>(())
    }.await;
    let cleaned = cleanup(&adapter, &cwd).await;
    result.unwrap();
    cleaned.unwrap();
}

const APPROVAL_WRITE_COMMAND: &str = "Set-Content -LiteralPath approval.txt -Value C3_APPROVED";
const APPROVAL_PROFILE: &str = "phase0-c3-approval-write";

fn approval_case(id: &str, prompt: &str) -> (TaskSpec, RunContext) {
    let (mut task, context) = case(id, prompt);
    task.permission_profile_id = APPROVAL_PROFILE.into();
    task.workspace.mode = WorkspaceMode::Directory;
    task.workspace.read_only = false;
    task.workspace.workspace_id = Some(context.workspace_id.clone());
    (task, context)
}

fn approval_scope(task: &TaskSpec, context: &RunContext) -> bool {
    task.permission_profile_id == APPROVAL_PROFILE
        && !task.workspace.read_only
        && task.workspace.workspace_id.as_deref() == Some(context.workspace_id.as_str())
        && task.workspace.host_id == context.host_id
        && task.id == context.task_id
        && task.revision == context.task_revision
}

struct ApprovalResolver {
    inner: LiveResolver,
}
impl CodexLaunchResolver for ApprovalResolver {
    fn resolve(&self, task: &TaskSpec, context: &RunContext) -> Result<CodexLaunch, AdapterError> {
        ensure(
            approval_scope(task, context),
            "live approval Task scope is not authorized",
        )?;
        let mut launch = self.inner.resolve(task, context)?;
        launch.approvals = CodexApprovals::Prompt;
        Ok(launch)
    }
    fn authorize_approval(
        &self,
        task: &TaskSpec,
        context: &RunContext,
        request: &Value,
    ) -> Result<(), AdapterError> {
        let params = &request["params"];
        let command = params["command"].as_str().unwrap_or("");
        // Native command previews escape Windows path separators; normalize only this representation.
        let normalized_command = command.replace("\\\\", "\\");
        let cwd = params["cwd"]
            .as_str()
            .and_then(|p| std::path::Path::new(p).canonicalize().ok());
        eprintln!(
            "approval candidate: method={} cwd={} command={command:?}",
            request["method"], params["cwd"]
        );
        let known_commands = [
            APPROVAL_WRITE_COMMAND.to_owned(),
            format!(
                "\"C:\\Program Files\\PowerShell\\7\\pwsh.exe\" -Command '{APPROVAL_WRITE_COMMAND}'"
            ),
            format!(
                "\"C:\\Program Files\\PowerShell\\7\\pwsh.exe\" -NoProfile -Command '{APPROVAL_WRITE_COMMAND}'"
            ),
        ];
        if approval_scope(task, context)
            && request["method"] == "item/commandExecution/requestApproval"
            && cwd.as_deref() == Some(self.inner.cwd.as_path())
            && params
                .get("networkApprovalContext")
                .is_none_or(Value::is_null)
            && known_commands.contains(&normalized_command)
        {
            Ok(())
        } else {
            Err(error(
                FailureKind::ToolDenied,
                "live approval was outside the single disposable write action",
            ))
        }
    }
}

#[test]
fn live_approval_policy_rejects_other_actions_locations_and_network_requests() {
    let cwd = std::env::temp_dir().canonicalize().unwrap();
    let policy = ApprovalResolver {
        inner: LiveResolver {
            cwd: cwd.clone(),
            model: "fixture".into(),
        },
    };
    let (task, context) = approval_case("approval-policy", "fixture");
    let request = json!({"method":"item/commandExecution/requestApproval","params":{"cwd":cwd,"command":APPROVAL_WRITE_COMMAND}});
    assert!(policy.authorize_approval(&task, &context, &request).is_ok());
    let mut readonly = task.clone();
    readonly.workspace.read_only = true;
    assert!(
        policy
            .authorize_approval(&readonly, &context, &request)
            .is_err()
    );
    let mut unrelated = context.clone();
    unrelated.task_id = "unrelated".into();
    assert!(
        policy
            .authorize_approval(&task, &unrelated, &request)
            .is_err()
    );
    let mut escaped = request.clone();
    escaped["params"]["command"] = json!(format!(
        "\"C:\\\\Program Files\\\\PowerShell\\\\7\\\\pwsh.exe\" -Command '{APPROVAL_WRITE_COMMAND}'"
    ));
    assert!(policy.authorize_approval(&task, &context, &escaped).is_ok());
    for (field, value) in [
        ("command", json!("Get-ChildItem")),
        ("cwd", json!(cwd.parent().unwrap())),
        ("networkApprovalContext", json!({"host":"invalid.example"})),
    ] {
        let mut invalid = request.clone();
        invalid["params"][field] = value;
        assert!(
            policy
                .authorize_approval(&task, &context, &invalid)
                .is_err()
        );
    }
    let mut invalid = request;
    invalid["method"] = json!("item/fileChange/requestApproval");
    assert!(
        policy
            .authorize_approval(&task, &context, &invalid)
            .is_err()
    );
}

// Spec §7.2, C3-X4: only the exact generated write action can pass the test policy.
#[tokio::test(flavor = "multi_thread")]
#[cfg(windows)]
#[ignore = "uses the approved account to test Allow and Deny for a disposable file"]
async fn env_approval_allow_and_deny_control_the_write() {
    for answer in [ApprovalAnswer::Deny, ApprovalAnswer::Allow] {
        let (mut adapter, cwd) = live();
        Arc::get_mut(&mut adapter.inner)
            .unwrap()
            .config
            .as_mut()
            .unwrap()
            .launcher = Arc::new(ApprovalResolver {
            inner: LiveResolver {
                cwd: cwd.clone(),
                model: std::env::var("TELOS_CODEX_MODEL").unwrap(),
            },
        });
        let label = if answer == ApprovalAnswer::Allow {
            "allow"
        } else {
            "deny"
        };
        let (task, context) = approval_case(
            &format!("approval-{label}"),
            &format!(
                "Use exec_command to run exactly `{APPROVAL_WRITE_COMMAND}` in the current directory. Request sandbox_permissions=require_escalated so the host can approve this one write. Do not use another tool or a fallback. If permission is denied, stop and reply DENIED; otherwise reply DONE."
            ),
        );
        let result = async {
            let started = adapter.start_run(&task, &context).await?;
            eprintln!(
                "approval {label}: thread={:?} turn={:?}",
                started.native_session_id, started.native_turn_id
            );
            let mut stream = adapter.events(&context.run_id, 0);
            let mut events = vec![];
            let mut answered = 0;
            tokio::time::timeout(Duration::from_secs(55), async {
                while let Some(event) = stream.next().await {
                    let event = event?;
                    if event.kind == ProviderEventKind::ApprovalRequired {
                        ensure(
                            event.payload["answerable"] == true,
                            "approval request was not answerable",
                        )?;
                        adapter
                            .answer_approval(
                                &context.run_id,
                                event.payload["requestId"].as_str().unwrap(),
                                answer,
                            )
                            .await?;
                        answered += 1;
                    }
                    events.push(event);
                }
                Ok::<(), AdapterError>(())
            })
            .await
            .map_err(|_| error(FailureKind::Timeout, "approval stream timed out"))??;
            ensure(
                check_run_events(&events).is_ok() && answered == 1,
                "expected one correlated approval and a valid event stream",
            )?;
            if answer == ApprovalAnswer::Allow {
                ensure(
                    completed(&events)
                        && std::fs::read_to_string(cwd.join("approval.txt"))
                            .is_ok_and(|s| s.trim() == "C3_APPROVED"),
                    "Allow did not produce the approved file",
                )?;
            } else {
                ensure(
                    !cwd.join("approval.txt").exists()
                        && events
                            .iter()
                            .any(|e| e.payload.get("outcome") == Some(&json!("denied"))),
                    "Deny did not prevent and report the write",
                )?;
            }
            eprintln!(
                "approval {label}: answered={answered} events={} fileExists={}",
                events.len(),
                cwd.join("approval.txt").exists()
            );
            Ok::<(), AdapterError>(())
        }
        .await;
        let cleaned = cleanup(&adapter, &cwd).await;
        result.unwrap();
        cleaned.unwrap();
    }
}

#[tokio::test]
#[ignore = "reads the explicitly selected local extension manifest; no model or App Server"]
async fn env_capability_metadata_has_no_model_calls() -> Result<(), AdapterError> {
    let (mut adapter, runtime) = configured(Mode::Good, CodexApprovals::Deny);
    let config = Arc::get_mut(&mut adapter.inner)
        .unwrap()
        .config
        .as_mut()
        .unwrap();
    config.executable = std::env::var_os("TELOS_CODEX_CLI")
        .expect("select extension executable")
        .into();
    config.cli_version = std::env::var("TELOS_CODEX_VERSION").expect("select CLI version");
    config.host_id = live_host_id();
    let identity =
        capabilities::extension_identity(&config.executable).expect("extension identity");
    eprintln!(
        "extension id={} version={} platform={}",
        identity.id, identity.version, identity.platform
    );
    let caps = adapter.detect(&live_host_id()).await.unwrap();
    for key in [
        capability_keys::MANAGED_RUN,
        capability_keys::EVENT_STREAM,
        capability_keys::CANCEL,
        capability_keys::NATIVE_RESUME,
        capability_keys::NATIVE_FORK,
        capability_keys::VSCODE_LISTING,
    ] {
        ensure(
            caps[key].state == CapabilityState::Supported,
            "known selected extension was not verified",
        )?;
        eprintln!(
            "capability {key}: {:?} {:?} {:?}",
            caps[key].state, caps[key].evidence_level, caps[key].reason
        );
    }
    assert_eq!(runtime.spawns.load(Ordering::SeqCst), 0);
    Ok::<(), AdapterError>(())
}

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
                e.kind == ProviderEventKind::ToolCompleted && e.payload["outcome"] == "denied"
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

//! Explicit paid tests; credentials stay inside the native CLI (Spec §20.1).
use super::*;
struct LiveResolver {
    cwd: PathBuf,
    model: String,
    prompts: Mutex<HashMap<String, String>>,
}
impl CodexLaunchResolver for LiveResolver {
    fn resolve(&self, task: &TaskSpec, _: &RunContext) -> Result<CodexLaunch, AdapterError> {
        Ok(CodexLaunch::read_only(
            self.cwd.clone(),
            self.prompts
                .lock()
                .unwrap()
                .get(&task.id)
                .cloned()
                .unwrap_or_else(|| task.objective.clone()),
            self.model.clone(),
        ))
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
    (
        CodexAdapter::new(CodexConfig {
            executable,
            cli_version: std::env::var("TELOS_CODEX_VERSION")
                .unwrap_or_else(|_| "unverified".into()),
            host_id: fixtures::HOST_ID.into(),
            runtime: Arc::new(NativeRuntime),
            launcher: Arc::new(LiveResolver {
                cwd: cwd.clone(),
                model,
                prompts: Mutex::new(HashMap::new()),
            }),
            request_timeout: Duration::from_secs(30),
            cancel_grace: Duration::from_secs(1),
        }),
        cwd,
    )
}
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the user's approved Codex account and uses model quota"]
async fn env_managed_smoke() {
    let (adapter, cwd) = live();
    let (mut task, context) = inputs();
    task.objective = "Reply with exactly TELOS_PONG. Do not use tools.".into();
    let result = async {
        let started = adapter.start_run(&task, &context).await?;
        let events = tokio::time::timeout(
            Duration::from_secs(130),
            adapter.events(&context.run_id, 0).collect::<Vec<_>>(),
        )
        .await
        .unwrap()
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
        if check_run_events(&events) != Ok(RunTermination::ProcessExited) {
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
        eprintln!(
            "run={} thread={} turn={} events={} reply={text:?}",
            started.run_id,
            started.native_session_id.unwrap(),
            started.native_turn_id.unwrap(),
            events.len()
        );
        if !events
            .iter()
            .any(|e| e.payload.get("status") == Some(&json!("completed")))
        {
            return Err(error(FailureKind::Provider, "live turn did not complete"));
        }
        if !text.contains("TELOS_PONG") {
            return Err(error(FailureKind::Provider, "live response did not match"));
        }
        Ok::<(), AdapterError>(())
    }
    .await;
    for thread in adapter.owned_thread_ids() {
        let archived = adapter.archive(&thread, cwd.clone()).await;
        eprintln!("archive {thread}: {}", archived.is_ok());
        archived.unwrap();
    }
    result.unwrap();
}

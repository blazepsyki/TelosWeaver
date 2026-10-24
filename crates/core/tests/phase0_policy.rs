//! Phase 0 운영자 정책·실행 조건·종료 정리 시험 (PH0-INTEGRATION, G1 준비).

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use futures::{StreamExt, stream::BoxStream};
use telos_adapters::{AdapterError, AgentAdapter, claude::ClaudeLaunchResolver};
use telos_core::{
    execution::RunAuthorizer,
    integration::{ClaudeLauncher, LocalPolicyConfig, LocalRunPolicy, TrackedAdapter},
};
use telos_schema::{
    ApprovalAnswer, Capability, ProviderEvent, RunContext, RunInspection, RunStarted, SessionMode,
    TaskSpec, WorkspaceMode, api::ApiErrorCode, fixtures,
};

struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir()
            .join("telos-phase0-policy")
            .join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("ws")).unwrap();
        Self(path)
    }
    fn write_policy(&self, json: serde_json::Value) -> PathBuf {
        let file = self.0.join("policy.json");
        std::fs::write(&file, serde_json::to_vec(&json).unwrap()).unwrap();
        file
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn policy_json(dir: &Dir, writable: bool) -> serde_json::Value {
    serde_json::json!({
        "revision": "test-1",
        "permissionProfiles": ["phase0"],
        "workspaces": [{ "id": "ws-1", "path": dir.0.join("ws"), "writable": writable }],
        "claude": {
            "models": ["haiku"],
            "permissionMode": "acceptEdits",
            "allowedTools": ["Read", "Write"],
            "disallowedTools": ["WebFetch"],
            "settingSources": "user"
        }
    })
}

fn load(dir: &Dir, writable: bool) -> Arc<LocalRunPolicy> {
    let (config, revision) =
        LocalPolicyConfig::load(&dir.write_policy(policy_json(dir, writable))).unwrap();
    Arc::new(LocalRunPolicy::new(config, revision, Default::default()))
}

fn task() -> TaskSpec {
    let mut task = fixtures::task_spec();
    task.agent.provider = "claude".into();
    task.permission_profile_id = "phase0".into();
    task.workspace.mode = WorkspaceMode::Directory;
    task.workspace.workspace_id = Some("ws-1".into());
    task.workspace.read_only = false;
    task.objective = "Create result.txt".into();
    task.constraints = vec!["Use only the Write tool".into()];
    task
}

fn session() -> telos_schema::SessionRequest {
    fixtures::run_context(&fixtures::task_spec()).session
}

async fn denied(policy: &LocalRunPolicy, task: &TaskSpec) -> String {
    let error = policy.authorize(task, &session()).await.unwrap_err();
    assert_eq!(error.code, ApiErrorCode::Forbidden);
    error.message
}

#[tokio::test]
async fn grants_only_configured_profile_provider_model_and_workspace() {
    let dir = Dir::new("grants");
    let policy = load(&dir, true);
    let permit = policy.authorize(&task(), &session()).await.unwrap();
    assert_eq!(permit.workspace_id, "ws-1");
    assert!(permit.policy_revision.starts_with("test-1#"));
    let snapshot = policy.snapshot(&permit.context_snapshot_id).unwrap();
    assert_eq!(snapshot.model, "haiku");
    assert_eq!(
        snapshot.workspace_path.canonicalize().unwrap(),
        dir.0.join("ws").canonicalize().unwrap()
    );
    // 확장 길이 경로(`\\?\`)가 아닌 일반 형식으로 Provider에 전달합니다.
    assert!(
        !snapshot
            .workspace_path
            .to_string_lossy()
            .starts_with(r"\\?\")
    );

    let mut t = task();
    t.permission_profile_id = "admin".into();
    assert!(denied(&policy, &t).await.contains("permission profile"));
    let mut t = task();
    t.agent.provider = "codex".into();
    assert!(denied(&policy, &t).await.contains("provider"));
    let mut t = task();
    t.agent.model = Some("opus".into());
    assert!(denied(&policy, &t).await.contains("model"));
    let mut t = task();
    t.workspace.workspace_id = Some("other".into());
    assert!(denied(&policy, &t).await.contains("workspace"));
    let mut t = task();
    t.workspace.mode = WorkspaceMode::None;
    assert!(denied(&policy, &t).await.contains("directory"));
    let mut s = session();
    s.mode = SessionMode::Capsule;
    assert_eq!(
        policy.authorize(&task(), &s).await.unwrap_err().code,
        ApiErrorCode::Forbidden
    );
}

#[tokio::test]
async fn read_only_grant_rejects_writes_and_disables_write_tools() {
    let dir = Dir::new("readonly");
    let policy = load(&dir, false);
    assert!(denied(&policy, &task()).await.contains("read-only"));

    let mut t = task();
    t.workspace.read_only = true;
    let permit = policy.authorize(&t, &session()).await.unwrap();
    let mut context = fixtures::run_context(&t);
    context.workspace_id = permit.workspace_id;
    context.context_snapshot_id = permit.context_snapshot_id;
    context.policy_revision = permit.policy_revision;
    let launch = ClaudeLauncher::new(policy.clone())
        .resolve(&t, &context)
        .unwrap();
    for tool in ["Write", "Edit", "Bash", "WebFetch"] {
        assert!(launch.disallowed_tools.iter().any(|d| d == tool), "{tool}");
    }
    assert_eq!(launch.model.as_deref(), Some("haiku"));
    assert!(
        launch.prompt.contains("Create result.txt")
            && launch.prompt.contains("Use only the Write tool")
    );
}

#[tokio::test]
async fn launcher_refuses_runs_without_matching_authorization() {
    let dir = Dir::new("launcher");
    let policy = load(&dir, true);
    let launcher = ClaudeLauncher::new(policy.clone());
    let t = task();
    let mut context = fixtures::run_context(&t);
    assert!(launcher.resolve(&t, &context).is_err(), "unknown snapshot");

    let permit = policy.authorize(&t, &session()).await.unwrap();
    context.workspace_id = permit.workspace_id;
    context.context_snapshot_id = permit.context_snapshot_id;
    context.policy_revision = permit.policy_revision;
    assert!(launcher.resolve(&t, &context).is_ok());
    let mut other = t.clone();
    other.revision += 1;
    assert!(
        launcher.resolve(&other, &context).is_err(),
        "different task revision"
    );
    context.workspace_id = "ws-2".into();
    assert!(
        launcher.resolve(&t, &context).is_err(),
        "different workspace"
    );
}

#[test]
fn invalid_policy_files_are_rejected() {
    let dir = Dir::new("invalid");
    let mut json = policy_json(&dir, true);
    json["claude"]["permissionMode"] = "bypassPermissions".into();
    assert!(LocalPolicyConfig::load(&dir.write_policy(json)).is_err());

    let mut json = policy_json(&dir, true);
    json["workspaces"][0]["path"] = "relative/ws".into();
    assert!(LocalPolicyConfig::load(&dir.write_policy(json)).is_err());

    let mut json = policy_json(&dir, true);
    json["token"] = "x".into();
    assert!(LocalPolicyConfig::load(&dir.write_policy(json)).is_err());

    let mut json = policy_json(&dir, true);
    json["claude"]["models"] = serde_json::json!([]);
    assert!(LocalPolicyConfig::load(&dir.write_policy(json)).is_err());
}

// --- 종료 정리 ---

struct Hanging {
    cancelled: AtomicBool,
    finished: tokio::sync::watch::Sender<bool>,
}

impl Default for Hanging {
    fn default() -> Self {
        Self {
            cancelled: AtomicBool::new(false),
            finished: tokio::sync::watch::channel(false).0,
        }
    }
}

#[async_trait]
impl AgentAdapter for Hanging {
    fn id(&self) -> &str {
        "claude"
    }
    async fn detect(&self, _: &str) -> Result<HashMap<String, Capability>, AdapterError> {
        Ok(HashMap::new())
    }
    async fn start_run(&self, _: &TaskSpec, c: &RunContext) -> Result<RunStarted, AdapterError> {
        Ok(RunStarted {
            run_id: c.run_id.clone(),
            native_session_id: None,
            native_turn_id: None,
        })
    }
    fn events(&self, _: &str, _: u64) -> BoxStream<'static, Result<ProviderEvent, AdapterError>> {
        let mut finished = self.finished.subscribe();
        futures::stream::once(async move {
            let _ = finished.wait_for(|done| *done).await;
        })
        .filter_map(|()| async { None })
        .boxed()
    }
    async fn send_input(&self, _: &str, _: &str, _: &str) -> Result<(), AdapterError> {
        Ok(())
    }
    async fn answer_approval(
        &self,
        _: &str,
        _: &str,
        _: ApprovalAnswer,
    ) -> Result<(), AdapterError> {
        Ok(())
    }
    async fn cancel_run(&self, _: &str) -> Result<bool, AdapterError> {
        self.cancelled.store(true, Ordering::SeqCst);
        self.finished.send_replace(true);
        Ok(true)
    }
    async fn inspect_run(&self, _: &str) -> Result<RunInspection, AdapterError> {
        Ok(if self.cancelled.load(Ordering::SeqCst) {
            RunInspection::Finished
        } else {
            RunInspection::Running
        })
    }
}

#[tokio::test]
async fn shutdown_cancels_active_runs_and_confirms_termination() {
    let inner = Arc::new(Hanging::default());
    let tracked = TrackedAdapter::new(inner.clone());
    let t = task();
    tracked
        .start_run(&t, &fixtures::run_context(&t))
        .await
        .unwrap();
    let report = tracked.shutdown(Duration::from_secs(2)).await;
    assert_eq!(report.cancelled, 1);
    assert!(report.unconfirmed.is_empty());
    assert!(inner.cancelled.load(Ordering::SeqCst));
}

struct NeverEnds;

#[async_trait]
impl AgentAdapter for NeverEnds {
    fn id(&self) -> &str {
        "claude"
    }
    async fn detect(&self, _: &str) -> Result<HashMap<String, Capability>, AdapterError> {
        Ok(HashMap::new())
    }
    async fn start_run(&self, _: &TaskSpec, c: &RunContext) -> Result<RunStarted, AdapterError> {
        Ok(RunStarted {
            run_id: c.run_id.clone(),
            native_session_id: None,
            native_turn_id: None,
        })
    }
    fn events(&self, _: &str, _: u64) -> BoxStream<'static, Result<ProviderEvent, AdapterError>> {
        futures::stream::pending().boxed()
    }
    async fn send_input(&self, _: &str, _: &str, _: &str) -> Result<(), AdapterError> {
        Ok(())
    }
    async fn answer_approval(
        &self,
        _: &str,
        _: &str,
        _: ApprovalAnswer,
    ) -> Result<(), AdapterError> {
        Ok(())
    }
    async fn cancel_run(&self, _: &str) -> Result<bool, AdapterError> {
        Ok(true)
    }
    async fn inspect_run(&self, _: &str) -> Result<RunInspection, AdapterError> {
        Ok(RunInspection::Running)
    }
}

#[tokio::test]
async fn shutdown_reports_runs_whose_termination_is_not_confirmed() {
    let tracked = TrackedAdapter::new(Arc::new(NeverEnds));
    let t = task();
    tracked
        .start_run(&t, &fixtures::run_context(&t))
        .await
        .unwrap();
    let report = tracked.shutdown(Duration::from_millis(100)).await;
    assert_eq!(report.unconfirmed, vec!["run-1".to_owned()]);
}

// --- Codex·Agy (G2) ---

fn multi_policy(dir: &Dir, writable: bool) -> Arc<LocalRunPolicy> {
    let mut json = policy_json(dir, writable);
    json["codex"] = serde_json::json!({
        "models": ["gpt-6-luna"],
        "effort": "low",
        "runTimeoutSecs": 120
    });
    json["agy"] = serde_json::json!({
        "models": ["gemini-3.8-flash-low"],
        "effort": "low",
        "mode": "accept-edits"
    });
    let (config, revision) = LocalPolicyConfig::load(&dir.write_policy(json)).unwrap();
    Arc::new(LocalRunPolicy::new(config, revision, Default::default()))
}

async fn authorized_context(policy: &LocalRunPolicy, task: &TaskSpec) -> RunContext {
    let permit = policy.authorize(task, &session()).await.unwrap();
    let mut context = fixtures::run_context(task);
    context.workspace_id = permit.workspace_id;
    context.context_snapshot_id = permit.context_snapshot_id;
    context.policy_revision = permit.policy_revision;
    context
}

#[tokio::test]
async fn codex_launch_follows_policy_and_workspace_permission() {
    use telos_adapters::codex::{CodexApprovals, CodexLaunchResolver, CodexSandbox};
    use telos_core::integration::CodexLauncher;

    let dir = Dir::new("codex");
    let policy = multi_policy(&dir, true);
    let launcher = CodexLauncher::new(policy.clone());

    let mut t = task();
    t.agent.provider = "codex".into();
    let launch = launcher
        .resolve(&t, &authorized_context(&policy, &t).await)
        .unwrap();
    assert_eq!(launch.model, "gpt-6-luna");
    assert_eq!(launch.effort, "low");
    assert_eq!(launch.sandbox, CodexSandbox::WorkspaceWrite);
    assert_eq!(launch.approvals, CodexApprovals::Deny);
    assert_eq!(launch.run_timeout, Duration::from_secs(120));

    t.workspace.read_only = true;
    let launch = launcher
        .resolve(&t, &authorized_context(&policy, &t).await)
        .unwrap();
    assert_eq!(launch.sandbox, CodexSandbox::ReadOnly);

    t.agent.model = Some("gpt-6".into());
    assert!(denied(&policy, &t).await.contains("model"));
}

#[tokio::test]
async fn agy_launch_uses_review_mode_for_read_only_runs() {
    use telos_adapters::agy::AgyLaunchResolver;
    use telos_core::integration::AgyLauncher;

    let dir = Dir::new("agy");
    let policy = multi_policy(&dir, true);
    let launcher = AgyLauncher::new(policy.clone());

    let mut t = task();
    t.agent.provider = "agy".into();
    let launch = launcher
        .resolve(&t, &authorized_context(&policy, &t).await)
        .unwrap();
    assert_eq!(launch.model.as_deref(), Some("gemini-3.8-flash-low"));
    assert_eq!(launch.effort.as_deref(), Some("low"));
    assert_eq!(launch.mode.as_deref(), Some("accept-edits"));
    assert!(launch.disable_slash_commands);
    assert!(launch.extra_args.is_empty());
    assert_eq!(launch.add_dirs, vec![launch.cwd.clone()]);
    assert!(!launch.cwd.to_string_lossy().starts_with(r"\\?\"));

    t.workspace.read_only = true;
    let launch = launcher
        .resolve(&t, &authorized_context(&policy, &t).await)
        .unwrap();
    assert_eq!(launch.mode, None, "read-only runs keep the CLI review mode");
}

#[tokio::test]
async fn a_snapshot_cannot_be_used_by_another_provider() {
    use telos_adapters::codex::CodexLaunchResolver;
    use telos_core::integration::CodexLauncher;

    let dir = Dir::new("cross");
    let policy = multi_policy(&dir, true);
    let claude_task = task();
    let context = authorized_context(&policy, &claude_task).await;
    let mut codex_task = claude_task.clone();
    codex_task.agent.provider = "codex".into();
    assert!(
        CodexLauncher::new(policy.clone())
            .resolve(&codex_task, &context)
            .is_err()
    );
    assert!(
        ClaudeLauncher::new(policy)
            .resolve(&claude_task, &context)
            .is_ok()
    );
}

#[test]
fn invalid_codex_and_agy_policies_are_rejected() {
    let dir = Dir::new("invalid-g2");
    let mut json = policy_json(&dir, true);
    json["codex"] = serde_json::json!({"models": ["m"], "effort": "low", "runTimeoutSecs": 0});
    assert!(LocalPolicyConfig::load(&dir.write_policy(json)).is_err());

    let mut json = policy_json(&dir, true);
    json["agy"] = serde_json::json!({"models": ["m"], "mode": "dangerously-skip-permissions"});
    assert!(LocalPolicyConfig::load(&dir.write_policy(json)).is_err());

    let mut json = policy_json(&dir, true);
    json["agy"] = serde_json::json!({"models": ["m"], "dataDir": "elsewhere"});
    assert!(LocalPolicyConfig::load(&dir.write_policy(json)).is_err());
}

// --- 마스터 (PH0-MASTER) ---

#[tokio::test]
async fn master_runs_are_read_only_and_get_no_claude_tools() {
    use telos_core::policy::{LookupDenial, TrustedLookup};

    let dir = Dir::new("master");
    let policy = load(&dir, true);
    let mut master = task();
    master.agent.role = "master".into();
    assert!(denied(&policy, &master).await.contains("read-only"));

    master.workspace.read_only = true;
    let launch = ClaudeLauncher::new(policy.clone())
        .resolve(&master, &authorized_context(&policy, &master).await)
        .unwrap();
    assert_eq!(launch.tools, Some(vec![]), "master gets no tools");
    assert!(launch.allowed_tools.is_empty());

    // 신뢰된 조회: Run 승인과 같은 규칙, 거부는 Denied.
    assert_eq!(policy.check_task(&master), Ok(()));
    let mut other = task();
    other.workspace.workspace_id = Some("not-granted".into());
    assert!(matches!(
        policy.check_task(&other),
        Err(LookupDenial::Denied(_))
    ));
}

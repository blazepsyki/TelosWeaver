//! 운영자 설정 기반 Phase 0 실행 승인과 Claude 실행 조건.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use telos_adapters::{
    AdapterError,
    claude::{ClaudeLaunch, ClaudeLaunchResolver},
};
use telos_schema::{
    CliDiscovery, FailureKind, RunContext, SessionMode, SessionRequest, TaskSpec, WorkspaceMode,
    api::{ApiError, ApiErrorCode},
};

use crate::execution::{RunAuthorizer, RunPermit, api_error, new_id};

/// 설정 파일 상한. 운영자 설정이며 큰 입력을 받지 않습니다.
const MAX_CONFIG_BYTES: u64 = 256 * 1024;

/// 읽기 전용 Workspace에서 끄는 도구입니다.
const WRITE_TOOLS: &[&str] = &["Write", "Edit", "NotebookEdit", "Bash"];

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocalPolicyConfig {
    /// 운영자가 바꿀 때마다 올리는 이름. 실제 Policy Revision은 이 값과 파일 해시로 만듭니다.
    pub revision: String,
    /// 허용하는 `TaskSpec::permission_profile_id`.
    pub permission_profiles: Vec<String>,
    pub workspaces: Vec<WorkspaceGrant>,
    #[serde(default)]
    pub claude: Option<ClaudePolicy>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceGrant {
    pub id: String,
    /// 실행 Host의 절대 경로. 시작 시 정규화합니다.
    pub path: PathBuf,
    /// `false`이면 `read_only` Task만 허용하고 쓰기 도구를 끕니다.
    pub writable: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClaudePolicy {
    /// 허용 모델. 첫 번째가 기본값이며 Task가 모델을 지정하면 이 목록에 있어야 합니다.
    pub models: Vec<String>,
    pub permission_mode: String,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    #[serde(default)]
    pub disallowed_tools: Vec<String>,
    #[serde(default)]
    pub tools: Option<Vec<String>>,
    #[serde(default)]
    pub setting_sources: Option<String>,
    /// §7.5-4 명시 경로. 없으면 확장에서 찾습니다.
    #[serde(default)]
    pub executable: Option<PathBuf>,
}

impl LocalPolicyConfig {
    /// 설정을 읽고 검사합니다. 실패하면 Daemon이 실행 승인 없이 시작하지 않도록 오류를 돌려줍니다.
    pub fn load(path: &Path) -> Result<(Self, String), String> {
        let metadata = std::fs::metadata(path).map_err(|e| format!("policy file: {e}"))?;
        if metadata.len() > MAX_CONFIG_BYTES {
            return Err("policy file is too large".into());
        }
        let bytes = std::fs::read(path).map_err(|e| format!("policy file: {e}"))?;
        let mut config: Self =
            serde_json::from_slice(&bytes).map_err(|e| format!("policy file: {e}"))?;
        config.validate()?;
        let digest = Sha256::digest(&bytes);
        let hash: String = digest[..6].iter().map(|b| format!("{b:02x}")).collect();
        let revision = format!("{}#{hash}", config.revision);
        Ok((config, revision))
    }

    fn validate(&mut self) -> Result<(), String> {
        if self.revision.trim().is_empty() {
            return Err("policy revision is empty".into());
        }
        let mut ids = HashSet::new();
        for grant in &mut self.workspaces {
            if grant.id.is_empty() || !ids.insert(grant.id.clone()) {
                return Err(format!(
                    "workspace id '{}' is empty or duplicated",
                    grant.id
                ));
            }
            if !grant.path.is_absolute() {
                return Err(format!("workspace '{}' path must be absolute", grant.id));
            }
            let canonical = grant
                .path
                .canonicalize()
                .map_err(|e| format!("workspace '{}': {e}", grant.id))?;
            if !canonical.is_dir() {
                return Err(format!("workspace '{}' is not a directory", grant.id));
            }
            grant.path = canonical;
        }
        if let Some(claude) = &self.claude {
            if claude.models.is_empty() {
                return Err("claude.models must not be empty".into());
            }
            if claude.permission_mode == "bypassPermissions" {
                return Err("claude.permissionMode bypassPermissions is not allowed".into());
            }
        }
        Ok(())
    }
}

/// 승인 시점에 고정한 실행 조건입니다. Phase 0 Context Snapshot의 최소 기록이며 메모리에만 둡니다.
#[derive(Debug, Clone)]
pub struct RunSnapshot {
    pub id: String,
    pub task_id: String,
    pub task_revision: u32,
    pub workspace_id: String,
    pub workspace_path: PathBuf,
    pub read_only: bool,
    pub provider: String,
    pub model: String,
    pub policy_revision: String,
    /// 이 Run이 쓸 CLI 탐지 결과 (§7.5-6).
    pub cli: Option<CliDiscovery>,
}

pub struct LocalRunPolicy {
    config: LocalPolicyConfig,
    revision: String,
    claude_cli: Option<CliDiscovery>,
    snapshots: Mutex<HashMap<String, RunSnapshot>>,
}

impl LocalRunPolicy {
    pub fn new(
        config: LocalPolicyConfig,
        revision: String,
        claude_cli: Option<CliDiscovery>,
    ) -> Self {
        Self {
            config,
            revision,
            claude_cli,
            snapshots: Mutex::new(HashMap::new()),
        }
    }

    pub fn revision(&self) -> &str {
        &self.revision
    }

    pub fn snapshot(&self, id: &str) -> Option<RunSnapshot> {
        self.snapshots.lock().unwrap().get(id).cloned()
    }

    fn check(&self, task: &TaskSpec, session: &SessionRequest) -> Result<RunSnapshot, ApiError> {
        let forbidden = |message: &str| api_error(ApiErrorCode::Forbidden, message);
        if !self
            .config
            .permission_profiles
            .contains(&task.permission_profile_id)
        {
            return Err(forbidden("permission profile is not granted"));
        }
        if task.agent.provider != telos_adapters::claude::PROVIDER_ID {
            return Err(forbidden("provider is not granted"));
        }
        let claude = self
            .config
            .claude
            .as_ref()
            .ok_or_else(|| forbidden("provider is not granted"))?;
        let model = match &task.agent.model {
            None => claude.models[0].clone(),
            Some(model) if claude.models.contains(model) => model.clone(),
            Some(_) => return Err(forbidden("model is not granted")),
        };
        if task.workspace.mode != WorkspaceMode::Directory {
            return Err(forbidden(
                "only granted directory workspaces are supported in Phase 0",
            ));
        }
        let grant = task
            .workspace
            .workspace_id
            .as_deref()
            .and_then(|id| self.config.workspaces.iter().find(|w| w.id == id))
            .ok_or_else(|| forbidden("workspace is not granted"))?;
        if !grant.writable && !task.workspace.read_only {
            return Err(forbidden("workspace is granted read-only"));
        }
        if session.mode == SessionMode::Capsule {
            return Err(forbidden("capsule sessions are not supported in Phase 0"));
        }
        Ok(RunSnapshot {
            id: new_id("ctx")?,
            task_id: task.id.clone(),
            task_revision: task.revision,
            workspace_id: grant.id.clone(),
            workspace_path: grant.path.clone(),
            read_only: task.workspace.read_only,
            provider: task.agent.provider.clone(),
            model,
            policy_revision: self.revision.clone(),
            cli: self.claude_cli.clone(),
        })
    }
}

#[async_trait]
impl RunAuthorizer for LocalRunPolicy {
    async fn authorize(
        &self,
        task: &TaskSpec,
        session: &SessionRequest,
    ) -> Result<RunPermit, ApiError> {
        let snapshot = self.check(task, session)?;
        let permit = RunPermit {
            workspace_id: snapshot.workspace_id.clone(),
            context_snapshot_id: snapshot.id.clone(),
            policy_revision: snapshot.policy_revision.clone(),
            controller_epoch: 1,
        };
        self.snapshots
            .lock()
            .unwrap()
            .insert(snapshot.id.clone(), snapshot);
        Ok(permit)
    }
}

/// 승인 Snapshot으로만 실행 조건을 만듭니다. Snapshot이 없거나 Task가 다르면 실행하지 않습니다.
pub struct ClaudeLauncher {
    policy: Arc<LocalRunPolicy>,
}

impl ClaudeLauncher {
    pub fn new(policy: Arc<LocalRunPolicy>) -> Self {
        Self { policy }
    }
}

impl ClaudeLaunchResolver for ClaudeLauncher {
    fn resolve(&self, task: &TaskSpec, context: &RunContext) -> Result<ClaudeLaunch, AdapterError> {
        let refuse = |message: &str| AdapterError::Failure {
            kind: FailureKind::Configuration,
            message: message.into(),
        };
        let snapshot = self
            .policy
            .snapshot(&context.context_snapshot_id)
            .ok_or_else(|| refuse("run is not authorized"))?;
        if snapshot.task_id != task.id
            || snapshot.task_revision != task.revision
            || snapshot.workspace_id != context.workspace_id
            || snapshot.policy_revision != context.policy_revision
        {
            return Err(refuse("run does not match its authorization"));
        }
        let claude = self
            .policy
            .config
            .claude
            .as_ref()
            .ok_or_else(|| refuse("provider is not granted"))?;
        let mut disallowed = claude.disallowed_tools.clone();
        if snapshot.read_only {
            for tool in WRITE_TOOLS {
                if !disallowed.iter().any(|t| t == tool) {
                    disallowed.push((*tool).into());
                }
            }
        }
        Ok(ClaudeLaunch {
            cwd: snapshot.workspace_path,
            prompt: prompt(task),
            model: Some(snapshot.model),
            permission_mode: claude.permission_mode.clone(),
            allowed_tools: claude.allowed_tools.clone(),
            disallowed_tools: disallowed,
            tools: claude.tools.clone(),
            setting_sources: claude.setting_sources.clone(),
        })
    }
}

/// Phase 0 최소 문맥 조립: 목표·제약·기대 출력만 씁니다. 입력 바인딩 해석은 P0 문맥 조립(§8)이 맡습니다.
fn prompt(task: &TaskSpec) -> String {
    let mut text = task.objective.clone();
    if !task.constraints.is_empty() {
        text.push_str("\n\nConstraints:");
        for constraint in &task.constraints {
            text.push_str("\n- ");
            text.push_str(constraint);
        }
    }
    let outputs: Vec<_> = task
        .outputs
        .iter()
        .filter_map(|o| {
            o.relative_path
                .as_ref()
                .map(|p| format!("{} ({p})", o.name))
        })
        .collect();
    if !outputs.is_empty() {
        text.push_str("\n\nWrite these outputs in the working directory:");
        for output in outputs {
            text.push_str("\n- ");
            text.push_str(&output);
        }
    }
    text
}

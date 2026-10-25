//! 운영자 설정 기반 Phase 0 실행 승인과 Provider별 실행 조건.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use telos_adapters::{
    AdapterError,
    agy::{AgyLaunch, AgyLaunchResolver},
    claude::{ClaudeLaunch, ClaudeLaunchResolver},
    codex::{CodexApprovals, CodexLaunch, CodexLaunchResolver, CodexSandbox},
};
use telos_schema::{
    CliDiscovery, FailureKind, RunContext, SessionMode, SessionRequest, TaskSpec, WorkspaceMode,
    api::{ApiError, ApiErrorCode},
};

use crate::{
    execution::{RunAuthorizer, RunPermit, api_error, new_id},
    policy::{LookupDenial, TrustedLookup},
};

/// 설정 파일 상한. 운영자 설정이며 큰 입력을 받지 않습니다.
const MAX_CONFIG_BYTES: u64 = 256 * 1024;

/// 읽기 전용 Workspace에서 끄는 Claude 도구입니다.
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
    #[serde(default)]
    pub codex: Option<CodexPolicy>,
    #[serde(default)]
    pub agy: Option<AgyPolicy>,
    /// RN Fork를 Core가 띄웁니다(Spec v1.5.1 §12.5, PH0-RN-07). 없으면 RN을 쓰지 않습니다.
    #[serde(default)]
    pub rn: Option<RnPolicy>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RnPolicy {
    /// `telos-rn` 실행 파일의 절대 경로. 자동 빌드·다운로드하지 않습니다.
    pub executable: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceGrant {
    pub id: String,
    /// 실행 Host의 절대 경로. 시작 시 정규화합니다.
    pub path: PathBuf,
    /// `false`이면 `read_only` Task만 허용하고 쓰기를 막습니다.
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
    #[serde(default)]
    pub vscode_listing: VsCodeListing,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodexPolicy {
    /// 허용 모델. 첫 번째가 기본값입니다. 다른 모델로 조용히 바꾸지 않습니다.
    pub models: Vec<String>,
    /// 추론 강도 (예: `low`).
    pub effort: String,
    /// Run 제한 시간(초).
    pub run_timeout_secs: u64,
    #[serde(default)]
    pub executable: Option<PathBuf>,
    /// `hidden`은 지원 수단이 없어 설정 단계에서 거부합니다 (§7.6, REQ-021).
    #[serde(default)]
    pub vscode_listing: VsCodeListing,
}

/// Managed Run 세션의 VS Code 확장 대화 목록 표시 (Spec §7.6, DEC-08, AGENT-05). 기본은 표시입니다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VsCodeListing {
    #[default]
    Listed,
    Hidden,
}

/// Antigravity 데이터 디렉터리 (Spec §7.5 확정 규칙).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgyDataDir {
    /// `--app_data_dir=antigravity`: 확장의 로그인 상태를 씁니다. 기본값.
    #[default]
    Extension,
    /// 옵션 생략: CLI 기본 디렉터리. 확장 대화 목록과 분리됩니다.
    Default,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgyPolicy {
    pub models: Vec<String>,
    #[serde(default)]
    pub effort: Option<String>,
    /// 쓰기 가능 Workspace에서 쓰는 실행 모드(`accept-edits` 등). 읽기 전용 Run은 CLI 기본(검토 요청)으로 둡니다.
    #[serde(default)]
    pub mode: Option<String>,
    /// `vscodeListing`과 같은 선택의 이전 표현입니다(`extension` = `listed`, `default` = `hidden`).
    #[serde(default)]
    pub data_dir: Option<AgyDataDir>,
    #[serde(default)]
    pub executable: Option<PathBuf>,
    #[serde(default)]
    pub vscode_listing: Option<VsCodeListing>,
}

impl AgyPolicy {
    /// 실제 적용할 표시 모드. 두 설정이 서로 다르면 오류입니다 (§7.6).
    pub fn listing(&self) -> Result<VsCodeListing, String> {
        let from_dir = self.data_dir.map(|dir| match dir {
            AgyDataDir::Extension => VsCodeListing::Listed,
            AgyDataDir::Default => VsCodeListing::Hidden,
        });
        match (self.vscode_listing, from_dir) {
            (Some(listing), Some(dir)) if listing != dir => {
                Err("agy.vscodeListing conflicts with agy.dataDir".into())
            }
            (listing, dir) => Ok(listing.or(dir).unwrap_or_default()),
        }
    }
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
            grant.path = plain_path(canonical);
        }
        if let Some(rn) = &self.rn
            && !rn.executable.is_absolute()
        {
            return Err("rn.executable must be an absolute path".into());
        }
        if let Some(claude) = &self.claude {
            if claude.models.is_empty() {
                return Err("claude.models must not be empty".into());
            }
            if claude.permission_mode == "bypassPermissions" {
                return Err("claude.permissionMode bypassPermissions is not allowed".into());
            }
        }
        if let Some(codex) = &self.codex {
            if codex.models.is_empty() || codex.effort.is_empty() {
                return Err("codex.models and codex.effort must not be empty".into());
            }
            if codex.run_timeout_secs == 0 {
                return Err("codex.runTimeoutSecs must be positive".into());
            }
            if codex.vscode_listing == VsCodeListing::Hidden {
                return Err(
                    "codex.vscodeListing hidden is not supported: App Server threads are listed as vscode (Spec §7.6)"
                        .into(),
                );
            }
        }
        if let Some(agy) = &self.agy {
            if agy.models.is_empty() {
                return Err("agy.models must not be empty".into());
            }
            if agy
                .mode
                .as_deref()
                .is_some_and(|m| m.contains("dangerously") || m == "bypassPermissions")
            {
                return Err("agy.mode must not bypass permissions".into());
            }
            agy.listing()?;
        }
        Ok(())
    }

    /// Provider별 표시 모드와 실제 지정 방식 (§7.6). 정책에 없는 Provider는 `None`입니다.
    pub fn vscode_listing(&self, provider: &str) -> Option<(VsCodeListing, &'static str)> {
        use VsCodeListing::*;
        let listing = match provider {
            telos_adapters::claude::PROVIDER_ID => self.claude.as_ref()?.vscode_listing,
            telos_adapters::codex::PROVIDER_ID => self.codex.as_ref()?.vscode_listing,
            telos_adapters::agy::PROVIDER_ID => self.agy.as_ref()?.listing().ok()?,
            _ => return None,
        };
        let method = match (provider, listing) {
            (telos_adapters::claude::PROVIDER_ID, Listed) => "CLAUDE_CODE_ENTRYPOINT=claude-vscode",
            (telos_adapters::claude::PROVIDER_ID, Hidden) => "CLAUDE_CODE_ENTRYPOINT=sdk-cli",
            (telos_adapters::codex::PROVIDER_ID, _) => {
                "persisted thread (ephemeral=false, threadSource=vscode)"
            }
            (_, Listed) => "--app_data_dir=antigravity",
            (_, Hidden) => "--app_data_dir omitted (antigravity-cli)",
        };
        Some((listing, method))
    }

    /// Provider별 허용 모델. 정책에 없는 Provider는 `None`입니다.
    fn models(&self, provider: &str) -> Option<&[String]> {
        match provider {
            telos_adapters::claude::PROVIDER_ID => self.claude.as_ref().map(|p| &p.models[..]),
            telos_adapters::codex::PROVIDER_ID => self.codex.as_ref().map(|p| &p.models[..]),
            telos_adapters::agy::PROVIDER_ID => self.agy.as_ref().map(|p| &p.models[..]),
            _ => None,
        }
    }
}

/// Windows `canonicalize`가 만드는 확장 길이 드라이브 경로(`\\?\C:\...`)를 일반 형식(`C:\...`)으로 바꿉니다.
/// Antigravity는 확장 길이 경로를 작업 영역으로 인식하지 못하고 자체 scratch 폴더에 씁니다(G2 실환경 관찰).
/// UNC(`\\?\UNC\...`) 등 다른 형식은 그대로 둡니다.
fn plain_path(path: PathBuf) -> PathBuf {
    match path.to_str().and_then(|s| s.strip_prefix(r"\\?\")) {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
        _ => path,
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
    /// VS Code 확장 목록 표시 모드와 실제 지정 방식 (§7.6, DEC-08).
    pub vscode_listing: VsCodeListing,
    pub vscode_listing_method: &'static str,
}

pub struct LocalRunPolicy {
    config: LocalPolicyConfig,
    revision: String,
    discoveries: HashMap<String, CliDiscovery>,
    snapshots: Mutex<HashMap<String, RunSnapshot>>,
}

impl LocalRunPolicy {
    /// `discoveries`는 Provider ID별 CLI 탐지 결과입니다.
    pub fn new(
        config: LocalPolicyConfig,
        revision: String,
        discoveries: HashMap<String, CliDiscovery>,
    ) -> Self {
        Self {
            config,
            revision,
            discoveries,
            snapshots: Mutex::new(HashMap::new()),
        }
    }

    pub fn config(&self) -> &LocalPolicyConfig {
        &self.config
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
        let models = self
            .config
            .models(&task.agent.provider)
            .ok_or_else(|| forbidden("provider is not granted"))?;
        let model = match &task.agent.model {
            None => models[0].clone(),
            Some(model) if models.contains(model) => model.clone(),
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
        // 마스터는 판단만 하고 작업은 Worker가 합니다(배정표 §6). 마스터 Run은 읽기 전용이어야 합니다.
        if task.agent.role == "master" && !task.workspace.read_only {
            return Err(forbidden("master runs must be read-only"));
        }
        if session.mode == SessionMode::Capsule {
            return Err(forbidden("capsule sessions are not supported in Phase 0"));
        }
        let (vscode_listing, vscode_listing_method) = self
            .config
            .vscode_listing(&task.agent.provider)
            .ok_or_else(|| forbidden("provider is not granted"))?;
        Ok(RunSnapshot {
            vscode_listing,
            vscode_listing_method,
            id: new_id("ctx")?,
            task_id: task.id.clone(),
            task_revision: task.revision,
            workspace_id: grant.id.clone(),
            workspace_path: grant.path.clone(),
            read_only: task.workspace.read_only,
            provider: task.agent.provider.clone(),
            model,
            policy_revision: self.revision.clone(),
            cli: self.discoveries.get(&task.agent.provider).cloned(),
        })
    }

    /// 실행 조건을 만들기 전에 Run이 승인 Snapshot과 일치하는지 확인합니다.
    fn authorized(
        &self,
        provider: &str,
        task: &TaskSpec,
        context: &RunContext,
    ) -> Result<RunSnapshot, AdapterError> {
        let snapshot = self
            .snapshot(&context.context_snapshot_id)
            .ok_or_else(|| refuse("run is not authorized"))?;
        if snapshot.provider != provider
            || snapshot.task_id != task.id
            || snapshot.task_revision != task.revision
            || snapshot.workspace_id != context.workspace_id
            || snapshot.policy_revision != context.policy_revision
        {
            return Err(refuse("run does not match its authorization"));
        }
        Ok(snapshot)
    }
}

fn refuse(message: &str) -> AdapterError {
    AdapterError::Failure {
        kind: FailureKind::Configuration,
        message: message.into(),
    }
}

/// 결정 단계의 신뢰된 조회(REQ-008). Run 승인과 같은 규칙으로 검사하되 Snapshot을 남기지 않습니다.
impl TrustedLookup for LocalRunPolicy {
    fn check_task(&self, task: &TaskSpec) -> Result<(), LookupDenial> {
        let fresh = SessionRequest {
            mode: SessionMode::Fresh,
            source_session_id: None,
            source_turn_id: None,
            capsule_version_id: None,
        };
        self.check(task, &fresh)
            .map(|_| ())
            .map_err(|error| match error.code {
                ApiErrorCode::Forbidden => LookupDenial::Denied(error.message),
                _ => LookupDenial::Unavailable(error.message),
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

/// 승인 Snapshot으로만 Claude 실행 조건을 만듭니다.
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
        let snapshot =
            self.policy
                .authorized(telos_adapters::claude::PROVIDER_ID, task, context)?;
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
        // 마스터 Run에는 도구를 주지 않습니다. 결정 JSON만 출력합니다(배정표 §6).
        let master = task.agent.role == "master";
        Ok(ClaudeLaunch {
            cwd: snapshot.workspace_path,
            prompt: prompt(task),
            model: Some(snapshot.model),
            permission_mode: claude.permission_mode.clone(),
            allowed_tools: if master {
                vec![]
            } else {
                claude.allowed_tools.clone()
            },
            disallowed_tools: if master { vec![] } else { disallowed },
            tools: if master {
                Some(vec![])
            } else {
                claude.tools.clone()
            },
            setting_sources: claude.setting_sources.clone(),
        })
    }
}

/// 승인 Snapshot으로만 Codex 실행 조건을 만듭니다. 실행 중 승인 요청은 Phase 0에서 거부합니다.
pub struct CodexLauncher {
    policy: Arc<LocalRunPolicy>,
}

impl CodexLauncher {
    pub fn new(policy: Arc<LocalRunPolicy>) -> Self {
        Self { policy }
    }
}

impl CodexLaunchResolver for CodexLauncher {
    fn resolve(&self, task: &TaskSpec, context: &RunContext) -> Result<CodexLaunch, AdapterError> {
        let snapshot = self
            .policy
            .authorized(telos_adapters::codex::PROVIDER_ID, task, context)?;
        let codex = self
            .policy
            .config
            .codex
            .as_ref()
            .ok_or_else(|| refuse("provider is not granted"))?;
        Ok(CodexLaunch {
            cwd: snapshot.workspace_path,
            prompt: prompt(task),
            model: snapshot.model,
            effort: codex.effort.clone(),
            sandbox: if snapshot.read_only {
                CodexSandbox::ReadOnly
            } else {
                CodexSandbox::WorkspaceWrite
            },
            approvals: CodexApprovals::Deny,
            run_timeout: Duration::from_secs(codex.run_timeout_secs),
        })
    }
    // `authorize_approval`은 기본 구현(미지원 거부)을 그대로 씁니다.
}

/// 승인 Snapshot으로만 Antigravity 실행 조건을 만듭니다.
pub struct AgyLauncher {
    policy: Arc<LocalRunPolicy>,
}

impl AgyLauncher {
    pub fn new(policy: Arc<LocalRunPolicy>) -> Self {
        Self { policy }
    }
}

impl AgyLaunchResolver for AgyLauncher {
    fn resolve(&self, task: &TaskSpec, context: &RunContext) -> Result<AgyLaunch, AdapterError> {
        let snapshot = self
            .policy
            .authorized(telos_adapters::agy::PROVIDER_ID, task, context)?;
        let agy = self
            .policy
            .config
            .agy
            .as_ref()
            .ok_or_else(|| refuse("provider is not granted"))?;
        let cwd = snapshot.workspace_path;
        Ok(AgyLaunch {
            cwd: cwd.clone(),
            prompt: prompt(task),
            model: Some(snapshot.model),
            effort: agy.effort.clone(),
            // 읽기 전용 Run은 CLI 기본(검토 요청)으로 두어 쓰기 도구가 Headless에서 거부되게 합니다 (§7.4).
            mode: if snapshot.read_only {
                None
            } else {
                agy.mode.clone()
            },
            // Antigravity는 cwd만으로 작업 영역을 정하지 않고 자체 scratch 폴더에 쓸 수 있으므로,
            // 승인된 Workspace를 작업 영역으로 명시합니다 (§7.2, G2 실환경 관찰).
            add_dirs: vec![cwd.clone()],
            disable_slash_commands: true,
            extra_args: vec![],
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

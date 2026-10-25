//! Phase 0 통합 구성 (G1~G3, Spec §19.5.8). 소유: PH0-INTEGRATION.
//!
//! - [`LocalRunPolicy`]: 운영자가 `serve --policy <file>`로 준 설정 파일만 근거로 Run을 승인합니다.
//!   Local API 클라이언트가 보낸 경로·권한을 그대로 믿지 않습니다 (§7.2, §9.1).
//!   정식 정책·Snapshot 서비스는 MASTER(`core/policy`)와 P0 구현이 대체합니다.
//! - [`ClaudeLauncher`]·[`CodexLauncher`]·[`AgyLauncher`]: 승인 때 기록한 Snapshot으로 Provider별 실행 조건을 만듭니다.
//! - [`discover`]: §7.5 탐지로 확장의 Provider CLI를 찾습니다.
//! - [`TrackedAdapter`]: Daemon 종료 시 활성 Run을 취소하고 실제 종료 이벤트를 확인합니다.
//! - [`RnService`]: 운영자 정책의 RN Fork를 프로젝트별 상태 폴더로 띄우고 연결하며 `core.health`의 `rn`으로 알립니다
//!   (Spec v1.5.1 §12.3·§12.5, PH0-RN-07).

mod policy;
mod rn;
mod tracked;

pub use policy::{
    AgyDataDir, AgyLauncher, AgyPolicy, ClaudeLauncher, ClaudePolicy, CodexLauncher, CodexPolicy,
    LocalPolicyConfig, LocalRunPolicy, RnPolicy, RunSnapshot, WorkspaceGrant,
};
pub use rn::{RnLaunch, RnService};
pub use tracked::{ShutdownReport, TrackedAdapter};

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use telos_adapters::{
    AgentAdapter, AgyAdapter, ClaudeCodeAdapter, CliDiscoverer, CodexAdapter, DiscoveryRequest,
    NativeCliDiscoverer, ProviderCliRule,
    agy::{AgyConfig, AgyDataDirMode},
    claude::ClaudeCodeConfig,
    codex::CodexConfig,
    runtime::ProcessRuntime,
};
use telos_schema::{CliDiscovery, CliDiscoveryFailure, CliDiscoveryOutcome};

/// 취소 시 정상 종료를 기다리는 시간. 지나면 강제 종료합니다.
const CANCEL_GRACE: Duration = Duration::from_secs(5);
/// Codex App Server 요청 하나의 제한 시간.
const CODEX_REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

/// 실행 Host의 VS Code `targetPlatform`.
pub fn target_platform() -> Option<&'static str> {
    Some(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "win32-x64",
        ("windows", "aarch64") => "win32-arm64",
        ("linux", "x86_64") => "linux-x64",
        ("linux", "aarch64") => "linux-arm64",
        ("macos", "x86_64") => "darwin-x64",
        ("macos", "aarch64") => "darwin-arm64",
        _ => return None,
    })
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}

/// §7.5-1 탐색 위치. 원격·WSL 실행 Host는 `~/.vscode-server/extensions`를 먼저 봅니다.
fn extension_roots(home: &std::path::Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if !cfg!(windows) {
        roots.push(home.join(".vscode-server/extensions"));
    }
    roots.push(home.join(".vscode/extensions"));
    roots.push(home.join(".vscode-insiders/extensions"));
    roots
}

/// 확장의 Provider CLI를 찾습니다. `rule`은 Provider 모듈의 `cli_rule`, `explicit`은 운영자 설정의 명시 경로입니다 (§7.5-4).
pub async fn discover(
    runtime: Arc<dyn ProcessRuntime>,
    host_id: &str,
    provider_id: &str,
    rule: fn(&str) -> Option<ProviderCliRule>,
    explicit: Option<PathBuf>,
) -> CliDiscovery {
    let checked_at = timestamp();
    let unavailable = |failure, reason: &str| CliDiscovery {
        provider_id: provider_id.into(),
        host_id: host_id.into(),
        checked_at: checked_at.clone(),
        outcome: CliDiscoveryOutcome::Unavailable {
            failure,
            source: None,
            reason: reason.into(),
        },
    };
    let Some(platform) = target_platform() else {
        return unavailable(
            CliDiscoveryFailure::UnsupportedPlatform,
            "unknown host platform",
        );
    };
    let Some(rule) = rule(platform) else {
        return unavailable(CliDiscoveryFailure::UnsupportedPlatform, "no CLI rule");
    };
    let Some(home) = home_dir() else {
        return unavailable(CliDiscoveryFailure::IoError, "home directory is unknown");
    };
    let request = DiscoveryRequest {
        host_id: host_id.into(),
        target_platform: platform.into(),
        explicit_path: explicit,
        extension_roots: extension_roots(&home),
        home_dir: home,
        path_env: std::env::var_os("PATH"),
        checked_at: checked_at.clone(),
    };
    NativeCliDiscoverer::new(runtime)
        .discover(provider_id, &rule, &request)
        .await
}

/// Daemon의 실행 구성입니다. 정책 파일이 없으면 모든 Run을 거부합니다(`DenyRuns`).
pub struct Phase0Execution {
    pub supervisor: Arc<crate::execution::ExecutionSupervisor>,
    /// 등록한 Provider Adapter. 종료 시 활성 Run을 정리합니다.
    pub adapters: Vec<Arc<TrackedAdapter>>,
    /// 정책에 있는 Provider의 CLI 탐지 결과.
    pub discoveries: Vec<CliDiscovery>,
    /// Codex 시험 Thread의 Archive(소유 Thread만)에 씁니다.
    pub codex: Option<Arc<CodexAdapter>>,
    /// Provider를 등록하지 않았거나 일부 기능을 막은 사유. 운영자에게 보여 줍니다.
    pub notes: Vec<String>,
    pub policy_revision: Option<String>,
    /// 운영자 정책. 마스터 결정의 신뢰된 조회(REQ-008)에도 씁니다.
    pub policy: Option<Arc<LocalRunPolicy>>,
}

impl Phase0Execution {
    pub async fn build(
        project_id: String,
        host_id: String,
        policy_file: Option<&std::path::Path>,
    ) -> Result<Self, String> {
        use crate::execution::{DenyRuns, ExecutionSupervisor};
        let Some(path) = policy_file else {
            return Ok(Self {
                supervisor: Arc::new(ExecutionSupervisor::new(
                    project_id,
                    host_id,
                    vec![],
                    Arc::new(DenyRuns),
                )),
                adapters: vec![],
                discoveries: vec![],
                codex: None,
                notes: vec![],
                policy_revision: None,
                policy: None,
            });
        };
        let (config, revision) = LocalPolicyConfig::load(path)?;
        let runtime: Arc<dyn ProcessRuntime> = Arc::new(telos_adapters::runtime::NativeRuntime);

        let mut discoveries = HashMap::new();
        if let Some(claude) = &config.claude {
            let found = discover(
                runtime.clone(),
                &host_id,
                telos_adapters::claude::PROVIDER_ID,
                telos_adapters::claude::cli_rule,
                claude.executable.clone(),
            )
            .await;
            discoveries.insert(telos_adapters::claude::PROVIDER_ID.to_owned(), found);
        }
        if let Some(codex) = &config.codex {
            let found = discover(
                runtime.clone(),
                &host_id,
                telos_adapters::codex::PROVIDER_ID,
                telos_adapters::codex::cli_rule,
                codex.executable.clone(),
            )
            .await;
            discoveries.insert(telos_adapters::codex::PROVIDER_ID.to_owned(), found);
        }
        if let Some(agy) = &config.agy {
            let found = discover(
                runtime.clone(),
                &host_id,
                telos_adapters::agy::PROVIDER_ID,
                telos_adapters::agy::cli_rule,
                agy.executable.clone(),
            )
            .await;
            discoveries.insert(telos_adapters::agy::PROVIDER_ID.to_owned(), found);
        }

        let agy_data_dir = config.agy.as_ref().map(|a| a.data_dir);
        let policy = Arc::new(LocalRunPolicy::new(
            config,
            revision.clone(),
            discoveries.clone(),
        ));
        let mut registered: Vec<Arc<dyn AgentAdapter>> = Vec::new();
        let mut notes = Vec::new();
        let mut codex_handle = None;

        if let Some(cli) = discoveries
            .get(telos_adapters::claude::PROVIDER_ID)
            .and_then(CliDiscovery::found)
        {
            registered.push(Arc::new(ClaudeCodeAdapter::new(ClaudeCodeConfig {
                executable: PathBuf::from(&cli.executable_path),
                cli_version: cli.cli_version.clone(),
                host_id: host_id.clone(),
                runtime: runtime.clone(),
                launcher: Arc::new(ClaudeLauncher::new(policy.clone())),
                cancel_grace: CANCEL_GRACE,
            })));
        }
        if let Some(cli) = discoveries
            .get(telos_adapters::codex::PROVIDER_ID)
            .and_then(CliDiscovery::found)
        {
            let codex = Arc::new(CodexAdapter::new(CodexConfig {
                executable: PathBuf::from(&cli.executable_path),
                cli_version: cli.cli_version.clone(),
                host_id: host_id.clone(),
                runtime: runtime.clone(),
                launcher: Arc::new(CodexLauncher::new(policy.clone())),
                request_timeout: CODEX_REQUEST_TIMEOUT,
                cancel_grace: CANCEL_GRACE,
            }));
            codex_handle = Some(codex.clone());
            registered.push(codex);
        }
        if let (Some(cli), Some(data_dir)) = (
            discoveries
                .get(telos_adapters::agy::PROVIDER_ID)
                .and_then(CliDiscovery::found),
            agy_data_dir,
        ) {
            let executable = PathBuf::from(&cli.executable_path);
            let (data_dir_mode, option_supported) = match data_dir {
                AgyDataDir::Default => (AgyDataDirMode::DefaultCli, true),
                AgyDataDir::Extension => {
                    // §7.5 확정 규칙: 실행 전 옵션 수용을 확인하고, 거부되면 다른 디렉터리로 바꾸지 않습니다.
                    let checked = AgyAdapter::verify_extension_data_dir(
                        runtime.as_ref(),
                        &executable,
                        &std::env::temp_dir(),
                    )
                    .await;
                    if let Err(error) = &checked {
                        notes.push(format!(
                            "agy: --app_data_dir is not accepted; runs are unsupported ({error})"
                        ));
                    }
                    (AgyDataDirMode::ExtensionShared, checked.is_ok())
                }
            };
            registered.push(Arc::new(AgyAdapter::new(AgyConfig {
                executable,
                cli_version: cli.cli_version.clone(),
                host_id: host_id.clone(),
                runtime: runtime.clone(),
                launcher: Arc::new(AgyLauncher::new(policy.clone())),
                cancel_grace: CANCEL_GRACE,
                data_dir_mode,
                data_dir_option_supported: option_supported,
            })));
        }
        for discovery in discoveries.values() {
            if let CliDiscoveryOutcome::Unavailable {
                failure, reason, ..
            } = &discovery.outcome
            {
                notes.push(format!(
                    "{}: not registered ({failure:?}: {reason})",
                    discovery.provider_id
                ));
            }
        }

        let adapters: Vec<_> = registered.into_iter().map(TrackedAdapter::new).collect();
        let tracked = adapters
            .iter()
            .map(|a| a.clone() as Arc<dyn AgentAdapter>)
            .collect();
        let mut discoveries: Vec<_> = discoveries.into_values().collect();
        discoveries.sort_by(|a, b| a.provider_id.cmp(&b.provider_id));
        Ok(Self {
            supervisor: Arc::new(ExecutionSupervisor::new(
                project_id,
                host_id,
                tracked,
                policy.clone(),
            )),
            adapters,
            discoveries,
            codex: codex_handle,
            notes,
            policy_revision: Some(revision),
            policy: Some(policy),
        })
    }

    /// Plan 하나의 마스터 제어를 만듭니다. 권한 상한은 운영자 정책에서 가져옵니다.
    /// Phase 0 정책에는 예산 Profile이 없으므로 `default` 하나만 허용합니다.
    pub fn master(
        &self,
        plan_id: &str,
        options: MasterOptions,
    ) -> Result<crate::master::MasterController, String> {
        let policy = self
            .policy
            .clone()
            .ok_or("a policy file is required for the master controller")?;
        let config = policy.config();
        let mut providers = Vec::new();
        if config.claude.is_some() {
            providers.push(telos_adapters::claude::PROVIDER_ID.to_owned());
        }
        if config.codex.is_some() {
            providers.push(telos_adapters::codex::PROVIDER_ID.to_owned());
        }
        if config.agy.is_some() {
            providers.push(telos_adapters::agy::PROVIDER_ID.to_owned());
        }
        let settings = crate::master::MasterSettings {
            project_id: self.supervisor.project_id().to_owned(),
            plan_id: plan_id.to_owned(),
            host_id: self.supervisor.host_id().to_owned(),
            policy_revision: policy.revision().to_owned(),
            limits: telos_schema::ControlLimits::default(),
            allowed_providers: providers,
            allowed_workspace_modes: vec![telos_schema::WorkspaceMode::Directory],
            allowed_permission_profile_ids: config.permission_profiles.clone(),
            allowed_budget_profile_ids: vec!["default".into()],
            master_provider: options.provider,
            master_model: options.model,
            master_workspace_id: options.workspace_id,
            master_permission_profile_id: options.permission_profile_id,
            worker_template: options.worker_template,
            turn_timeout: options.turn_timeout,
        };
        Ok(crate::master::MasterController::new(
            settings,
            Arc::new(self.supervisor.clone()),
            policy,
        ))
    }

    /// RN Fork를 띄우고(정책에 `rn` 절이 있을 때) `core.health`에 `rn`으로 알립니다. 상태 폴더는 Local API 토큰이 있는
    /// Core 프로젝트별 상태 폴더 아래 `rn`입니다(Spec v1.5.1 §12.5). 사용자 단위 공유 RN으로 바꿀 때 이 함수만 바꿉니다.
    pub async fn start_rn(&self, api_token: &std::path::Path) -> Arc<RnService> {
        let runtime: Arc<dyn ProcessRuntime> = Arc::new(telos_adapters::runtime::NativeRuntime);
        let rn = match self.policy.as_ref().and_then(|p| p.config().rn.clone()) {
            None => RnService::not_configured(runtime),
            Some(policy) => {
                let state = api_token
                    .parent()
                    .map(std::path::Path::to_path_buf)
                    .unwrap_or_default();
                let codex_bin = self
                    .discovery(telos_adapters::codex::PROVIDER_ID)
                    .and_then(CliDiscovery::found)
                    .map(|cli| PathBuf::from(&cli.executable_path));
                let launch = RnLaunch {
                    executable: policy.executable,
                    home: state.join("rn"),
                    host_id: self.supervisor.host_id().to_owned(),
                    codex_bin,
                };
                RnService::start(launch, runtime).await
            }
        };
        let component = rn.clone();
        self.supervisor
            .add_health_component(Arc::new(move || component.component()));
        rn
    }

    pub fn discovery(&self, provider_id: &str) -> Option<&CliDiscovery> {
        self.discoveries
            .iter()
            .find(|d| d.provider_id == provider_id)
    }

    /// 활성 Run을 취소하고 종료를 확인합니다. 확인하지 못한 Run이 있으면 보고합니다.
    pub async fn shutdown(&self, timeout: Duration) -> ShutdownReport {
        let mut total = ShutdownReport::default();
        for adapter in &self.adapters {
            let report = adapter.shutdown(timeout).await;
            total.cancelled += report.cancelled;
            total.unconfirmed.extend(report.unconfirmed);
        }
        total
    }
}

/// 마스터 실행 조건 (배정표 §6: Phase 0 마스터 Provider는 임시 Claude).
#[derive(Debug, Clone)]
pub struct MasterOptions {
    pub provider: String,
    pub model: Option<String>,
    /// 마스터가 읽기 전용으로 쓰는 승인된 Workspace.
    pub workspace_id: String,
    pub permission_profile_id: String,
    pub worker_template: telos_schema::TaskSpec,
    pub turn_timeout: Duration,
}

/// RFC 3339 UTC 초 단위 시각.
pub(crate) fn timestamp() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    let (days, rem) = ((seconds / 86_400) as i64, seconds % 86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        rem % 3_600 / 60,
        rem % 60
    )
}

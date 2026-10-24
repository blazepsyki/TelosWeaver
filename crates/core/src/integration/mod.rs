//! Phase 0 통합 구성 (G1~G3, Spec §19.5.8). 소유: PH0-INTEGRATION.
//!
//! - [`LocalRunPolicy`]: 운영자가 `serve --policy <file>`로 준 설정 파일만 근거로 Run을 승인합니다.
//!   Local API 클라이언트가 보낸 경로·권한을 그대로 믿지 않습니다 (§7.2, §9.1).
//!   정식 정책·Snapshot 서비스는 MASTER(`core/policy`)와 P0 구현이 대체합니다.
//! - [`ClaudeLauncher`]: 승인 때 기록한 Snapshot으로 Claude 실행 조건을 만듭니다.
//! - [`discover_claude`]: §7.5 탐지로 확장의 Claude CLI를 찾습니다.
//! - [`TrackedAdapter`]: Daemon 종료 시 활성 Run을 취소하고 실제 종료 이벤트를 확인합니다.

mod policy;
mod tracked;

pub use policy::{
    ClaudeLauncher, ClaudePolicy, LocalPolicyConfig, LocalRunPolicy, RunSnapshot, WorkspaceGrant,
};
pub use tracked::{ShutdownReport, TrackedAdapter};

use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use telos_adapters::{
    AgentAdapter, ClaudeCodeAdapter, CliDiscoverer, DiscoveryRequest, NativeCliDiscoverer,
    claude::{ClaudeCodeConfig, cli_rule},
    runtime::ProcessRuntime,
};
use telos_schema::{CliDiscovery, CliDiscoveryFailure, CliDiscoveryOutcome};

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

/// 확장의 Claude CLI를 찾습니다. `explicit`은 운영자 설정의 명시 경로입니다 (§7.5-4).
pub async fn discover_claude(
    runtime: Arc<dyn ProcessRuntime>,
    host_id: &str,
    explicit: Option<PathBuf>,
) -> CliDiscovery {
    let checked_at = timestamp();
    let unavailable = |failure, reason: &str| CliDiscovery {
        provider_id: telos_adapters::claude::PROVIDER_ID.into(),
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
    let Some(rule) = cli_rule(platform) else {
        return unavailable(
            CliDiscoveryFailure::UnsupportedPlatform,
            "no Claude CLI rule",
        );
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
        .discover(telos_adapters::claude::PROVIDER_ID, &rule, &request)
        .await
}

/// 탐지에 성공했으면 Claude Adapter를 만듭니다. 실패하면 등록하지 않습니다(`provider is not registered`).
pub fn claude_adapter(
    discovery: &CliDiscovery,
    runtime: Arc<dyn ProcessRuntime>,
    policy: Arc<LocalRunPolicy>,
) -> Option<Arc<dyn AgentAdapter>> {
    let cli = discovery.found()?;
    Some(Arc::new(ClaudeCodeAdapter::new(ClaudeCodeConfig {
        executable: PathBuf::from(&cli.executable_path),
        cli_version: cli.cli_version.clone(),
        host_id: discovery.host_id.clone(),
        runtime,
        launcher: Arc::new(ClaudeLauncher::new(policy)),
        cancel_grace: Duration::from_secs(5),
    })))
}

/// Daemon의 실행 구성입니다. 정책 파일이 없으면 모든 Run을 거부합니다(`DenyRuns`).
pub struct Phase0Execution {
    pub supervisor: Arc<crate::execution::ExecutionSupervisor>,
    /// 등록한 Provider Adapter. 종료 시 활성 Run을 정리합니다.
    pub adapters: Vec<Arc<TrackedAdapter>>,
    pub claude_discovery: Option<CliDiscovery>,
    pub policy_revision: Option<String>,
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
                claude_discovery: None,
                policy_revision: None,
            });
        };
        let (config, revision) = LocalPolicyConfig::load(path)?;
        let runtime: Arc<dyn ProcessRuntime> = Arc::new(telos_adapters::runtime::NativeRuntime);
        let discovery = match &config.claude {
            Some(claude) => {
                Some(discover_claude(runtime.clone(), &host_id, claude.executable.clone()).await)
            }
            None => None,
        };
        let policy = Arc::new(LocalRunPolicy::new(
            config,
            revision.clone(),
            discovery.clone(),
        ));
        let adapters: Vec<_> = discovery
            .as_ref()
            .and_then(|d| claude_adapter(d, runtime, policy.clone()))
            .map(TrackedAdapter::new)
            .into_iter()
            .collect();
        let registered = adapters
            .iter()
            .map(|a| a.clone() as Arc<dyn AgentAdapter>)
            .collect();
        Ok(Self {
            supervisor: Arc::new(ExecutionSupervisor::new(
                project_id, host_id, registered, policy,
            )),
            adapters,
            claude_discovery: discovery,
            policy_revision: Some(revision),
        })
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

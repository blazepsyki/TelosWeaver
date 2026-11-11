//! Spec §7.1 Capability 판정 기준 (H0 Capability 판정, 이월 C5, H0 D11·D14)
//!
//! Adapter는 기능의 수단이 없으면 `unsupported`를 사유와 함께 보고합니다. 수단이 있는 기능의 `supported`/`unverified`는
//! 이 모듈의 [`judge`]로 정해 Provider 간 같은 증거에 같은 상태를 줍니다.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{CapabilityState, SessionMode, capability_keys};

/// 실환경 시험 한 건의 결과입니다 (`provider_capabilities`, §14.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityEvidence {
    /// 공통 Capability 키
    pub key: String,
    pub provider: String,
    pub host_id: String,
    /// 예: `win32-x64`, `linux-x64`
    pub platform: String,
    pub cli_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extension_version: Option<String>,
    /// 시험 이름(예: `env_resume_and_fork`)
    pub test: String,
    pub passed: bool,
    pub checked_at: String,
    /// 증거 문서·Commit
    pub evidence_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "check",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum PreflightCheck {
    /// `--version` 출력이 기대 형식인지
    VersionOutput,
    /// 사용하는 옵션을 CLI가 받아들이는지(예: `--app_data_dir=antigravity --version`)
    OptionAccepted { option: String },
    /// `--help`에 필수 옵션이 있는지
    HelpContains { flag: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PreflightResult {
    pub check: PreflightCheck,
    pub passed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Core 시작 때와 CLI 버전 변화를 감지했을 때 모델을 호출하지 않고 실행하는 점검입니다 (H0 D14).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PreflightReport {
    pub provider: String,
    pub host_id: String,
    pub cli_version: String,
    pub results: Vec<PreflightResult>,
    pub checked_at: String,
}

impl PreflightReport {
    pub fn passed(&self) -> bool {
        !self.results.is_empty() && self.results.iter().all(|r| r.passed)
    }
}

/// 버전 문자열에서 처음 나오는 `major.minor`를 찾습니다. 예: `codex-cli 0.164.0-alpha.11.1` → (0, 164).
pub fn major_minor(version: &str) -> Option<(u64, u64)> {
    version
        .split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .find_map(|token| {
            let mut parts = token.split('.');
            let major = parts.next()?.parse().ok()?;
            let minor = parts.next()?.parse().ok()?;
            Some((major, minor))
        })
}

/// 같은 Host·플랫폼의 통과한 실환경 증거와 현재 버전·사전 점검으로 상태를 정합니다 (H0 D11·D14).
/// 같은 버전의 증거가 있으면 `supported`, 같은 major.minor의 증거가 있고 사전 점검을 통과하면 `supported`,
/// 그 밖에는 `unverified`입니다. 사전 점검이 없거나 실패하면 같은 major.minor라도 인정하지 않습니다.
pub fn judge(
    passed_versions: &[&str],
    current: &str,
    preflight: Option<&PreflightReport>,
) -> CapabilityState {
    if passed_versions.contains(&current) {
        return CapabilityState::Supported;
    }
    let same_line = major_minor(current)
        .is_some_and(|mm| passed_versions.iter().any(|v| major_minor(v) == Some(mm)));
    if same_line && preflight.is_some_and(PreflightReport::passed) {
        CapabilityState::Supported
    } else {
        CapabilityState::Unverified
    }
}

impl CapabilityState {
    /// Core는 `supported`인 기능만 씁니다. `unverified`·`unsupported`·키 없음은 쓰지 않고 대체하거나 거부합니다 (§7.1).
    pub fn permits_use(self) -> bool {
        self == Self::Supported
    }
}

/// 요청한 문맥 전략을 쓸 수 없을 때의 대체입니다. Capsule로 대체하며 필수 입력·권한은 그대로 둡니다 (§8.2 6번, E2E-13).
/// 대체가 필요 없으면 `None`, 대체하면 (전략, 거부된 Capability 키)입니다.
pub fn session_fallback(
    requested: SessionMode,
    resume: CapabilityState,
    fork: CapabilityState,
) -> Option<(SessionMode, &'static str)> {
    match requested {
        SessionMode::Resume if !resume.permits_use() => {
            Some((SessionMode::Capsule, capability_keys::NATIVE_RESUME))
        }
        SessionMode::NativeFork if !fork.permits_use() => {
            Some((SessionMode::Capsule, capability_keys::NATIVE_FORK))
        }
        _ => None,
    }
}

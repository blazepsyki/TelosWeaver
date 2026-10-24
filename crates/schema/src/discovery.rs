//! Spec §7.5 VS Code 확장의 CLI 탐지 결과 (REQ-010).
//!
//! 탐색 입력 규칙과 Port(`ProviderCliRule`, `DiscoveryRequest`, `CliDiscoverer`)는 `telos-adapters`에 있습니다.
//! 이 결과는 Capability 판정(§7.5-5)과 Run의 Context Snapshot 기록(§7.5-6), Extension 표시에 같은 형태로 씁니다.
//!
//! - 경로는 실행 Host의 절대 경로 문자열입니다. 비 UTF-8 경로는 탐색 결과로 내보내지 않고
//!   `unavailable`(`invalid_path`)로 보고합니다.
//! - 버전은 기록용입니다. 실행 파일 선택 규칙에 버전 문자열을 쓰지 않습니다 (§7.5-2).
//! - `found`는 실행 파일과 버전 확인이 끝났다는 뜻이며, Provider 기능 지원이나 로그인 상태를 뜻하지 않습니다.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 실행 파일을 고른 출처 (§7.5-4 해석 순서).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum CliSource {
    /// 사용자·Host 설정의 명시 경로.
    Explicit,
    /// 확장이 포함하거나 관리하는 실행 파일.
    Extension,
    /// PATH 대체.
    Path,
}

/// 확장 폴더를 정한 방법 (§7.5-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ExtensionResolution {
    /// `extensions.json`의 `relativeLocation`.
    Metadata,
    /// `<확장 ID>-<version>[-<targetPlatform>]` 폴더 이름 파싱 (`.obsolete` 제외).
    FolderScan,
}

/// 실행 파일 위치를 정한 방법 (§7.5-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum BinaryLocation {
    /// Provider 규칙의 확장 내부 상대 경로.
    RelativePath,
    /// 확장이 확장 폴더 밖에 내려받아 관리하는 경로 (예: Antigravity `~/.gemini/bin/agy`).
    ManagedPath,
    /// 상대 경로가 없어 확장 폴더 안을 파일 이름으로 찾았고 후보가 정확히 하나였습니다.
    NameSearch,
    /// 명시 경로나 PATH 항목을 그대로 썼습니다.
    Direct,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionInstall {
    /// 예: `openai.chatgpt`.
    pub extension_id: String,
    pub version: String,
    /// VS Code `targetPlatform` (예: `win32-x64`). 플랫폼 공용 패키지는 생략합니다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_platform: Option<String>,
    pub install_path: String,
    pub resolution: ExtensionResolution,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscoveredCli {
    pub source: CliSource,
    pub location: BinaryLocation,
    pub executable_path: String,
    /// 버전 명령 출력의 첫 비어 있지 않은 줄.
    pub cli_version: String,
    /// 소문자 16진수 SHA-256. 버전 확인 전후 값이 같아야 합니다.
    pub sha256: String,
    /// `source = extension`일 때만 있습니다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extension: Option<ExtensionInstall>,
}

/// 탐지 실패 분류. 상세 원인은 `reason`에 요약하며 파일 내용·토큰을 넣지 않습니다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CliDiscoveryFailure {
    /// 세 출처 모두에서 후보가 없습니다.
    NotFound,
    /// 파일 이름 검색 후보가 여러 개입니다. 추측하지 않습니다.
    Ambiguous,
    /// `extensions.json`·`.obsolete`를 해석할 수 없거나 경로가 확장 폴더를 벗어납니다.
    InvalidMetadata,
    /// 경로가 실행 가능한 일반 파일이 아니거나 UTF-8로 기록할 수 없습니다.
    InvalidPath,
    /// 버전 명령이 시작·종료·출력 검사에 실패했거나 제한 시간을 넘었습니다.
    VersionCheckFailed,
    /// 확인 중 실행 파일 해시가 바뀌었습니다.
    ChangedDuringDiscovery,
    /// 실행 Host 플랫폼에 Provider 규칙이 없습니다.
    UnsupportedPlatform,
    /// 그 밖의 I/O 오류.
    IoError,
}

/// 탐지 결과. `found`는 [`DiscoveredCli`]의 필드를, `unavailable`은 실패 정보만 가집니다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "status",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum CliDiscoveryOutcome {
    Found {
        cli: DiscoveredCli,
    },
    Unavailable {
        failure: CliDiscoveryFailure,
        /// 실패한 단계의 출처. 명시 경로의 확인 실패는 다른 출처로 넘어가지 않으므로 `explicit`입니다 (§7.5-5).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<CliSource>,
        reason: String,
    },
}

/// 한 Provider·한 실행 Host의 탐지 기록.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CliDiscovery {
    /// Adapter ID (`AgentAdapter::id`).
    pub provider_id: String,
    pub host_id: String,
    /// RFC 3339 시각. Core가 요청에 넣은 값을 그대로 씁니다.
    pub checked_at: String,
    pub outcome: CliDiscoveryOutcome,
}

impl CliDiscovery {
    pub fn found(&self) -> Option<&DiscoveredCli> {
        match &self.outcome {
            CliDiscoveryOutcome::Found { cli } => Some(cli),
            CliDiscoveryOutcome::Unavailable { .. } => None,
        }
    }
}

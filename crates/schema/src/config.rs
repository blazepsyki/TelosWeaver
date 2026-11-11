//! Spec §13.1 설정 계층, CFG-01~03 (H0 설정)
//!
//! 파일은 TOML이며 사용자/Host는 `%APPDATA%\telos-weaver\config.toml`(Linux `$XDG_CONFIG_HOME/telos-weaver/config.toml`),
//! 프로젝트는 `<프로젝트>/.telos-weaver/config.toml`입니다 (H0 D7). 각 계층 파일은 [`ConfigFile`] 형식입니다.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{AgentSelection, BudgetProfile, PermissionSet};

/// 해석 순서입니다. 뒤 계층이 일반 설정을 덮어씁니다.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum ConfigLayer {
    ProductDefault,
    UserHost,
    Project,
    Task,
}

/// 항목 종류에 따라 계층 규칙이 다릅니다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum SettingKind {
    /// 뒤 계층이 덮어씁니다.
    General,
    /// 덮어쓰지 않고 모든 계층의 교집합만 씁니다. 저장소 설정으로 넓히지 못합니다 (CFG-02).
    PermissionBound,
    /// 실행 파일 경로·인증처럼 Host에 귀속합니다. 사용자/Host 계층에서만 정할 수 있습니다 (§13.1).
    HostBound,
}

impl ConfigLayer {
    /// 이 계층이 해당 종류의 항목을 정할 수 있는지입니다.
    pub fn may_set(self, kind: SettingKind) -> bool {
        match kind {
            SettingKind::HostBound => matches!(self, Self::ProductDefault | Self::UserHost),
            SettingKind::General | SettingKind::PermissionBound => true,
        }
    }
}

/// `command` 검증에 쓸 수 있는 명령입니다 (H0 D8). Task 계약은 이 이름으로만 참조합니다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValidationCommand {
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// Task 계약이 덧붙일 수 있는 인자. 여기 없는 인자는 거부합니다.
    #[serde(default)]
    pub allowed_extra_args: Vec<String>,
    pub timeout_secs: u64,
}

/// Provider별 설정입니다. `executable`은 Host 귀속이며 비우면 VS Code 확장 탐지 규칙(§7.5)을 따릅니다.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executable: Option<String>,
    #[serde(default)]
    pub models: Vec<String>,
    /// `listed`(기본) 또는 `hidden` (§7.6)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vscode_listing: Option<String>,
}

/// 한 계층 설정 파일의 형식입니다. 없는 항목은 아래 계층 값을 씁니다.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub master: Option<AgentSelection>,
    #[serde(default)]
    pub providers: BTreeMap<String, ProviderConfig>,
    /// 이 계층의 권한 상한. 최종 권한은 모든 계층과 Plan·부모 위임·Task 요청의 교집합입니다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permissions: Option<PermissionSet>,
    #[serde(default)]
    pub budget_profiles: Vec<BudgetProfile>,
    #[serde(default)]
    pub commands: BTreeMap<String, ValidationCommand>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_max_bytes: Option<u64>,
    /// RN Fork 실행 파일 (Host 귀속)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rn_executable: Option<String>,
}

impl ConfigFile {
    /// 이 계층에 둘 수 없는 Host 귀속 항목의 경로를 돌려줍니다. 비어 있지 않으면 설정 로드를 거부합니다.
    pub fn host_bound_violations(&self, layer: ConfigLayer) -> Vec<String> {
        if layer.may_set(SettingKind::HostBound) {
            return vec![];
        }
        let mut out: Vec<String> = self
            .providers
            .iter()
            .filter(|(_, p)| p.executable.is_some())
            .map(|(name, _)| format!("providers.{name}.executable"))
            .collect();
        if self.rn_executable.is_some() {
            out.push("rnExecutable".into());
        }
        out
    }
}

/// 최종 설정의 항목 하나와 출처입니다 (CFG-01).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedSetting {
    /// 예: `master.model`, `providers.codex.models`
    pub key: String,
    pub value: serde_json::Value,
    pub layer: ConfigLayer,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
}

/// Run에 고정하는 최종 설정입니다. `digest`는 정렬한 항목의 SHA-256입니다.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedConfig {
    pub settings: Vec<ResolvedSetting>,
    pub digest: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum InstallOwner {
    RnFork,
    TaskMcp,
}

/// TelosWeaver가 사용자 설정 파일에 써 넣은 항목의 기록입니다 (CFG-03, E2E-09).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InstalledSetting {
    pub owner: InstallOwner,
    pub target_file: String,
    /// 파일 안의 위치(예: `mcpServers.telos-rn`)
    pub key_path: String,
    /// 써 넣은 값의 SHA-256
    pub installed_digest: String,
    pub installed_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InstallDecision {
    /// 키가 없으므로 씁니다.
    Write,
    /// 같은 값이 이미 있습니다.
    AlreadyInstalled,
    /// 사용자 값이 있습니다. 덮어쓰지 않고 충돌로 보고합니다.
    Conflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UninstallDecision {
    /// 설치한 값 그대로이므로 지웁니다.
    Remove,
    /// 설치 뒤 사용자가 바꿨습니다. 보존하고 알립니다.
    KeepUserModified,
    /// 이미 없습니다.
    Absent,
}

/// 설치 판정입니다. `current_digest`는 파일의 현재 값 Digest(없으면 `None`)입니다.
pub fn install_decision(current_digest: Option<&str>, new_digest: &str) -> InstallDecision {
    match current_digest {
        None => InstallDecision::Write,
        Some(d) if d == new_digest => InstallDecision::AlreadyInstalled,
        Some(_) => InstallDecision::Conflict,
    }
}

impl InstalledSetting {
    pub fn uninstall_decision(&self, current_digest: Option<&str>) -> UninstallDecision {
        match current_digest {
            None => UninstallDecision::Absent,
            Some(d) if d == self.installed_digest => UninstallDecision::Remove,
            Some(_) => UninstallDecision::KeepUserModified,
        }
    }
}

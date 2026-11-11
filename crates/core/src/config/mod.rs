//! 설정 (Spec §16, PH1-CONFIG). H0 공개 인터페이스입니다.
//!
//! 사용자(`%APPDATA%\telos-weaver\config.toml`)와 프로젝트(`.telos-weaver/config.toml`) TOML을 합칩니다(H0 D7).
//! 프로젝트 층이 Host 한정 설정을 바꾸려 하면(`ConfigFile::host_bound_violations`) 무시하고 경고합니다.
//! 사용자 설정 파일에 써 넣는 항목은 `InstalledSetting`으로 기록하고, Digest가 같을 때만 지웁니다 (CFG-03).

use std::path::Path;

use telos_schema::{InstallOwner, InstalledSetting, ResolvedConfig};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    /// TOML 문법 오류·알 수 없는 키
    Invalid {
        file: String,
        message: String,
    },
    /// 사용자가 바꾼 항목이라 덮어쓰거나 지우지 않음
    UserModified {
        target_file: String,
        key_path: String,
    },
    Io {
        message: String,
    },
}

pub trait ConfigService: Send + Sync {
    /// 층을 합친 최종 설정과 경고입니다. Run 시작 때 `digest`를 고정합니다.
    fn resolve(&self, project_root: &Path) -> Result<(ResolvedConfig, Vec<String>), ConfigError>;
    fn install(
        &self,
        owner: InstallOwner,
        target_file: &Path,
        key_path: &str,
        value: serde_json::Value,
    ) -> Result<InstalledSetting, ConfigError>;
    fn uninstall(&self, setting: &InstalledSetting) -> Result<(), ConfigError>;
    fn installed(&self) -> Result<Vec<InstalledSetting>, ConfigError>;
}

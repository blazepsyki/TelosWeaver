//! TelosWeaver Core (Spec §3~§15). 모듈 배치 계획은 `crates/core/README.md`를 따릅니다.
//! 모듈 등록은 PH0-INTEGRATION이 관리합니다 (Spec §19.5.4-3).

pub mod api;
pub mod execution;
pub mod integration;
pub mod master;
pub mod policy;

/// 실행 파일·상태 디렉터리·서비스 식별자 (Spec §3.3).
pub const SERVICE_ID: &str = "telos-weaver";
pub const STATE_DIR_NAME: &str = ".telos-weaver";

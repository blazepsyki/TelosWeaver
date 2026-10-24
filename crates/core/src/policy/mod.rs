//! Decision Validator — kind별 Payload·권한·Epoch·멱등 검사 (Spec §4.2, §13). 소유: PH0-MASTER.
//!
//! 검증 입력은 `telos_schema::DecisionContext`, 결과는 `telos_schema::DecisionRejection`입니다.
//! `DecisionContext`는 목록·상태 기반 검사만 담으며, 그 Snapshot에 없는 검사(Workspace 경로·모델·Provider 설정)는
//! [`TrustedLookup`]으로 수행합니다 (G0 개정 1 REQ-008).
//!
//! - 신뢰된 조회의 조회 실패·검사 불가는 허용으로 간주하지 않고 `policy_unavailable`·`check_unsupported`로 거부합니다.
//! - Phase 0 조회 구현은 운영자 정책 파일(`telos_core::integration::LocalRunPolicy`)입니다.
//!   정식 정책 서비스가 생기면 이 Trait의 구현만 바꿉니다.

mod validator;

pub use validator::{Caller, PHASE0_EXECUTED_KINDS, validate};

use telos_schema::TaskSpec;

/// 신뢰된 조회가 허용하지 않은 이유입니다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LookupDenial {
    /// 정책이 명시적으로 허용하지 않습니다.
    Denied(String),
    /// 정책을 조회할 수 없습니다. 허용으로 간주하지 않습니다.
    Unavailable(String),
    /// 이 단계에서 수행할 수 없는 검사가 필요합니다.
    Unsupported(String),
}

pub trait TrustedLookup: Send + Sync {
    /// 새 Task가 권한 상한(Provider·모델·Workspace·권한 Profile) 안에 있는지 확인합니다.
    /// 결정 단계의 검사이며 Run 시작 시의 재검사(§9.1)를 대신하지 않습니다.
    fn check_task(&self, task: &TaskSpec) -> Result<(), LookupDenial>;
}

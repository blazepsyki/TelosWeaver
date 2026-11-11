//! 검증·승인 (Spec §11, PH1-VALIDATION). H0 공개 인터페이스이며 구현은 PH1-VALIDATION이 합니다.
//!
//! 명령 검증은 설정에 이름으로 등록한 명령만 실행합니다(H0 D8). 판정은 [`ValidationJob`]으로 기록하고
//! Scheduler의 `on_validation_finished`로 알립니다. 승인은 실행 직전 `ApprovalDecision::permits`로 다시 확인합니다.

use async_trait::async_trait;
use telos_schema::{
    ApprovalAction, ApprovalActor, ApprovalAnswer, ApprovalDecision, ApprovalKind,
    ArtifactVersionRef, ValidationContract, ValidationJob,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// 설정에 없는 명령 이름 등
    NotAllowed {
        reason: String,
    },
    NotFound {
        entity: String,
    },
    /// 사용자 전용 승인을 마스터가 결정하려 함 등
    Forbidden {
        reason: String,
    },
    VersionConflict {
        entity: String,
    },
    Storage {
        message: String,
    },
}

#[async_trait]
pub trait Validator: Send + Sync {
    /// 계약 Revision과 대상 Version을 고정해 검증을 시작합니다.
    async fn start(
        &self,
        task_id: &str,
        task_revision: u32,
        contract: ValidationContract,
        targets: Vec<ArtifactVersionRef>,
    ) -> Result<ValidationJob, ValidationError>;
    async fn cancel(&self, validation_id: &str) -> Result<bool, ValidationError>;
}

/// 승인 요청에 넣는 내용입니다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalRequest {
    pub kind: ApprovalKind,
    pub plan_id: String,
    pub task_id: Option<String>,
    pub run_id: Option<String>,
    pub contract_revision: Option<u32>,
    pub targets: Vec<ArtifactVersionRef>,
    pub action: Option<ApprovalAction>,
    pub user_only: bool,
    pub expires_at: Option<String>,
}

#[async_trait]
pub trait ApprovalService: Send + Sync {
    async fn request(&self, request: ApprovalRequest) -> Result<ApprovalDecision, ValidationError>;
    async fn decide(
        &self,
        approval_id: &str,
        actor: ApprovalActor,
        answer: ApprovalAnswer,
    ) -> Result<ApprovalDecision, ValidationError>;
    /// 실행 직전 확인. 맞으면 한 번 쓰는 승인을 `Consumed`로 바꾸고 참을 돌려줍니다.
    async fn consume(
        &self,
        approval_id: &str,
        targets: &[ArtifactVersionRef],
        action: Option<&ApprovalAction>,
    ) -> Result<bool, ValidationError>;
    /// 대상이나 Policy가 바뀌면 기존 승인을 무효화합니다.
    async fn revoke(&self, approval_id: &str, reason: &str) -> Result<(), ValidationError>;
}

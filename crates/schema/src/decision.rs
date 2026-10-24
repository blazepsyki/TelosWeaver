//! Spec §4.2 마스터 결정의 kind별 Payload·검증 입력·결과 (G0 마스터 결정 계약).
//!
//! 마스터 출력은 신뢰하지 않는 입력이므로 Payload는 알 수 없는 필드를 거부합니다.
//! 집행 순서와 거부 사유는 §4.2를 따르며 판정은 Core의 policy 모듈이 소유합니다.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    ApprovalAnswer, ArtifactVersionRef, ControlLimits, JsonObject, MasterDecision,
    MasterDecisionKind, SessionRequest, TaskSpec, TaskStatus, WorkspaceMode,
};

/// P0 마스터는 제한된 Task 목록과 하위 위임을 생성합니다 (§4.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateTasksPayload {
    pub tasks: Vec<TaskSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequestRunPayload {
    pub task_id: String,
    pub task_revision: u32,
    pub session: SessionRequest,
}

/// Worker 요청(§4.3)에 대한 마스터의 답변입니다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnswerActionPayload {
    pub request_id: String,
    pub answer: ApprovalAnswer,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequestValidationPayload {
    pub task_id: String,
    pub task_revision: u32,
    pub run_id: String,
    /// 비어 있으면 Task의 모든 필수 Validation Contract를 뜻합니다.
    pub validation_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequestReworkPayload {
    pub task_id: String,
    pub task_revision: u32,
    pub failed_run_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequestCancelPayload {
    pub task_id: String,
    pub reason: String,
    /// 자손 Task로 취소를 전파할지. 기본 정책은 전파입니다 (§9.3).
    pub propagate_to_children: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AskUserPayload {
    pub question: String,
    pub reason: String,
    pub options: Vec<String>,
    pub related_task_ids: Vec<String>,
}

/// 완료 판정 요청입니다. 검증·승인 조건을 우회하지 않습니다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProposeCompletionPayload {
    pub task_id: String,
    pub task_revision: u32,
    pub run_id: String,
    pub artifacts: Vec<ArtifactVersionRef>,
    pub summary: String,
}

/// kind별 Payload의 합. Wire에서는 `MasterDecision::payload`에 태그 없이 들어갑니다.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum DecisionPayload {
    CreateTasks(CreateTasksPayload),
    RequestRun(RequestRunPayload),
    AnswerAction(AnswerActionPayload),
    RequestValidation(RequestValidationPayload),
    RequestRework(RequestReworkPayload),
    RequestCancel(RequestCancelPayload),
    AskUser(AskUserPayload),
    ProposeCompletion(ProposeCompletionPayload),
}

impl DecisionPayload {
    pub fn kind(&self) -> MasterDecisionKind {
        match self {
            Self::CreateTasks(_) => MasterDecisionKind::CreateTasks,
            Self::RequestRun(_) => MasterDecisionKind::RequestRun,
            Self::AnswerAction(_) => MasterDecisionKind::AnswerAction,
            Self::RequestValidation(_) => MasterDecisionKind::RequestValidation,
            Self::RequestRework(_) => MasterDecisionKind::RequestRework,
            Self::RequestCancel(_) => MasterDecisionKind::RequestCancel,
            Self::AskUser(_) => MasterDecisionKind::AskUser,
            Self::ProposeCompletion(_) => MasterDecisionKind::ProposeCompletion,
        }
    }

    /// `kind`에 맞는 Payload로 해석합니다. 실패는 `RejectionCode::InvalidPayload`로 거부합니다.
    pub fn parse(kind: MasterDecisionKind, payload: &JsonObject) -> serde_json::Result<Self> {
        use MasterDecisionKind as K;
        let value = serde_json::Value::Object(payload.clone());
        Ok(match kind {
            K::CreateTasks => Self::CreateTasks(serde_json::from_value(value)?),
            K::RequestRun => Self::RequestRun(serde_json::from_value(value)?),
            K::AnswerAction => Self::AnswerAction(serde_json::from_value(value)?),
            K::RequestValidation => Self::RequestValidation(serde_json::from_value(value)?),
            K::RequestRework => Self::RequestRework(serde_json::from_value(value)?),
            K::RequestCancel => Self::RequestCancel(serde_json::from_value(value)?),
            K::AskUser => Self::AskUser(serde_json::from_value(value)?),
            K::ProposeCompletion => Self::ProposeCompletion(serde_json::from_value(value)?),
        })
    }

    pub fn from_decision(decision: &MasterDecision) -> serde_json::Result<Self> {
        Self::parse(decision.kind, &decision.payload)
    }

    pub fn to_object(&self) -> JsonObject {
        match serde_json::to_value(self) {
            Ok(serde_json::Value::Object(map)) => map,
            _ => unreachable!("decision payloads always serialize to JSON objects"),
        }
    }
}

/// 결정 검증에 쓰는 Task 요약입니다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TaskSummary {
    pub id: String,
    pub revision: u32,
    pub status: TaskStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_task_id: Option<String>,
    /// 최상위 Task는 0입니다.
    pub delegation_depth: u32,
}

/// 결정 검증 시점의 입력 Snapshot (Phase 0 최소). 검증 결과는 이 Snapshot과 함께 기록합니다.
/// 권한 상한은 사용자·Host·프로젝트·Plan·부모 위임의 교집합을 Core가 계산해 넣습니다 (§13.1).
///
/// # 검사 범위 (G0 개정 1 REQ-008)
/// 이 Snapshot은 **목록·상태 기반 검사**만 담습니다: Epoch·Revision, 한도, Task 트리, Project,
/// 허용 Provider·Workspace 모드·Profile·Host 목록. 경로·명령·Session 소유권·호출자와 Run의 관계·예산 잔액처럼
/// 여기에 없는 검사는 PH0-MASTER가 `core/policy`에 정의하는 신뢰된 조회로 수행합니다.
/// 조회에 실패하면 [`RejectionCode::PolicyUnavailable`], 이 단계에서 수행할 수 없는 검사가 필요하면
/// [`RejectionCode::CheckUnsupported`]로 거부하며 허용으로 간주하지 않습니다.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DecisionContext {
    pub project_id: String,
    pub plan_id: String,
    pub plan_revision: u32,
    pub controller_epoch: u64,
    pub policy_revision: String,
    pub limits: ControlLimits,
    pub tasks: Vec<TaskSummary>,
    pub allowed_providers: Vec<String>,
    pub allowed_host_ids: Vec<String>,
    pub allowed_workspace_modes: Vec<WorkspaceMode>,
    pub allowed_permission_profile_ids: Vec<String>,
    pub allowed_budget_profile_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DecisionStatus {
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RejectionCode {
    /// 호출자·Run 소유권 확인 실패
    UnauthorizedCaller,
    InvalidPayload,
    /// 이 단계에서 집행하지 않는 kind
    UnsupportedKind,
    StaleControllerEpoch,
    StalePlanRevision,
    StalePolicyRevision,
    /// 같은 멱등 키에 다른 Payload
    IdempotencyConflict,
    PermissionDenied,
    BudgetExceeded,
    LimitExceeded,
    DependencyCycle,
    UnknownTask,
    UnsupportedCapability,
    /// 사용자 전용 결정이라 마스터가 집행할 수 없음 (§4.3)
    UserDecisionRequired,
    /// 필요한 신뢰된 정책 조회에 실패함. 허용으로 간주하지 않음
    PolicyUnavailable,
    /// 필요한 검사를 이 단계에서 수행할 수 없음
    CheckUnsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DecisionRejection {
    pub code: RejectionCode,
    pub message: String,
    /// 문제가 된 Payload 필드의 JSON Pointer
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
}

/// 집행이 남긴 결과입니다. 요청 접수와 실제 완료를 구분합니다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum DecisionEffect {
    TaskCreated { task_id: String, task_revision: u32 },
    RunRequested { task_id: String, run_id: String },
    ActionAnswered { request_id: String },
    ValidationRequested { task_id: String, run_id: String },
    ReworkRequested { task_id: String },
    CancelRequested { task_id: String },
    UserQuestionOpened { question_id: String },
    CompletionEvaluated { task_id: String, completed: bool },
}

/// 결정 집행 결과 (G0 개정 1 REQ-004). `status`가 태그이며 조합을 타입으로 강제합니다.
///
/// - `accepted`는 `effects`만, `rejected`는 `rejection`만 가집니다. 다른 조합·미정의 필드는 역직렬화에서 거부합니다.
/// - `accepted`는 결정 단계의 검사 통과를 뜻하며 Run 시작 시의 권한 재검사(§9.1)를 대신하지 않습니다.
/// - `replayed: true`는 같은 멱등 키·같은 Payload의 재요청에 처음 기록한 결과를 그대로 반환했다는 뜻입니다.
///   `effects`는 처음 집행한 효과이며 다시 집행하지 않습니다.
/// - 부분 집행은 Phase 0 계약에 없습니다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "status",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum DecisionOutcome {
    Accepted {
        decision_id: String,
        idempotency_key: String,
        effects: Vec<DecisionEffect>,
        replayed: bool,
        recorded_at: String,
    },
    Rejected {
        decision_id: String,
        idempotency_key: String,
        rejection: DecisionRejection,
        replayed: bool,
        recorded_at: String,
    },
}

impl DecisionOutcome {
    pub fn status(&self) -> DecisionStatus {
        match self {
            Self::Accepted { .. } => DecisionStatus::Accepted,
            Self::Rejected { .. } => DecisionStatus::Rejected,
        }
    }

    pub fn decision_id(&self) -> &str {
        match self {
            Self::Accepted { decision_id, .. } | Self::Rejected { decision_id, .. } => decision_id,
        }
    }

    pub fn idempotency_key(&self) -> &str {
        match self {
            Self::Accepted {
                idempotency_key, ..
            }
            | Self::Rejected {
                idempotency_key, ..
            } => idempotency_key,
        }
    }

    pub fn replayed(&self) -> bool {
        match self {
            Self::Accepted { replayed, .. } | Self::Rejected { replayed, .. } => *replayed,
        }
    }

    /// 거부된 결정은 효과가 없으므로 빈 슬라이스입니다.
    pub fn effects(&self) -> &[DecisionEffect] {
        match self {
            Self::Accepted { effects, .. } => effects,
            Self::Rejected { .. } => &[],
        }
    }
}

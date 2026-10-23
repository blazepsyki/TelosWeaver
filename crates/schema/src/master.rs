//! Spec §4.2 구조화 결정 계약

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::JsonObject;

/// 공통 외형입니다. 각 `kind`의 Payload는 별도 JSON Schema로 검증합니다.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MasterDecision {
    pub decision_id: String,
    pub plan_id: String,
    pub plan_revision: u32,
    pub master_run_id: String,
    pub controller_epoch: u64,
    pub idempotency_key: String,
    pub kind: MasterDecisionKind,
    pub payload: JsonObject,
    pub rationale: String,
    pub policy_revision: String,
}

/// `ProposeCompletion`은 완료 판정 요청일 뿐 검증을 우회하지 않습니다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MasterDecisionKind {
    CreateTasks,
    RequestRun,
    AnswerAction,
    RequestValidation,
    RequestRework,
    RequestCancel,
    AskUser,
    ProposeCompletion,
}

//! Spec §7.2 공통 실행·제어 계약의 데이터 타입. `AgentAdapter` Trait은 `telos-adapters`에 있습니다.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::JsonObject;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum CapabilityState {
    Supported,
    Unsupported,
    Unverified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceLevel {
    Documented,
    ContractTested,
    MockTested,
    EnvironmentTested,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Capability {
    pub state: CapabilityState,
    pub provider_version: String,
    pub host_id: String,
    pub checked_at: String,
    pub evidence_level: EvidenceLevel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Spec §8.1 문맥 전략.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum SessionMode {
    Fresh,
    Resume,
    NativeFork,
    Capsule,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionRequest {
    pub mode: SessionMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capsule_version_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunContext {
    /// Core가 발급합니다. Adapter가 임의의 다른 Run을 만들지 않습니다.
    pub run_id: String,
    pub task_id: String,
    pub task_revision: u32,
    pub attempt: u32,
    pub controller_epoch: u64,
    pub host_id: String,
    pub workspace_id: String,
    pub context_snapshot_id: String,
    pub policy_revision: String,
    pub idempotency_key: String,
    pub session: SessionRequest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProviderEventKind {
    SessionStarted,
    TextDelta,
    ToolStarted,
    ToolCompleted,
    ApprovalRequired,
    Usage,
    TurnFinished,
    ProcessExited,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProviderEvent {
    pub event_id: String,
    pub run_id: String,
    /// 원본에 순서 정보가 없으면 Adapter가 수신 순서를 부여합니다.
    pub sequence: u64,
    pub occurred_at: String,
    pub schema_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    pub kind: ProviderEventKind,
    pub payload: JsonObject,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_event_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunStarted {
    pub run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_turn_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalAnswer {
    Allow,
    Deny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum RunInspection {
    Running,
    Finished,
    Interrupted,
    Unknown,
}

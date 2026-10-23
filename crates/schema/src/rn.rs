//! Reason Network Fork Local RPC 계약 (Spec §12, G0 RN RPC 계약).
//!
//! Core와 RN Fork는 각자의 프로세스·DB를 가지며 이 메시지로만 통신합니다 (DEC-05).
//! 전송은 Named Pipe/Unix Socket 위의 NDJSON이며 Core가 접근을 중재합니다 (§12.1).
//! 메서드별 `params`·`result` 타입은 [`RnMethod`] 각 항목의 문서를 따릅니다.
//! 전달 상태는 Task 완료를 뜻하지 않습니다 (DELEG-09).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ArtifactVersionRef, DeliveryReceiptStatus, DeliveryTier};

/// RN Fork의 `rpc_schema_version`과 맞춰야 하는 값입니다.
pub const RN_PROTOCOL_VERSION: &str = "0.1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub enum RnMethod {
    /// 빈 객체 → [`RnHealth`]
    #[serde(rename = "health.get")]
    HealthGet,
    /// 빈 객체 → `Vec<ProviderConnector>`
    #[serde(rename = "providers.list")]
    ProvidersList,
    /// [`SessionQuery`] → `Vec<SessionEndpoint>`
    #[serde(rename = "sessions.discover")]
    SessionsDiscover,
    /// [`EndpointRef`] → [`SessionEndpoint`]
    #[serde(rename = "sessions.resolve")]
    SessionsResolve,
    /// [`EndpointRef`] → [`SessionCapabilities`]
    #[serde(rename = "sessions.capabilities")]
    SessionsCapabilities,
    /// [`MessageEnvelope`] → [`DeliveryReceipt`]
    #[serde(rename = "messages.send")]
    MessagesSend,
    /// [`MessageRef`] → [`DeliveryReceipt`]
    #[serde(rename = "messages.status")]
    MessagesStatus,
    /// [`SubscribeParams`] → 이후 [`RnEvent`] 스트림
    #[serde(rename = "messages.subscribe")]
    MessagesSubscribe,
    /// P1. Agy 탐지·버전·기능 진단
    #[serde(rename = "providers.agy.probe")]
    ProvidersAgyProbe,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RnRequest {
    pub protocol_version: String,
    pub request_id: String,
    pub method: RnMethod,
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RnResponse {
    pub protocol_version: String,
    /// 요청과 같은 값입니다.
    pub request_id: String,
    #[serde(flatten)]
    pub outcome: RnOutcome,
}

/// Wire에서 `result` 또는 `error` 필드 하나로 나타납니다.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum RnOutcome {
    Result(serde_json::Value),
    Error(RnError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RnErrorCode {
    /// Fork/RPC 비호환 (COMM-08)
    UnsupportedProtocolVersion,
    UnknownMethod,
    InvalidParams,
    NotFound,
    /// Native ID·Host·Fingerprint 불일치 또는 주소 재바인딩 (COMM-06)
    IdentityMismatch,
    Refused,
    /// 같은 멱등 키에 다른 내용
    IdempotencyConflict,
    Expired,
    Unavailable,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RnError {
    pub code: RnErrorCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RnHealthStatus {
    Ok,
    Degraded,
}

/// `health.get` 결과. Fork 배포 메타데이터를 함께 보고합니다 (§12.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RnHealth {
    pub status: RnHealthStatus,
    pub rpc_version: String,
    pub upstream_commit: String,
    pub fork_commit: String,
    pub host_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConnector {
    pub provider: String,
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// 사람이 읽는 주소는 영구 Session ID가 아닙니다. 대상은 항상 이 정체성으로 고정합니다 (§12.2).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EndpointRef {
    pub host_id: String,
    pub provider: String,
    pub native_session_id: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PresenceState {
    Busy,
    Idle,
    Offline,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionEndpoint {
    pub endpoint: EndpointRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    pub presence: PresenceState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_activity_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionCapabilities {
    pub endpoint: EndpointRef,
    /// 검증된 Delivery Tier만 나열합니다.
    pub tiers: Vec<DeliveryTier>,
    pub can_reply: bool,
    pub can_resume: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MessageBodyType {
    Text,
    Structured,
}

/// `messages.send`의 Envelope (§12.2). 큰 내용은 본문 대신 Artifact로 참조합니다 (§10).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MessageEnvelope {
    pub message_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    pub correlation_id: String,
    pub idempotency_key: String,
    pub sender: EndpointRef,
    pub recipient: EndpointRef,
    pub tier: DeliveryTier,
    pub expires_at: String,
    pub body_type: MessageBodyType,
    pub body: serde_json::Value,
    pub artifacts: Vec<ArtifactVersionRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MessageRef {
    pub message_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryReceipt {
    pub message_id: String,
    pub status: DeliveryReceiptStatus,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeParams {
    /// 이전 구독의 마지막 커서. 없으면 지금부터 받습니다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RnEventKind {
    Message,
    Receipt,
    Presence,
    /// 커서가 보존 범위를 벗어났습니다. 상태를 다시 조회한 뒤 새로 구독합니다.
    ResyncRequired,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RnEvent {
    pub cursor: String,
    pub sequence: u64,
    pub kind: RnEventKind,
    pub payload: serde_json::Value,
}

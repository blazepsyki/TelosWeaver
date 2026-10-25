//! Reason Network Fork Local RPC 계약 (Spec §12, G0 RN RPC 계약).
//!
//! Core와 RN Fork는 각자의 프로세스·DB를 가지며 이 메시지로만 통신합니다 (DEC-05).
//! 전송은 Named Pipe/Unix Socket 위의 NDJSON이며 Core가 접근을 중재합니다 (§12.1).
//! 메서드별 `params`·`result` 타입은 [`RnMethod`] 각 항목의 문서를 따릅니다.
//! 전달 상태는 Task 완료를 뜻하지 않습니다 (DELEG-09).
//!
//! # 접속 (REQ-018, 배정표 §6 "RN RPC")
//! - RPC는 RN이 에이전트에게 여는 원본 로컬 API와 다른 전용 Pipe/Socket을 씁니다. 주소는 RN 상태 디렉터리
//!   (`TELOS_RN_HOME`, Core가 RN을 시작할 때 정함)의 `rpc.json`([`RnRpcInfo`])에 적혀 있습니다.
//! - RN은 시작할 때마다 32바이트 난수(소문자 hex 64자) 토큰을 같은 디렉터리의 `rpc-token`에 사용자 전용 권한으로 쓴 뒤에만
//!   연결을 받습니다. Pipe/Socket과 토큰 파일은 RN을 실행한 OS 사용자만 접근할 수 있습니다(Windows는 현재 사용자 SID만
//!   허용하는 DACL, Unix는 0600). 권한을 적용하지 못하면 RPC를 열지 않습니다(fail closed).
//! - 연결의 첫 요청은 [`RnMethod::Hello`]여야 합니다. 그 전의 다른 요청은 `unauthenticated`로 거부합니다. RN이
//!   재시작하면 연결이 끊기고, Client는 토큰을 다시 읽어 `rn.hello`부터 다시 합니다.
//! - 한계: 같은 OS 사용자로 실행되는 프로세스는 토큰 파일과 원본 로컬 API에도 접근할 수 있습니다(SAFE-01과 같음).
//!
//! # 프레임
//! - UTF-8 JSON 한 개를 한 줄로 보내는 NDJSON이며 상한은 Local Core API와 같은 [`crate::api::MAX_FRAME_BYTES`]
//!   (종료 `\n` 제외)입니다. 상한을 넘는 요청에는 `requestId: ""`, `frame_too_large` 오류를 보낸 뒤 연결을 닫습니다.
//! - 요청 [`RnRequest`]와 응답 [`RnResponse`]는 `requestId`로 짝을 맞춥니다. 알림 [`RnNotification`]은 응답과 섞여
//!   도착할 수 있습니다. 다만 한 구독의 알림은 그 구독을 만든 `messages.subscribe` 응답 뒤에만 보냅니다.
//!
//! # 구독
//! - 구독은 그 연결이 살아 있는 동안만 유지됩니다(해지 메서드 없음, 연결을 닫으면 끝).
//! - 이벤트는 Core 연결에 관계된 것만 보냅니다: Core가 보낸 메시지의 Receipt 변경(`receipt`), Core 메일함으로 온
//!   메시지(`message`), 이 Host 세션의 접속·해제(`presence`). 각 Payload는 [`RnEventKind`]의 문서를 따릅니다.
//! - 커서는 RN이 메모리에 둔 최근 이벤트의 위치입니다. 보존 범위를 벗어났거나 RN이 재시작한 뒤의 커서로 구독하면
//!   `resync_required` 이벤트 하나를 보내고, 이후 새 이벤트를 보냅니다. Receipt는 RN DB에 남으므로 Client는
//!   `messages.status`로 다시 맞춥니다.
//!
//! # 연결된 세션만 전송 (Phase 0)
//! - RN MCP 서버나 Hook으로 연결되지 않은 세션(Provider 세션 목록에서 찾기만 한 세션)은 `sessions.discover`에
//!   [`SessionEndpoint::attached`] `false`로 나오고, `sessions.capabilities`의 Tier가 비며, 그 세션으로의
//!   `messages.send`는 `refused`입니다. 기존 IDE 세션으로의 실제 전달은 P1(COMM-03)입니다.

use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ArtifactVersionRef, DeliveryReceiptStatus, DeliveryTier, SecretString};

/// RN Fork의 `rpc_schema_version`과 맞춰야 하는 값입니다.
pub const RN_PROTOCOL_VERSION: &str = "0.1";

/// RN 상태 디렉터리의 `rpc.json`. RN이 RPC를 연 뒤 씁니다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RnRpcInfo {
    /// Windows Named Pipe 이름(`\\.\pipe\…`) 또는 Unix Socket 경로
    pub endpoint: String,
    pub rpc_version: String,
    /// 이 정보를 쓴 RN 프로세스
    pub pid: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub enum RnMethod {
    /// [`RnHelloParams`] → [`RnHelloResult`]. 연결의 첫 요청이어야 합니다.
    #[serde(rename = "rn.hello")]
    Hello,
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
    /// [`SubscribeParams`] → [`SubscribeResult`], 이후 [`RnNotification`] (`messages.event`)
    #[serde(rename = "messages.subscribe")]
    MessagesSubscribe,
    /// P1. Agy 탐지·버전·기능 진단. Phase 0에서는 `refused`입니다.
    #[serde(rename = "providers.agy.probe")]
    ProvidersAgyProbe,
}

/// `Debug`는 `params.token`을 가립니다 (SAFE-04).
#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RnRequest {
    pub protocol_version: String,
    pub request_id: String,
    pub method: RnMethod,
    pub params: serde_json::Value,
}

impl fmt::Debug for RnRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut params = self.params.clone();
        if let Some(token) = params.get_mut("token") {
            *token = serde_json::Value::String(SecretString::REDACTED.into());
        }
        f.debug_struct("RnRequest")
            .field("protocol_version", &self.protocol_version)
            .field("request_id", &self.request_id)
            .field("method", &self.method)
            .field("params", &params)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RnHelloParams {
    pub client_name: String,
    pub client_version: String,
    /// `rpc-token`의 내용
    pub token: SecretString,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RnHelloResult {
    pub rpc_version: String,
    pub host_id: String,
    pub connection_id: String,
    /// Core 메일함 Endpoint(provider `telosweaver`). Core가 보내는 Envelope의 `sender`로 쓰며, 이리로 온 메시지는
    /// `message` 이벤트로 받습니다.
    pub endpoint: EndpointRef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub enum RnNotificationMethod {
    /// [`RnEventParams`]
    #[serde(rename = "messages.event")]
    Event,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RnNotification {
    pub protocol_version: String,
    pub method: RnNotificationMethod,
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RnEventParams {
    pub subscription_id: String,
    pub event: RnEvent,
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
    /// `rn.hello` 전의 요청, 또는 토큰 불일치
    Unauthenticated,
    /// 프레임이 [`crate::api::MAX_FRAME_BYTES`]를 넘었습니다. 이 응답 뒤 연결을 닫습니다.
    FrameTooLarge,
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
    /// RN MCP 서버나 Hook으로 연결된 세션입니다. `false`(Provider 세션 목록에서 찾기만 함)이면 Phase 0에서는
    /// 메시지를 보낼 수 없습니다(`messages.send` → `refused`).
    #[serde(default)]
    pub attached: bool,
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

/// Core 메일함으로 온 메시지(`message` 이벤트의 Payload). 구독자에게 보낸 시점에 그 Receipt는 `delivered`입니다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InboundMessage {
    pub message_id: String,
    /// 보낸 세션이 이 Host의 Endpoint로 확인되면 그 Endpoint
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender: Option<EndpointRef>,
    /// 보낸 쪽의 RN 주소(`person/name`). 사람이 읽는 값이며 정체성이 아닙니다.
    pub sender_address: String,
    /// 답하는 대상 메시지(`messageId`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
    pub body: String,
    pub received_at: String,
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
    /// 이전 구독에서 마지막으로 받은 이벤트의 커서. 그 뒤의 이벤트부터 받습니다. 없으면 지금부터 받습니다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeResult {
    pub subscription_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RnEventKind {
    /// Payload [`InboundMessage`]: Core 메일함으로 온 메시지
    Message,
    /// Payload [`DeliveryReceipt`]: Core가 보낸 메시지의 Receipt가 바뀜
    Receipt,
    /// Payload [`SessionEndpoint`]: 이 Host 세션의 접속·해제(`presence`가 `offline`이면 해제)
    Presence,
    /// Payload 빈 객체. 요청한 커서가 보존 범위를 벗어났거나 RN이 재시작했습니다. 그 사이 이벤트는 오지 않으므로
    /// `messages.status`·`sessions.discover`로 상태를 다시 맞춥니다. 구독은 이어지며 이후 새 이벤트를 받습니다.
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

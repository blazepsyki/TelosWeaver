//! Local Core API 계약 (Spec §3.3, §17, G0 Local Core API).
//!
//! # 전송
//! - Windows는 Named Pipe `\\.\pipe\telos-weaver-<projectKey>`, Unix는 `<stateDir>/core.sock`을 사용합니다.
//!   `projectKey`는 정규화한 프로젝트 루트 절대 경로의 SHA-256 앞 16자(소문자 hex)입니다.
//! - 메시지는 UTF-8 JSON 한 개를 한 줄로 보내는 NDJSON이며, 한 줄은 [`MAX_FRAME_BYTES`]를 넘지 않습니다.
//!
//! # 접근 범위
//! - Pipe/Socket은 Core를 실행한 OS 사용자만 접근할 수 있게 만듭니다.
//! - Core는 시작 시 `<stateDir>/api-token`(사용자 전용 권한)을 만들고, Client는 첫 요청 `core.hello`에 그 값을 보냅니다.
//!   `core.hello` 성공 전 다른 요청은 `unauthenticated`로 거부합니다. 한 연결은 한 프로젝트의 Core에만 접근합니다.
//! - 토큰은 로그·이벤트·UI에 남기지 않습니다 (SAFE-04).
//!
//! # 메시지
//! - 요청 [`ApiRequest`]와 응답 [`ApiResponse`]는 `requestId`로 짝을 맞춥니다.
//! - 구독 이벤트는 [`ApiNotification`]으로 보내며 응답과 섞여 도착할 수 있습니다.
//! - 메서드별 `params`·`result` 타입은 [`ApiMethod`] 각 항목의 문서를 따릅니다.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ProviderEvent, RunStatus, SessionRequest, TaskSpec};

pub const API_PROTOCOL_VERSION: &str = "0.1";
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub enum ApiMethod {
    /// [`HelloParams`] → [`HelloResult`]. 연결마다 첫 요청이어야 합니다.
    #[serde(rename = "core.hello")]
    Hello,
    /// 빈 객체 → [`HealthResult`]
    #[serde(rename = "core.health")]
    Health,
    /// [`RunsStartParams`] → [`RunsStartResult`]. Run ID는 Core가 발급합니다.
    #[serde(rename = "runs.start")]
    RunsStart,
    /// [`RunRef`] → [`RunsInspectResult`]
    #[serde(rename = "runs.inspect")]
    RunsInspect,
    /// [`RunsCancelParams`] → [`RunsCancelResult`]. 접수 여부이며 실제 종료가 아닙니다.
    #[serde(rename = "runs.cancel")]
    RunsCancel,
    /// [`RunsSubscribeParams`] → [`RunsSubscribeResult`], 이후 `runs.event` 알림
    #[serde(rename = "runs.subscribe")]
    RunsSubscribe,
    /// [`SubscriptionRef`] → 빈 객체
    #[serde(rename = "runs.unsubscribe")]
    RunsUnsubscribe,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApiRequest {
    pub protocol_version: String,
    pub request_id: String,
    pub method: ApiMethod,
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApiResponse {
    pub protocol_version: String,
    pub request_id: String,
    #[serde(flatten)]
    pub outcome: ApiOutcome,
}

/// Wire에서 `result` 또는 `error` 필드 하나로 나타납니다.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ApiOutcome {
    Result(serde_json::Value),
    Error(ApiError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ApiErrorCode {
    Unauthenticated,
    Forbidden,
    InvalidRequest,
    UnsupportedMethod,
    UnsupportedProtocolVersion,
    NotFound,
    Conflict,
    /// 필요한 구성 요소(Provider, RN 등)를 지금 쓸 수 없음
    Unavailable,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApiError {
    pub code: ApiErrorCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub enum ApiNotificationMethod {
    /// [`RunEventParams`]
    #[serde(rename = "runs.event")]
    RunEvent,
    /// [`SubscriptionClosedParams`]
    #[serde(rename = "runs.subscriptionClosed")]
    SubscriptionClosed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApiNotification {
    pub protocol_version: String,
    pub method: ApiNotificationMethod,
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HelloParams {
    pub client_name: String,
    pub client_version: String,
    pub token: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HelloResult {
    pub server_version: String,
    pub protocol_version: String,
    pub project_id: String,
    pub host_id: String,
    pub connection_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Ok,
    Degraded,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ComponentHealth {
    /// 예: `scheduler`, `persistence`, `rn`, `provider:codex`
    pub name: String,
    pub status: HealthStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HealthResult {
    pub status: HealthStatus,
    pub server_version: String,
    pub components: Vec<ComponentHealth>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunsStartParams {
    pub task: TaskSpec,
    pub session: SessionRequest,
    /// 같은 키의 재요청은 같은 Run을 반환하고 프로세스를 새로 만들지 않습니다.
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunsStartResult {
    pub run_id: String,
    pub task_id: String,
    pub task_revision: u32,
    pub attempt: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunRef {
    pub run_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunsInspectResult {
    pub run_id: String,
    pub status: RunStatus,
    /// 지금까지 기록한 마지막 이벤트 순서. 이벤트가 없으면 0입니다.
    pub last_sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunsCancelParams {
    pub run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunsCancelResult {
    /// `false`면 이미 끝난 Run이라 취소할 것이 없습니다.
    pub accepted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunsSubscribeParams {
    pub run_id: String,
    /// 이 값보다 큰 `sequence`부터 보냅니다. 0은 처음부터입니다.
    pub after_sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunsSubscribeResult {
    pub subscription_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionRef {
    pub subscription_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunEventParams {
    pub subscription_id: String,
    pub event: ProviderEvent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionCloseReason {
    /// Run의 마지막 이벤트까지 보냈습니다.
    RunFinished,
    /// 요청한 순서의 이벤트가 보존 범위를 벗어났습니다. `runs.inspect` 후 다시 구독합니다.
    EventsUnavailable,
    /// 느린 구독자로 판단해 끊었습니다. 제어 이벤트는 영속 기록에서 다시 조회할 수 있습니다 (§15.1).
    SlowConsumer,
    Unsubscribed,
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionClosedParams {
    pub subscription_id: String,
    pub reason: SubscriptionCloseReason,
    /// 마지막으로 보낸 이벤트 순서. 재구독의 `afterSequence`로 씁니다.
    pub last_sequence: u64,
}

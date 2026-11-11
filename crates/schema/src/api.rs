//! Local Core API 계약 (Spec §3.3, §17, G0 Local Core API).
//!
//! G0 개정 1(REQ-005·007)에서 주소·인증·프레임 규칙을 확정했습니다. 프로토콜 버전은 [`API_PROTOCOL_VERSION`]입니다.
//!
//! # 프로젝트 식별
//! - `projectKey`: 프로젝트 루트를 OS 기능으로 정규화(심볼릭 링크 해석, Windows는 `canonicalize`)한 절대 경로에
//!   [`normalize_project_root`]를 적용하고 [`project_key`]로 계산합니다(UTF-8 SHA-256 앞 16자, 소문자 hex).
//! - HOST·EXT는 같은 결과를 내야 하며 공통 시험 벡터 `schemas/vectors/project-key.json`으로 확인합니다.
//!
//! # 주소
//! - Windows: Named Pipe `\\.\pipe\telos-weaver-<projectKey>`.
//! - Unix: `${XDG_RUNTIME_DIR}/telos-weaver/<projectKey>.sock`. `XDG_RUNTIME_DIR`이 없으면
//!   `/tmp/telos-weaver-<uid>/<projectKey>.sock`. 디렉터리는 권한 0700입니다. 경로 길이 제한 때문에 프로젝트 폴더에 두지 않습니다.
//!
//! # 인증과 접근 범위
//! - 토큰 파일은 프로젝트 폴더 밖의 사용자 디렉터리에 둡니다. Workspace·Git에 섞이지 않게 하기 위해서입니다.
//!   Windows `%LOCALAPPDATA%\telos-weaver\<projectKey>\api-token`,
//!   Unix `${XDG_STATE_HOME:-~/.local/state}/telos-weaver/<projectKey>/api-token`.
//! - Core는 시작할 때마다 32바이트 난수(소문자 hex 64자)로 새 토큰을 만들고, 사용자 전용 권한으로 쓴 뒤에만 연결을 받습니다.
//!   토큰 파일 권한·Pipe ACL·Socket 디렉터리 권한을 적용하지 못하면 시작하지 않습니다(fail closed).
//! - Core가 재시작하면 기존 연결은 끊기고 Client는 토큰을 다시 읽어 `core.hello`부터 다시 합니다.
//! - Pipe/Socket은 Core를 실행한 OS 사용자만 접근할 수 있게 만듭니다. `core.hello` 성공 전 다른 요청은 `unauthenticated`로 거부합니다.
//!   한 연결은 한 프로젝트의 Core에만 접근합니다.
//! - 한계: 같은 OS 사용자로 실행되는 Worker도 사용자 디렉터리에 접근할 수 있습니다. 이를 막는 것은 권한 Profile·Sandbox의 책임입니다 (SAFE-01).
//! - 토큰은 로그·이벤트·UI·URI·오류 메시지에 남기지 않습니다 (SAFE-04). [`HelloParams::token`]은 [`SecretString`]이며
//!   [`ApiRequest`]의 `Debug`는 `params.token`을 가립니다. 로그에는 `Debug` 형식만 쓰고 `params` 원문을 직접 출력하지 않습니다.
//!
//! # 프레임
//! - 메시지는 UTF-8 JSON 한 개를 한 줄로 보내는 NDJSON입니다. JSON 안에 줄바꿈 문자를 그대로 넣지 않습니다.
//! - [`MAX_FRAME_BYTES`]는 종료 `\n`을 뺀 UTF-8 바이트 수입니다. 송신은 [`encode_frame`]을 사용합니다.
//! - 상한을 넘는 요청을 받으면 `requestId: ""`, `frame_too_large` 오류를 보낸 뒤 연결을 닫습니다.
//! - 서버는 상한을 넘는 프레임을 보내지 않습니다. 큰 텍스트는 여러 이벤트로 나누고, 큰 원본은 `rawEventRef` Artifact로 참조합니다.
//!
//! # 메시지
//! - 요청 [`ApiRequest`]와 응답 [`ApiResponse`]는 `requestId`로 짝을 맞춥니다.
//! - 구독 이벤트는 [`ApiNotification`]으로 보내며 응답과 섞여 도착할 수 있습니다.
//! - 다만 한 구독의 `runs.event`·`runs.subscriptionClosed`는 그 구독을 만든 `runs.subscribe` 응답 뒤에만 보냅니다
//!   (재생 이벤트 포함). Client는 응답을 받는 즉시 `subscriptionId`를 연결하면 그 구독의 알림을 놓치지 않습니다.
//!   다른 요청의 응답과 다른 구독의 알림은 사이사이에 올 수 있습니다 (REQ-017).
//! - 메서드별 `params`·`result` 타입은 [`ApiMethod`] 각 항목의 문서를 따릅니다.

use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{ProviderEvent, RunStatus, SecretString, SessionRequest, TaskSpec};

pub const API_PROTOCOL_VERSION: &str = "0.2";
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;

/// OS에서 정규화한 프로젝트 루트 경로를 `projectKey` 계산용 문자열로 바꿉니다.
///
/// - `windows`: `\\?\UNC\` → `//`, `\\?\` 접두사 제거, `\` → `/`, 소문자로 변환
/// - 모든 OS: 루트(`/`, `c:/`)가 아니면 끝의 `/`를 제거
pub fn normalize_project_root(canonical: &str, windows: bool) -> String {
    let mut path = if windows {
        let stripped = if let Some(rest) = canonical.strip_prefix(r"\\?\UNC\") {
            format!(r"\\{rest}")
        } else {
            canonical
                .strip_prefix(r"\\?\")
                .unwrap_or(canonical)
                .to_owned()
        };
        stripped.replace('\\', "/").to_lowercase()
    } else {
        canonical.to_owned()
    };
    let is_root = path == "/" || (windows && path.len() == 3 && path.ends_with(":/"));
    while !is_root && path.len() > 1 && path.ends_with('/') {
        path.pop();
    }
    path
}

/// [`normalize_project_root`] 결과의 UTF-8 SHA-256 앞 16자(소문자 hex)입니다.
pub fn project_key(normalized_root: &str) -> String {
    let digest = Sha256::digest(normalized_root.as_bytes());
    digest[..8].iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameTooLarge {
    pub bytes: usize,
}

impl fmt::Display for FrameTooLarge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "frame of {} bytes exceeds {MAX_FRAME_BYTES}", self.bytes)
    }
}

impl std::error::Error for FrameTooLarge {}

/// 메시지를 한 줄 프레임(종료 `\n` 포함)으로 직렬화합니다. 상한은 `\n`을 뺀 바이트 수로 검사합니다.
pub fn encode_frame<T: Serialize>(message: &T) -> Result<String, FrameTooLarge> {
    let mut line = serde_json::to_string(message).expect("contract types always serialize");
    if line.len() > MAX_FRAME_BYTES {
        return Err(FrameTooLarge { bytes: line.len() });
    }
    line.push('\n');
    Ok(line)
}

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
    // --- Phase 1 H0 (§17, §19.6.2). 구현 전에는 Core가 `unsupported_method`로 답합니다. ---
    /// [`PlansCreateParams`] → [`PlansCreateResult`]
    #[serde(rename = "plans.create")]
    PlansCreate,
    /// [`PlanRef`] → [`PlansInspectResult`] (Snapshot과 Journal 커서)
    #[serde(rename = "plans.inspect")]
    PlansInspect,
    /// [`PlansCancelParams`] → [`AcceptedResult`]
    #[serde(rename = "plans.cancel")]
    PlansCancel,
    /// [`PlansMessageParams`] → [`PlansMessageResult`]. 사용자가 마스터에게 보내는 목표·답변
    #[serde(rename = "plans.message")]
    PlansMessage,
    /// [`TaskRef`] → [`TasksInspectResult`]
    #[serde(rename = "tasks.inspect")]
    TasksInspect,
    /// [`TasksCancelParams`] → [`AcceptedResult`]
    #[serde(rename = "tasks.cancel")]
    TasksCancel,
    /// [`TasksToolParams`] → 도구 출력 또는 `TaskToolError`. Run 범위 자격 연결에서만 허용합니다 (§9.2).
    #[serde(rename = "tasks.tool")]
    TasksTool,
    /// [`ArtifactsReadParams`] → [`ArtifactsReadResult`]
    #[serde(rename = "artifacts.read")]
    ArtifactsRead,
    /// [`PlanRef`] → [`InterventionsListResult`]
    #[serde(rename = "interventions.list")]
    InterventionsList,
    /// [`InterventionsAnswerParams`] → [`AcceptedResult`]
    #[serde(rename = "interventions.answer")]
    InterventionsAnswer,
    /// [`JournalSubscribeParams`] → [`JournalSubscribeResult`], 이후 `journal.entry` 알림
    #[serde(rename = "journal.subscribe")]
    JournalSubscribe,
    /// [`SubscriptionRef`] → 빈 객체
    #[serde(rename = "journal.unsubscribe")]
    JournalUnsubscribe,
    /// 빈 객체 → [`RnStatusResult`] (COMM-01·08)
    #[serde(rename = "rn.status")]
    RnStatus,
    /// [`RnSendParams`] → [`RnSendResult`]. 통제된 RPC 전송, 중복·TTL 검사 (COMM-03·07)
    #[serde(rename = "rn.send")]
    RnSend,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApiRequest {
    pub protocol_version: String,
    pub request_id: String,
    pub method: ApiMethod,
    pub params: serde_json::Value,
}

/// `params` 최상위의 `token` 값을 가립니다 (REQ-005).
impl fmt::Debug for ApiRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut params = self.params.clone();
        if let Some(token) = params.get_mut("token") {
            *token = serde_json::Value::String(SecretString::REDACTED.into());
        }
        f.debug_struct("ApiRequest")
            .field("protocol_version", &self.protocol_version)
            .field("request_id", &self.request_id)
            .field("method", &self.method)
            .field("params", &params)
            .finish()
    }
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
    /// 프레임이 [`MAX_FRAME_BYTES`]를 넘었습니다. 이 응답 뒤 연결을 닫습니다.
    FrameTooLarge,
    InvalidRequest,
    UnsupportedMethod,
    UnsupportedProtocolVersion,
    NotFound,
    Conflict,
    /// 필요한 구성 요소(Provider, RN 등)를 지금 쓸 수 없음
    Unavailable,
    Internal,
    /// 구독 커서가 보존 범위 밖입니다. `plans.inspect`로 Snapshot을 다시 받고 그 커서부터 구독합니다 (Phase 1 H0).
    CursorExpired,
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
    /// [`JournalEntryParams`] (Phase 1 H0)
    #[serde(rename = "journal.entry")]
    JournalEntry,
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
    pub token: SecretString,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HelloResult {
    pub server_version: String,
    pub protocol_version: String,
    pub project_id: String,
    pub host_id: String,
    pub connection_id: String,
    /// 인증한 자격의 범위입니다. 없으면 Phase 0과 같은 Client 범위입니다 (Phase 1 H0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<ConnectionScope>,
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

// --- Phase 1 H0 Local Core API (§17, §19.6.2) ---
//
// Phase 1 API는 호환 없이 버전을 올립니다(H0 D13). 버전 상향(`1.0`)은 HOST·EXT가 새 메서드를 연결하는 같은 변경에서 합니다.
// 모든 변경 요청은 `idempotencyKey`를 받고 같은 키·다른 내용은 `conflict`로 거부합니다.

/// 연결의 접근 범위입니다. 프로젝트 API 토큰은 `client`, Run 범위 Task MCP 자격은 `task-mcp`입니다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum ConnectionScope {
    Client,
    /// `tasks.tool`만 허용하며 호출자는 이 Run으로 고정됩니다.
    TaskMcp {
        run_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlansCreateParams {
    pub idempotency_key: String,
    pub goal: String,
    #[serde(default)]
    pub constraints: Vec<String>,
    /// 비우면 설정의 기본 마스터(Claude `sonnet`, H0 D6)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub master: Option<crate::AgentSelection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlansCreateResult {
    pub plan_id: String,
    pub master_task_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanRef {
    pub plan_id: String,
}

/// Plan의 현재 상태 Snapshot입니다. `cursor`부터 `journal.subscribe`하면 빠짐없이 이어집니다 (§14.1, UI-07).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlansInspectResult {
    pub plan: crate::Plan,
    pub tasks: Vec<crate::TaskRecord>,
    pub runs: Vec<crate::RunRecord>,
    pub pending_interventions: u32,
    pub cursor: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlansCancelParams {
    pub idempotency_key: String,
    pub plan_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlansMessageParams {
    pub idempotency_key: String,
    pub plan_id: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlansMessageResult {
    pub message_id: String,
}

/// 접수 여부입니다. 실제 정지·적용 결과는 Journal로 확인합니다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AcceptedResult {
    pub request_id: String,
    pub accepted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskRef {
    pub task_id: String,
}

/// Task 상세입니다 (UI-06). 계약·입출력 Version·Snapshot·실행·검증·승인 기록을 한 번에 돌려줍니다.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TasksInspectResult {
    pub task: crate::TaskRecord,
    pub runs: Vec<crate::RunRecord>,
    pub results: Vec<crate::TaskResult>,
    pub artifacts: Vec<crate::ArtifactVersion>,
    pub validations: Vec<crate::ValidationJob>,
    pub approvals: Vec<crate::ApprovalDecision>,
    pub snapshots: Vec<crate::ContextSnapshot>,
    pub diffs: Vec<crate::DiffSummary>,
    pub cursor: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TasksCancelParams {
    pub idempotency_key: String,
    pub task_id: String,
    pub reason: String,
}

/// Task MCP 도구 호출 중계입니다. 입력·출력 형식은 도구별 `Mcp*` Schema를 따릅니다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TasksToolParams {
    pub tool: crate::TaskTool,
    pub input: crate::JsonObject,
}

/// 한 번에 읽는 최대 바이트입니다. Base64 확장 뒤에도 프레임 상한 안에 들어가게 둡니다.
pub const MAX_ARTIFACT_READ_BYTES: u32 = 512 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactsReadParams {
    pub artifact: crate::ArtifactVersionRef,
    pub offset: u64,
    /// [`MAX_ARTIFACT_READ_BYTES`] 이하
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactsReadResult {
    pub content_type: String,
    pub total_bytes: u64,
    pub offset: u64,
    pub data_base64: String,
    pub eof: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum InterventionKind {
    /// 마스터의 `ask_user`
    Question,
    /// 사용자 경로로 올라온 승인 (`ProviderApprovalRequest`·`ApprovalDecision`)
    Approval,
}

/// 사용자 개입 요청입니다 (UI-03, §17 Approval / Intervention).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Intervention {
    pub intervention_id: String,
    pub plan_id: String,
    pub kind: InterventionKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    pub summary: String,
    pub reason: String,
    pub options: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InterventionsListResult {
    pub interventions: Vec<Intervention>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum InterventionAnswer {
    Choice { option: String },
    Text { text: String },
    Approval { answer: crate::ApprovalAnswer },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InterventionsAnswerParams {
    pub idempotency_key: String,
    pub intervention_id: String,
    pub answer: InterventionAnswer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JournalSubscribeParams {
    /// 비우면 프로젝트 전체입니다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_id: Option<String>,
    /// 이 값보다 큰 `sequence`부터 보냅니다. 보존 범위 밖이면 `cursor_expired`입니다.
    pub after_sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JournalSubscribeResult {
    pub subscription_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JournalEntryParams {
    pub subscription_id: String,
    pub entry: crate::JournalEntry,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RnStatusResult {
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<crate::rn::RnHealth>,
    /// 비호환·Degraded 사유 (COMM-08)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RnSendParams {
    pub idempotency_key: String,
    pub endpoint: crate::rn::EndpointRef,
    pub text: String,
    /// 이 시간이 지나면 전달하지 않습니다 (COMM-07).
    pub ttl_secs: u32,
}

/// 전송 접수와 실제 전달·실행·검증은 다른 상태입니다 (COMM-05).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RnSendResult {
    pub message: crate::rn::MessageRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt: Option<crate::rn::DeliveryReceipt>,
}

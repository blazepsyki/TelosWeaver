//! Provider Adapter (Spec §7).
//!
//! 메서드가 존재한다는 것이 기능 지원을 뜻하지 않습니다. 미지원 제어는
//! [`AdapterError::UnsupportedCapability`]를 반환합니다.

use std::{collections::HashMap, fmt};

use async_trait::async_trait;
use futures::stream::BoxStream;
use telos_schema::{
    ApprovalAnswer, Capability, FailureKind, ProviderEvent, RunContext, RunInspection, RunStarted,
    TaskSpec,
};

mod agy;
mod claude;
mod codex;
pub mod runtime;
mod unimplemented;

pub use agy::AgyAdapter;
pub use claude::ClaudeCodeAdapter;
pub use codex::CodexAdapter;
pub use unimplemented::UnsupportedAdapter;

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AdapterError {
    /// 이 Provider·버전·Host에서 지원하지 않거나 아직 구현하지 않은 제어입니다.
    UnsupportedCapability { capability: String, reason: String },
    /// §7.3 분류에 따른 실행 실패입니다. Provider 원본 오류는 `message`에 요약합니다.
    Failure { kind: FailureKind, message: String },
    /// 이 Adapter가 모르는 Run입니다. 재시작 뒤 추적을 잃은 경우도 포함합니다.
    UnknownRun { run_id: String },
    /// 같은 멱등 키로 다른 Task·Run의 시작을 요청했습니다.
    IdempotencyConflict { idempotency_key: String },
    /// 요청한 순서의 이벤트가 보존 범위를 벗어났습니다. `inspect_run` 후 다시 구독합니다.
    EventsUnavailable {
        run_id: String,
        after_sequence: u64,
        oldest_available: u64,
    },
}

impl AdapterError {
    /// 실행 실패로 분류할 수 있는 오류의 §7.3 분류입니다.
    pub fn failure_kind(&self) -> Option<FailureKind> {
        match self {
            Self::UnsupportedCapability { .. } => Some(FailureKind::UnsupportedCapability),
            Self::Failure { kind, .. } => Some(*kind),
            Self::UnknownRun { .. }
            | Self::IdempotencyConflict { .. }
            | Self::EventsUnavailable { .. } => None,
        }
    }
}

impl fmt::Display for AdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedCapability { capability, reason } => {
                write!(f, "unsupported capability '{capability}': {reason}")
            }
            Self::Failure { kind, message } => write!(f, "{kind:?}: {message}"),
            Self::UnknownRun { run_id } => write!(f, "unknown run '{run_id}'"),
            Self::IdempotencyConflict { idempotency_key } => {
                write!(
                    f,
                    "idempotency key '{idempotency_key}' was used for a different run"
                )
            }
            Self::EventsUnavailable {
                run_id,
                after_sequence,
                oldest_available,
            } => write!(
                f,
                "events after {after_sequence} of run '{run_id}' are no longer retained \
                 (oldest available: {oldest_available})"
            ),
        }
    }
}

impl std::error::Error for AdapterError {}

/// Core는 여러 Provider를 `Arc<dyn AgentAdapter>`로 다룹니다.
///
/// 구현은 아래 계약을 따릅니다 (G0 Adapter 실행·이벤트·취소 계약).
#[async_trait]
pub trait AgentAdapter: Send + Sync {
    fn id(&self) -> &str;

    /// 확인한 Capability만 보고합니다. 확인하지 못한 기능은 `unverified`로 두거나 생략합니다.
    async fn detect(&self, host_id: &str) -> Result<HashMap<String, Capability>, AdapterError>;

    /// - Run ID는 Core가 `context.run_id`로 발급합니다. 반환하는 `RunStarted::run_id`는 같은 값입니다.
    /// - 같은 `context.idempotency_key`와 같은 Run의 재요청은 처음의 `RunStarted`를 반환하고
    ///   프로세스를 새로 만들지 않습니다. 같은 키로 다른 Run을 요청하면 `IdempotencyConflict`입니다.
    /// - 반환 시점에 프로세스가 이미 끝났을 수 있습니다. 결과는 `events`로 확인합니다.
    /// - 오류를 반환한 Run은 이벤트를 남기지 않으며 그 Run의 `events`는 `UnknownRun`입니다.
    ///   반환 뒤 프로세스 시작이 실패하면 종료 `error`(`start_failed`)로 Run을 끝냅니다.
    async fn start_run(
        &self,
        task: &TaskSpec,
        context: &RunContext,
    ) -> Result<RunStarted, AdapterError>;

    /// - `sequence > after_sequence`인 이벤트를 순서대로 보냅니다. `after_sequence = 0`은 처음부터입니다.
    /// - 시작 직후부터 이벤트를 보존하여 구독 전에 끝난 Run도 회수할 수 있게 합니다 (§7.2).
    /// - 보존 범위를 벗어난 요청은 첫 항목으로 `EventsUnavailable`을 보냅니다.
    /// - Run의 종료 이벤트(`process_exited`, `turn_finished`, 종료 `error`)를 보낸 뒤 닫힙니다.
    ///   종료 규칙은 `telos_schema` events 모듈 문서(G0 개정 1 REQ-006)를 따르며
    ///   `telos_schema::check_run_events`로 Contract 시험합니다.
    /// - Payload 형식은 `telos_schema::ProviderEventPayload`를 따릅니다.
    fn events(
        &self,
        run_id: &str,
        after_sequence: u64,
    ) -> BoxStream<'static, Result<ProviderEvent, AdapterError>>;

    async fn send_input(
        &self,
        run_id: &str,
        input_id: &str,
        text: &str,
    ) -> Result<(), AdapterError>;

    /// 실행 중 응답 경로가 없는 Provider는 `UnsupportedCapability`를 반환합니다 (§7.4).
    async fn answer_approval(
        &self,
        run_id: &str,
        request_id: &str,
        decision: ApprovalAnswer,
    ) -> Result<(), AdapterError>;

    /// 반환값은 취소 요청 접수 여부이며 실제 종료가 아닙니다. `false`는 이미 끝난 Run입니다.
    /// 실제 종료는 `process_exited`(또는 `turn_finished`의 `cancelled`)와 `inspect_run`으로 확인합니다.
    async fn cancel_run(&self, run_id: &str) -> Result<bool, AdapterError>;

    /// 추적을 잃었으면 `Unknown`을 반환합니다. 성공으로 추정하지 않습니다.
    async fn inspect_run(&self, run_id: &str) -> Result<RunInspection, AdapterError>;
}

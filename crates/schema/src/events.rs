//! Spec §7.2 ProviderEvent의 kind별 정규 Payload (G0 이벤트·취소 계약).
//!
//! `ProviderEvent::payload`는 Wire에서 JSON 객체이며, 각 `kind`의 Payload는 아래 타입을 따릅니다.
//! Provider 원본 이벤트는 `raw_event_ref`로 연결하고 이 타입으로 변환해 보냅니다.
//!
//! # 순서 규칙
//! - `sequence`는 Run마다 1부터 시작해 1씩 증가합니다. 재구독은 `after_sequence` 다음부터 이어지며 다시 매기지 않습니다.
//! - Turn 완료(`turn_finished`)와 프로세스 종료(`process_exited`)는 별도 사건입니다.
//! - 취소 접수 뒤에 도착한 이벤트도 순서대로 보존합니다 (§9.5).
//! - 사용량을 알 수 없는 값은 0이 아니라 생략(`None`)합니다 (§15.2).
//!
//! # 종료 규칙 (G0 개정 1 REQ-006)
//! Run의 마지막 이벤트(종료 이벤트)는 [`RunTermination`] 중 하나이며 그 뒤에는 어떤 이벤트도 없습니다.
//! - `process_exited`: 그 Run 전용 프로세스가 끝났을 때. 두 출력 스트림이 닫히고(`Closed`/`ReadError`)
//!   자식 회수를 시도한 뒤 보냅니다.
//! - `turn_finished`: 프로세스가 Run보다 오래 사는 경우. 공유 프로세스의 종료는 Run 이벤트로 보내지 않습니다.
//! - `error`(`terminal: true`): 프로세스가 시작되지 않았거나 추적을 잃어 종료를 확인할 수 없을 때.
//!   존재하지 않은 프로세스의 `process_exited`를 만들지 않습니다.
//!
//! 전용 프로세스 Run에서 `turn_finished` 뒤에는 `usage`·`error`·`process_exited`만 올 수 있습니다.
//! `start_run`이 오류를 반환한 Run에는 이벤트가 없습니다. 이 규칙은 [`check_run_events`]로 검사합니다.

use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{FailureKind, JsonObject, ProviderEvent, ProviderEventKind};

/// `ProviderEvent::schema_version`의 현재 값입니다. 0.2: 종료 규칙 확정(G0 개정 1).
pub const PROVIDER_EVENT_SCHEMA_VERSION: &str = "0.2";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionStartedPayload {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TextChannel {
    /// 에이전트의 응답 본문
    Assistant,
    /// 진행 상황 등 Provider가 표시용으로 보낸 텍스트
    Status,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TextDeltaPayload {
    pub channel: TextChannel,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ToolStartedPayload {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolOutcome {
    Succeeded,
    Failed,
    /// 권한 정책이나 Provider가 실행을 거부했습니다.
    Denied,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ToolCompletedPayload {
    pub name: String,
    pub outcome: ToolOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequiredPayload {
    pub request_id: String,
    pub action: String,
    pub summary: String,
    /// 실행 중 응답을 보낼 수 있는지. Agy처럼 응답 경로가 없으면 `false`입니다 (§7.4).
    pub answerable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UsageScope {
    /// 직전 usage 이벤트 이후의 증가량
    Delta,
    /// Run 시작 이후 누적값
    Cumulative,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UsageSource {
    ProviderReported,
    Estimated,
}

/// 캐시 값이 입력 합계에 포함되어 있으면 `cache_included_in_input`을 `true`로 보고합니다 (§8.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UsagePayload {
    pub scope: UsageScope,
    pub source: UsageSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_included_in_input: Option<bool>,
    /// API 실제 비용만 기록합니다. CLI 구독 사용량을 비용으로 환산하지 않습니다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TurnStatus {
    Completed,
    Failed,
    Cancelled,
    Interrupted,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TurnFinishedPayload {
    pub status: TurnStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProcessTermination {
    Exited,
    TerminatedOnRequest,
    Killed,
    External,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProcessExitedPayload {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    pub termination: ProcessTermination,
    /// 자식 프로세스까지 회수했는지. 확인할 수 없으면 생략합니다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub children_reaped: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ErrorPayload {
    pub kind: FailureKind,
    pub message: String,
    /// 이 오류 뒤 Run이 더 진행되지 않는지. 종료 이벤트는 별도로 보냅니다.
    pub terminal: bool,
}

/// kind별 Payload의 합. Wire에서는 태그 없이 Payload 객체만 보냅니다.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum ProviderEventPayload {
    SessionStarted(SessionStartedPayload),
    TextDelta(TextDeltaPayload),
    ToolStarted(ToolStartedPayload),
    ToolCompleted(ToolCompletedPayload),
    ApprovalRequired(ApprovalRequiredPayload),
    Usage(UsagePayload),
    TurnFinished(TurnFinishedPayload),
    ProcessExited(ProcessExitedPayload),
    Error(ErrorPayload),
}

impl ProviderEventPayload {
    pub fn kind(&self) -> ProviderEventKind {
        match self {
            Self::SessionStarted(_) => ProviderEventKind::SessionStarted,
            Self::TextDelta(_) => ProviderEventKind::TextDelta,
            Self::ToolStarted(_) => ProviderEventKind::ToolStarted,
            Self::ToolCompleted(_) => ProviderEventKind::ToolCompleted,
            Self::ApprovalRequired(_) => ProviderEventKind::ApprovalRequired,
            Self::Usage(_) => ProviderEventKind::Usage,
            Self::TurnFinished(_) => ProviderEventKind::TurnFinished,
            Self::ProcessExited(_) => ProviderEventKind::ProcessExited,
            Self::Error(_) => ProviderEventKind::Error,
        }
    }

    /// `kind`에 맞는 Payload 타입으로 해석합니다.
    pub fn parse(kind: ProviderEventKind, payload: &JsonObject) -> serde_json::Result<Self> {
        let value = serde_json::Value::Object(payload.clone());
        Ok(match kind {
            ProviderEventKind::SessionStarted => {
                Self::SessionStarted(serde_json::from_value(value)?)
            }
            ProviderEventKind::TextDelta => Self::TextDelta(serde_json::from_value(value)?),
            ProviderEventKind::ToolStarted => Self::ToolStarted(serde_json::from_value(value)?),
            ProviderEventKind::ToolCompleted => Self::ToolCompleted(serde_json::from_value(value)?),
            ProviderEventKind::ApprovalRequired => {
                Self::ApprovalRequired(serde_json::from_value(value)?)
            }
            ProviderEventKind::Usage => Self::Usage(serde_json::from_value(value)?),
            ProviderEventKind::TurnFinished => Self::TurnFinished(serde_json::from_value(value)?),
            ProviderEventKind::ProcessExited => Self::ProcessExited(serde_json::from_value(value)?),
            ProviderEventKind::Error => Self::Error(serde_json::from_value(value)?),
        })
    }

    pub fn to_object(&self) -> JsonObject {
        match serde_json::to_value(self) {
            Ok(serde_json::Value::Object(map)) => map,
            _ => unreachable!("event payloads always serialize to JSON objects"),
        }
    }
}

/// 끝난 Run의 종료 이벤트 종류입니다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RunTermination {
    ProcessExited,
    TurnFinished,
    TerminalError,
}

/// [`check_run_events`]가 찾은 규칙 위반입니다. `index`는 0부터 센 이벤트 위치입니다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunEventViolation {
    Empty,
    MixedRun {
        index: usize,
    },
    SequenceGap {
        index: usize,
        expected: u64,
        actual: u64,
    },
    InvalidPayload {
        index: usize,
        message: String,
    },
    EventAfterTermination {
        index: usize,
    },
    NotAllowedAfterTurn {
        index: usize,
        kind: ProviderEventKind,
    },
    MissingTermination,
}

impl fmt::Display for RunEventViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("run has no events"),
            Self::MixedRun { index } => write!(f, "event {index} belongs to another run"),
            Self::SequenceGap {
                index,
                expected,
                actual,
            } => write!(
                f,
                "event {index}: expected sequence {expected}, got {actual}"
            ),
            Self::InvalidPayload { index, message } => {
                write!(f, "event {index}: payload does not match kind: {message}")
            }
            Self::EventAfterTermination { index } => {
                write!(f, "event {index} follows the termination event")
            }
            Self::NotAllowedAfterTurn { index, kind } => {
                write!(
                    f,
                    "event {index} ({kind:?}) is not allowed after turn_finished"
                )
            }
            Self::MissingTermination => f.write_str("last event is not a termination event"),
        }
    }
}

impl std::error::Error for RunEventViolation {}

/// 끝난 Run의 전체 이벤트가 순서·Payload·종료 규칙을 지키는지 검사합니다.
/// Provider 담당은 자기 Adapter가 만든 이벤트를 이 함수로 Contract 시험합니다.
pub fn check_run_events(events: &[ProviderEvent]) -> Result<RunTermination, RunEventViolation> {
    let first = events.first().ok_or(RunEventViolation::Empty)?;
    let mut termination = None;
    let mut turn_finished = false;

    for (index, event) in events.iter().enumerate() {
        if event.run_id != first.run_id {
            return Err(RunEventViolation::MixedRun { index });
        }
        let expected = index as u64 + 1;
        if event.sequence != expected {
            return Err(RunEventViolation::SequenceGap {
                index,
                expected,
                actual: event.sequence,
            });
        }
        let payload = ProviderEventPayload::parse(event.kind, &event.payload).map_err(|e| {
            RunEventViolation::InvalidPayload {
                index,
                message: e.to_string(),
            }
        })?;
        if termination.is_some() {
            return Err(RunEventViolation::EventAfterTermination { index });
        }
        if turn_finished
            && !matches!(
                event.kind,
                ProviderEventKind::Usage
                    | ProviderEventKind::Error
                    | ProviderEventKind::ProcessExited
            )
        {
            return Err(RunEventViolation::NotAllowedAfterTurn {
                index,
                kind: event.kind,
            });
        }
        match payload {
            ProviderEventPayload::ProcessExited(_) => {
                termination = Some(RunTermination::ProcessExited)
            }
            ProviderEventPayload::Error(ErrorPayload { terminal: true, .. }) => {
                termination = Some(RunTermination::TerminalError)
            }
            ProviderEventPayload::TurnFinished(_) => turn_finished = true,
            _ => {}
        }
    }

    match termination {
        Some(termination) => Ok(termination),
        None if events.last().map(|e| e.kind) == Some(ProviderEventKind::TurnFinished) => {
            Ok(RunTermination::TurnFinished)
        }
        None => Err(RunEventViolation::MissingTermination),
    }
}

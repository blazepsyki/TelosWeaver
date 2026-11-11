//! Spec §14.1 데이터 소유권, §14.3 재조정과 중복 방지 (H0 영속 기록)
//!
//! 규칙: 상태 변경과 그 [`JournalEntry`]·[`RunIntent`] 갱신은 같은 DB Transaction으로 기록합니다.
//! 한쪽만 기록된 상태를 만들지 않으며, 기록에 실패하면 후속 실행을 보류합니다 (§15.1).
//! Provider 원본 이벤트는 기존 `ProviderEvent`(Run별 `sequence`)로 저장하며 이 Journal과 별개입니다.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::JsonObject;

/// Core DB Schema 버전입니다. Migration을 추가할 때 올립니다.
pub const CORE_DB_SCHEMA_VERSION: u32 = 1;

/// 실행 의도의 단계입니다 (§9.1 4번, §14.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum IntentPhase {
    /// 의도만 기록했고 Adapter에 전달하지 않았습니다. 외부 부작용이 없습니다.
    Recorded,
    /// Adapter에 시작을 요청했지만 시작 결과(`RunStarted`)를 받지 못했습니다.
    Dispatched,
    /// 시작 결과와 Native ID·프로세스 정체성을 받았습니다.
    Started,
    /// 종료 결과를 확정했습니다.
    Settled,
    /// 전달 전에 취소·거부되어 실행하지 않았습니다.
    Abandoned,
}

/// Core가 비정상 종료한 뒤 재시작했을 때 미완 의도의 처리입니다 (§14.2·§14.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum RecoveryAction {
    /// 외부 실행이 없었으므로 의도를 버리거나 새로 전달할 수 있습니다.
    ResumeDispatch,
    /// 실제 프로세스·Session을 대조합니다. 확정하지 못하면 NeedsReconciliation으로 두고 재실행하지 않습니다.
    Reconcile,
    /// 이미 끝난 의도입니다.
    Nothing,
}

impl IntentPhase {
    pub fn recovery_action(self) -> RecoveryAction {
        match self {
            Self::Recorded => RecoveryAction::ResumeDispatch,
            Self::Dispatched | Self::Started => RecoveryAction::Reconcile,
            Self::Settled | Self::Abandoned => RecoveryAction::Nothing,
        }
    }
}

/// 실행 중인 프로세스를 다시 찾을 때 쓰는 증거입니다. PID만으로 정체성을 확정하지 않습니다 (§14.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProcessEvidence {
    pub host_id: String,
    pub pid: u32,
    /// OS가 보고한 프로세스 시작 시각
    pub process_started_at: String,
    /// Run별 실행 토큰의 SHA-256. 토큰 원문은 저장하지 않습니다 (§14.1).
    pub run_token_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunIntent {
    pub intent_id: String,
    pub run_id: String,
    pub task_id: String,
    pub task_revision: u32,
    pub attempt: u32,
    pub idempotency_key: String,
    /// 고정한 입력·정책·설정의 SHA-256입니다. 같은 키에 다른 값이 오면 거부합니다.
    pub payload_digest: String,
    pub phase: IntentPhase,
    pub host_id: String,
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process: Option<ProcessEvidence>,
    pub recorded_at: String,
    pub updated_at: String,
}

/// Journal 항목이 가리키는 범위입니다.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JournalScope {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum JournalKind {
    PlanChanged,
    TaskChanged,
    RunChanged,
    IntentChanged,
    SessionChanged,
    DecisionApplied,
    ArtifactRegistered,
    ValidationRecorded,
    ApprovalRecorded,
    LockChanged,
    /// 긴급 취소·권한 철회 등 활성 실행에 적용하는 제어 이벤트 (§13.1)
    Control,
}

/// 상태 변경 기록입니다. `sequence`는 프로젝트 DB 안에서 1부터 빈틈없이 올라가며 구독 커서로 씁니다.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JournalEntry {
    pub sequence: u64,
    pub recorded_at: String,
    pub scope: JournalScope,
    pub kind: JournalKind,
    /// 변경 뒤 대상의 `stateVersion`
    pub entity_version: u64,
    pub payload: JsonObject,
}

/// 멱등 키 기록입니다. 범위(`scope`)는 키를 쓰는 기능 단위(예: `runs.start`, `decision`)입니다.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct IdempotencyRecord {
    pub scope: String,
    pub key: String,
    pub payload_digest: String,
    pub result: JsonObject,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IdempotencyCheck {
    /// 처음 보는 키
    New,
    /// 같은 키·같은 내용: 처음 결과를 그대로 돌려줍니다.
    Replay,
    /// 같은 키·다른 내용: 거부합니다 (§4.2).
    Conflict,
}

pub fn check_idempotency(existing_digest: Option<&str>, payload_digest: &str) -> IdempotencyCheck {
    match existing_digest {
        None => IdempotencyCheck::New,
        Some(d) if d == payload_digest => IdempotencyCheck::Replay,
        Some(_) => IdempotencyCheck::Conflict,
    }
}

/// DB를 열 때의 판정입니다. 더 새 Schema를 이전 Core가 열지 않습니다 (§14.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DbOpenDecision {
    Current,
    /// 백업을 만든 뒤 `from`에서 현재 버전까지 Migration합니다. 백업 실패 시 열지 않습니다.
    MigrateWithBackup {
        from: u32,
    },
    /// 더 새 Core가 만든 DB입니다. 열지 않고 오류를 보고합니다.
    TooNew {
        found: u32,
    },
}

pub fn db_open_decision(found: u32) -> DbOpenDecision {
    match found.cmp(&CORE_DB_SCHEMA_VERSION) {
        std::cmp::Ordering::Equal => DbOpenDecision::Current,
        std::cmp::Ordering::Less => DbOpenDecision::MigrateWithBackup { from: found },
        std::cmp::Ordering::Greater => DbOpenDecision::TooNew { found },
    }
}

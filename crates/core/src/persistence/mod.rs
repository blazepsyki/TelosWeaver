//! 영속 저장소 (Spec §14, PH1-PERSIST).
//!
//! 프로젝트별 SQLite 파일 하나(H0 D1)를 동기 API로 다룹니다. 호출자는 필요하면 `spawn_blocking`으로 부릅니다.
//! 상태 변경·실행 의도·Journal 기록은 [`Store::apply`] 한 번이 한 Transaction입니다. 쓰기마다 예상 버전을 확인하고
//! 하나라도 맞지 않으면 전체를 적용하지 않습니다 (§14.1, §14.3, SAFE-06·13).
//!
//! 저장소는 원자성과 조회만 제공합니다. 상태 전이의 옳고 그름(`TaskStatus::can_transition_to` 등)과 재실행 여부는
//! Scheduler가 정합니다 (§19.6.3). 구현은 [`SqliteStore`]입니다.

mod sqlite;

pub use sqlite::{SqliteOptions, SqliteStore, core_db_path};

use telos_schema::{
    ApprovalDecision, ArtifactReference, ArtifactVersion, ArtifactVersionRef, BudgetLedger,
    ContextSnapshot, IdempotencyRecord, JournalEntry, JournalKind, JournalScope, JsonObject, Plan,
    ProviderEvent, ResourceLock, RunIntent, RunRecord, RunSession, SessionRecord, TaskRecord,
    UsageRecord, ValidationJob, WorkspaceRecord,
};

/// `stateVersion` 필드가 없는 기록(Plan·예산 장부)과 저장소가 관리하는 버전입니다.
#[derive(Debug, Clone, PartialEq)]
pub struct Versioned<T> {
    pub value: T,
    pub version: u64,
}

/// 조건부 쓰기 하나입니다.
///
/// `expected_version`은 현재 저장된 버전이며 새로 만들 때는 `None`입니다. 새 기록의 버전은 1, 갱신은 예상 버전 + 1입니다.
/// `stateVersion` 필드가 있는 기록(Task·Run·Workspace·검증·승인)은 저장소가 그 필드를 새 버전으로 덮어써 저장합니다.
/// 아래에 따로 적지 않은 쓰기는 같은 Transaction에 [`JournalEntry`] 하나를 추가합니다.
// 배치는 한 번 적용하고 버리는 짧은 값이라 큰 변형을 Box로 감싸지 않습니다.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum StateWrite {
    Plan {
        plan: Plan,
        expected_version: Option<u64>,
    },
    Task {
        record: TaskRecord,
        expected_version: Option<u64>,
    },
    Run {
        record: RunRecord,
        expected_version: Option<u64>,
    },
    /// 새 의도는 `recorded`로만 만들 수 있고, 단계는 앞으로만 옮깁니다
    /// (`recorded → dispatched → started → settled`, `abandoned`는 `recorded`에서만). 어긋나면 `VersionConflict`입니다.
    Intent { intent: RunIntent },
    /// 한 번 기록하면 바꾸지 않습니다. 같은 내용을 다시 쓰면 아무 일도 하지 않고, 다른 내용이면 `VersionConflict`입니다.
    Session { record: SessionRecord },
    /// Run과 Session의 연결. 같은 연결을 다시 쓰면 Turn 목록 등을 갱신합니다.
    RunSession { link: RunSession },
    /// Run 시작 전에 고정한 문맥입니다. Run마다 하나이며 바꾸지 않습니다. Journal 항목을 만들지 않습니다 (CTX-01).
    ContextSnapshot { snapshot: ContextSnapshot },
    Workspace {
        record: WorkspaceRecord,
        expected_version: Option<u64>,
    },
    /// 같은 자원에 충돌하는 잠금(`LockMode::conflicts_with`)이 있으면 `LockConflict`입니다.
    /// 넘겨받을 때는 같은 배치에서 이전 잠금을 풀고 Epoch를 올린 새 잠금을 잡습니다 (§14.3).
    AcquireLock { lock: ResourceLock },
    /// 잠금이 있고 소유 Epoch가 같을 때만 풉니다. 아니면 `VersionConflict`입니다(넘겨받은 뒤 이전 소유자의 해제 거부).
    ReleaseLock { lock_id: String, owner_epoch: u64 },
    /// 불변입니다. 같은 내용을 다시 쓰면 아무 일도 하지 않고, 다른 내용이면 `VersionConflict`입니다 (ART-03).
    ArtifactVersion { version: ArtifactVersion },
    /// 같은 참조는 한 번만 저장합니다. Journal 항목을 만들지 않습니다.
    ArtifactReference { reference: ArtifactReference },
    Validation {
        job: ValidationJob,
        expected_version: Option<u64>,
    },
    Approval {
        decision: ApprovalDecision,
        expected_version: Option<u64>,
    },
    /// Plan별 예산 장부 (§13.2)
    Budget {
        ledger: BudgetLedger,
        expected_version: Option<u64>,
    },
    /// 같은 범위·키가 없으면 저장합니다. 있으면 배치 전체를 적용하지 않고, Digest가 같으면 `IdempotencyReplay`,
    /// 다르면 `IdempotencyConflict`를 돌려줍니다. Journal 항목을 만들지 않습니다 (§14.3, SAFE-13).
    Idempotency { record: IdempotencyRecord },
    /// 같은 ID·같은 내용을 다시 쓰면 아무 일도 하지 않습니다(중복 합산 방지).
    Usage { record: UsageRecord },
    /// 대상 기록이 없는 Journal 항목입니다. `DecisionApplied`와 `Control`만 받습니다.
    Journal {
        scope: JournalScope,
        kind: JournalKind,
        payload: JsonObject,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct WriteBatch {
    pub writes: Vec<StateWrite>,
}

impl WriteBatch {
    pub fn new(writes: Vec<StateWrite>) -> Self {
        Self { writes }
    }
}

/// 적용된 배치가 만든 Journal 범위입니다. 항목을 `entries`개 만들었고 마지막 번호가 `last_sequence`입니다.
/// 항목을 만들지 않은 배치는 `entries`가 0이고 `last_sequence`는 그 시점의 마지막 번호입니다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppliedBatch {
    pub entries: u64,
    pub last_sequence: u64,
}

impl AppliedBatch {
    pub fn first_sequence(&self) -> Option<u64> {
        (self.entries > 0).then(|| self.last_sequence + 1 - self.entries)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    /// 예상 버전·Epoch·단계가 맞지 않거나 불변 기록을 바꾸려 함. 다시 읽고 판단합니다.
    VersionConflict {
        entity: String,
    },
    LockConflict {
        resource: String,
    },
    /// 같은 멱등 키·같은 Digest가 이미 적용됨. 배치는 적용하지 않았고 저장된 결과를 재생합니다.
    IdempotencyReplay {
        scope: String,
        key: String,
    },
    /// 같은 멱등 키·다른 Digest (§4.2)
    IdempotencyConflict {
        scope: String,
        key: String,
    },
    /// 저장소 규칙에 맞지 않는 쓰기(호출자 오류)
    Invalid {
        reason: String,
    },
    /// Core보다 새 Schema (§14.1)
    SchemaTooNew {
        found: u32,
    },
    /// 다른 프로세스가 이 프로젝트 DB를 쓰고 있음 (OPS-01)
    AlreadyOpen {
        path: String,
    },
    /// 기록 실패. 호출자는 후속 실행을 보류합니다 (§15.1).
    Io {
        message: String,
    },
    Corrupt {
        message: String,
    },
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::VersionConflict { entity } => write!(f, "version conflict: {entity}"),
            Self::LockConflict { resource } => write!(f, "lock conflict: {resource}"),
            Self::IdempotencyReplay { scope, key } => {
                write!(f, "idempotency key already applied: {scope}/{key}")
            }
            Self::IdempotencyConflict { scope, key } => {
                write!(
                    f,
                    "idempotency key reused with another payload: {scope}/{key}"
                )
            }
            Self::Invalid { reason } => write!(f, "invalid write: {reason}"),
            Self::SchemaTooNew { found } => {
                write!(f, "database schema {found} is newer than this core")
            }
            Self::AlreadyOpen { path } => {
                write!(f, "database is in use by another process: {path}")
            }
            Self::Io { message } => write!(f, "storage failure: {message}"),
            Self::Corrupt { message } => write!(f, "stored data is corrupt: {message}"),
        }
    }
}

impl std::error::Error for StoreError {}

pub trait Store: Send + Sync {
    fn apply(&self, batch: WriteBatch) -> Result<AppliedBatch, StoreError>;

    fn plan(&self, plan_id: &str) -> Result<Option<Versioned<Plan>>, StoreError>;
    fn plans(&self) -> Result<Vec<Versioned<Plan>>, StoreError>;
    fn task(&self, task_id: &str) -> Result<Option<TaskRecord>, StoreError>;
    /// 만든 순서
    fn plan_tasks(&self, plan_id: &str) -> Result<Vec<TaskRecord>, StoreError>;
    /// 만든 순서
    fn task_runs(&self, task_id: &str) -> Result<Vec<RunRecord>, StoreError>;
    fn run(&self, run_id: &str) -> Result<Option<RunRecord>, StoreError>;
    fn run_intent(&self, run_id: &str) -> Result<Option<RunIntent>, StoreError>;
    fn session(&self, session_id: &str) -> Result<Option<SessionRecord>, StoreError>;
    fn task_sessions(&self, owner_task_id: &str) -> Result<Vec<SessionRecord>, StoreError>;
    fn run_sessions(&self, run_id: &str) -> Result<Vec<RunSession>, StoreError>;
    fn context_snapshot(&self, run_id: &str) -> Result<Option<ContextSnapshot>, StoreError>;
    fn workspace(&self, workspace_id: &str) -> Result<Option<WorkspaceRecord>, StoreError>;
    fn task_workspaces(&self, owner_task_id: &str) -> Result<Vec<WorkspaceRecord>, StoreError>;
    fn artifact_version(
        &self,
        target: &ArtifactVersionRef,
    ) -> Result<Option<ArtifactVersion>, StoreError>;
    /// 등록 순서
    fn task_artifacts(&self, task_id: &str) -> Result<Vec<ArtifactVersion>, StoreError>;
    fn artifact_references(
        &self,
        target: &ArtifactVersionRef,
    ) -> Result<Vec<ArtifactReference>, StoreError>;
    fn validation(&self, validation_id: &str) -> Result<Option<ValidationJob>, StoreError>;
    fn task_validations(&self, task_id: &str) -> Result<Vec<ValidationJob>, StoreError>;
    fn approval(&self, approval_id: &str) -> Result<Option<ApprovalDecision>, StoreError>;
    fn plan_approvals(&self, plan_id: &str) -> Result<Vec<ApprovalDecision>, StoreError>;
    fn budget(&self, plan_id: &str) -> Result<Option<Versioned<BudgetLedger>>, StoreError>;
    fn idempotency(&self, scope: &str, key: &str) -> Result<Option<IdempotencyRecord>, StoreError>;
    fn plan_usage(&self, plan_id: &str) -> Result<Vec<UsageRecord>, StoreError>;

    /// 재시작 대조용: `settled`·`abandoned`가 아닌 실행 의도와 현재 잠금 (§14.3)
    fn unfinished_intents(&self) -> Result<Vec<RunIntent>, StoreError>;
    fn locks(&self) -> Result<Vec<ResourceLock>, StoreError>;

    /// `after_sequence`보다 큰 항목을 최대 `limit`개. `plan_id`를 주면 그 Plan 범위만 돌려줍니다.
    /// `after_sequence`가 보존 범위보다 앞이면 `Ok(None)`(`cursor_expired`)이고 호출자는 Snapshot부터 다시 받습니다.
    fn journal_after(
        &self,
        plan_id: Option<&str>,
        after_sequence: u64,
        limit: usize,
    ) -> Result<Option<Vec<JournalEntry>>, StoreError>;
    fn last_sequence(&self) -> Result<u64, StoreError>;
    /// `sequence`가 `keep_from`보다 작은 Journal 항목을 지웁니다. 지운 개수를 돌려줍니다.
    fn prune_journal(&self, keep_from: u64) -> Result<u64, StoreError>;

    /// Provider 원본 이벤트를 Run별 `sequence`로 저장합니다. Journal과 별개입니다 (§14.3).
    /// 같은 Run·같은 `sequence`가 이미 있으면 그 항목은 건너뜁니다(재전달).
    fn append_run_events(&self, events: &[ProviderEvent]) -> Result<(), StoreError>;
    /// `sequence`가 `from_sequence` 이상인 이벤트를 순서대로 최대 `limit`개
    fn run_events(
        &self,
        run_id: &str,
        from_sequence: u64,
        limit: usize,
    ) -> Result<Vec<ProviderEvent>, StoreError>;
}

//! 영속 저장소 (Spec §14, PH1-PERSIST). H0 공개 인터페이스이며 구현은 PH1-PERSIST가 합니다.
//!
//! 프로젝트별 SQLite 파일 하나(H0 D1)를 동기 API로 다룹니다. 호출자는 필요하면 `spawn_blocking`으로 부릅니다.
//! 상태 변경·실행 의도·Journal 기록은 [`Store::apply`] 한 번이 한 Transaction입니다. 쓰기마다 예상 버전을 확인하고
//! 하나라도 맞지 않으면 전체를 적용하지 않습니다 (§14.1, §14.3).

use telos_schema::{
    ApprovalDecision, ArtifactReference, ArtifactVersion, ArtifactVersionRef, IdempotencyRecord,
    JournalEntry, Plan, ResourceLock, RunIntent, RunRecord, RunSession, SessionRecord, TaskRecord,
    UsageRecord, ValidationJob, WorkspaceRecord,
};

/// 조건부 쓰기 하나입니다. `expected_version`은 현재 저장된 `stateVersion`이며 새로 만들 때는 `None`입니다.
/// 저장소는 쓰기마다 대상의 `stateVersion`을 1 올리고 해당 [`JournalEntry`]를 같은 Transaction에 추가합니다.
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
    /// 실행 의도는 단계(`phase`)를 앞으로만 옮깁니다.
    Intent {
        intent: RunIntent,
    },
    Session {
        record: SessionRecord,
    },
    RunSession {
        link: RunSession,
    },
    Workspace {
        record: WorkspaceRecord,
        expected_version: Option<u64>,
    },
    /// 같은 자원의 충돌하는 잠금이 있으면 실패합니다.
    AcquireLock {
        lock: ResourceLock,
    },
    /// 소유 Epoch가 맞을 때만 풉니다.
    ReleaseLock {
        lock_id: String,
        owner_epoch: u64,
    },
    ArtifactVersion {
        version: ArtifactVersion,
    },
    ArtifactReference {
        reference: ArtifactReference,
    },
    Validation {
        job: ValidationJob,
        expected_version: Option<u64>,
    },
    Approval {
        decision: ApprovalDecision,
        expected_version: Option<u64>,
    },
    /// 같은 범위·키가 있으면 Digest를 비교합니다(같으면 재생, 다르면 충돌).
    Idempotency {
        record: IdempotencyRecord,
    },
    Usage {
        record: UsageRecord,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct WriteBatch {
    pub writes: Vec<StateWrite>,
}

/// 적용된 배치가 만든 Journal 범위입니다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppliedBatch {
    pub first_sequence: u64,
    pub last_sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    /// 예상 버전·Epoch·단계가 맞지 않음. 다시 읽고 판단합니다.
    VersionConflict {
        entity: String,
    },
    LockConflict {
        resource: String,
    },
    IdempotencyConflict {
        scope: String,
        key: String,
    },
    /// Core보다 새 Schema (§14.1)
    SchemaTooNew {
        found: u32,
    },
    /// 기록 실패. 호출자는 후속 실행을 보류합니다 (§15.1).
    Io {
        message: String,
    },
    Corrupt {
        message: String,
    },
}

pub trait Store: Send + Sync {
    fn apply(&self, batch: WriteBatch) -> Result<AppliedBatch, StoreError>;

    fn plan(&self, plan_id: &str) -> Result<Option<Plan>, StoreError>;
    fn task(&self, task_id: &str) -> Result<Option<TaskRecord>, StoreError>;
    fn plan_tasks(&self, plan_id: &str) -> Result<Vec<TaskRecord>, StoreError>;
    fn task_runs(&self, task_id: &str) -> Result<Vec<RunRecord>, StoreError>;
    fn run(&self, run_id: &str) -> Result<Option<RunRecord>, StoreError>;
    fn session(&self, session_id: &str) -> Result<Option<SessionRecord>, StoreError>;
    fn workspace(&self, workspace_id: &str) -> Result<Option<WorkspaceRecord>, StoreError>;
    fn artifact_version(
        &self,
        target: &ArtifactVersionRef,
    ) -> Result<Option<ArtifactVersion>, StoreError>;
    fn artifact_references(
        &self,
        target: &ArtifactVersionRef,
    ) -> Result<Vec<ArtifactReference>, StoreError>;
    fn approval(&self, approval_id: &str) -> Result<Option<ApprovalDecision>, StoreError>;
    fn idempotency(&self, scope: &str, key: &str) -> Result<Option<IdempotencyRecord>, StoreError>;

    /// 재시작 대조용: `settled`·`abandoned`가 아닌 실행 의도와 현재 잠금 (§14.3)
    fn unfinished_intents(&self) -> Result<Vec<RunIntent>, StoreError>;
    fn locks(&self) -> Result<Vec<ResourceLock>, StoreError>;

    /// `after_sequence`보다 큰 항목을 최대 `limit`개. 보존 범위 밖이면 `Ok(None)` (`cursor_expired`).
    fn journal_after(
        &self,
        plan_id: Option<&str>,
        after_sequence: u64,
        limit: usize,
    ) -> Result<Option<Vec<JournalEntry>>, StoreError>;
    fn last_sequence(&self) -> Result<u64, StoreError>;
}

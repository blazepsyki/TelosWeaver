//! Task Scheduler (Spec §9, §14.3, PH1-SCHED). H0 공개 인터페이스이며 구현은 PH1-SCHED가 합니다.
//!
//! 프로젝트별 Scheduler 소유권은 하나입니다 (OPS-01). 상태 전이는 `TaskStatus::can_transition_to`·
//! `RunStatus::can_transition_to`만 따르고, 영속은 [`crate::persistence::Store`]의 조건부 쓰기로 합니다.
//! MASTER(결정 집행)·MCP(`delegate_task` 등)·API(`tasks.cancel` 등)는 이 Trait으로만 Task를 바꿉니다.

use async_trait::async_trait;
use telos_schema::api::AcceptedResult;
use telos_schema::{
    ApprovalAnswer, ContextSnapshot, DelegateTaskInput, ProcessEvidence, ProviderEvent, RunContext,
    RunStarted, TaskRecord, TaskSpec, ValidationVerdict,
};

/// 변경을 요청한 주체입니다. 권한·예산 판정과 감사 기록에 씁니다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Actor {
    User,
    Master {
        plan_id: String,
        master_run_id: String,
        controller_epoch: u64,
    },
    /// Task MCP를 호출한 Worker Run
    Worker {
        task_id: String,
        run_id: String,
    },
    /// Core 내부(재시작 대조·기한 만료 등)
    Core,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchedulerError {
    NotFound {
        entity: String,
    },
    /// 전이 규칙·조건부 갱신 실패
    InvalidTransition {
        task_id: String,
        reason: String,
    },
    Forbidden {
        reason: String,
    },
    LimitExceeded {
        reason: String,
    },
    IdempotencyConflict {
        key: String,
    },
    /// 지금 실행할 수 없음. 즉시 설명 가능한 거부이며 무한 대기하지 않습니다 (§9.3).
    Unavailable {
        reason: String,
    },
    Storage {
        message: String,
    },
}

/// 재시작 대조 결과입니다 (§14.2·§14.3).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RecoveryReport {
    /// 다시 전달한 `recorded` 의도
    pub redispatched: Vec<String>,
    /// 실제 상태를 확인해 이어서 추적하는 Run
    pub reattached: Vec<String>,
    /// 확정할 수 없어 NeedsReconciliation으로 둔 Task
    pub held: Vec<String>,
}

#[async_trait]
pub trait Scheduler: Send + Sync {
    /// 새 Task를 등록합니다. 권한·예산은 부모(또는 Plan)의 남은 범위에서 정합니다 (§9.3, §13).
    async fn create_task(
        &self,
        actor: Actor,
        parent_task_id: Option<&str>,
        input: DelegateTaskInput,
    ) -> Result<TaskRecord, SchedulerError>;
    /// Ready Task의 새 Run을 시작합니다(§9.1의 2~5단계).
    async fn request_run(
        &self,
        actor: Actor,
        task_id: &str,
        idempotency_key: &str,
    ) -> Result<String, SchedulerError>;
    /// 접수와 실제 정지는 다릅니다. 전파 정책은 Task의 `cancelPropagation`을 따릅니다.
    async fn cancel_task(
        &self,
        actor: Actor,
        task_id: &str,
        reason: &str,
        idempotency_key: &str,
    ) -> Result<AcceptedResult, SchedulerError>;
    /// 재작업: 이전 Run 결과를 확정한 뒤 Ready로 되돌립니다 (§9.4).
    async fn request_rework(
        &self,
        actor: Actor,
        task_id: &str,
        reason: &str,
    ) -> Result<(), SchedulerError>;
    /// 완료 판정 요청. 필수 검증·승인·완료 정책을 모두 확인합니다 (§5.3).
    async fn propose_completion(
        &self,
        actor: Actor,
        task_id: &str,
    ) -> Result<TaskRecord, SchedulerError>;

    /// HOST가 전달하는 정규화 이벤트입니다.
    async fn on_run_event(&self, event: ProviderEvent) -> Result<(), SchedulerError>;
    /// VALIDATION이 판정을 기록한 뒤 알립니다.
    async fn on_validation_finished(
        &self,
        task_id: &str,
        validation_id: &str,
        verdict: ValidationVerdict,
    ) -> Result<(), SchedulerError>;
    /// Provider 승인 요청에 답이 정해졌을 때(마스터·사용자) 실행 직전 재검사 뒤 Adapter로 보냅니다 (§4.3).
    async fn answer_provider_approval(
        &self,
        actor: Actor,
        request_id: &str,
        answer: ApprovalAnswer,
    ) -> Result<(), SchedulerError>;

    /// Core 시작 때 미완 의도·Run·Lock을 대조합니다.
    async fn recover(&self) -> Result<RecoveryReport, SchedulerError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutorError {
    Rejected {
        reason: String,
    },
    /// 시작 요청 뒤 결과를 알 수 없음. 의도를 `dispatched`로 두고 대조합니다.
    Unknown {
        message: String,
    },
    UnknownRun {
        run_id: String,
    },
}

/// Scheduler가 실행 감독(PH1-HOST `execution`)에 요구하는 Port입니다. HOST가 구현합니다.
#[async_trait]
pub trait RunExecutor: Send + Sync {
    async fn start(
        &self,
        task: &TaskSpec,
        context: &RunContext,
        snapshot: &ContextSnapshot,
    ) -> Result<(RunStarted, Option<ProcessEvidence>), ExecutorError>;
    /// 접수 여부입니다. 실제 종료는 이벤트로 옵니다.
    async fn cancel(&self, run_id: &str) -> Result<bool, ExecutorError>;
    async fn answer_approval(
        &self,
        run_id: &str,
        native_request_id: &str,
        answer: ApprovalAnswer,
    ) -> Result<(), ExecutorError>;
    /// 재시작 대조: 증거로 프로세스를 찾습니다. 살아 있으면 `Some(true)`, 확인 불가면 `None` (§14.3).
    async fn probe(&self, evidence: &ProcessEvidence) -> Option<bool>;
}

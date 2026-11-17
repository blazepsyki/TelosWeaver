//! Task Scheduler (Spec §9, §14.3, PH1-SCHED).
//!
//! 프로젝트별 Scheduler 소유권은 하나입니다 (OPS-01). 프로젝트 DB를 독점한 Core 프로세스 안에서
//! [`TaskScheduler`] 하나가 모든 Task·Run 상태 변경을 차례로 처리합니다. 상태 전이는
//! `TaskStatus::can_transition_to`·`RunStatus::can_transition_to`만 따르고, 영속은
//! [`crate::persistence::Store`]의 조건부 쓰기로 합니다. MASTER(결정 집행)·MCP(`delegate_task` 등)·
//! API(`tasks.cancel` 등)·HOST(Run 종료 보고)·VALIDATION(판정 통지)은 [`Scheduler`] Trait으로만 Task를 바꿉니다.
//!
//! Scheduler는 Task를 `Completed`로 만드는 유일한 경로([`Scheduler::complete_task`])를 갖고, Run 성공이나
//! 요청 접수만으로 완료 처리하지 않습니다 (§9.4, DELEG-09).

mod service;
mod time;

pub use service::{SchedulerDeps, TaskScheduler};

use async_trait::async_trait;
use telos_schema::api::AcceptedResult;
use telos_schema::{
    BudgetProfile, ContextSnapshot, DelegateTaskInput, FailureKind, ProcessEvidence, RunContext,
    RunStarted, RunStatus, SessionRecord, SessionRequest, TaskRecord, TaskSpec,
};

/// 변경을 요청한 주체입니다. 권한 판정과 감사 기록에 씁니다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Actor {
    User,
    /// 마스터 결정의 집행. Plan이 `Active`이고 제어 Epoch가 같을 때만 받아들입니다 (§4.1, §14.3).
    Master {
        plan_id: String,
        master_run_id: String,
        controller_epoch: u64,
    },
    /// Task MCP를 호출한 Worker Run. 그 Run이 살아 있고 그 Task의 Run일 때만 받아들입니다 (SAFE-08).
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
    /// 지금 상태에서 할 수 없는 요청
    InvalidTransition {
        task_id: String,
        reason: String,
    },
    Forbidden {
        reason: String,
    },
    /// 위임 깊이·전체 Task·재시도·재작업 상한
    LimitExceeded {
        reason: String,
    },
    DependencyCycle {
        task_ids: Vec<String>,
    },
    /// 같은 멱등 키에 다른 요청 내용
    IdempotencyConflict {
        key: String,
    },
    /// 지금 실행할 수 없음(동시성·잠금·막는 입력 문제). 즉시 설명 가능한 거부이며 무한 대기하지 않습니다 (§9.3).
    Unavailable {
        reason: String,
    },
    /// Run을 만들었지만 시작하지 못했습니다. `confirmed`가 거짓이면 시작 여부를 알 수 없어
    /// Task를 `NeedsReconciliation`으로 두었습니다 (§14.3).
    StartFailed {
        run_id: String,
        reason: String,
        confirmed: bool,
    },
    /// 다른 쓰기와 겹쳤습니다. 다시 읽고 요청합니다.
    Conflict {
        entity: String,
    },
    /// 기록 실패. 후속 실행을 보류합니다 (§15.1).
    Storage {
        message: String,
    },
}

impl std::fmt::Display for SchedulerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound { entity } => write!(f, "not found: {entity}"),
            Self::InvalidTransition { task_id, reason } => write!(f, "task {task_id}: {reason}"),
            Self::Forbidden { reason } => write!(f, "forbidden: {reason}"),
            Self::LimitExceeded { reason } => write!(f, "limit exceeded: {reason}"),
            Self::DependencyCycle { task_ids } => {
                write!(f, "dependency cycle: {}", task_ids.join(" -> "))
            }
            Self::IdempotencyConflict { key } => {
                write!(f, "idempotency key reused with another request: {key}")
            }
            Self::Unavailable { reason } => write!(f, "unavailable: {reason}"),
            Self::StartFailed {
                run_id,
                reason,
                confirmed,
            } => write!(
                f,
                "run {run_id} did not start ({}): {reason}",
                if *confirmed {
                    "confirmed"
                } else {
                    "unconfirmed"
                }
            ),
            Self::Conflict { entity } => write!(f, "concurrent change: {entity}"),
            Self::Storage { message } => write!(f, "storage failure: {message}"),
        }
    }
}

impl std::error::Error for SchedulerError {}

/// Run 요청의 결과입니다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunAdmission {
    Started {
        run_id: String,
    },
    /// Provider·Host 동시성 상한에 걸려 대기열에 넣었습니다. 자리가 나면 요청 순서대로 시작합니다.
    /// 대기열은 메모리에만 있어 Core 재시작 뒤에는 Task가 `Ready`로 남고 다시 요청해야 합니다.
    Queued {
        position: usize,
    },
}

/// 실행 감독(HOST)이 확정한 Run의 종료·중단 보고입니다. Provider 이벤트 해석은 HOST가 합니다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunReport {
    pub run_id: String,
    /// `Succeeded`·`Failed`·`Cancelled`·`Unknown`(결과 불명)·`Interrupted`(실행 주체 유실) 중 하나
    pub status: RunStatus,
    pub failure: Option<FailureKind>,
    pub message: Option<String>,
    pub native_session_id: Option<String>,
    pub native_turn_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitKind {
    Master,
    User,
}

/// `NeedsReconciliation`·`Interrupted` Task에 대해 사용자나 마스터가 실제 상태를 확인한 뒤 내리는 판단입니다 (§14.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reconciliation {
    /// 실행이 끝났고 다시 실행해도 됨: `Ready`
    Retry,
    /// 실행 결과가 남아 있어 채택함: `Executed` (`NeedsReconciliation`에서만)
    AcceptExecuted,
    Fail,
    Cancel,
}

/// 재시작 대조 결과입니다 (§14.2·§14.3). 값은 Task ID입니다.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RecoveryReport {
    /// Adapter에 전달하기 전이라 외부 실행이 없었음: 의도를 버리고 `Ready`로 되돌림
    pub returned_to_ready: Vec<String>,
    /// 프로세스가 살아 있지만 Core가 추적하지 못함: `Interrupted`
    pub interrupted: Vec<String>,
    /// 결과를 확정할 수 없음: `NeedsReconciliation`. 다시 실행하지 않습니다.
    pub needs_reconciliation: Vec<String>,
    /// 주인 Run이 없거나 확정 종료된 잠금을 풀었음 (잠금 ID)
    pub released_locks: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DeadlineReport {
    /// 실행 시간 상한을 넘겨 취소를 요청한 Run
    pub timed_out_runs: Vec<String>,
    /// 대기 기한이 지난 Task
    pub expired_waits: Vec<String>,
}

#[async_trait]
pub trait Scheduler: Send + Sync {
    /// 마스터(또는 사용자)가 정한 Task들을 등록합니다. 의존성·순환·부모·깊이·Plan 전체 Task 상한을 검사하고,
    /// 의존성이 모두 `Completed`면 `Ready`, 아니면 `Blocked`로 만듭니다 (§9.1 1번, TASK-05, DELEG-04).
    async fn register_tasks(
        &self,
        actor: Actor,
        specs: Vec<TaskSpec>,
        idempotency_key: &str,
    ) -> Result<Vec<TaskRecord>, SchedulerError>;

    /// Worker의 `delegate_task`입니다. 권한·예산 Profile은 정책이 부모 범위 안에서 정하고, 자식을 등록한 뒤 바로 실행을 시도합니다.
    /// 지금 실행할 수 없으면 자식을 취소하고 `Unavailable`로 답합니다. `waitForResult`면 부모를 `WaitingChild`로 둡니다 (§9.2, §9.3).
    async fn delegate_task(
        &self,
        actor: Actor,
        input: DelegateTaskInput,
    ) -> Result<TaskRecord, SchedulerError>;

    /// `Ready` Task의 새 Run을 시작합니다(§9.1 2~5번). 요청 없이 Scheduler가 스스로 다시 실행하지 않습니다.
    async fn request_run(
        &self,
        actor: Actor,
        task_id: &str,
        session: SessionRequest,
        idempotency_key: &str,
    ) -> Result<RunAdmission, SchedulerError>;

    /// 취소를 접수합니다. 실행이 없으면 바로 `Cancelled`, 실행·검증 중이면 `Cancelling`으로 두고 실제 정지를 기다립니다.
    /// `cancelPropagation`이 `descendants`인 자손에 전파합니다 (§9.3, §9.4, DELEG-10).
    async fn cancel_task(
        &self,
        actor: Actor,
        task_id: &str,
        reason: &str,
        idempotency_key: &str,
    ) -> Result<AcceptedResult, SchedulerError>;

    /// 재작업: 실행이 끝난 Task를 `Ready`로 되돌립니다. 재작업 상한을 넘으면 거부합니다 (§9.4).
    async fn request_rework(
        &self,
        actor: Actor,
        task_id: &str,
        reason: &str,
    ) -> Result<TaskRecord, SchedulerError>;

    /// `Executed → Validating`. 검증 Job을 만들기 전에 VALIDATION이 부릅니다.
    async fn begin_validation(
        &self,
        actor: Actor,
        task_id: &str,
    ) -> Result<TaskRecord, SchedulerError>;

    /// VALIDATION이 판정을 저장한 뒤 알립니다. 필수 검증이 모두 끝났으면 통과는 `Verified`,
    /// 실패는 재작업(`Ready`, 상한을 넘으면 `Failed`), 오류·판정 불가는 `WaitingMaster`로 둡니다 (§11.1).
    async fn on_validation_finished(
        &self,
        task_id: &str,
        validation_id: &str,
    ) -> Result<TaskRecord, SchedulerError>;

    /// 마스터·사용자 응답 대기에 들어갑니다. 복귀 상태는 지금 상태입니다 (§9.4).
    async fn begin_wait(
        &self,
        actor: Actor,
        task_id: &str,
        kind: WaitKind,
        waiting_for: Vec<String>,
        deadline: Option<String>,
    ) -> Result<TaskRecord, SchedulerError>;

    /// 대기를 끝내고 복귀 상태로 돌아갑니다.
    async fn end_wait(&self, actor: Actor, task_id: &str) -> Result<TaskRecord, SchedulerError>;

    /// 완료 판정입니다. `Executed`(필수 검증이 없을 때) 또는 `Verified`이고 완료 정책의 승인 조건을 채웠을 때만
    /// `Completed`로 바꾸고, 후속 Task를 풀고 기다리는 부모를 깨웁니다 (§9.1 6번, §9.4).
    async fn complete_task(
        &self,
        actor: Actor,
        task_id: &str,
    ) -> Result<TaskRecord, SchedulerError>;

    /// 결과 불명·중단 Task의 대조 판단을 적용하고 남은 실행 의도와 잠금을 정리합니다 (§14.3).
    async fn reconcile_task(
        &self,
        actor: Actor,
        task_id: &str,
        resolution: Reconciliation,
    ) -> Result<TaskRecord, SchedulerError>;

    /// HOST의 Run 종료·중단 보고입니다. 이미 끝난 Run의 보고와 이전 Run의 늦은 보고는 Task를 바꾸지 않습니다 (§9.5).
    async fn on_run_finished(&self, report: RunReport) -> Result<(), SchedulerError>;

    /// 실행 시간·대기 기한을 넘긴 항목을 처리합니다. Core가 주기적으로 부릅니다 (§13.2, TASK-06).
    async fn enforce_deadlines(&self) -> Result<DeadlineReport, SchedulerError>;

    /// Core 시작 때 미완 의도·Run·Lock을 대조합니다 (§14.2, OPS-03).
    async fn recover(&self) -> Result<RecoveryReport, SchedulerError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutorError {
    /// 시작하지 않았음이 확실합니다.
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

/// Scheduler가 실행 감독(PH1-HOST `execution`)에 요구하는 Port입니다. HOST가 구현하고,
/// Run이 끝나면 [`Scheduler::on_run_finished`]로 알립니다.
#[async_trait]
pub trait RunExecutor: Send + Sync {
    async fn start(
        &self,
        task: &TaskSpec,
        context: &RunContext,
        snapshot: &ContextSnapshot,
    ) -> Result<(RunStarted, Option<ProcessEvidence>), ExecutorError>;
    /// 접수 여부입니다. 실제 종료는 종료 보고로 옵니다.
    async fn cancel(&self, run_id: &str) -> Result<bool, ExecutorError>;
    /// 재시작 대조: 증거로 프로세스를 찾습니다. 살아 있으면 `Some(true)`, 없으면 `Some(false)`, 확인 불가면 `None` (§14.3).
    async fn probe(&self, evidence: &ProcessEvidence) -> Option<bool>;
}

/// 자식 Task에 줄 Profile입니다. 부모의 남은 범위를 넘지 않습니다 (§9.3, §13.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChildProfiles {
    pub permission_profile_id: String,
    pub budget_profile_id: String,
    pub completion_policy_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunAuthorization {
    pub policy_revision: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CompletionRule {
    /// 참이면 이 Task Revision에 대한 허용된 결과 검토 승인이 있어야 완료합니다.
    pub require_result_approval: bool,
}

/// Scheduler가 정책(PH1-MASTER `policy`)에 요구하는 Port입니다. 권한·예산·완료 정책의 판단은 정책이 하고
/// Scheduler는 그 결과와 상한을 집행합니다. 거부 사유는 문자열로 돌려줍니다.
pub trait SchedulerPolicy: Send + Sync {
    fn budget_profile(&self, profile_id: &str) -> Option<BudgetProfile>;
    fn child_profiles(
        &self,
        parent: &TaskSpec,
        input: &DelegateTaskInput,
    ) -> Result<ChildProfiles, String>;
    /// Run 시작 직전의 Host·Provider·권한·Session 소유권 확인입니다 (§9.1 2번, §8.1).
    /// `source`는 `resume`·`native-fork`가 가리키는 Session 기록입니다.
    fn authorize_run(
        &self,
        task: &TaskSpec,
        session: &SessionRequest,
        source: Option<&SessionRecord>,
    ) -> Result<RunAuthorization, String>;
    fn completion_rule(&self, task: &TaskSpec) -> CompletionRule;
}

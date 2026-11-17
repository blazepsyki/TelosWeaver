//! [`Scheduler`]의 구현입니다 (Spec §9, §14.3).
//!
//! 모든 변경은 하나의 문(`gate`) 안에서 차례로 처리하고, 한 요청의 상태 변경은 저장소의 한 배치로 기록합니다.
//! 외부 호출(Workspace 준비·문맥 구성·Adapter 시작)은 배치 사이에 두며, 그 앞뒤 상태를 실행 의도로 남겨
//! 중간에 Core가 죽어도 [`Scheduler::recover`]가 구분할 수 있게 합니다.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
};

use async_trait::async_trait;
use serde::Serialize;
use sha2::{Digest, Sha256};
use telos_schema::api::AcceptedResult;
use telos_schema::{
    ApprovalAnswer, ApprovalKind, ApprovalState, BudgetLedger, BudgetProfile, CancelPropagation,
    DelegateTaskInput, ExecutionOutcome, FailureKind, IdempotencyRecord, IntentPhase, JsonObject,
    LockMode, LockResource, PlanStatus, ResourceLock, RunContext, RunIntent, RunRecord, RunSession,
    RunStatus, SessionMode, SessionRecord, SessionRequest, SessionUse, TaskMcpScope, TaskRecord,
    TaskSpec, TaskStatus, TaskTool, ValidationJobStatus, ValidationVerdict, WaitState,
};

use super::{
    Actor, DeadlineReport, ExecutorError, Reconciliation, RecoveryReport, RunAdmission,
    RunExecutor, RunReport, Scheduler, SchedulerError, SchedulerPolicy, WaitKind, time,
};
use crate::{
    context::ContextBuilder,
    mcp::TaskMcpService,
    persistence::{StateWrite, Store, StoreError, WriteBatch},
    workspace::{WorkspaceError, WorkspaceManager},
};

type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;

pub struct SchedulerDeps {
    pub store: Arc<dyn Store>,
    pub executor: Arc<dyn RunExecutor>,
    pub context: Arc<dyn ContextBuilder>,
    pub workspace: Arc<dyn WorkspaceManager>,
    pub policy: Arc<dyn SchedulerPolicy>,
    /// 있으면 Run마다 Task MCP 연결 정보를 발급해 `RunContext.taskMcp`로 넘기고 Run이 끝나면 폐기합니다.
    pub task_mcp: Option<Arc<dyn TaskMcpService>>,
    /// 현재 시각(Unix 초). 없으면 시스템 시각입니다.
    pub clock: Option<Clock>,
}

#[derive(Debug, Clone)]
struct QueuedRun {
    actor: Actor,
    task_id: String,
    session: SessionRequest,
    idempotency_key: String,
}

pub struct TaskScheduler {
    store: Arc<dyn Store>,
    executor: Arc<dyn RunExecutor>,
    context: Arc<dyn ContextBuilder>,
    workspace: Arc<dyn WorkspaceManager>,
    policy: Arc<dyn SchedulerPolicy>,
    task_mcp: Option<Arc<dyn TaskMcpService>>,
    clock: Clock,
    /// 단일 Scheduler 소유권: 모든 변경이 이 문을 차례로 지납니다. 값은 실행 대기열입니다.
    gate: tokio::sync::Mutex<VecDeque<QueuedRun>>,
}

type Result<T> = std::result::Result<T, SchedulerError>;

fn store_error(error: StoreError) -> SchedulerError {
    match error {
        StoreError::VersionConflict { entity } => SchedulerError::Conflict { entity },
        StoreError::LockConflict { resource } => SchedulerError::Unavailable {
            reason: format!("resource is locked: {resource}"),
        },
        StoreError::IdempotencyReplay { key, .. } | StoreError::IdempotencyConflict { key, .. } => {
            SchedulerError::IdempotencyConflict { key }
        }
        other => SchedulerError::Storage {
            message: other.to_string(),
        },
    }
}

fn not_found(kind: &str, id: &str) -> SchedulerError {
    SchedulerError::NotFound {
        entity: format!("{kind}:{id}"),
    }
}

fn forbidden(reason: impl Into<String>) -> SchedulerError {
    SchedulerError::Forbidden {
        reason: reason.into(),
    }
}

fn invalid(task_id: &str, reason: impl Into<String>) -> SchedulerError {
    SchedulerError::InvalidTransition {
        task_id: task_id.to_string(),
        reason: reason.into(),
    }
}

fn digest<T: Serialize>(value: &T) -> String {
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    let hash = Sha256::digest(&bytes);
    format!(
        "sha256:{}",
        hash.iter().map(|b| format!("{b:02x}")).collect::<String>()
    )
}

fn new_id(prefix: &str) -> Result<String> {
    crate::execution::new_id(prefix).map_err(|e| SchedulerError::Storage { message: e.message })
}

fn outcome_of(status: RunStatus) -> Option<ExecutionOutcome> {
    match status {
        RunStatus::Succeeded => Some(ExecutionOutcome::Succeeded),
        RunStatus::Failed => Some(ExecutionOutcome::Failed),
        RunStatus::Cancelled => Some(ExecutionOutcome::Cancelled),
        RunStatus::Unknown => Some(ExecutionOutcome::Unknown),
        _ => None,
    }
}

/// 같은 입력으로 다시 실행해도 결과가 같을 실패는 재시도하지 않습니다 (§9.5).
fn retriable(kind: Option<FailureKind>) -> bool {
    !matches!(
        kind,
        Some(
            FailureKind::Authentication
                | FailureKind::Configuration
                | FailureKind::UnsupportedCapability
                | FailureKind::ToolDenied
        )
    )
}

struct LedgerState {
    ledger: BudgetLedger,
    expected: Option<u64>,
    dirty: bool,
}

/// 한 요청이 만드는 변경 묶음입니다. 변경 중인 Task의 최신 모습을 들고 있다가 한 배치로 기록합니다.
struct Work {
    now: String,
    tasks: HashMap<String, TaskRecord>,
    loaded_plans: HashSet<String>,
    ledgers: HashMap<String, LedgerState>,
    writes: Vec<StateWrite>,
}

impl Work {
    fn new(now: String) -> Self {
        Self {
            now,
            tasks: HashMap::new(),
            loaded_plans: HashSet::new(),
            ledgers: HashMap::new(),
            writes: Vec::new(),
        }
    }

    fn load_plan(&mut self, store: &dyn Store, plan_id: &str) -> Result<()> {
        if self.loaded_plans.insert(plan_id.to_string()) {
            for task in store.plan_tasks(plan_id).map_err(store_error)? {
                self.tasks.entry(task.spec.id.clone()).or_insert(task);
            }
        }
        Ok(())
    }

    /// Task와 그 Plan의 모든 Task를 읽어 둡니다.
    fn load_task(&mut self, store: &dyn Store, task_id: &str) -> Result<TaskRecord> {
        if let Some(task) = self.tasks.get(task_id) {
            return Ok(task.clone());
        }
        let task = store
            .task(task_id)
            .map_err(store_error)?
            .ok_or_else(|| not_found("task", task_id))?;
        self.load_plan(store, &task.spec.plan_id)?;
        Ok(self.tasks.get(task_id).cloned().unwrap_or(task))
    }

    fn task(&self, task_id: &str) -> Result<&TaskRecord> {
        self.tasks
            .get(task_id)
            .ok_or_else(|| not_found("task", task_id))
    }

    fn insert(&mut self, mut record: TaskRecord) {
        self.writes.push(StateWrite::Task {
            record: record.clone(),
            expected_version: None,
        });
        record.state_version = 1;
        self.tasks.insert(record.spec.id.clone(), record);
    }

    /// 상태를 바꾸지 않는 갱신(대기 대상 변경 등)과 전이 모두 이 함수로 기록합니다.
    fn update(&mut self, task_id: &str, change: impl FnOnce(&mut TaskRecord)) -> Result<()> {
        let record = self
            .tasks
            .get_mut(task_id)
            .ok_or_else(|| not_found("task", task_id))?;
        let expected = record.state_version;
        change(record);
        record.updated_at = self.now.clone();
        self.writes.push(StateWrite::Task {
            record: record.clone(),
            expected_version: Some(expected),
        });
        record.state_version = expected + 1;
        Ok(())
    }

    /// §9.4 전이 표를 지키는 상태 변경입니다. Wait* 밖으로 나가면 대기 기록을 지웁니다.
    fn transition(&mut self, task_id: &str, next: TaskStatus) -> Result<()> {
        let current = self.task(task_id)?.status;
        if !current.can_transition_to(next) {
            return Err(invalid(
                task_id,
                format!("{current:?} cannot become {next:?}"),
            ));
        }
        self.update(task_id, |record| {
            record.status = next;
            if !next.is_waiting() {
                record.wait = None;
            }
        })
    }

    fn wait(&mut self, task_id: &str, next: TaskStatus, wait: WaitState) -> Result<()> {
        let current = self.task(task_id)?.status;
        if !current.can_transition_to(next) {
            return Err(invalid(
                task_id,
                format!("{current:?} cannot become {next:?}"),
            ));
        }
        self.update(task_id, |record| {
            record.status = next;
            record.wait = Some(wait);
        })
    }

    fn ledger(&mut self, store: &dyn Store, plan_id: &str) -> Result<&mut LedgerState> {
        if !self.ledgers.contains_key(plan_id) {
            let state = match store.budget(plan_id).map_err(store_error)? {
                Some(stored) => LedgerState {
                    ledger: stored.value,
                    expected: Some(stored.version),
                    dirty: false,
                },
                None => LedgerState {
                    ledger: BudgetLedger {
                        plan_id: plan_id.to_string(),
                        tokens_measurable: true,
                        ..BudgetLedger::default()
                    },
                    expected: None,
                    dirty: false,
                },
            };
            self.ledgers.insert(plan_id.to_string(), state);
        }
        Ok(self.ledgers.get_mut(plan_id).expect("inserted above"))
    }

    /// Task가 종료 상태가 된 뒤의 처리: 예산 장부, 후속 Task 해제, 기다리는 부모 깨우기 (§9.1 6번, §9.3).
    fn after_terminal(&mut self, store: &dyn Store, task_id: &str) -> Result<()> {
        let task = self.task(task_id)?.clone();
        debug_assert!(task.status.is_terminal());

        let state = self.ledger(store, &task.spec.plan_id)?;
        state.ledger.tasks_reserved = state.ledger.tasks_reserved.saturating_sub(1);
        state.ledger.tasks_consumed = state.ledger.tasks_consumed.saturating_add(1);
        state.dirty = true;

        if task.status == TaskStatus::Completed {
            let blocked: Vec<String> = self
                .tasks
                .values()
                .filter(|other| {
                    other.status == TaskStatus::Blocked
                        && other.spec.plan_id == task.spec.plan_id
                        && other.spec.dependencies.contains(&task.spec.id)
                })
                .map(|other| other.spec.id.clone())
                .collect();
            for id in blocked {
                let ready = self.task(&id)?.spec.dependencies.iter().all(|dep| {
                    self.tasks
                        .get(dep)
                        .is_some_and(|d| d.status == TaskStatus::Completed)
                });
                if ready {
                    self.transition(&id, TaskStatus::Ready)?;
                }
            }
        }

        if let Some(parent_id) = task.spec.parent_task_id.clone() {
            let waiting = self.tasks.get(&parent_id).and_then(|parent| {
                (parent.status == TaskStatus::WaitingChild)
                    .then(|| parent.wait.clone())
                    .flatten()
            });
            if let Some(mut wait) = waiting
                && wait.waiting_for.contains(&task.spec.id)
            {
                wait.waiting_for.retain(|id| id != &task.spec.id);
                if wait.waiting_for.is_empty() {
                    self.transition(&parent_id, wait.resume_status)?;
                } else {
                    self.update(&parent_id, |record| record.wait = Some(wait))?;
                }
            }
        }
        Ok(())
    }

    /// 전이하고, 종료 상태면 뒤처리까지 합니다.
    fn finish(&mut self, store: &dyn Store, task_id: &str, next: TaskStatus) -> Result<()> {
        self.transition(task_id, next)?;
        if next.is_terminal() {
            self.after_terminal(store, task_id)?;
        }
        Ok(())
    }

    fn into_writes(mut self) -> Vec<StateWrite> {
        for (_, state) in self.ledgers {
            if state.dirty {
                self.writes.push(StateWrite::Budget {
                    ledger: state.ledger,
                    expected_version: state.expected,
                });
            }
        }
        self.writes
    }
}

fn run_transition(
    run: &mut RunRecord,
    next: RunStatus,
    now: &str,
    writes: &mut Vec<StateWrite>,
) -> Result<()> {
    if !run.status.can_transition_to(next) {
        return Err(SchedulerError::Conflict {
            entity: format!("run:{} {:?} cannot become {next:?}", run.run_id, run.status),
        });
    }
    let expected = run.state_version;
    run.status = next;
    if next.is_terminal() {
        run.outcome = outcome_of(next);
        run.finished_at = Some(now.to_string());
    }
    writes.push(StateWrite::Run {
        record: run.clone(),
        expected_version: Some(expected),
    });
    run.state_version = expected + 1;
    Ok(())
}

/// 새로 등록하는 Task들 사이의 의존성 순환을 찾습니다. 기존 Task는 새 Task에 기댈 수 없으므로 새 것만 봅니다.
fn find_cycle(specs: &[TaskSpec]) -> Option<Vec<String>> {
    let deps: HashMap<&str, Vec<&str>> = specs
        .iter()
        .map(|spec| {
            (
                spec.id.as_str(),
                spec.dependencies.iter().map(String::as_str).collect(),
            )
        })
        .collect();
    // 0: 안 봄, 1: 보는 중, 2: 끝
    let mut color: HashMap<&str, u8> = HashMap::new();
    fn visit<'a>(
        node: &'a str,
        deps: &HashMap<&'a str, Vec<&'a str>>,
        color: &mut HashMap<&'a str, u8>,
        path: &mut Vec<&'a str>,
    ) -> Option<Vec<String>> {
        match color.get(node) {
            Some(2) => return None,
            Some(1) => {
                let start = path.iter().position(|n| *n == node).unwrap_or(0);
                let mut cycle: Vec<String> = path[start..].iter().map(|n| n.to_string()).collect();
                cycle.push(node.to_string());
                return Some(cycle);
            }
            _ => {}
        }
        color.insert(node, 1);
        path.push(node);
        for next in deps.get(node).into_iter().flatten() {
            if deps.contains_key(next)
                && let Some(cycle) = visit(next, deps, color, path)
            {
                return Some(cycle);
            }
        }
        path.pop();
        color.insert(node, 2);
        None
    }
    for spec in specs {
        if let Some(cycle) = visit(spec.id.as_str(), &deps, &mut color, &mut Vec::new()) {
            return Some(cycle);
        }
    }
    None
}

impl TaskScheduler {
    pub fn new(deps: SchedulerDeps) -> Self {
        Self {
            store: deps.store,
            executor: deps.executor,
            context: deps.context,
            workspace: deps.workspace,
            policy: deps.policy,
            task_mcp: deps.task_mcp,
            clock: deps.clock.unwrap_or_else(|| Arc::new(time::now_secs)),
            gate: tokio::sync::Mutex::new(VecDeque::new()),
        }
    }

    fn now(&self) -> u64 {
        (self.clock)()
    }

    fn now_text(&self) -> String {
        time::format(self.now())
    }

    fn work(&self) -> Work {
        Work::new(self.now_text())
    }

    fn commit(&self, writes: Vec<StateWrite>) -> Result<()> {
        if writes.is_empty() {
            return Ok(());
        }
        self.store
            .apply(WriteBatch::new(writes))
            .map(|_| ())
            .map_err(store_error)
    }

    fn profile(&self, spec: &TaskSpec) -> Result<BudgetProfile> {
        self.policy
            .budget_profile(&spec.budget_profile_id)
            .ok_or_else(|| forbidden(format!("unknown budget profile {}", spec.budget_profile_id)))
    }

    /// 마스터 결정은 Plan이 `Active`이고 제어 Epoch가 같을 때만 집행합니다 (§4.1, MASTER-04).
    fn check_plan_actor(&self, actor: &Actor, plan_id: &str) -> Result<()> {
        if let Actor::Master {
            plan_id: own,
            controller_epoch,
            ..
        } = actor
        {
            if own != plan_id {
                return Err(forbidden("master of another plan"));
            }
            let plan = self
                .store
                .plan(plan_id)
                .map_err(store_error)?
                .ok_or_else(|| not_found("plan", plan_id))?
                .value;
            if plan.controller_epoch != *controller_epoch {
                return Err(forbidden("stale controller epoch"));
            }
            if plan.status != PlanStatus::Active {
                return Err(forbidden("plan is not active"));
            }
        }
        Ok(())
    }

    fn deny_worker(actor: &Actor) -> Result<()> {
        if matches!(actor, Actor::Worker { .. }) {
            return Err(forbidden("workers change tasks only through delegation"));
        }
        Ok(())
    }

    /// 저장된 멱등 결과입니다. 같은 키·같은 내용이면 `Some`, 다른 내용이면 충돌입니다 (SAFE-13).
    fn replayed(&self, scope: &str, key: &str, payload_digest: &str) -> Result<Option<JsonObject>> {
        match self.store.idempotency(scope, key).map_err(store_error)? {
            None => Ok(None),
            Some(record) if record.payload_digest == payload_digest => Ok(Some(record.result)),
            Some(_) => Err(SchedulerError::IdempotencyConflict {
                key: key.to_string(),
            }),
        }
    }

    fn idempotency_write(
        &self,
        scope: &str,
        key: &str,
        payload_digest: String,
        result: serde_json::Value,
    ) -> StateWrite {
        StateWrite::Idempotency {
            record: IdempotencyRecord {
                scope: scope.to_string(),
                key: key.to_string(),
                payload_digest,
                result: result.as_object().cloned().unwrap_or_default(),
                created_at: self.now_text(),
            },
        }
    }

    fn revision_runs(&self, task: &TaskRecord) -> Result<Vec<RunRecord>> {
        Ok(self
            .store
            .task_runs(&task.spec.id)
            .map_err(store_error)?
            .into_iter()
            .filter(|run| run.task_revision == task.spec.revision)
            .collect())
    }

    /// 확정 종료하지 않은 Run이 있는지입니다. 살아 있을 수 있는 프로세스로 봅니다.
    fn open_runs(&self, task_id: &str) -> Result<Vec<RunRecord>> {
        Ok(self
            .store
            .task_runs(task_id)
            .map_err(store_error)?
            .into_iter()
            .filter(|run| !run.status.is_terminal())
            .collect())
    }

    /// Provider·Host의 실행 자리가 찼는지입니다. 끝이 확정되지 않은 실행 의도를 모두 셉니다.
    /// 자식을 기다리는 부모의 프로세스도 살아 있으므로 수에서 빼지 않습니다 (§9.3, TASK-03).
    fn at_capacity(&self, provider: &str, host_id: &str, limit: u32) -> Result<bool> {
        let active = self
            .store
            .unfinished_intents()
            .map_err(store_error)?
            .into_iter()
            .filter(|intent| intent.provider == provider && intent.host_id == host_id)
            .count();
        Ok(active >= limit.max(1) as usize)
    }

    fn release_run_locks(&self, run_id: &str, writes: &mut Vec<StateWrite>) -> Result<()> {
        for lock in self.store.locks().map_err(store_error)? {
            if lock.holder_run_id == run_id {
                writes.push(StateWrite::ReleaseLock {
                    lock_id: lock.lock_id,
                    owner_epoch: lock.owner_epoch,
                });
            }
        }
        Ok(())
    }

    fn settle_intent(&self, run_id: &str, writes: &mut Vec<StateWrite>) -> Result<()> {
        if let Some(mut intent) = self.store.run_intent(run_id).map_err(store_error)? {
            let next = match intent.phase {
                IntentPhase::Recorded => IntentPhase::Abandoned,
                IntentPhase::Dispatched | IntentPhase::Started => IntentPhase::Settled,
                IntentPhase::Settled | IntentPhase::Abandoned => return Ok(()),
            };
            intent.phase = next;
            intent.updated_at = self.now_text();
            writes.push(StateWrite::Intent { intent });
        }
        Ok(())
    }

    /// Run과 Native Session의 관계를 기록합니다 (SESS-01).
    fn record_session(
        &self,
        run: &RunRecord,
        native_session_id: &str,
        native_turn_ids: &[String],
        source: Option<&SessionRecord>,
        writes: &mut Vec<StateWrite>,
    ) -> Result<()> {
        let links = self.store.run_sessions(&run.run_id).map_err(store_error)?;
        if let Some(mut link) = links.into_iter().next() {
            let before = link.native_turn_ids.len();
            for turn in native_turn_ids {
                if !link.native_turn_ids.contains(turn) {
                    link.native_turn_ids.push(turn.clone());
                }
            }
            if link.native_turn_ids.len() != before {
                writes.push(StateWrite::RunSession { link });
            }
            return Ok(());
        }
        let known = match source {
            Some(session) if session.native_session_id == native_session_id => {
                Some(session.clone())
            }
            _ => self
                .store
                .task_sessions(&run.task_id)
                .map_err(store_error)?
                .into_iter()
                .find(|s| s.native_session_id == native_session_id && s.provider == run.provider),
        };
        let (session_id, usage) = match known {
            Some(session) => (session.session_id, SessionUse::Resumed),
            None => {
                let session_id = new_id("session")?;
                let forked = run.session_mode == SessionMode::NativeFork;
                writes.push(StateWrite::Session {
                    record: SessionRecord {
                        session_id: session_id.clone(),
                        provider: run.provider.clone(),
                        host_id: run.host_id.clone(),
                        native_session_id: native_session_id.to_string(),
                        owner_task_id: run.task_id.clone(),
                        parent_session_id: source.filter(|_| forked).map(|s| s.session_id.clone()),
                        fork_basis: None,
                        created_by_run_id: run.run_id.clone(),
                        created_at: self.now_text(),
                    },
                });
                (
                    session_id,
                    if forked {
                        SessionUse::Forked
                    } else {
                        SessionUse::Created
                    },
                )
            }
        };
        writes.push(StateWrite::RunSession {
            link: RunSession {
                run_id: run.run_id.clone(),
                session_id,
                usage,
                native_turn_ids: native_turn_ids.to_vec(),
            },
        });
        Ok(())
    }

    /// 실행 실패 뒤 Task의 다음 상태입니다. 재시도 상한 안이고 다시 해 볼 만한 실패면 `Ready`, 아니면 `Failed` (§9.4, §9.5).
    fn after_failure(&self, task: &TaskRecord, kind: Option<FailureKind>) -> Result<TaskStatus> {
        let limit = self.profile(&task.spec)?.limits.execution_retry_limit;
        // 지금 실패한 Run은 아직 저장 전이라 세지 않습니다.
        let failed_before = self
            .revision_runs(task)?
            .iter()
            .filter(|run| run.status == RunStatus::Failed)
            .count() as u32;
        Ok(if retriable(kind) && failed_before < limit {
            TaskStatus::Ready
        } else {
            TaskStatus::Failed
        })
    }

    /// 실행이 끝난 뒤 반려된 Task의 다음 상태입니다. 재작업 상한 안이면 `Ready`.
    fn rework_allowed(&self, task: &TaskRecord) -> Result<bool> {
        let limit = self.profile(&task.spec)?.limits.rework_limit;
        let succeeded = self
            .revision_runs(task)?
            .iter()
            .filter(|run| run.status == RunStatus::Succeeded)
            .count() as u32;
        // 처음 성공한 Run 하나는 재작업이 아닙니다.
        Ok(succeeded <= limit)
    }

    /// Run 종료·중단 보고를 Run·실행 의도·잠금·Session·Task에 반영하는 변경을 만듭니다.
    /// 돌려주는 값은 실행 자리가 비었는지(확정 종료)입니다.
    fn apply_report(&self, work: &mut Work, report: &RunReport) -> Result<bool> {
        let mut run = self
            .store
            .run(&report.run_id)
            .map_err(store_error)?
            .ok_or_else(|| not_found("run", &report.run_id))?;
        // 이미 끝난 Run의 중복·지연 보고는 이력만 남아 있으면 됩니다 (§9.5).
        if run.status.is_terminal() {
            return Ok(false);
        }
        let task = work.load_task(self.store.as_ref(), &run.task_id)?;
        let is_current = self
            .store
            .task_runs(&run.task_id)
            .map_err(store_error)?
            .last()
            .is_some_and(|latest| latest.run_id == run.run_id);
        let enforced_stop = run.status == RunStatus::Cancelling
            && matches!(
                task.status,
                TaskStatus::Running
                    | TaskStatus::WaitingChild
                    | TaskStatus::WaitingMaster
                    | TaskStatus::WaitingUser
            );

        let mut target = report.status;
        let mut failure = report.failure;
        if !matches!(
            target,
            RunStatus::Succeeded
                | RunStatus::Failed
                | RunStatus::Cancelled
                | RunStatus::Unknown
                | RunStatus::Interrupted
        ) {
            return Err(SchedulerError::Conflict {
                entity: format!("run:{} report status {target:?}", run.run_id),
            });
        }
        // Scheduler가 실행 시간 상한으로 멈춘 Run은 취소가 아니라 Timeout 실패입니다.
        if enforced_stop && is_current && target == RunStatus::Cancelled {
            target = RunStatus::Failed;
            failure = Some(FailureKind::Timeout);
        }
        if run.status == target {
            return Ok(false);
        }
        if !run.status.can_transition_to(target) {
            // 예: 취소 중인 Run의 추적을 잃음. 결과를 확정할 수 없습니다.
            target = RunStatus::Unknown;
        }
        let certain = matches!(
            target,
            RunStatus::Succeeded | RunStatus::Failed | RunStatus::Cancelled
        );

        let mut writes = Vec::new();
        run_transition(&mut run, target, &work.now, &mut writes)?;
        if certain {
            self.settle_intent(&run.run_id, &mut writes)?;
            self.release_run_locks(&run.run_id, &mut writes)?;
        }
        if let Some(native) = &report.native_session_id {
            self.record_session(&run, native, &report.native_turn_ids, None, &mut writes)?;
        }
        work.writes.append(&mut writes);

        if !is_current {
            // 이전 Run의 늦은 결과는 새 Run의 결과로 채택하지 않습니다 (§9.5).
            return Ok(certain);
        }
        let store = self.store.as_ref();
        let id = task.spec.id.as_str();
        match task.status {
            TaskStatus::Cancelling => {
                // 취소 의도가 확정된 Task는 Run이 성공으로 끝났어도 완료로 되돌리지 않습니다.
                if certain {
                    work.finish(store, id, TaskStatus::Cancelled)?;
                } else {
                    work.transition(id, TaskStatus::NeedsReconciliation)?;
                }
            }
            TaskStatus::Running
            | TaskStatus::WaitingChild
            | TaskStatus::WaitingMaster
            | TaskStatus::WaitingUser => match target {
                RunStatus::Succeeded => work.transition(id, TaskStatus::Executed)?,
                RunStatus::Failed => {
                    let next = self.after_failure(&task, failure)?;
                    work.finish(store, id, next)?;
                }
                RunStatus::Cancelled => {
                    // Task 취소 없이 Run만 취소된 경우에도 취소로 확정합니다.
                    work.transition(id, TaskStatus::Cancelling)?;
                    work.finish(store, id, TaskStatus::Cancelled)?;
                }
                RunStatus::Interrupted => work.transition(id, TaskStatus::Interrupted)?,
                _ => {
                    // 결과 불명. 다시 실행하지 않고 대조를 기다립니다 (§14.3).
                    let next = if task.status == TaskStatus::Running {
                        TaskStatus::NeedsReconciliation
                    } else {
                        TaskStatus::Interrupted
                    };
                    work.transition(id, next)?;
                }
            },
            // 대조 중이거나 이미 다른 판단이 내려진 Task는 Run 기록만 갱신합니다.
            _ => {}
        }
        Ok(certain)
    }

    async fn revoke_mcp(&self, run_id: &str) {
        if let Some(mcp) = &self.task_mcp {
            // 폐기 실패는 자격의 만료 시각이 막습니다. 종료 처리를 되돌리지 않습니다.
            let _ = mcp.revoke(run_id).await;
        }
    }

    /// 대기열에서 자리가 난 요청을 순서대로 시작합니다.
    async fn drain(&self, queue: &mut VecDeque<QueuedRun>) {
        let mut index = 0;
        while index < queue.len() {
            let candidate = queue[index].clone();
            let runnable = match self.store.task(&candidate.task_id) {
                Ok(Some(task)) if task.status == TaskStatus::Ready => {
                    let limit = self
                        .profile(&task.spec)
                        .map(|p| p.limits.provider_concurrency)
                        .unwrap_or(1);
                    match self.at_capacity(
                        &task.spec.agent.provider,
                        &task.spec.workspace.host_id,
                        limit,
                    ) {
                        Ok(false) => Some(true),
                        Ok(true) => Some(false),
                        Err(_) => None,
                    }
                }
                // 그사이 취소되었거나 다른 상태가 된 요청은 버립니다.
                _ => None,
            };
            match runnable {
                Some(false) => index += 1,
                Some(true) => {
                    queue.remove(index);
                    // 실패한 시작은 Task·Run 상태와 Journal에 남습니다.
                    let _ = self
                        .start(
                            queue,
                            &candidate.actor,
                            &candidate.task_id,
                            candidate.session,
                            &candidate.idempotency_key,
                            false,
                        )
                        .await;
                }
                None => {
                    queue.remove(index);
                }
            }
        }
    }

    const RUN_SCOPE: &'static str = "scheduler.request_run";

    /// §9.1의 2~5번입니다. 문(`gate`)을 쥔 채로 부릅니다.
    async fn start(
        &self,
        queue: &mut VecDeque<QueuedRun>,
        actor: &Actor,
        task_id: &str,
        session: SessionRequest,
        idempotency_key: &str,
        may_queue: bool,
    ) -> Result<RunAdmission> {
        let store = self.store.as_ref();
        let task = store
            .task(task_id)
            .map_err(store_error)?
            .ok_or_else(|| not_found("task", task_id))?;
        self.check_plan_actor(actor, &task.spec.plan_id)?;
        if task.status != TaskStatus::Ready {
            return Err(invalid(
                task_id,
                format!("a run starts from Ready, task is {:?}", task.status),
            ));
        }
        let profile = self.profile(&task.spec)?;
        let plan = store
            .plan(&task.spec.plan_id)
            .map_err(store_error)?
            .ok_or_else(|| not_found("plan", &task.spec.plan_id))?
            .value;

        // 2번: Session 소유권·Host·Provider·권한
        let source = match (&session.mode, &session.source_session_id) {
            (SessionMode::Resume | SessionMode::NativeFork, Some(id)) => Some(
                store
                    .session(id)
                    .map_err(store_error)?
                    .ok_or_else(|| not_found("session", id))?,
            ),
            (SessionMode::Resume | SessionMode::NativeFork, None) => {
                return Err(forbidden("resume and native-fork need a source session"));
            }
            _ => None,
        };
        let authorization = self
            .policy
            .authorize_run(&task.spec, &session, source.as_ref())
            .map_err(forbidden)?;

        // 3번: 동시성
        let provider = task.spec.agent.provider.clone();
        let host_id = task.spec.workspace.host_id.clone();
        if self.at_capacity(&provider, &host_id, profile.limits.provider_concurrency)? {
            if may_queue {
                if let Some(position) = queue
                    .iter()
                    .position(|q| q.task_id == task_id && q.idempotency_key == idempotency_key)
                {
                    return Ok(RunAdmission::Queued {
                        position: position + 1,
                    });
                }
                if queue.iter().any(|q| q.task_id == task_id) {
                    return Err(invalid(task_id, "a run request is already queued"));
                }
                queue.push_back(QueuedRun {
                    actor: actor.clone(),
                    task_id: task_id.to_string(),
                    session,
                    idempotency_key: idempotency_key.to_string(),
                });
                return Ok(RunAdmission::Queued {
                    position: queue.len(),
                });
            }
            return Err(SchedulerError::Unavailable {
                reason: format!(
                    "provider {provider} on host {host_id} is at its concurrency limit ({})",
                    profile.limits.provider_concurrency.max(1)
                ),
            });
        }

        // 3번: Workspace와 Writer 잠금. 잠금 충돌은 기다리지 않고 바로 알립니다 (§9.3, DELEG-10).
        let run_id = new_id("run")?;
        let attempt = self.revision_runs(&task)?.len() as u32 + 1;
        let workspace = match self.workspace.prepare(&task, &run_id).await {
            Ok(workspace) => workspace,
            Err(WorkspaceError::LockConflict { resource }) => {
                return Err(SchedulerError::Unavailable {
                    reason: format!("workspace is locked by another writer: {resource}"),
                });
            }
            Err(WorkspaceError::SourceChanged { reason }) => {
                return Err(SchedulerError::Unavailable {
                    reason: format!("workspace source changed: {reason}"),
                });
            }
            Err(other) => {
                return Err(SchedulerError::Storage {
                    message: format!("workspace: {other:?}"),
                });
            }
        };

        // 여기부터 실패하면 준비 과정에서 잡은 잠금을 풉니다.
        let abort = |error: SchedulerError| -> SchedulerError {
            let mut writes = Vec::new();
            if self.release_run_locks(&run_id, &mut writes).is_ok() {
                let _ = self.commit(writes);
            }
            error
        };

        // 4번: Snapshot·입력·설정 고정
        let mut context = RunContext {
            run_id: run_id.clone(),
            task_id: task_id.to_string(),
            task_revision: task.spec.revision,
            attempt,
            controller_epoch: plan.controller_epoch,
            host_id: host_id.clone(),
            workspace_id: workspace.workspace_id.clone(),
            context_snapshot_id: format!("ctx-{run_id}"),
            policy_revision: authorization.policy_revision,
            idempotency_key: idempotency_key.to_string(),
            session: session.clone(),
            task_mcp: None,
        };
        let snapshot = match self.context.build(&task, &context).await {
            Ok(snapshot) => snapshot,
            Err(error) => {
                return Err(abort(SchedulerError::Unavailable {
                    reason: format!("context could not be built: {error:?}"),
                }));
            }
        };
        if snapshot.run_id != run_id || snapshot.snapshot_id != context.context_snapshot_id {
            return Err(abort(SchedulerError::Storage {
                message: "context snapshot does not belong to this run".into(),
            }));
        }
        if snapshot.blocks_execution() {
            // 막는 입력 문제: 실행하지 않고 Blocked로 둡니다. 다음 판단은 마스터·사용자가 합니다 (CTX-02).
            let reasons: Vec<String> = snapshot
                .issues
                .iter()
                .filter(|issue| issue.blocking)
                .map(|issue| format!("{issue:?}"))
                .collect();
            let mut work = self.work();
            work.load_task(store, task_id)?;
            work.transition(task_id, TaskStatus::Blocked)?;
            let mut writes = work.into_writes();
            if let Err(error) = self.release_run_locks(&run_id, &mut writes) {
                return Err(abort(error));
            }
            self.commit(writes).map_err(abort)?;
            return Err(SchedulerError::Unavailable {
                reason: format!("required input is not usable: {}", reasons.join("; ")),
            });
        }
        if snapshot.strategy != session.mode {
            // 요청 전략을 쓸 수 없어 문맥 구성이 대체했습니다 (§8.2, FORK-03).
            context.session = SessionRequest {
                mode: snapshot.strategy,
                source_session_id: None,
                source_turn_id: None,
                capsule_version_id: snapshot.capsule.as_ref().map(|c| c.version_id.clone()),
            };
        }
        let used_source = source.filter(|_| {
            matches!(
                context.session.mode,
                SessionMode::Resume | SessionMode::NativeFork
            )
        });

        if let Some(mcp) = &self.task_mcp {
            let depth_left = profile
                .limits
                .max_delegation_depth
                .saturating_sub(task.delegation_depth);
            let scope = TaskMcpScope {
                project_id: task.spec.project_id.clone(),
                plan_id: task.spec.plan_id.clone(),
                task_id: task_id.to_string(),
                run_id: run_id.clone(),
                allowed_tools: if depth_left == 0 {
                    vec![
                        TaskTool::GetTaskResult,
                        TaskTool::ListTasks,
                        TaskTool::GetAgentStatus,
                        TaskTool::ReportTaskResult,
                    ]
                } else {
                    vec![
                        TaskTool::DelegateTask,
                        TaskTool::GetTaskResult,
                        TaskTool::ListTasks,
                        TaskTool::CancelTask,
                        TaskTool::GetAgentStatus,
                        TaskTool::ReportTaskResult,
                    ]
                },
                remaining_children: if depth_left == 0 {
                    0
                } else {
                    profile.limits.max_plan_tasks
                },
                expires_at: time::format(self.now() + profile.run_timeout_secs),
            };
            match mcp.issue_binding(scope).await {
                Ok(binding) => context.task_mcp = Some(binding),
                Err(error) => {
                    return Err(abort(SchedulerError::Unavailable {
                        reason: format!("task MCP binding failed: {}", error.message),
                    }));
                }
            }
        }

        // 4번: Run 발급과 실행 의도 기록. Task·Run·의도·Snapshot·멱등 키가 한 Transaction입니다.
        let now = self.now_text();
        let mut run = RunRecord {
            run_id: run_id.clone(),
            task_id: task_id.to_string(),
            task_revision: task.spec.revision,
            attempt,
            status: RunStatus::Starting,
            state_version: 0,
            provider: provider.clone(),
            host_id: host_id.clone(),
            session_mode: context.session.mode,
            outcome: None,
            created_at: now.clone(),
            finished_at: None,
        };
        let mut intent = RunIntent {
            intent_id: new_id("intent")?,
            run_id: run_id.clone(),
            task_id: task_id.to_string(),
            task_revision: task.spec.revision,
            attempt,
            idempotency_key: idempotency_key.to_string(),
            payload_digest: digest(&(&task.spec, &context, &snapshot.contract)),
            phase: IntentPhase::Recorded,
            host_id: host_id.clone(),
            provider,
            native_session_id: None,
            process: None,
            recorded_at: now.clone(),
            updated_at: now.clone(),
        };
        let mut work = self.work();
        work.load_task(store, task_id)?;
        work.transition(task_id, TaskStatus::Running)?;
        let mut writes = work.into_writes();
        writes.push(StateWrite::Run {
            record: run.clone(),
            expected_version: None,
        });
        run.state_version = 1;
        writes.push(StateWrite::Intent {
            intent: intent.clone(),
        });
        writes.push(StateWrite::ContextSnapshot {
            snapshot: snapshot.clone(),
        });
        if context.session.mode == SessionMode::Resume
            && let Some(session) = &used_source
        {
            // 같은 Session의 쓰기 Turn은 한 번에 하나입니다 (§8.1).
            writes.push(StateWrite::AcquireLock {
                lock: ResourceLock {
                    lock_id: new_id("lock")?,
                    resource: LockResource::Session {
                        session_id: session.session_id.clone(),
                    },
                    mode: LockMode::Writer,
                    holder_task_id: task_id.to_string(),
                    holder_run_id: run_id.clone(),
                    owner_epoch: 1,
                    process: None,
                    acquired_at: now.clone(),
                    expires_at: None,
                },
            });
        }
        writes.push(self.idempotency_write(
            Self::RUN_SCOPE,
            idempotency_key,
            digest(&(task_id, &session)),
            serde_json::json!({ "runId": run_id }),
        ));
        if let Err(error) = self.commit(writes) {
            self.revoke_mcp(&run_id).await;
            return Err(abort(error));
        }

        // 5번: Adapter 전달. 전달 직전에 의도를 `dispatched`로 올려, 응답을 잃어도 재시작 때 구분합니다.
        intent.phase = IntentPhase::Dispatched;
        intent.updated_at = self.now_text();
        self.commit(vec![StateWrite::Intent {
            intent: intent.clone(),
        }])?;

        match self.executor.start(&task.spec, &context, &snapshot).await {
            Ok((started, evidence)) => {
                let mut writes = Vec::new();
                intent.phase = IntentPhase::Started;
                intent.native_session_id = started.native_session_id.clone();
                intent.process = evidence;
                intent.updated_at = self.now_text();
                writes.push(StateWrite::Intent { intent });
                run_transition(&mut run, RunStatus::Running, &self.now_text(), &mut writes)?;
                if let Some(native) = &started.native_session_id {
                    let turns: Vec<String> = started.native_turn_id.into_iter().collect();
                    self.record_session(&run, native, &turns, used_source.as_ref(), &mut writes)?;
                }
                self.commit(writes)?;
                Ok(RunAdmission::Started { run_id })
            }
            Err(ExecutorError::Unknown { message }) => {
                // 시작했는지 알 수 없습니다. 의도는 `dispatched`로 남기고 다시 실행하지 않습니다 (§14.3).
                let mut work = self.work();
                work.load_task(store, task_id)?;
                run_transition(
                    &mut run,
                    RunStatus::Interrupted,
                    &work.now,
                    &mut work.writes,
                )?;
                work.transition(task_id, TaskStatus::NeedsReconciliation)?;
                self.commit(work.into_writes())?;
                Err(SchedulerError::StartFailed {
                    run_id,
                    reason: message,
                    confirmed: false,
                })
            }
            Err(ExecutorError::Rejected { reason })
            | Err(ExecutorError::UnknownRun { run_id: reason }) => {
                let mut work = self.work();
                self.apply_report(
                    &mut work,
                    &RunReport {
                        run_id: run_id.clone(),
                        status: RunStatus::Failed,
                        failure: Some(FailureKind::StartFailed),
                        message: Some(reason.clone()),
                        native_session_id: None,
                        native_turn_ids: vec![],
                    },
                )?;
                self.commit(work.into_writes())?;
                self.revoke_mcp(&run_id).await;
                Err(SchedulerError::StartFailed {
                    run_id,
                    reason,
                    confirmed: true,
                })
            }
        }
    }

    /// `task_id`와 전파 대상 자손을 부모가 먼저 오도록 돌려줍니다.
    fn cancel_targets(work: &Work, task_id: &str) -> Vec<String> {
        let mut targets = vec![task_id.to_string()];
        let mut index = 0;
        while index < targets.len() {
            let parent = targets[index].clone();
            let mut children: Vec<&TaskRecord> = work
                .tasks
                .values()
                .filter(|task| {
                    task.spec.parent_task_id.as_deref() == Some(parent.as_str())
                        && task.cancel_propagation == CancelPropagation::Descendants
                })
                .collect();
            children.sort_by(|a, b| a.spec.id.cmp(&b.spec.id));
            targets.extend(children.into_iter().map(|task| task.spec.id.clone()));
            index += 1;
        }
        targets
    }

    fn is_descendant(work: &Work, ancestor: &str, task_id: &str) -> bool {
        let mut current = work.tasks.get(task_id);
        let mut steps = 0;
        while let Some(task) = current {
            match task.spec.parent_task_id.as_deref() {
                Some(parent) if parent == ancestor => return true,
                Some(parent) if steps < work.tasks.len() => {
                    current = work.tasks.get(parent);
                    steps += 1;
                }
                _ => return false,
            }
        }
        false
    }
}

#[async_trait]
impl Scheduler for TaskScheduler {
    async fn register_tasks(
        &self,
        actor: Actor,
        specs: Vec<TaskSpec>,
        idempotency_key: &str,
    ) -> Result<Vec<TaskRecord>> {
        let _gate = self.gate.lock().await;
        Self::deny_worker(&actor)?;
        let Some(first) = specs.first() else {
            return Ok(vec![]);
        };
        let plan_id = first.plan_id.clone();
        if specs.iter().any(|spec| spec.plan_id != plan_id) {
            return Err(forbidden("tasks of one request belong to one plan"));
        }
        let store = self.store.as_ref();
        let plan = store
            .plan(&plan_id)
            .map_err(store_error)?
            .ok_or_else(|| not_found("plan", &plan_id))?
            .value;
        self.check_plan_actor(&actor, &plan_id)?;

        let scope = format!("scheduler.register:{plan_id}");
        let payload_digest = digest(&specs);
        if let Some(result) = self.replayed(&scope, idempotency_key, &payload_digest)? {
            let mut records = Vec::new();
            for id in result
                .get("taskIds")
                .and_then(|v| v.as_array())
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str())
            {
                records.push(
                    store
                        .task(id)
                        .map_err(store_error)?
                        .ok_or_else(|| not_found("task", id))?,
                );
            }
            return Ok(records);
        }

        let mut work = self.work();
        work.load_plan(store, &plan_id)?;
        let by_id: HashMap<&str, &TaskSpec> =
            specs.iter().map(|spec| (spec.id.as_str(), spec)).collect();
        if by_id.len() != specs.len() {
            return Err(forbidden("duplicate task id in one request"));
        }
        for spec in &specs {
            if spec.project_id != plan.project_id {
                return Err(forbidden(format!(
                    "task {} belongs to another project",
                    spec.id
                )));
            }
            if work.tasks.contains_key(&spec.id) {
                return Err(invalid(&spec.id, "task already exists"));
            }
            for dep in &spec.dependencies {
                if dep == &spec.id {
                    return Err(SchedulerError::DependencyCycle {
                        task_ids: vec![dep.clone(), dep.clone()],
                    });
                }
                if !work.tasks.contains_key(dep) && !by_id.contains_key(dep.as_str()) {
                    return Err(not_found("dependency", dep));
                }
            }
        }
        if let Some(task_ids) = find_cycle(&specs) {
            return Err(SchedulerError::DependencyCycle { task_ids });
        }

        // 위임 깊이: 부모가 같은 요청 안에 있을 수 있어 부모 사슬을 따라 셉니다.
        let mut depths: HashMap<String, u32> = HashMap::new();
        for spec in &specs {
            let mut chain = vec![spec];
            let base = loop {
                let current = chain.last().expect("chain is never empty");
                match current.parent_task_id.as_deref() {
                    None => break 0,
                    Some(parent) => {
                        if let Some(depth) = depths.get(parent) {
                            break depth + 1;
                        }
                        if let Some(existing) = work.tasks.get(parent) {
                            break existing.delegation_depth + 1;
                        }
                        match by_id.get(parent) {
                            Some(next) if chain.len() <= specs.len() => chain.push(next),
                            Some(_) => {
                                return Err(forbidden(format!(
                                    "parent chain of {} loops",
                                    spec.id
                                )));
                            }
                            None => return Err(not_found("parent task", parent)),
                        }
                    }
                }
            };
            for (offset, item) in chain.iter().rev().enumerate() {
                depths
                    .entry(item.id.clone())
                    .or_insert(base + offset as u32);
            }
        }

        let count = specs.len() as u32;
        for spec in &specs {
            let profile = self.profile(spec)?;
            let depth = depths[&spec.id];
            if depth > profile.limits.max_delegation_depth {
                return Err(SchedulerError::LimitExceeded {
                    reason: format!(
                        "task {} would be at delegation depth {depth}, limit is {}",
                        spec.id, profile.limits.max_delegation_depth
                    ),
                });
            }
            let ledger = &work.ledger(store, &plan_id)?.ledger;
            if !ledger.can_reserve_tasks(&profile, count, true) {
                return Err(SchedulerError::LimitExceeded {
                    reason: format!(
                        "plan {plan_id} would exceed its task limit ({})",
                        profile.limits.max_plan_tasks
                    ),
                });
            }
        }

        let now = work.now.clone();
        for spec in &specs {
            let ready = spec.dependencies.iter().all(|dep| {
                work.tasks
                    .get(dep)
                    .is_some_and(|d| d.status == TaskStatus::Completed)
            });
            work.insert(TaskRecord {
                spec: spec.clone(),
                status: if ready {
                    TaskStatus::Ready
                } else {
                    TaskStatus::Blocked
                },
                state_version: 0,
                wait: None,
                cancel_propagation: CancelPropagation::Descendants,
                delegation_depth: depths[&spec.id],
                created_at: now.clone(),
                updated_at: now.clone(),
            });
        }
        let state = work.ledger(store, &plan_id)?;
        state.ledger.tasks_reserved += count;
        state.dirty = true;

        let ids: Vec<String> = specs.iter().map(|spec| spec.id.clone()).collect();
        let records = ids
            .iter()
            .map(|id| work.task(id).cloned())
            .collect::<Result<Vec<_>>>()?;
        let mut writes = work.into_writes();
        writes.push(self.idempotency_write(
            &scope,
            idempotency_key,
            payload_digest,
            serde_json::json!({ "taskIds": ids }),
        ));
        self.commit(writes)?;
        Ok(records)
    }

    async fn delegate_task(&self, actor: Actor, input: DelegateTaskInput) -> Result<TaskRecord> {
        let mut queue = self.gate.lock().await;
        let Actor::Worker {
            task_id: parent_id,
            run_id,
        } = &actor
        else {
            return Err(forbidden("delegate_task is called by a worker run"));
        };
        let store = self.store.as_ref();
        let mut work = self.work();
        let parent = work.load_task(store, parent_id)?;
        let run = store
            .run(run_id)
            .map_err(store_error)?
            .ok_or_else(|| not_found("run", run_id))?;
        if run.task_id != *parent_id || run.status.is_terminal() {
            return Err(forbidden("caller run does not own this task"));
        }

        let scope = format!("scheduler.delegate:{parent_id}");
        let payload_digest = digest(&input);
        if let Some(result) = self.replayed(&scope, &input.idempotency_key, &payload_digest)? {
            let id = result.get("taskId").and_then(|v| v.as_str()).unwrap_or("");
            return store
                .task(id)
                .map_err(store_error)?
                .ok_or_else(|| not_found("task", id));
        }
        if parent.status != TaskStatus::Running {
            return Err(invalid(
                parent_id,
                format!("only a running task delegates, task is {:?}", parent.status),
            ));
        }

        // 권한·예산은 호출자가 고르지 않고 정책이 부모 범위 안에서 정합니다 (§9.3, §13.1).
        let profiles = self
            .policy
            .child_profiles(&parent.spec, &input)
            .map_err(forbidden)?;
        let child_id = new_id("task")?;
        let spec = TaskSpec {
            id: child_id.clone(),
            revision: 1,
            project_id: parent.spec.project_id.clone(),
            plan_id: parent.spec.plan_id.clone(),
            parent_task_id: Some(parent_id.clone()),
            r#type: input.r#type,
            objective: input.objective.clone(),
            constraints: input.constraints.clone(),
            dependencies: vec![],
            agent: input.agent.clone(),
            workspace: input.workspace.clone(),
            inputs: input.inputs.clone(),
            outputs: input.outputs.clone(),
            validation: input.validation.clone(),
            permission_profile_id: profiles.permission_profile_id,
            budget_profile_id: profiles.budget_profile_id,
            completion_policy_id: profiles.completion_policy_id,
        };
        let profile = self.profile(&spec)?;
        let depth = parent.delegation_depth + 1;
        if depth > profile.limits.max_delegation_depth {
            return Err(SchedulerError::LimitExceeded {
                reason: format!(
                    "delegation depth {depth} exceeds the limit {}",
                    profile.limits.max_delegation_depth
                ),
            });
        }
        let ledger = &work.ledger(store, &spec.plan_id)?.ledger;
        if !ledger.can_reserve_tasks(&profile, 1, false) {
            return Err(SchedulerError::LimitExceeded {
                reason: format!("plan {} has no task budget left for workers", spec.plan_id),
            });
        }
        // 지금 실행할 수 없는 위임은 등록하지 않고 바로 거부합니다. 부모가 자리를 쥐고 기다리는 교착을 만들지 않습니다.
        if self.at_capacity(
            &spec.agent.provider,
            &spec.workspace.host_id,
            profile.limits.provider_concurrency,
        )? {
            return Err(SchedulerError::Unavailable {
                reason: format!(
                    "provider {} on host {} is at its concurrency limit ({}); the child cannot run now",
                    spec.agent.provider,
                    spec.workspace.host_id,
                    profile.limits.provider_concurrency.max(1)
                ),
            });
        }

        let now = work.now.clone();
        work.insert(TaskRecord {
            spec,
            status: TaskStatus::Ready,
            state_version: 0,
            wait: None,
            cancel_propagation: input.cancel_propagation,
            delegation_depth: depth,
            created_at: now.clone(),
            updated_at: now,
        });
        let state = work.ledger(store, &parent.spec.plan_id)?;
        state.ledger.tasks_reserved += 1;
        state.dirty = true;
        if input.wait_for_result {
            let deadline = input
                .deadline
                .clone()
                .unwrap_or_else(|| time::format(self.now() + profile.wait_timeout_secs));
            work.wait(
                parent_id,
                TaskStatus::WaitingChild,
                WaitState {
                    resume_status: TaskStatus::Running,
                    waiting_for: vec![child_id.clone()],
                    deadline: Some(deadline),
                },
            )?;
        }
        let mut writes = work.into_writes();
        writes.push(self.idempotency_write(
            &scope,
            &input.idempotency_key,
            payload_digest,
            serde_json::json!({ "taskId": child_id }),
        ));
        self.commit(writes)?;

        let started = self
            .start(
                &mut queue,
                &Actor::Core,
                &child_id,
                SessionRequest {
                    mode: SessionMode::Fresh,
                    source_session_id: None,
                    source_turn_id: None,
                    capsule_version_id: None,
                },
                &format!("delegate:{child_id}"),
                false,
            )
            .await;
        if let Err(error) = started {
            // 시작하지 못한 자식을 남겨 부모가 끝없이 기다리게 하지 않습니다.
            let mut work = self.work();
            let child = work.load_task(store, &child_id)?;
            if !child.status.is_terminal() && child.status != TaskStatus::NeedsReconciliation {
                let next = if matches!(child.status, TaskStatus::Running | TaskStatus::Validating) {
                    TaskStatus::Failed
                } else {
                    TaskStatus::Cancelled
                };
                work.finish(store, &child_id, next)?;
            }
            // 대조가 필요한 자식은 그대로 두고 부모만 깨웁니다.
            let parent_now = work.task(parent_id)?.clone();
            if parent_now.status == TaskStatus::WaitingChild {
                work.transition(parent_id, TaskStatus::Running)?;
            }
            self.commit(work.into_writes())?;
            return Err(error);
        }
        store
            .task(&child_id)
            .map_err(store_error)?
            .ok_or_else(|| not_found("task", &child_id))
    }

    async fn request_run(
        &self,
        actor: Actor,
        task_id: &str,
        session: SessionRequest,
        idempotency_key: &str,
    ) -> Result<RunAdmission> {
        let mut queue = self.gate.lock().await;
        Self::deny_worker(&actor)?;
        let payload_digest = digest(&(task_id, &session));
        if let Some(result) = self.replayed(Self::RUN_SCOPE, idempotency_key, &payload_digest)? {
            let run_id = result
                .get("runId")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            return Ok(RunAdmission::Started { run_id });
        }
        self.start(&mut queue, &actor, task_id, session, idempotency_key, true)
            .await
    }

    async fn cancel_task(
        &self,
        actor: Actor,
        task_id: &str,
        reason: &str,
        idempotency_key: &str,
    ) -> Result<AcceptedResult> {
        let mut queue = self.gate.lock().await;
        let store = self.store.as_ref();
        let mut work = self.work();
        let task = work.load_task(store, task_id)?;
        match &actor {
            Actor::Worker {
                task_id: caller,
                run_id,
            } => {
                let run = store
                    .run(run_id)
                    .map_err(store_error)?
                    .ok_or_else(|| not_found("run", run_id))?;
                if run.task_id != *caller || run.status.is_terminal() {
                    return Err(forbidden("caller run does not own this task"));
                }
                if !Self::is_descendant(&work, caller, task_id) {
                    return Err(forbidden("a worker cancels only tasks it delegated"));
                }
            }
            other => self.check_plan_actor(other, &task.spec.plan_id)?,
        }

        let scope = "scheduler.cancel_task";
        let payload_digest = digest(&(task_id, reason));
        if let Some(result) = self.replayed(scope, idempotency_key, &payload_digest)? {
            return Ok(AcceptedResult {
                request_id: result
                    .get("requestId")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                accepted: true,
            });
        }

        let request_id = new_id("cancel")?;
        let mut to_stop = Vec::new();
        for id in Self::cancel_targets(&work, task_id) {
            let status = work.task(&id)?.status;
            if status.is_terminal() || status == TaskStatus::Cancelling {
                continue;
            }
            let open = self.open_runs(&id)?;
            to_stop.extend(open.iter().map(|run| run.run_id.clone()));
            if status.can_transition_to(TaskStatus::Cancelled) {
                // 외부 실행이 없는 상태. 중단·대조 중인 Task의 Run은 정지만 요청하고 의도·잠금은 대조에 남깁니다.
                work.finish(store, &id, TaskStatus::Cancelled)?;
            } else {
                work.transition(&id, TaskStatus::Cancelling)?;
                if open.is_empty() {
                    // 멈출 프로세스가 없으면 바로 확정합니다(검증·승인 대기 중 취소).
                    work.finish(store, &id, TaskStatus::Cancelled)?;
                }
            }
            queue.retain(|queued| queued.task_id != id);
        }
        let mut writes = work.into_writes();
        // 정지를 요청할 Run은 `Cancelling`으로 적어 둡니다.
        for run_id in &to_stop {
            if let Some(mut run) = store.run(run_id).map_err(store_error)?
                && run.status.can_transition_to(RunStatus::Cancelling)
            {
                run_transition(
                    &mut run,
                    RunStatus::Cancelling,
                    &self.now_text(),
                    &mut writes,
                )?;
            }
        }
        writes.push(self.idempotency_write(
            scope,
            idempotency_key,
            payload_digest,
            serde_json::json!({ "requestId": request_id, "reason": reason }),
        ));
        self.commit(writes)?;

        // 접수와 실제 정지는 다릅니다. 정지 결과는 종료 보고로 옵니다 (§7.3).
        for run_id in &to_stop {
            let _ = self.executor.cancel(run_id).await;
        }
        Ok(AcceptedResult {
            request_id,
            accepted: true,
        })
    }

    async fn request_rework(
        &self,
        actor: Actor,
        task_id: &str,
        _reason: &str,
    ) -> Result<TaskRecord> {
        let _gate = self.gate.lock().await;
        Self::deny_worker(&actor)?;
        let store = self.store.as_ref();
        let mut work = self.work();
        let task = work.load_task(store, task_id)?;
        self.check_plan_actor(&actor, &task.spec.plan_id)?;
        let finished_execution = match task.status {
            TaskStatus::Executed | TaskStatus::Validating | TaskStatus::Verified => true,
            TaskStatus::WaitingMaster | TaskStatus::WaitingUser => task
                .wait
                .as_ref()
                .is_some_and(|wait| wait.resume_status != TaskStatus::Running),
            _ => false,
        };
        if !finished_execution {
            return Err(invalid(
                task_id,
                format!(
                    "rework follows a finished execution, task is {:?}",
                    task.status
                ),
            ));
        }
        if !self.rework_allowed(&task)? {
            return Err(SchedulerError::LimitExceeded {
                reason: format!("task {task_id} reached its rework limit"),
            });
        }
        work.transition(task_id, TaskStatus::Ready)?;
        let record = work.task(task_id)?.clone();
        self.commit(work.into_writes())?;
        Ok(record)
    }

    async fn begin_validation(&self, actor: Actor, task_id: &str) -> Result<TaskRecord> {
        let _gate = self.gate.lock().await;
        Self::deny_worker(&actor)?;
        let mut work = self.work();
        let task = work.load_task(self.store.as_ref(), task_id)?;
        self.check_plan_actor(&actor, &task.spec.plan_id)?;
        if task.status != TaskStatus::Executed {
            return Err(invalid(
                task_id,
                format!("validation follows Executed, task is {:?}", task.status),
            ));
        }
        work.transition(task_id, TaskStatus::Validating)?;
        let record = work.task(task_id)?.clone();
        self.commit(work.into_writes())?;
        Ok(record)
    }

    async fn on_validation_finished(
        &self,
        task_id: &str,
        _validation_id: &str,
    ) -> Result<TaskRecord> {
        let _gate = self.gate.lock().await;
        let store = self.store.as_ref();
        let mut work = self.work();
        let task = work.load_task(store, task_id)?;
        if task.status != TaskStatus::Validating {
            // 취소·재작업 뒤에 도착한 판정은 기록으로만 남습니다.
            return Ok(task);
        }
        let jobs = store.task_validations(task_id).map_err(store_error)?;
        let mut all_passed = true;
        let mut any_failed = false;
        let mut undecided = Vec::new();
        for contract in task.spec.validation.iter().filter(|c| c.required) {
            // 같은 계약의 검증을 다시 돌렸으면 마지막 Job이 유효합니다.
            let latest = jobs.iter().rev().find(|job| {
                job.task_revision == task.spec.revision
                    && job.contract.id == contract.id
                    && job.contract.revision == contract.revision
            });
            match latest {
                None => return Ok(task),
                Some(job) => match (job.status, job.verdict) {
                    (ValidationJobStatus::Finished, Some(ValidationVerdict::Passed)) => {}
                    (ValidationJobStatus::Finished, Some(ValidationVerdict::Failed)) => {
                        all_passed = false;
                        any_failed = true;
                    }
                    (ValidationJobStatus::Finished | ValidationJobStatus::Cancelled, _) => {
                        // 실행 오류·판정 불가·건너뜀·취소는 통과가 아닙니다.
                        all_passed = false;
                        undecided.push(job.validation_id.clone());
                    }
                    _ => return Ok(task),
                },
            }
        }
        if all_passed {
            work.transition(task_id, TaskStatus::Verified)?;
        } else if any_failed {
            let next = if self.rework_allowed(&task)? {
                TaskStatus::Ready
            } else {
                TaskStatus::Failed
            };
            work.finish(store, task_id, next)?;
        } else {
            work.wait(
                task_id,
                TaskStatus::WaitingMaster,
                WaitState {
                    resume_status: TaskStatus::Validating,
                    waiting_for: undecided,
                    deadline: None,
                },
            )?;
        }
        let record = work.task(task_id)?.clone();
        self.commit(work.into_writes())?;
        Ok(record)
    }

    async fn begin_wait(
        &self,
        actor: Actor,
        task_id: &str,
        kind: WaitKind,
        waiting_for: Vec<String>,
        deadline: Option<String>,
    ) -> Result<TaskRecord> {
        let _gate = self.gate.lock().await;
        Self::deny_worker(&actor)?;
        let store = self.store.as_ref();
        let mut work = self.work();
        let task = work.load_task(store, task_id)?;
        self.check_plan_actor(&actor, &task.spec.plan_id)?;
        if !matches!(
            task.status,
            TaskStatus::Running
                | TaskStatus::Executed
                | TaskStatus::Validating
                | TaskStatus::Verified
        ) {
            return Err(invalid(
                task_id,
                format!("{:?} cannot wait for an answer", task.status),
            ));
        }
        let next = match kind {
            WaitKind::Master => TaskStatus::WaitingMaster,
            WaitKind::User => TaskStatus::WaitingUser,
        };
        work.wait(
            task_id,
            next,
            WaitState {
                resume_status: task.status,
                waiting_for,
                deadline,
            },
        )?;
        let record = work.task(task_id)?.clone();
        let mut writes = work.into_writes();
        if task.status == TaskStatus::Running {
            for mut run in self.open_runs(task_id)? {
                if run.status == RunStatus::Running {
                    run_transition(&mut run, RunStatus::Waiting, &self.now_text(), &mut writes)?;
                }
            }
        }
        self.commit(writes)?;
        Ok(record)
    }

    async fn end_wait(&self, actor: Actor, task_id: &str) -> Result<TaskRecord> {
        let _gate = self.gate.lock().await;
        Self::deny_worker(&actor)?;
        let store = self.store.as_ref();
        let mut work = self.work();
        let task = work.load_task(store, task_id)?;
        self.check_plan_actor(&actor, &task.spec.plan_id)?;
        let (TaskStatus::WaitingMaster | TaskStatus::WaitingUser, Some(wait)) =
            (task.status, task.wait.clone())
        else {
            return Err(invalid(task_id, "task is not waiting for an answer"));
        };
        work.transition(task_id, wait.resume_status)?;
        let record = work.task(task_id)?.clone();
        let mut writes = work.into_writes();
        if wait.resume_status == TaskStatus::Running {
            for mut run in self.open_runs(task_id)? {
                if run.status == RunStatus::Waiting {
                    run_transition(&mut run, RunStatus::Running, &self.now_text(), &mut writes)?;
                }
            }
        }
        self.commit(writes)?;
        Ok(record)
    }

    async fn complete_task(&self, actor: Actor, task_id: &str) -> Result<TaskRecord> {
        let _gate = self.gate.lock().await;
        Self::deny_worker(&actor)?;
        let store = self.store.as_ref();
        let mut work = self.work();
        let task = work.load_task(store, task_id)?;
        self.check_plan_actor(&actor, &task.spec.plan_id)?;
        let has_required = task.spec.validation.iter().any(|c| c.required);
        match task.status {
            TaskStatus::Verified => {}
            // 검증을 요구하지 않는 Task만 실행 완료에서 바로 완료할 수 있습니다 (§9.4).
            TaskStatus::Executed if !has_required => {}
            TaskStatus::Executed => {
                return Err(invalid(task_id, "required validation has not passed"));
            }
            other => {
                return Err(invalid(
                    task_id,
                    format!("completion follows Executed or Verified, task is {other:?}"),
                ));
            }
        }
        if self
            .policy
            .completion_rule(&task.spec)
            .require_result_approval
        {
            let approved = store
                .plan_approvals(&task.spec.plan_id)
                .map_err(store_error)?
                .iter()
                .any(|approval| {
                    approval.task_id.as_deref() == Some(task_id)
                        && approval.kind == ApprovalKind::ResultReview
                        && approval.contract_revision == Some(task.spec.revision)
                        && approval.state == ApprovalState::Granted
                        && approval.answer == Some(ApprovalAnswer::Allow)
                });
            if !approved {
                return Err(invalid(task_id, "result review approval is missing"));
            }
        }
        work.finish(store, task_id, TaskStatus::Completed)?;
        let record = work.task(task_id)?.clone();
        self.commit(work.into_writes())?;
        Ok(record)
    }

    async fn reconcile_task(
        &self,
        actor: Actor,
        task_id: &str,
        resolution: Reconciliation,
    ) -> Result<TaskRecord> {
        let mut queue = self.gate.lock().await;
        Self::deny_worker(&actor)?;
        let store = self.store.as_ref();
        let mut work = self.work();
        let task = work.load_task(store, task_id)?;
        self.check_plan_actor(&actor, &task.spec.plan_id)?;
        if !matches!(
            task.status,
            TaskStatus::NeedsReconciliation | TaskStatus::Interrupted
        ) {
            return Err(invalid(
                task_id,
                format!("{:?} does not need reconciliation", task.status),
            ));
        }
        let (next, run_status) = match resolution {
            Reconciliation::Retry => (TaskStatus::Ready, RunStatus::Failed),
            Reconciliation::AcceptExecuted => (TaskStatus::Executed, RunStatus::Succeeded),
            Reconciliation::Fail => (TaskStatus::Failed, RunStatus::Failed),
            Reconciliation::Cancel => (TaskStatus::Cancelled, RunStatus::Cancelled),
        };
        // 확인이 끝났으므로 이 Task의 미확정 Run·실행 의도·잠금을 정리합니다.
        let mut stopped = Vec::new();
        for mut run in store.task_runs(task_id).map_err(store_error)? {
            let unsettled = store
                .run_intent(&run.run_id)
                .map_err(store_error)?
                .is_some_and(|intent| {
                    !matches!(intent.phase, IntentPhase::Settled | IntentPhase::Abandoned)
                });
            if run.status.is_terminal() && !unsettled {
                continue;
            }
            if !run.status.is_terminal() {
                let target = if run.status.can_transition_to(run_status) {
                    run_status
                } else {
                    RunStatus::Unknown
                };
                run_transition(&mut run, target, &work.now, &mut work.writes)?;
            }
            self.settle_intent(&run.run_id, &mut work.writes)?;
            self.release_run_locks(&run.run_id, &mut work.writes)?;
            stopped.push(run.run_id.clone());
        }
        work.finish(store, task_id, next)?;
        let record = work.task(task_id)?.clone();
        self.commit(work.into_writes())?;
        for run_id in &stopped {
            self.revoke_mcp(run_id).await;
        }
        self.drain(&mut queue).await;
        Ok(record)
    }

    async fn on_run_finished(&self, report: RunReport) -> Result<()> {
        let mut queue = self.gate.lock().await;
        let mut work = self.work();
        let freed = self.apply_report(&mut work, &report)?;
        self.commit(work.into_writes())?;
        if freed {
            self.revoke_mcp(&report.run_id).await;
            self.drain(&mut queue).await;
        }
        Ok(())
    }

    async fn enforce_deadlines(&self) -> Result<DeadlineReport> {
        let _gate = self.gate.lock().await;
        let store = self.store.as_ref();
        let now = self.now();
        let mut report = DeadlineReport::default();

        // 실행 시간 상한: 정지를 요청하고 Run을 `Cancelling`으로 적습니다. Task는 종료 보고 때 Timeout 실패로 처리합니다.
        let mut writes = Vec::new();
        for intent in store.unfinished_intents().map_err(store_error)? {
            let Some(mut run) = store.run(&intent.run_id).map_err(store_error)? else {
                continue;
            };
            if run.status != RunStatus::Running {
                continue;
            }
            let Some(task) = store.task(&run.task_id).map_err(store_error)? else {
                continue;
            };
            let Some(profile) = self.policy.budget_profile(&task.spec.budget_profile_id) else {
                continue;
            };
            let started = time::parse(&run.created_at).unwrap_or(now);
            if now > started.saturating_add(profile.run_timeout_secs) {
                run_transition(
                    &mut run,
                    RunStatus::Cancelling,
                    &time::format(now),
                    &mut writes,
                )?;
                report.timed_out_runs.push(run.run_id.clone());
            }
        }
        self.commit(writes)?;
        for run_id in &report.timed_out_runs {
            let _ = self.executor.cancel(run_id).await;
        }

        // 대기 기한: 무한 대기를 만들지 않습니다 (§9.3).
        let mut work = self.work();
        for plan in store.plans().map_err(store_error)? {
            work.load_plan(store, &plan.value.id)?;
        }
        let mut expired: Vec<(String, TaskStatus, TaskStatus)> = work
            .tasks
            .values()
            .filter_map(|task| {
                let wait = task.wait.as_ref()?;
                let deadline = time::parse(wait.deadline.as_deref()?)?;
                (task.status.is_waiting() && now > deadline)
                    .then(|| (task.spec.id.clone(), task.status, wait.resume_status))
            })
            .collect();
        expired.sort_by(|a, b| a.0.cmp(&b.0));
        let mut resumed_runs = Vec::new();
        for (id, status, resume) in expired {
            if status == TaskStatus::WaitingChild || resume == TaskStatus::Running {
                // 실행 중인 프로세스가 있는 대기는 기다림만 끝냅니다. 자식 결과 없이 계속하거나,
                // 승인 요청이면 호출자가 거부로 답합니다 (§4.3).
                work.transition(&id, resume)?;
                if status != TaskStatus::WaitingChild {
                    resumed_runs.push(id.clone());
                }
            } else {
                work.finish(store, &id, TaskStatus::Failed)?;
            }
            report.expired_waits.push(id);
        }
        let mut writes = work.into_writes();
        for id in resumed_runs {
            for mut run in self.open_runs(&id)? {
                if run.status == RunStatus::Waiting {
                    run_transition(
                        &mut run,
                        RunStatus::Running,
                        &time::format(now),
                        &mut writes,
                    )?;
                }
            }
        }
        self.commit(writes)?;
        Ok(report)
    }

    async fn recover(&self) -> Result<RecoveryReport> {
        let _gate = self.gate.lock().await;
        let store = self.store.as_ref();
        let mut report = RecoveryReport::default();

        for intent in store.unfinished_intents().map_err(store_error)? {
            let mut work = self.work();
            let Some(mut run) = store.run(&intent.run_id).map_err(store_error)? else {
                continue;
            };
            let task = work.load_task(store, &intent.task_id)?;
            let id = task.spec.id.as_str();
            let is_current = store
                .task_runs(id)
                .map_err(store_error)?
                .last()
                .is_some_and(|latest| latest.run_id == run.run_id);

            if intent.phase == IntentPhase::Recorded {
                // Adapter에 전달하기 전: 외부 실행이 없었습니다. 의도를 버리고 Task를 되돌립니다.
                if !run.status.is_terminal() {
                    let target = if run.status.can_transition_to(RunStatus::Cancelled) {
                        RunStatus::Cancelled
                    } else {
                        RunStatus::Unknown
                    };
                    run_transition(&mut run, target, &work.now, &mut work.writes)?;
                }
                self.settle_intent(&run.run_id, &mut work.writes)?;
                self.release_run_locks(&run.run_id, &mut work.writes)?;
                if is_current {
                    match task.status {
                        TaskStatus::Running => {
                            work.transition(id, TaskStatus::Ready)?;
                            report.returned_to_ready.push(id.to_string());
                        }
                        TaskStatus::Cancelling => work.finish(store, id, TaskStatus::Cancelled)?,
                        _ => {}
                    }
                }
                self.commit(work.into_writes())?;
                continue;
            }
            if run.status == RunStatus::Unknown {
                // 이미 결과 불명으로 확정한 Run입니다. 사용자·마스터의 대조 판단을 기다립니다.
                if is_current {
                    match task.status {
                        TaskStatus::NeedsReconciliation => {
                            report.needs_reconciliation.push(id.to_string());
                        }
                        TaskStatus::Interrupted => report.interrupted.push(id.to_string()),
                        _ => {}
                    }
                }
                continue;
            }

            // 전달한 뒤: 실제 프로세스를 대조합니다. PID만으로 확정하지 않고 증거 전체를 넘깁니다 (§14.3).
            let alive = match &intent.process {
                Some(evidence) => self.executor.probe(evidence).await,
                None => None,
            };
            // 살아 있어도 이 Core는 출력을 다시 받을 수 없습니다(P1 범위). 죽었거나 모르면 결과를 확정할 수 없습니다.
            let can_stay_interrupted = run.status == RunStatus::Interrupted
                || run.status.can_transition_to(RunStatus::Interrupted);
            let run_target = if alive == Some(true) && can_stay_interrupted {
                RunStatus::Interrupted
            } else {
                RunStatus::Unknown
            };
            if run.status != run_target {
                run_transition(&mut run, run_target, &work.now, &mut work.writes)?;
            }
            if is_current {
                let next = match (task.status, run_target) {
                    (TaskStatus::Running, RunStatus::Interrupted) => Some(TaskStatus::Interrupted),
                    (TaskStatus::Running, _) => Some(TaskStatus::NeedsReconciliation),
                    (TaskStatus::Cancelling, _) => Some(TaskStatus::NeedsReconciliation),
                    (
                        TaskStatus::WaitingChild
                        | TaskStatus::WaitingMaster
                        | TaskStatus::WaitingUser,
                        _,
                    ) => Some(TaskStatus::Interrupted),
                    (TaskStatus::Interrupted, RunStatus::Unknown) => {
                        Some(TaskStatus::NeedsReconciliation)
                    }
                    _ => None,
                };
                if let Some(next) = next {
                    work.transition(id, next)?;
                    if next == TaskStatus::Interrupted {
                        report.interrupted.push(id.to_string());
                    } else {
                        report.needs_reconciliation.push(id.to_string());
                    }
                } else if task.status == TaskStatus::Interrupted {
                    report.interrupted.push(id.to_string());
                } else if task.status == TaskStatus::NeedsReconciliation {
                    report.needs_reconciliation.push(id.to_string());
                }
            }
            self.commit(work.into_writes())?;
        }

        // 주인이 없거나 확정 종료한 Run의 잠금은 풉니다. 결과를 모르는 Run의 잠금은 대조가 끝날 때까지 둡니다.
        let mut writes = Vec::new();
        for lock in store.locks().map_err(store_error)? {
            let settled = match store.run(&lock.holder_run_id).map_err(store_error)? {
                None => true,
                Some(run) => matches!(
                    run.status,
                    RunStatus::Succeeded | RunStatus::Failed | RunStatus::Cancelled
                ),
            };
            if settled {
                report.released_locks.push(lock.lock_id.clone());
                writes.push(StateWrite::ReleaseLock {
                    lock_id: lock.lock_id,
                    owner_epoch: lock.owner_epoch,
                });
            }
        }
        self.commit(writes)?;
        Ok(report)
    }
}

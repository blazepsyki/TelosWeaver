//! PH1-SCHED 시험용 대역입니다. 저장소는 실제 `SqliteStore`를 쓰고, 실행 감독·문맥·Workspace·정책만 대역입니다.
#![allow(dead_code)]

use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use async_trait::async_trait;
use telos_core::{
    context::{ContextBuilder, ContextError},
    persistence::{SqliteStore, StateWrite, Store, StoreError, WriteBatch},
    scheduler::{
        Actor, ChildProfiles, CompletionRule, ExecutorError, RunAdmission, RunAuthorization,
        RunExecutor, RunReport, Scheduler, SchedulerDeps, SchedulerPolicy, TaskScheduler,
    },
    workspace::{WorkspaceError, WorkspaceManager},
};
use telos_schema::*;

use super::persistence as records;

/// 2026-11-16T00:00:00Z
pub const START: u64 = 1_794_787_200;
pub const PLAN: &str = "plan-1";

type StartResult = Result<(RunStarted, Option<ProcessEvidence>), ExecutorError>;

#[derive(Default)]
pub struct FakeExecutor {
    pub started: Mutex<Vec<(TaskSpec, RunContext)>>,
    pub cancelled: Mutex<Vec<String>>,
    /// 다음 시작 요청들의 결과. 비어 있으면 성공입니다.
    pub next_start: Mutex<VecDeque<ExecutorError>>,
    pub alive: Mutex<Option<bool>>,
    pub probed: Mutex<Vec<ProcessEvidence>>,
}

impl FakeExecutor {
    pub fn fail_next(&self, error: ExecutorError) {
        self.next_start.lock().unwrap().push_back(error);
    }

    pub fn started_runs(&self) -> Vec<String> {
        self.started
            .lock()
            .unwrap()
            .iter()
            .map(|(_, context)| context.run_id.clone())
            .collect()
    }

    pub fn last_context(&self) -> RunContext {
        self.started.lock().unwrap().last().unwrap().1.clone()
    }
}

pub fn evidence(run_id: &str) -> ProcessEvidence {
    ProcessEvidence {
        host_id: "host-local".into(),
        pid: 4242,
        process_started_at: "2026-11-16T00:00:00Z".into(),
        run_token_digest: format!("sha256:{run_id}"),
    }
}

#[async_trait]
impl RunExecutor for FakeExecutor {
    async fn start(
        &self,
        task: &TaskSpec,
        context: &RunContext,
        _snapshot: &ContextSnapshot,
    ) -> StartResult {
        if let Some(error) = self.next_start.lock().unwrap().pop_front() {
            return Err(error);
        }
        self.started
            .lock()
            .unwrap()
            .push((task.clone(), context.clone()));
        Ok((
            RunStarted {
                run_id: context.run_id.clone(),
                native_session_id: Some(format!("native-{}", context.run_id)),
                native_turn_id: Some("turn-1".into()),
            },
            Some(evidence(&context.run_id)),
        ))
    }

    async fn cancel(&self, run_id: &str) -> Result<bool, ExecutorError> {
        self.cancelled.lock().unwrap().push(run_id.to_string());
        Ok(true)
    }

    async fn probe(&self, evidence: &ProcessEvidence) -> Option<bool> {
        self.probed.lock().unwrap().push(evidence.clone());
        *self.alive.lock().unwrap()
    }
}

#[derive(Default)]
pub struct FakeContext {
    pub blocking: AtomicBool,
    /// 요청 전략 대신 쓸 전략(대체 상황)
    pub fallback: Mutex<Option<SessionMode>>,
    pub fail: AtomicBool,
}

#[async_trait]
impl ContextBuilder for FakeContext {
    async fn build(
        &self,
        task: &TaskRecord,
        run: &RunContext,
    ) -> Result<ContextSnapshot, ContextError> {
        if self.fail.load(Ordering::SeqCst) {
            return Err(ContextError::Unresolvable {
                reason: "input artifact is gone".into(),
            });
        }
        let mut snapshot = records::snapshot(&run.run_id, &task.spec.id);
        snapshot.snapshot_id = run.context_snapshot_id.clone();
        snapshot.strategy = run.session.mode;
        if let Some(mode) = *self.fallback.lock().unwrap() {
            snapshot.fallback = Some(StrategyFallback {
                requested: run.session.mode,
                used: mode,
                reason: "requested strategy is unsupported".into(),
                capability: Some(capability_keys::NATIVE_FORK.into()),
            });
            snapshot.strategy = mode;
            snapshot.capsule = Some(records::artifact_ref("capsule", "v1"));
        }
        if self.blocking.load(Ordering::SeqCst) {
            snapshot.issues.push(InputIssue {
                kind: InputIssueKind::Missing,
                subject: "draft".into(),
                detail: "required input has no producer".into(),
                blocking: true,
            });
        }
        Ok(snapshot)
    }

    async fn write_capsule(
        &self,
        _task: &TaskRecord,
        _run_id: &str,
        _content: CapsuleContent,
    ) -> Result<ArtifactVersionRef, ContextError> {
        Err(ContextError::Storage {
            message: "not used".into(),
        })
    }

    fn render_prompt(&self, _snapshot: &ContextSnapshot) -> String {
        String::new()
    }
}

/// 쓰기 Task마다 Workspace Writer 잠금을 저장소에 잡습니다. 읽기 전용 Task는 잠금 없이 준비합니다.
pub struct FakeWorkspace {
    pub store: Arc<SqliteStore>,
    pub prepared: Mutex<Vec<String>>,
}

impl FakeWorkspace {
    fn id_for(task: &TaskRecord) -> String {
        task.spec
            .workspace
            .workspace_id
            .clone()
            .unwrap_or_else(|| format!("ws-{}", task.spec.id))
    }
}

#[async_trait]
impl WorkspaceManager for FakeWorkspace {
    async fn prepare(
        &self,
        task: &TaskRecord,
        run_id: &str,
    ) -> Result<WorkspaceRecord, WorkspaceError> {
        let workspace_id = Self::id_for(task);
        if !task.spec.workspace.read_only {
            let lock = ResourceLock {
                lock_id: format!("lock-{run_id}"),
                resource: LockResource::Workspace {
                    workspace_id: workspace_id.clone(),
                },
                mode: LockMode::Writer,
                holder_task_id: task.spec.id.clone(),
                holder_run_id: run_id.to_string(),
                owner_epoch: 1,
                process: None,
                acquired_at: records::AT.into(),
                expires_at: None,
            };
            match self
                .store
                .apply(WriteBatch::new(vec![StateWrite::AcquireLock { lock }]))
            {
                Ok(_) => {}
                Err(StoreError::LockConflict { resource }) => {
                    return Err(WorkspaceError::LockConflict { resource });
                }
                Err(other) => {
                    return Err(WorkspaceError::Storage {
                        message: other.to_string(),
                    });
                }
            }
        }
        self.prepared.lock().unwrap().push(run_id.to_string());
        let mut record = records::workspace(&workspace_id, &task.spec.id);
        record.created_by_run_id = run_id.to_string();
        record.read_only = task.spec.workspace.read_only;
        Ok(record)
    }

    async fn diff(&self, _workspace_id: &str) -> Result<DiffSummary, WorkspaceError> {
        Err(WorkspaceError::Git {
            message: "not used".into(),
        })
    }

    async fn freeze(&self, _workspace_id: &str, _message: &str) -> Result<String, WorkspaceError> {
        Err(WorkspaceError::Git {
            message: "not used".into(),
        })
    }

    async fn merge(
        &self,
        _request: SourceMergeRequest,
    ) -> Result<SourceMergeResult, WorkspaceError> {
        Err(WorkspaceError::Git {
            message: "not used".into(),
        })
    }

    async fn release(&self, _workspace_id: &str) -> Result<(), WorkspaceError> {
        Ok(())
    }

    async fn acquire(
        &self,
        _resource: LockResource,
        _mode: LockMode,
        _holder_task_id: &str,
    ) -> Result<ResourceLock, WorkspaceError> {
        Err(WorkspaceError::Storage {
            message: "not used".into(),
        })
    }

    async fn release_lock(&self, _lock: &ResourceLock) -> Result<(), WorkspaceError> {
        Ok(())
    }
}

pub struct FakePolicy {
    pub profile: Mutex<BudgetProfile>,
    pub require_result_approval: AtomicBool,
    pub deny_children: AtomicBool,
}

pub fn budget_profile(limits: ControlLimits) -> BudgetProfile {
    BudgetProfile {
        id: "default".into(),
        limits,
        run_timeout_secs: 600,
        wait_timeout_secs: 300,
        plan_timeout_secs: None,
        max_tokens: None,
        max_cost_usd: None,
        require_exact_cost: false,
        master_reserve_tasks: 0,
        master_reserve_tokens: None,
    }
}

impl SchedulerPolicy for FakePolicy {
    fn budget_profile(&self, profile_id: &str) -> Option<BudgetProfile> {
        let profile = self.profile.lock().unwrap().clone();
        (profile.id == profile_id).then_some(profile)
    }

    fn child_profiles(
        &self,
        parent: &TaskSpec,
        _input: &DelegateTaskInput,
    ) -> Result<ChildProfiles, String> {
        if self.deny_children.load(Ordering::SeqCst) {
            return Err("this task may not delegate".into());
        }
        Ok(ChildProfiles {
            permission_profile_id: parent.permission_profile_id.clone(),
            budget_profile_id: parent.budget_profile_id.clone(),
            completion_policy_id: parent.completion_policy_id.clone(),
        })
    }

    fn authorize_run(
        &self,
        task: &TaskSpec,
        _session: &SessionRequest,
        source: Option<&SessionRecord>,
    ) -> Result<RunAuthorization, String> {
        if task.agent.provider == "forbidden" {
            return Err("provider is not allowed".into());
        }
        if source.is_some_and(|session| session.owner_task_id != task.id) {
            return Err("session belongs to another task".into());
        }
        Ok(RunAuthorization {
            policy_revision: "policy-1".into(),
        })
    }

    fn completion_rule(&self, _task: &TaskSpec) -> CompletionRule {
        CompletionRule {
            require_result_approval: self.require_result_approval.load(Ordering::SeqCst),
        }
    }
}

pub struct Harness {
    pub temp: records::TempDir,
    pub store: Arc<SqliteStore>,
    pub scheduler: TaskScheduler,
    pub executor: Arc<FakeExecutor>,
    pub context: Arc<FakeContext>,
    pub workspace: Arc<FakeWorkspace>,
    pub policy: Arc<FakePolicy>,
    pub clock: Arc<AtomicU64>,
}

pub fn limits(concurrency: u32) -> ControlLimits {
    ControlLimits {
        provider_concurrency: concurrency,
        max_delegation_depth: 2,
        max_plan_tasks: 8,
        execution_retry_limit: 1,
        rework_limit: 1,
    }
}

impl Harness {
    pub fn new(name: &str) -> Self {
        Self::with_limits(name, limits(4))
    }

    pub fn with_limits(name: &str, limits: ControlLimits) -> Self {
        let temp = records::TempDir::new(&format!("sched-{name}"));
        let harness = Self::open(temp, limits);
        harness
            .store
            .apply(records::batch(vec![StateWrite::Plan {
                plan: plan(),
                expected_version: None,
            }]))
            .unwrap();
        harness
    }

    /// 같은 DB 위에 Scheduler를 새로 세웁니다(Core 재시작).
    pub fn restart(self) -> Self {
        let Self {
            temp,
            store,
            scheduler,
            executor,
            context,
            workspace,
            policy,
            clock,
        } = self;
        let limits = policy.profile.lock().unwrap().limits;
        // 이전 Core가 쥔 저장소 참조를 모두 놓아야 DB를 다시 열 수 있습니다(단일 소유권).
        drop((scheduler, executor, context, workspace, policy, clock));
        drop(store);
        Self::open(temp, limits)
    }

    fn open(temp: records::TempDir, limits: ControlLimits) -> Self {
        let store = Arc::new(temp.open());
        let executor = Arc::new(FakeExecutor::default());
        let context = Arc::new(FakeContext::default());
        let workspace = Arc::new(FakeWorkspace {
            store: Arc::clone(&store),
            prepared: Mutex::new(vec![]),
        });
        let policy = Arc::new(FakePolicy {
            profile: Mutex::new(budget_profile(limits)),
            require_result_approval: AtomicBool::new(false),
            deny_children: AtomicBool::new(false),
        });
        let clock = Arc::new(AtomicU64::new(START));
        let now = Arc::clone(&clock);
        let scheduler = TaskScheduler::new(SchedulerDeps {
            store: store.clone(),
            executor: executor.clone(),
            context: context.clone(),
            workspace: workspace.clone(),
            policy: policy.clone(),
            task_mcp: None,
            clock: Some(Arc::new(move || now.load(Ordering::SeqCst))),
        });
        Self {
            temp,
            store,
            scheduler,
            executor,
            context,
            workspace,
            policy,
            clock,
        }
    }

    pub fn advance(&self, seconds: u64) {
        self.clock.fetch_add(seconds, Ordering::SeqCst);
    }

    pub fn task(&self, id: &str) -> TaskRecord {
        self.store.task(id).unwrap().unwrap()
    }

    pub fn status(&self, id: &str) -> TaskStatus {
        self.task(id).status
    }

    pub fn run(&self, id: &str) -> RunRecord {
        self.store.run(id).unwrap().unwrap()
    }

    pub fn ledger(&self) -> BudgetLedger {
        self.store.budget(PLAN).unwrap().unwrap().value
    }

    pub async fn register(&self, specs: Vec<TaskSpec>) -> Vec<TaskRecord> {
        let key = format!("register-{}", specs[0].id);
        self.scheduler
            .register_tasks(Actor::User, specs, &key)
            .await
            .unwrap()
    }

    /// `Ready` Task를 실행해 Run ID를 돌려줍니다.
    pub async fn start(&self, task_id: &str) -> String {
        let key = format!(
            "run-{task_id}-{}",
            self.store.task_runs(task_id).unwrap().len() + 1
        );
        match self
            .scheduler
            .request_run(Actor::User, task_id, fresh(), &key)
            .await
            .unwrap()
        {
            RunAdmission::Started { run_id } => run_id,
            other => panic!("expected a started run, got {other:?}"),
        }
    }

    pub async fn finish(&self, run_id: &str, status: RunStatus) {
        self.scheduler
            .on_run_finished(report(run_id, status, None))
            .await
            .unwrap();
    }
}

pub fn plan() -> Plan {
    let mut plan = records::plan(PLAN);
    plan.project_id = fixtures::PROJECT_ID.into();
    plan
}

/// 쓰기 Workspace가 없는 읽기 전용 Task 계약입니다.
pub fn spec(id: &str, dependencies: &[&str]) -> TaskSpec {
    let mut spec = fixtures::task_spec();
    spec.id = id.into();
    spec.plan_id = PLAN.into();
    spec.dependencies = dependencies.iter().map(|d| d.to_string()).collect();
    spec
}

pub fn writer_spec(id: &str, workspace_id: &str) -> TaskSpec {
    let mut spec = spec(id, &[]);
    spec.workspace = WorkspaceRequest {
        mode: WorkspaceMode::Directory,
        workspace_id: Some(workspace_id.into()),
        host_id: fixtures::HOST_ID.into(),
        read_only: false,
    };
    spec
}

pub fn child_of(id: &str, parent: &str) -> TaskSpec {
    let mut spec = spec(id, &[]);
    spec.parent_task_id = Some(parent.into());
    spec
}

pub fn fresh() -> SessionRequest {
    SessionRequest {
        mode: SessionMode::Fresh,
        source_session_id: None,
        source_turn_id: None,
        capsule_version_id: None,
    }
}

pub fn report(run_id: &str, status: RunStatus, failure: Option<FailureKind>) -> RunReport {
    RunReport {
        run_id: run_id.into(),
        status,
        failure,
        message: None,
        native_session_id: None,
        native_turn_ids: vec![],
    }
}

pub fn delegate_input(key: &str, wait: bool) -> DelegateTaskInput {
    let base = fixtures::task_spec();
    DelegateTaskInput {
        idempotency_key: key.into(),
        r#type: TaskType::General,
        objective: "review the draft".into(),
        constraints: vec![],
        agent: base.agent,
        workspace: base.workspace,
        inputs: vec![],
        outputs: base.outputs,
        validation: vec![],
        deadline: None,
        wait_for_result: wait,
        cancel_propagation: CancelPropagation::Descendants,
    }
}

pub fn required_check(id: &str) -> ValidationContract {
    ValidationContract {
        id: id.into(),
        revision: 1,
        kind: ValidationKind::File,
        target_output_names: vec!["answer".into()],
        parameters: JsonObject::new(),
        required: true,
    }
}

/// VALIDATION이 하듯 판정을 저장합니다.
pub fn store_verdict(
    store: &SqliteStore,
    validation_id: &str,
    task_id: &str,
    contract_id: &str,
    verdict: ValidationVerdict,
) {
    let mut job = records::validation(validation_id, task_id);
    job.contract = required_check(contract_id);
    job.status = ValidationJobStatus::Finished;
    job.verdict = Some(verdict);
    store
        .apply(records::batch(vec![StateWrite::Validation {
            job,
            expected_version: None,
        }]))
        .unwrap();
}

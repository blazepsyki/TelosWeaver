//! Plan 하나의 마스터 제어: 결정 회수·검증·집행·기록과 마스터 Turn 실행.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use futures::{StreamExt, stream::BoxStream};
use serde_json::{Value, json};
use telos_schema::{
    ControlLimits, DecisionContext, DecisionEffect, DecisionOutcome, DecisionPayload,
    DecisionRejection, MasterDecision, ProviderEvent, ProviderEventPayload, RejectionCode,
    RunStatus, SessionMode, SessionRequest, TaskSpec, TaskStatus, TaskSummary, TextChannel,
    WorkspaceMode,
    api::{ApiError, ApiErrorCode, RunsInspectResult, RunsStartParams, RunsStartResult},
};

use super::output::{master_prompt, parse_master_output};
use crate::{
    execution::{ExecutionSupervisor, new_id},
    policy::{Caller, TrustedLookup, validate},
};

/// 결정 집행과 마스터 Turn 실행에 쓰는 Run 경로입니다. 실제 구현은 `ExecutionSupervisor`입니다.
#[async_trait]
pub trait RunExecutor: Send + Sync {
    async fn start(&self, params: RunsStartParams) -> Result<RunsStartResult, ApiError>;
    async fn cancel(&self, run_id: &str) -> Result<bool, ApiError>;
    fn inspect(&self, run_id: &str) -> Result<RunsInspectResult, ApiError>;
    fn events(
        &self,
        run_id: &str,
    ) -> Result<BoxStream<'static, Result<ProviderEvent, ApiError>>, ApiError>;
}

#[async_trait]
impl RunExecutor for Arc<ExecutionSupervisor> {
    async fn start(&self, params: RunsStartParams) -> Result<RunsStartResult, ApiError> {
        ExecutionSupervisor::start(self, params).await
    }
    async fn cancel(&self, run_id: &str) -> Result<bool, ApiError> {
        Ok(ExecutionSupervisor::cancel(self, run_id).await?.accepted)
    }
    fn inspect(&self, run_id: &str) -> Result<RunsInspectResult, ApiError> {
        ExecutionSupervisor::inspect(self, run_id)
    }
    fn events(
        &self,
        run_id: &str,
    ) -> Result<BoxStream<'static, Result<ProviderEvent, ApiError>>, ApiError> {
        self.subscribe(run_id, 0)
    }
}

/// Plan의 권한 상한과 마스터 실행 조건입니다. 신뢰된 정책에서 만들어 넣습니다.
#[derive(Debug, Clone)]
pub struct MasterSettings {
    pub project_id: String,
    pub plan_id: String,
    pub host_id: String,
    pub policy_revision: String,
    pub limits: ControlLimits,
    pub allowed_providers: Vec<String>,
    pub allowed_workspace_modes: Vec<WorkspaceMode>,
    pub allowed_permission_profile_ids: Vec<String>,
    pub allowed_budget_profile_ids: Vec<String>,
    /// 마스터 Provider (배정표 §6: Phase 0 임시 Claude). 고정하지 않습니다 (MASTER-01).
    pub master_provider: String,
    pub master_model: Option<String>,
    /// 마스터 Run이 읽기 전용으로 쓰는 승인된 Workspace.
    pub master_workspace_id: String,
    pub master_permission_profile_id: String,
    /// 마스터가 새 Task를 만들 때 복사하는 Worker Task 틀.
    pub worker_template: TaskSpec,
    /// 마스터 Turn 하나의 제한 시간.
    pub turn_timeout: Duration,
}

/// 마스터 Turn의 결과입니다.
#[derive(Debug, Clone)]
pub struct MasterTurn {
    pub run_id: String,
    pub run_status: RunStatus,
    pub native_session_id: Option<String>,
    pub text: String,
    /// 회수·검증·집행 결과. 결정을 해석하지 못하면 `invalid_payload` 거부입니다.
    pub outcome: DecisionOutcome,
}

/// Plan의 Task 상태 보기입니다.
#[derive(Debug, Clone)]
pub struct PlanTaskView {
    pub task: TaskSpec,
    pub status: TaskStatus,
    pub run_id: Option<String>,
}

struct PlanTask {
    spec: TaskSpec,
    status: TaskStatus,
    run_id: Option<String>,
    depth: u32,
}

struct PlanState {
    plan_revision: u32,
    controller_epoch: u64,
    tasks: Vec<PlanTask>,
    /// 멱등 키 → (결정 내용 지문, 처음 결과).
    decisions: HashMap<String, (Value, DecisionOutcome)>,
    log: Vec<DecisionOutcome>,
    active_master_run: Option<String>,
    master_session: Option<String>,
    turns: u32,
}

pub struct MasterController {
    settings: MasterSettings,
    executor: Arc<dyn RunExecutor>,
    lookup: Arc<dyn TrustedLookup>,
    state: Mutex<PlanState>,
}

impl MasterController {
    pub fn new(
        settings: MasterSettings,
        executor: Arc<dyn RunExecutor>,
        lookup: Arc<dyn TrustedLookup>,
    ) -> Self {
        Self {
            settings,
            executor,
            lookup,
            state: Mutex::new(PlanState {
                plan_revision: 1,
                controller_epoch: 1,
                tasks: Vec::new(),
                decisions: HashMap::new(),
                log: Vec::new(),
                active_master_run: None,
                master_session: None,
                turns: 0,
            }),
        }
    }

    pub fn settings(&self) -> &MasterSettings {
        &self.settings
    }

    /// 현재 활성 마스터 Run. 이 Run에서 회수한 결정만 받습니다.
    pub fn active_master_run(&self) -> Option<String> {
        self.state.lock().unwrap().active_master_run.clone()
    }

    /// 마스터를 교체·재개할 때 Epoch를 올립니다. 이전 마스터의 늦은 결정은 거부됩니다 (§4.1).
    pub fn bump_epoch(&self) -> u64 {
        let mut state = self.state.lock().unwrap();
        state.controller_epoch += 1;
        state.active_master_run = None;
        state.controller_epoch
    }

    pub fn tasks(&self) -> Vec<PlanTaskView> {
        self.state
            .lock()
            .unwrap()
            .tasks
            .iter()
            .map(|t| PlanTaskView {
                task: t.spec.clone(),
                status: t.status,
                run_id: t.run_id.clone(),
            })
            .collect()
    }

    /// 감사 기록: 처리한 결정 결과를 순서대로 돌려줍니다(재생 결과 포함).
    pub fn decision_log(&self) -> Vec<DecisionOutcome> {
        self.state.lock().unwrap().log.clone()
    }

    /// 실행 중인 Task의 상태를 Run 결과로 갱신합니다. Run 성공은 Task 완료가 아니라 `Executed`입니다 (§9.4).
    pub fn refresh(&self) {
        let mut state = self.state.lock().unwrap();
        for task in &mut state.tasks {
            let Some(run_id) = &task.run_id else { continue };
            let Ok(inspect) = self.executor.inspect(run_id) else {
                continue;
            };
            task.status = match inspect.status {
                RunStatus::Succeeded => TaskStatus::Executed,
                RunStatus::Failed => TaskStatus::Failed,
                RunStatus::Cancelled => TaskStatus::Cancelled,
                RunStatus::Interrupted | RunStatus::Unknown => TaskStatus::Interrupted,
                RunStatus::Cancelling => TaskStatus::Cancelling,
                RunStatus::Starting | RunStatus::Running | RunStatus::Waiting => {
                    TaskStatus::Running
                }
            };
        }
    }

    /// 결정 검증 입력 Snapshot입니다.
    pub fn context(&self) -> DecisionContext {
        let state = self.state.lock().unwrap();
        self.context_of(&state)
    }

    fn context_of(&self, state: &PlanState) -> DecisionContext {
        let s = &self.settings;
        DecisionContext {
            project_id: s.project_id.clone(),
            plan_id: s.plan_id.clone(),
            plan_revision: state.plan_revision,
            controller_epoch: state.controller_epoch,
            policy_revision: s.policy_revision.clone(),
            limits: s.limits,
            tasks: state
                .tasks
                .iter()
                .map(|t| TaskSummary {
                    id: t.spec.id.clone(),
                    revision: t.spec.revision,
                    status: t.status,
                    parent_task_id: t.spec.parent_task_id.clone(),
                    delegation_depth: t.depth,
                })
                .collect(),
            allowed_providers: s.allowed_providers.clone(),
            allowed_host_ids: vec![s.host_id.clone()],
            allowed_workspace_modes: s.allowed_workspace_modes.clone(),
            allowed_permission_profile_ids: s.allowed_permission_profile_ids.clone(),
            allowed_budget_profile_ids: s.allowed_budget_profile_ids.clone(),
        }
    }

    /// 결정을 검증·집행하고 결과를 기록합니다 (§4.2 집행 순서).
    pub async fn decide(&self, decision: MasterDecision) -> DecisionOutcome {
        let fingerprint = json!({
            "planId": decision.plan_id,
            "masterRunId": decision.master_run_id,
            "kind": decision.kind,
            "payload": decision.payload,
        });
        let validated = {
            let mut state = self.state.lock().unwrap();
            if let Some((seen, first)) = state.decisions.get(&decision.idempotency_key) {
                let outcome = if *seen == fingerprint {
                    replayed(first.clone())
                } else {
                    rejected(
                        &decision,
                        DecisionRejection {
                            code: RejectionCode::IdempotencyConflict,
                            message: "idempotency key was used with different content".into(),
                            field: None,
                        },
                    )
                };
                state.log.push(outcome.clone());
                return outcome;
            }
            let context = self.context_of(&state);
            let active = state.active_master_run.clone().unwrap_or_default();
            validate(
                &decision,
                &context,
                Caller {
                    active_master_run_id: &active,
                },
                self.lookup.as_ref(),
            )
        };
        let outcome = match validated {
            Err(rejection) => rejected(&decision, rejection),
            Ok(payload) => match self.execute(&decision, payload).await {
                Ok(effects) => DecisionOutcome::Accepted {
                    decision_id: decision.decision_id.clone(),
                    idempotency_key: decision.idempotency_key.clone(),
                    effects,
                    replayed: false,
                    recorded_at: crate::integration::timestamp(),
                },
                Err(rejection) => rejected(&decision, rejection),
            },
        };
        let mut state = self.state.lock().unwrap();
        state.decisions.insert(
            decision.idempotency_key.clone(),
            (fingerprint, outcome.clone()),
        );
        state.log.push(outcome.clone());
        outcome
    }

    async fn execute(
        &self,
        decision: &MasterDecision,
        payload: DecisionPayload,
    ) -> Result<Vec<DecisionEffect>, DecisionRejection> {
        match payload {
            DecisionPayload::CreateTasks(p) => {
                let mut state = self.state.lock().unwrap();
                let mut effects = Vec::new();
                for spec in p.tasks {
                    let depth = spec
                        .parent_task_id
                        .as_ref()
                        .and_then(|parent| state.tasks.iter().find(|t| &t.spec.id == parent))
                        .map_or(0, |parent| parent.depth + 1);
                    effects.push(DecisionEffect::TaskCreated {
                        task_id: spec.id.clone(),
                        task_revision: spec.revision,
                    });
                    state.tasks.push(PlanTask {
                        spec,
                        status: TaskStatus::Ready,
                        run_id: None,
                        depth,
                    });
                }
                // Task 구성이 바뀌었으므로 Plan Revision을 올립니다. 이전 Revision의 결정은 거부됩니다.
                state.plan_revision += 1;
                Ok(effects)
            }
            DecisionPayload::RequestRun(p) => {
                let spec = {
                    let state = self.state.lock().unwrap();
                    state
                        .tasks
                        .iter()
                        .find(|t| t.spec.id == p.task_id)
                        .map(|t| t.spec.clone())
                        .expect("validated task exists")
                };
                let started = self
                    .executor
                    .start(RunsStartParams {
                        task: spec,
                        session: p.session,
                        idempotency_key: format!("{}:run", decision.idempotency_key),
                    })
                    .await
                    .map_err(run_rejection)?;
                let mut state = self.state.lock().unwrap();
                if let Some(task) = state.tasks.iter_mut().find(|t| t.spec.id == p.task_id) {
                    task.status = TaskStatus::Running;
                    task.run_id = Some(started.run_id.clone());
                }
                Ok(vec![DecisionEffect::RunRequested {
                    task_id: p.task_id,
                    run_id: started.run_id,
                }])
            }
            DecisionPayload::RequestCancel(p) => {
                let run_id = {
                    let state = self.state.lock().unwrap();
                    state
                        .tasks
                        .iter()
                        .find(|t| t.spec.id == p.task_id)
                        .and_then(|t| t.run_id.clone())
                };
                // 취소 접수와 실제 종료는 다릅니다. 최종 상태는 `refresh`가 Run 결과로 정합니다.
                let accepted = match &run_id {
                    Some(run_id) => self.executor.cancel(run_id).await.map_err(run_rejection)?,
                    None => true,
                };
                let mut state = self.state.lock().unwrap();
                if let Some(task) = state.tasks.iter_mut().find(|t| t.spec.id == p.task_id) {
                    task.status = match (&run_id, accepted) {
                        (None, _) => TaskStatus::Cancelled,
                        (Some(_), true) => TaskStatus::Cancelling,
                        (Some(_), false) => task.status,
                    };
                }
                Ok(vec![DecisionEffect::CancelRequested { task_id: p.task_id }])
            }
            _ => unreachable!("validator rejects kinds that are not executed in Phase 0"),
        }
    }

    /// 마스터 Turn 하나를 실행하고, 회수한 결정을 검증·집행합니다.
    /// 첫 Turn은 새 세션, 이후 Turn은 Resume으로 같은 대화에 집행 결과를 전달합니다(결과 왕복).
    pub async fn turn(&self, message: &str) -> Result<MasterTurn, ApiError> {
        self.refresh();
        let (task, session, state_json) = {
            let mut state = self.state.lock().unwrap();
            state.turns += 1;
            let task = self.master_task(state.turns);
            let session = match &state.master_session {
                None => SessionRequest {
                    mode: SessionMode::Fresh,
                    source_session_id: None,
                    source_turn_id: None,
                    capsule_version_id: None,
                },
                Some(id) => SessionRequest {
                    mode: SessionMode::Resume,
                    source_session_id: Some(id.clone()),
                    source_turn_id: None,
                    capsule_version_id: None,
                },
            };
            (task, session, self.state_json(&state))
        };
        let prompt = master_prompt(message, &state_json, &self.settings.worker_template);
        let mut task = task;
        task.objective = prompt;
        let started = self
            .executor
            .start(RunsStartParams {
                idempotency_key: format!("{}:master:{}", self.settings.plan_id, task.revision),
                task,
                session,
            })
            .await?;
        self.state.lock().unwrap().active_master_run = Some(started.run_id.clone());

        let (text, native_session_id) = self.collect(&started.run_id).await?;
        let run_status = self.final_status(&started.run_id).await?;
        if let Some(id) = &native_session_id {
            self.state.lock().unwrap().master_session = Some(id.clone());
        }

        let decision = {
            let state = self.state.lock().unwrap();
            (state.plan_revision, state.controller_epoch, state.turns)
        };
        let (plan_revision, controller_epoch, turn) = decision;
        let decision_id = new_id("decision")?;
        let outcome = match parse_master_output(&text) {
            Ok(output) => {
                self.decide(MasterDecision {
                    decision_id,
                    plan_id: self.settings.plan_id.clone(),
                    plan_revision,
                    master_run_id: started.run_id.clone(),
                    controller_epoch,
                    idempotency_key: format!("{}:turn:{turn}", self.settings.plan_id),
                    kind: output.kind,
                    payload: output.payload,
                    rationale: output.rationale,
                    policy_revision: self.settings.policy_revision.clone(),
                })
                .await
            }
            Err(message) => {
                let outcome = DecisionOutcome::Rejected {
                    decision_id,
                    idempotency_key: format!("{}:turn:{turn}", self.settings.plan_id),
                    rejection: DecisionRejection {
                        code: RejectionCode::InvalidPayload,
                        message,
                        field: None,
                    },
                    replayed: false,
                    recorded_at: crate::integration::timestamp(),
                };
                self.state.lock().unwrap().log.push(outcome.clone());
                outcome
            }
        };
        Ok(MasterTurn {
            run_id: started.run_id,
            run_status,
            native_session_id,
            text,
            outcome,
        })
    }

    /// 마스터 제어용 Task입니다(§4.1: `type: general`, `role: master`). 읽기 전용 Workspace에서 실행합니다.
    fn master_task(&self, turn: u32) -> TaskSpec {
        let s = &self.settings;
        let mut task = s.worker_template.clone();
        task.id = format!("{}-master", s.plan_id);
        task.revision = turn;
        task.plan_id = s.plan_id.clone();
        task.project_id = s.project_id.clone();
        task.parent_task_id = None;
        task.r#type = telos_schema::TaskType::General;
        task.constraints = vec![];
        task.dependencies = vec![];
        task.agent.provider = s.master_provider.clone();
        task.agent.model = s.master_model.clone();
        task.agent.role = "master".into();
        task.workspace.mode = WorkspaceMode::Directory;
        task.workspace.workspace_id = Some(s.master_workspace_id.clone());
        task.workspace.host_id = s.host_id.clone();
        task.workspace.read_only = true;
        task.inputs = vec![];
        task.outputs = vec![];
        task.validation = vec![];
        task.permission_profile_id = s.master_permission_profile_id.clone();
        task
    }

    fn state_json(&self, state: &PlanState) -> Value {
        let tasks: Vec<Value> = state
            .tasks
            .iter()
            .map(|t| {
                let run = t.run_id.as_ref().map(|id| {
                    let status = self.executor.inspect(id).ok().map(|i| i.status);
                    json!({"runId": id, "runStatus": status})
                });
                json!({
                    "id": t.spec.id,
                    "revision": t.spec.revision,
                    "status": t.status,
                    "objective": t.spec.objective,
                    "run": run,
                })
            })
            .collect();
        json!({"planRevision": state.plan_revision, "tasks": tasks})
    }

    async fn collect(&self, run_id: &str) -> Result<(String, Option<String>), ApiError> {
        let mut events = self.executor.events(run_id)?;
        let mut text = String::new();
        let mut native_session_id = None;
        let deadline = tokio::time::Instant::now() + self.settings.turn_timeout;
        loop {
            let next = tokio::time::timeout_at(deadline, events.next())
                .await
                .map_err(|_| api(ApiErrorCode::Unavailable, "master turn timed out"))?;
            let Some(event) = next else { break };
            let event = event?;
            if native_session_id.is_none() {
                native_session_id = event.native_session_id.clone();
            }
            if let Ok(ProviderEventPayload::TextDelta(delta)) =
                ProviderEventPayload::parse(event.kind, &event.payload)
                && delta.channel == TextChannel::Assistant
            {
                text.push_str(&delta.text);
            }
        }
        Ok((text, native_session_id))
    }

    /// 마지막 이벤트와 실행 감독의 최종 상태 갱신 경합을 짧게 기다립니다.
    async fn final_status(&self, run_id: &str) -> Result<RunStatus, ApiError> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        loop {
            let status = self.executor.inspect(run_id)?.status;
            let settled = !matches!(
                status,
                RunStatus::Starting
                    | RunStatus::Running
                    | RunStatus::Waiting
                    | RunStatus::Cancelling
            );
            if settled || tokio::time::Instant::now() >= deadline {
                return Ok(status);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

fn api(code: ApiErrorCode, message: &str) -> ApiError {
    ApiError {
        code,
        message: message.into(),
        details: None,
    }
}

fn rejected(decision: &MasterDecision, rejection: DecisionRejection) -> DecisionOutcome {
    DecisionOutcome::Rejected {
        decision_id: decision.decision_id.clone(),
        idempotency_key: decision.idempotency_key.clone(),
        rejection,
        replayed: false,
        recorded_at: crate::integration::timestamp(),
    }
}

fn replayed(first: DecisionOutcome) -> DecisionOutcome {
    match first {
        DecisionOutcome::Accepted {
            decision_id,
            idempotency_key,
            effects,
            recorded_at,
            ..
        } => DecisionOutcome::Accepted {
            decision_id,
            idempotency_key,
            effects,
            replayed: true,
            recorded_at,
        },
        DecisionOutcome::Rejected {
            decision_id,
            idempotency_key,
            rejection,
            recorded_at,
            ..
        } => DecisionOutcome::Rejected {
            decision_id,
            idempotency_key,
            rejection,
            replayed: true,
            recorded_at,
        },
    }
}

/// Run 시작·취소 실패를 결정 거부로 바꿉니다. Run 시작 시 권한 재검사(§9.1)의 거부도 여기로 옵니다.
fn run_rejection(error: ApiError) -> DecisionRejection {
    let code = match error.code {
        ApiErrorCode::Forbidden => RejectionCode::PermissionDenied,
        ApiErrorCode::Unavailable => RejectionCode::PolicyUnavailable,
        ApiErrorCode::Conflict => RejectionCode::IdempotencyConflict,
        _ => RejectionCode::CheckUnsupported,
    };
    DecisionRejection {
        code,
        message: error.message,
        field: None,
    }
}

//! Phase 0 execution supervision (Spec §7.2, §9.1, PH0-HOST).
use async_trait::async_trait;
use futures::{StreamExt, stream::BoxStream};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};
use telos_adapters::{AdapterError, AgentAdapter};
use telos_schema::{api::*, *};
use tokio::sync::watch;

pub fn api_error(code: ApiErrorCode, message: &str) -> ApiError {
    ApiError {
        code,
        message: message.into(),
        details: None,
    }
}
pub fn new_id(prefix: &str) -> Result<String, ApiError> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|_| api_error(ApiErrorCode::Internal, "secure ID generation failed"))?;
    Ok(format!(
        "{prefix}-{}",
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ))
}
/// Trusted policy output. HOST does not invent snapshots or privileges.
#[derive(Debug, Clone)]
pub struct RunPermit {
    pub workspace_id: String,
    pub context_snapshot_id: String,
    pub policy_revision: String,
    pub controller_epoch: u64,
}
#[async_trait]
pub trait RunAuthorizer: Send + Sync {
    async fn authorize(
        &self,
        task: &TaskSpec,
        session: &SessionRequest,
    ) -> Result<RunPermit, ApiError>;
}
pub struct DenyRuns;
#[async_trait]
impl RunAuthorizer for DenyRuns {
    async fn authorize(&self, _: &TaskSpec, _: &SessionRequest) -> Result<RunPermit, ApiError> {
        Err(api_error(
            ApiErrorCode::Unavailable,
            "trusted run policy is not configured",
        ))
    }
}
struct RunState {
    status: RunStatus,
    events: Vec<ProviderEvent>,
    bytes: usize,
    complete: bool,
    failure: Option<ApiError>,
}
struct RunRecord {
    adapter: Arc<dyn AgentAdapter>,
    state: Mutex<RunState>,
    changed: watch::Sender<u64>,
}
struct StartSlot {
    request: serde_json::Value,
    result: watch::Sender<Option<Result<RunsStartResult, ApiError>>>,
}
#[derive(Default)]
struct Registry {
    starts: HashMap<String, Arc<StartSlot>>,
    runs: HashMap<String, Arc<RunRecord>>,
    attempts: HashMap<(String, u32), u32>,
    request_bytes: usize,
}
pub struct ExecutionSupervisor {
    project_id: String,
    host_id: String,
    adapters: HashMap<String, Arc<dyn AgentAdapter>>,
    policy: Arc<dyn RunAuthorizer>,
    registry: Mutex<Registry>,
    max_runs: usize,
    event_bytes: Arc<AtomicUsize>,
    /// Components the integration adds to `core.health`, e.g. `rn` (REQ-019, Spec §12.3).
    health_components: Mutex<Vec<HealthComponent>>,
}
/// Reports one `core.health` component when asked; it must not block.
pub type HealthComponent = Arc<dyn Fn() -> ComponentHealth + Send + Sync>;
impl ExecutionSupervisor {
    pub fn new(
        project_id: String,
        host_id: String,
        adapters: Vec<Arc<dyn AgentAdapter>>,
        policy: Arc<dyn RunAuthorizer>,
    ) -> Self {
        Self {
            project_id,
            host_id,
            adapters: adapters.into_iter().map(|a| (a.id().into(), a)).collect(),
            policy,
            registry: Mutex::new(Registry::default()),
            max_runs: 128,
            event_bytes: Arc::new(AtomicUsize::new(0)),
            health_components: Mutex::new(Vec::new()),
        }
    }
    /// Adds a component to `core.health` after the built-in ones (REQ-019). The overall status is unchanged.
    pub fn add_health_component(&self, component: HealthComponent) {
        self.health_components.lock().unwrap().push(component);
    }
    pub fn project_id(&self) -> &str {
        &self.project_id
    }
    pub fn host_id(&self) -> &str {
        &self.host_id
    }
    pub fn health(&self) -> HealthResult {
        let added: Vec<ComponentHealth> = self
            .health_components
            .lock()
            .unwrap()
            .iter()
            .map(|c| c())
            .collect();
        let mut health = HealthResult {
            status: HealthStatus::Degraded,
            server_version: env!("CARGO_PKG_VERSION").into(),
            components: vec![
                ComponentHealth {
                    name: "execution".into(),
                    status: HealthStatus::Ok,
                    detail: Some("phase0 in-memory registry; no crash recovery".into()),
                },
                ComponentHealth {
                    name: "scheduler".into(),
                    status: HealthStatus::Unavailable,
                    detail: Some("not implemented".into()),
                },
                ComponentHealth {
                    name: "persistence".into(),
                    status: HealthStatus::Unavailable,
                    detail: Some("not implemented".into()),
                },
            ],
        };
        health.components.extend(added);
        health
    }
    pub async fn start(
        self: &Arc<Self>,
        params: RunsStartParams,
    ) -> Result<RunsStartResult, ApiError> {
        if params.idempotency_key.is_empty() || params.idempotency_key.len() > 256 {
            return Err(api_error(
                ApiErrorCode::InvalidRequest,
                "invalid idempotency key",
            ));
        }
        if params.task.project_id != self.project_id
            || params.task.workspace.host_id != self.host_id
        {
            return Err(api_error(
                ApiErrorCode::Forbidden,
                "task belongs to another project or host",
            ));
        }
        let request = serde_json::to_value(&params)
            .map_err(|_| api_error(ApiErrorCode::InvalidRequest, "invalid start request"))?;
        let request_bytes = serde_json::to_vec(&request)
            .map_err(|_| api_error(ApiErrorCode::InvalidRequest, "invalid start request"))?
            .len();
        if request_bytes > MAX_FRAME_BYTES {
            return Err(api_error(
                ApiErrorCode::FrameTooLarge,
                "start request exceeds the frame limit",
            ));
        }
        let (slot, created) = {
            let mut registry = self.registry.lock().unwrap();
            if let Some(slot) = registry.starts.get(&params.idempotency_key) {
                if slot.request != request {
                    return Err(api_error(
                        ApiErrorCode::Conflict,
                        "idempotency key has different content",
                    ));
                }
                (slot.clone(), false)
            } else {
                if registry.starts.len() >= self.max_runs
                    || registry.request_bytes.saturating_add(request_bytes) > 8 * 1024 * 1024
                {
                    return Err(api_error(ApiErrorCode::Unavailable, "run registry is full"));
                }
                let slot = Arc::new(StartSlot {
                    request,
                    result: watch::channel(None).0,
                });
                registry.request_bytes += request_bytes;
                registry
                    .starts
                    .insert(params.idempotency_key.clone(), slot.clone());
                (slot, true)
            }
        };
        if created {
            let supervisor = self.clone();
            let slot = slot.clone();
            tokio::spawn(async move {
                let result = supervisor.start_once(params).await;
                slot.result.send_replace(Some(result));
            });
        }
        let mut result = slot.result.subscribe();
        result
            .wait_for(Option::is_some)
            .await
            .map_err(|_| api_error(ApiErrorCode::Internal, "start result lost"))?
            .as_ref()
            .unwrap()
            .clone()
    }
    async fn start_once(
        self: &Arc<Self>,
        params: RunsStartParams,
    ) -> Result<RunsStartResult, ApiError> {
        let permit = self.policy.authorize(&params.task, &params.session).await?;
        if permit.workspace_id.is_empty()
            || permit.context_snapshot_id.is_empty()
            || permit.policy_revision.is_empty()
        {
            return Err(api_error(
                ApiErrorCode::Forbidden,
                "incomplete run authorization",
            ));
        }
        let adapter = self
            .adapters
            .get(&params.task.agent.provider)
            .cloned()
            .ok_or_else(|| api_error(ApiErrorCode::Unavailable, "provider is not registered"))?;
        let run_id = new_id("run")?;
        let record = Arc::new(RunRecord {
            adapter: adapter.clone(),
            state: Mutex::new(RunState {
                status: RunStatus::Starting,
                events: Vec::new(),
                bytes: 0,
                complete: false,
                failure: None,
            }),
            changed: watch::channel(0).0,
        });
        let attempt = {
            let mut registry = self.registry.lock().unwrap();
            let attempt = registry
                .attempts
                .entry((params.task.id.clone(), params.task.revision))
                .or_default();
            *attempt += 1;
            let number = *attempt;
            registry.runs.insert(run_id.clone(), record.clone());
            number
        };
        let context = RunContext {
            run_id: run_id.clone(),
            task_id: params.task.id.clone(),
            task_revision: params.task.revision,
            attempt,
            controller_epoch: permit.controller_epoch,
            host_id: self.host_id.clone(),
            workspace_id: permit.workspace_id,
            context_snapshot_id: permit.context_snapshot_id,
            policy_revision: permit.policy_revision,
            idempotency_key: params.idempotency_key,
            session: params.session,
        };
        match adapter.start_run(&params.task, &context).await {
            Ok(started) if started.run_id == run_id => {}
            Ok(_) => {
                Self::fault(&record, "adapter returned a different Run ID");
                let _ = adapter.cancel_run(&run_id).await;
                return Err(api_error(
                    ApiErrorCode::Internal,
                    "adapter returned a different Run ID",
                ));
            }
            Err(error) => {
                Self::fault(&record, "adapter start failed");
                return Err(adapter_error(error));
            }
        }
        record.state.lock().unwrap().status = RunStatus::Running;
        let started = RunsStartResult {
            run_id: run_id.clone(),
            task_id: params.task.id,
            task_revision: params.task.revision,
            attempt,
        };
        let event_bytes = self.event_bytes.clone();
        tokio::spawn(async move {
            let mut stream = adapter.events(&run_id, 0);
            while let Some(event) = stream.next().await {
                let event = match event {
                    Ok(event) => event,
                    Err(_) => {
                        Self::fault(&record, "provider event collection failed");
                        let _ = adapter.cancel_run(&run_id).await;
                        return;
                    }
                };
                let valid = {
                    let mut state = record.state.lock().unwrap();
                    let bytes = serde_json::to_vec(&event)
                        .map(|v| v.len())
                        .unwrap_or(usize::MAX);
                    if event.run_id != run_id
                        || event.sequence != state.events.len() as u64 + 1
                        || event.schema_version != PROVIDER_EVENT_SCHEMA_VERSION
                        || bytes > MAX_FRAME_BYTES.saturating_sub(1024)
                        || state.bytes.saturating_add(bytes) > 8 * 1024 * 1024
                        || state.events.len() >= 4096
                        || event_bytes
                            .try_update(Ordering::AcqRel, Ordering::Acquire, |total| {
                                total
                                    .checked_add(bytes)
                                    .filter(|total| *total <= 32 * 1024 * 1024)
                            })
                            .is_err()
                    {
                        false
                    } else {
                        state.bytes += bytes;
                        state.events.push(event);
                        match check_run_events(&state.events) {
                            Ok(_) | Err(RunEventViolation::MissingTermination) => true,
                            _ => {
                                state.events.pop();
                                state.bytes -= bytes;
                                event_bytes.fetch_sub(bytes, Ordering::AcqRel);
                                false
                            }
                        }
                    }
                };
                record.changed.send_modify(|v| *v += 1);
                if !valid {
                    Self::fault(&record, "invalid or oversized provider event sequence");
                    let _ = adapter.cancel_run(&run_id).await;
                    return;
                }
            }
            let mut state = record.state.lock().unwrap();
            state.status =
                match check_run_events(&state.events) {
                    Ok(RunTermination::TerminalError) => match state
                        .events
                        .last()
                        .and_then(|e| ProviderEventPayload::parse(e.kind, &e.payload).ok())
                    {
                        Some(ProviderEventPayload::Error(ErrorPayload {
                            kind: FailureKind::Unknown,
                            ..
                        })) => RunStatus::Interrupted,
                        _ => RunStatus::Failed,
                    },
                    Ok(_) => {
                        let turn = state.events.iter().rev().find_map(|e| {
                            match ProviderEventPayload::parse(e.kind, &e.payload).ok()? {
                                ProviderEventPayload::TurnFinished(p) => Some(p.status),
                                _ => None,
                            }
                        });
                        let process = state.events.last().and_then(|e| {
                            match ProviderEventPayload::parse(e.kind, &e.payload).ok()? {
                                ProviderEventPayload::ProcessExited(p) => Some(p),
                                _ => None,
                            }
                        });
                        let uncertain = process.as_ref().is_some_and(|p| {
                            p.children_reaped != Some(true)
                                || p.termination == ProcessTermination::Unknown
                                || p.termination == ProcessTermination::External
                        });
                        let nonzero = process.as_ref().is_some_and(|p| p.exit_code != Some(0));
                        match turn {
                            // Spec §7.3: cancellation cannot hide unconfirmed teardown.
                            _ if uncertain => RunStatus::Interrupted,
                            Some(TurnStatus::Cancelled) => RunStatus::Cancelled,
                            Some(TurnStatus::Interrupted | TurnStatus::Unknown) => {
                                RunStatus::Interrupted
                            }
                            Some(TurnStatus::Failed) => RunStatus::Failed,
                            _ if nonzero => RunStatus::Failed,
                            _ => RunStatus::Succeeded,
                        }
                    }
                    Err(_) => {
                        state.failure = Some(api_error(
                            ApiErrorCode::Unavailable,
                            "provider stream ended without valid termination",
                        ));
                        RunStatus::Interrupted
                    }
                };
            state.complete = true;
            drop(state);
            record.changed.send_modify(|v| *v += 1);
        });
        Ok(started)
    }
    fn fault(record: &RunRecord, message: &str) {
        let mut state = record.state.lock().unwrap();
        state.status = RunStatus::Interrupted;
        state.complete = true;
        state.failure = Some(api_error(ApiErrorCode::Unavailable, message));
        drop(state);
        record.changed.send_modify(|v| *v += 1);
    }
    fn record(&self, id: &str) -> Result<Arc<RunRecord>, ApiError> {
        self.registry
            .lock()
            .unwrap()
            .runs
            .get(id)
            .cloned()
            .ok_or_else(|| api_error(ApiErrorCode::NotFound, "unknown Run"))
    }
    pub fn inspect(&self, id: &str) -> Result<RunsInspectResult, ApiError> {
        let record = self.record(id)?;
        let state = record.state.lock().unwrap();
        Ok(RunsInspectResult {
            run_id: id.into(),
            status: state.status,
            last_sequence: state.events.last().map_or(0, |e| e.sequence),
        })
    }
    pub async fn cancel(&self, id: &str) -> Result<RunsCancelResult, ApiError> {
        let record = self.record(id)?;
        if record.state.lock().unwrap().complete {
            return Ok(RunsCancelResult { accepted: false });
        }
        let accepted = record.adapter.cancel_run(id).await.map_err(adapter_error)?;
        if accepted {
            let mut state = record.state.lock().unwrap();
            // Accepted cancellation is provisional; final events determine the outcome.
            if !state.complete {
                state.status = RunStatus::Cancelling;
            }
        }
        Ok(RunsCancelResult { accepted })
    }
    pub fn subscribe(
        &self,
        id: &str,
        after: u64,
    ) -> Result<BoxStream<'static, Result<ProviderEvent, ApiError>>, ApiError> {
        let record = self.record(id)?;
        if after > record.state.lock().unwrap().events.len() as u64 {
            return Err(api_error(
                ApiErrorCode::Conflict,
                "cursor is ahead of Run events",
            ));
        }
        let receiver = record.changed.subscribe();
        Ok(futures::stream::unfold(
            (record, receiver, after, false),
            |(record, mut receiver, mut cursor, mut ended)| async move {
                loop {
                    if ended {
                        return None;
                    }
                    receiver.borrow_and_update();
                    let (event, complete, failure) = {
                        let state = record.state.lock().unwrap();
                        (
                            state.events.get(cursor as usize).cloned(),
                            state.complete,
                            state.failure.clone(),
                        )
                    };
                    if let Some(event) = event {
                        cursor = event.sequence;
                        return Some((Ok(event), (record, receiver, cursor, ended)));
                    }
                    if complete {
                        if let Some(error) = failure {
                            ended = true;
                            return Some((Err(error), (record, receiver, cursor, ended)));
                        }
                        return None;
                    }
                    if receiver.changed().await.is_err() {
                        return None;
                    }
                }
            },
        )
        .boxed())
    }
}
fn adapter_error(error: AdapterError) -> ApiError {
    match error {
        AdapterError::UnknownRun { .. } => api_error(ApiErrorCode::NotFound, "adapter has no Run"),
        AdapterError::IdempotencyConflict { .. } => {
            api_error(ApiErrorCode::Conflict, "adapter idempotency conflict")
        }
        _ => api_error(
            ApiErrorCode::Unavailable,
            "adapter cannot execute this request",
        ),
    }
}

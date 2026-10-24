//! Run event retention, control correlation and confirmed teardown (Spec §7.2, §7.3).
use super::{
    CodexApprovals, CodexLaunch,
    protocol::{Mapped, Mapper},
    rpc::{RpcClient, error},
    timestamp,
};
use crate::{
    AdapterError,
    runtime::{ChildrenReaped, TerminationCause},
};
use futures::{StreamExt, stream::BoxStream};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};
use telos_schema::*;
use tokio::sync::watch;

struct State {
    events: Vec<ProviderEvent>,
    bytes: usize,
    finished: bool,
    interrupted: bool,
}
#[derive(Clone)]
struct Approval {
    native_id: Value,
    request: Value,
}
type InputResult = (String, Result<(), AdapterError>);
pub(super) type ApprovalCheck = Arc<dyn Fn(&Value) -> Result<(), AdapterError> + Send + Sync>;
pub(super) struct RunEntry {
    pub started: RunStarted,
    rpc: Arc<RpcClient>,
    launch: CodexLaunch,
    state: Mutex<State>,
    changed: watch::Sender<u64>,
    done: watch::Sender<bool>,
    cancelled: AtomicBool,
    generation_done: AtomicBool,
    approvals: Mutex<HashMap<String, Approval>>,
    approval_serial: AtomicU64,
    answers: Mutex<HashMap<String, ApprovalAnswer>>,
    inputs: Mutex<HashMap<String, InputResult>>,
    actions: tokio::sync::Mutex<()>,
    approval_check: ApprovalCheck,
}
impl RunEntry {
    pub fn new(
        run_id: String,
        rpc: Arc<RpcClient>,
        thread: String,
        turn: String,
        launch: CodexLaunch,
        version: String,
        approval_check: ApprovalCheck,
    ) -> Arc<Self> {
        let run = Arc::new(Self {
            started: RunStarted {
                run_id,
                native_session_id: Some(thread),
                native_turn_id: Some(turn),
            },
            rpc,
            launch,
            state: Mutex::new(State {
                events: vec![],
                bytes: 0,
                finished: false,
                interrupted: false,
            }),
            changed: watch::channel(0).0,
            done: watch::channel(false).0,
            cancelled: AtomicBool::new(false),
            generation_done: AtomicBool::new(false),
            approvals: Mutex::new(HashMap::new()),
            approval_serial: AtomicU64::new(1),
            answers: Mutex::new(HashMap::new()),
            inputs: Mutex::new(HashMap::new()),
            actions: tokio::sync::Mutex::new(()),
            approval_check,
        });
        run.push(
            ProviderEventPayload::SessionStarted(SessionStartedPayload {
                provider_version: Some(version),
                model: Some(run.launch.model.clone()),
            })
            .into(),
            true,
        );
        run
    }
    fn push(&self, mapped: Mapped, final_event: bool) -> bool {
        let mut state = self.state.lock().unwrap();
        if state.finished {
            return false;
        }
        let sequence = state.events.len() as u64 + 1;
        let event = ProviderEvent {
            event_id: format!("{}-{sequence}", self.started.run_id),
            run_id: self.started.run_id.clone(),
            sequence,
            occurred_at: timestamp(),
            schema_version: PROVIDER_EVENT_SCHEMA_VERSION.into(),
            native_session_id: self.started.native_session_id.clone(),
            native_turn_id: self.started.native_turn_id.clone(),
            tool_call_id: mapped.tool_id,
            kind: mapped.payload.kind(),
            payload: mapped.payload.to_object(),
            raw_event_ref: None,
        };
        let bytes = serde_json::to_vec(&event)
            .map(|v| v.len())
            .unwrap_or(usize::MAX);
        if !final_event
            && (state.events.len() >= 4093
                || state.bytes.saturating_add(bytes) > 8 * 1024 * 1024 - 4096)
        {
            return false;
        }
        state.bytes = state.bytes.saturating_add(bytes);
        state.events.push(event);
        drop(state);
        self.changed.send_modify(|v| *v += 1);
        true
    }
    pub fn inspect(&self) -> RunInspection {
        let state = self.state.lock().unwrap();
        if state.finished {
            if state.interrupted {
                RunInspection::Interrupted
            } else {
                RunInspection::Finished
            }
        } else {
            RunInspection::Running
        }
    }
    pub fn subscribe(
        self: &Arc<Self>,
        after: u64,
    ) -> BoxStream<'static, Result<ProviderEvent, AdapterError>> {
        if after > self.state.lock().unwrap().events.len() as u64 {
            let error = AdapterError::EventsUnavailable {
                run_id: self.started.run_id.clone(),
                after_sequence: after,
                oldest_available: 1,
            };
            return futures::stream::once(async move { Err(error) }).boxed();
        }
        let run = self.clone();
        let receiver = self.changed.subscribe();
        futures::stream::unfold(
            (run, receiver, after),
            |(run, mut receiver, mut cursor)| async move {
                loop {
                    receiver.borrow_and_update();
                    let (event, finished) = {
                        let state = run.state.lock().unwrap();
                        (state.events.get(cursor as usize).cloned(), state.finished)
                    };
                    if let Some(event) = event {
                        cursor = event.sequence;
                        return Some((Ok(event), (run, receiver, cursor)));
                    }
                    if finished || receiver.changed().await.is_err() {
                        return None;
                    }
                }
            },
        )
        .boxed()
    }
    pub fn cancel(self: &Arc<Self>, grace: Duration) -> bool {
        if self.state.lock().unwrap().finished || self.generation_done.load(Ordering::Acquire) {
            return false;
        }
        if !self.cancelled.swap(true, Ordering::AcqRel) {
            let run = self.clone();
            tokio::spawn(async move {
                let _=tokio::time::timeout(Duration::from_millis(500),run.rpc.request("turn/interrupt",json!({"threadId":run.started.native_session_id,"turnId":run.started.native_turn_id}))).await;
                let mut done = run.done.subscribe();
                if tokio::time::timeout(grace, done.wait_for(|done| *done))
                    .await
                    .is_err()
                {
                    let _ = run.rpc.close(Duration::ZERO).await;
                }
            });
        }
        true
    }
    pub async fn send_input(&self, id: &str, text: &str) -> Result<(), AdapterError> {
        if id.is_empty() || id.len() > 256 || text.len() > 65536 {
            return Err(error(FailureKind::Configuration, "invalid Codex input"));
        }
        let _action = self.actions.lock().await;
        if let Some((previous, result)) = self.inputs.lock().unwrap().get(id) {
            return if previous == text {
                result.clone()
            } else {
                Err(AdapterError::IdempotencyConflict {
                    idempotency_key: id.into(),
                })
            };
        }
        if self.state.lock().unwrap().finished || self.generation_done.load(Ordering::Acquire) {
            return Err(error(
                FailureKind::Configuration,
                "Codex turn is no longer active",
            ));
        }
        if self.inputs.lock().unwrap().len() >= 32 {
            return Err(error(
                FailureKind::ResourceLimit,
                "Codex input registry is full",
            ));
        }
        let result=self.rpc.request("turn/steer",json!({"threadId":self.started.native_session_id,"expectedTurnId":self.started.native_turn_id,"input":[{"type":"text","text":text}]})).await.and_then(|response|{
            if response.get("turnId").and_then(Value::as_str)!=self.started.native_turn_id.as_deref(){Err(error(FailureKind::ProtocolParse,"steered Codex turn ID does not match"))}else{Ok(())}
        });
        self.inputs
            .lock()
            .unwrap()
            .insert(id.into(), (text.into(), result.clone()));
        result
    }
    pub async fn answer_approval(
        &self,
        id: &str,
        decision: ApprovalAnswer,
    ) -> Result<(), AdapterError> {
        if self.launch.approvals != CodexApprovals::Prompt {
            return Err(AdapterError::UnsupportedCapability {
                capability: "approval".into(),
                reason: "this Run does not permit interactive approval".into(),
            });
        }
        let _action = self.actions.lock().await;
        if let Some(previous) = self.answers.lock().unwrap().get(id) {
            return if *previous == decision {
                Ok(())
            } else {
                Err(AdapterError::IdempotencyConflict {
                    idempotency_key: id.into(),
                })
            };
        }
        let approval = self
            .approvals
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| error(FailureKind::Configuration, "unknown Codex approval request"))?;
        if decision == ApprovalAnswer::Allow {
            (self.approval_check)(&approval.request)?;
        }
        self.rpc
            .respond(
                approval.native_id,
                json!({"decision":if decision==ApprovalAnswer::Allow{"accept"}else{"decline"}}),
            )
            .await?;
        self.approvals.lock().unwrap().remove(id);
        self.answers.lock().unwrap().insert(id.into(), decision);
        Ok(())
    }
    async fn server_request(
        &self,
        message: &Value,
        mapper: &mut Mapper,
    ) -> Result<(), AdapterError> {
        let id = message
            .get("id")
            .cloned()
            .ok_or_else(|| error(FailureKind::ProtocolParse, "missing server request ID"))?;
        let params = &message["params"];
        if params.get("threadId").and_then(Value::as_str)
            != self.started.native_session_id.as_deref()
            || params.get("turnId").and_then(Value::as_str)
                != self.started.native_turn_id.as_deref()
        {
            return self.rpc.reject(id).await;
        }
        let method = message.get("method").and_then(Value::as_str).unwrap_or("");
        let name = match method {
            "item/commandExecution/requestApproval" => "command_execution",
            "item/fileChange/requestApproval" => "file_change",
            "item/permissions/requestApproval" => "permissions",
            _ => return self.rpc.reject(id).await,
        };
        let item_id = params
            .get("itemId")
            .and_then(Value::as_str)
            .ok_or_else(|| error(FailureKind::ProtocolParse, "missing approval item ID"))?
            .to_owned();
        let answerable = name != "permissions"
            && self.launch.approvals == CodexApprovals::Prompt
            && params
                .get("networkApprovalContext")
                .is_none_or(Value::is_null);
        let request_id = format!(
            "approval-{}",
            self.approval_serial.fetch_add(1, Ordering::Relaxed)
        );
        if answerable {
            let mut approvals = self.approvals.lock().unwrap();
            if approvals.len() >= 16 {
                return Err(error(
                    FailureKind::ResourceLimit,
                    "too many pending Codex approvals",
                ));
            }
            approvals.insert(
                request_id.clone(),
                Approval {
                    native_id: id.clone(),
                    request: message.clone(),
                },
            );
        }
        if !self.push(
            ProviderEventPayload::ApprovalRequired(ApprovalRequiredPayload {
                request_id: request_id.clone(),
                action: name.into(),
                summary: "Codex requested permission for a tool operation".into(),
                answerable,
            })
            .into(),
            false,
        ) {
            return Err(error(
                FailureKind::ResourceLimit,
                "Codex event buffer is full",
            ));
        }
        if !answerable {
            self.rpc
                .respond(
                    id,
                    if name == "permissions" {
                        json!({"permissions":{},"scope":"turn"})
                    } else {
                        json!({"decision":"decline"})
                    },
                )
                .await?;
            for event in mapper.denied(&item_id, name) {
                if !self.push(event, false) {
                    return Err(error(
                        FailureKind::ResourceLimit,
                        "Codex event buffer is full",
                    ));
                }
            }
        }
        Ok(())
    }
    pub async fn supervise(
        self: Arc<Self>,
        mut events: BoxStream<'static, Result<Value, AdapterError>>,
        grace: Duration,
    ) {
        let mut mapper = Mapper::new(
            self.started.native_session_id.clone().unwrap(),
            self.started.native_turn_id.clone().unwrap(),
        );
        let deadline = tokio::time::Instant::now() + self.launch.run_timeout;
        let mut failure = None;
        loop {
            let message = match tokio::time::timeout_at(deadline, events.next()).await {
                Ok(Some(Ok(message))) => message,
                Ok(Some(Err(error))) => {
                    failure = Some(error);
                    break;
                }
                Ok(None) => {
                    failure = Some(error(
                        FailureKind::Provider,
                        "Codex ended before turn completion",
                    ));
                    break;
                }
                Err(_) => {
                    failure = Some(error(FailureKind::Timeout, "Codex Run timed out"));
                    break;
                }
            };
            let mapped = if message.get("id").is_some() {
                self.server_request(&message, &mut mapper)
                    .await
                    .map(|_| vec![])
            } else {
                mapper.map(message["method"].as_str().unwrap_or(""), &message["params"])
            };
            match mapped {
                Ok(mapped) => {
                    for event in mapped {
                        if matches!(event.payload, ProviderEventPayload::TurnFinished(_)) {
                            continue;
                        }
                        if !self.push(event, false) {
                            failure = Some(error(
                                FailureKind::ResourceLimit,
                                "Codex event buffer is full",
                            ));
                            break;
                        }
                    }
                }
                Err(error) => {
                    failure = Some(error);
                }
            }
            if failure.is_some() {
                break;
            }
            if mapper.finished {
                self.generation_done.store(true, Ordering::Release);
                break;
            }
        }
        // Capture a bounded tail of usage without allowing another Turn into this Run.
        if mapper.finished && failure.is_none() {
            for _ in 0..16 {
                let Ok(Some(Ok(message))) =
                    tokio::time::timeout(Duration::from_millis(50), events.next()).await
                else {
                    break;
                };
                if message.get("id").is_some() {
                    let _ = self.rpc.reject(message["id"].clone()).await;
                    continue;
                }
                match mapper.map(message["method"].as_str().unwrap_or(""), &message["params"]) {
                    Ok(mapped) => {
                        for event in mapped {
                            if !self.push(event, false) {
                                failure = Some(error(
                                    FailureKind::ResourceLimit,
                                    "Codex event buffer is full",
                                ));
                                break;
                            }
                        }
                    }
                    Err(error) => {
                        failure = Some(error);
                        break;
                    }
                }
            }
        }
        let report = self
            .rpc
            .close(if failure.is_some() {
                Duration::ZERO
            } else {
                grace
            })
            .await;
        let cancelled = self.cancelled.load(Ordering::Acquire);
        if let Some(failure) = &failure
            && !cancelled
        {
            self.push(
                ProviderEventPayload::Error(ErrorPayload {
                    kind: failure.failure_kind().unwrap_or(FailureKind::Unknown),
                    message: failure.to_string(),
                    terminal: false,
                })
                .into(),
                true,
            );
        }
        let status = if let Some(status) = mapper.status.filter(|_| failure.is_none()) {
            status
        } else if cancelled {
            TurnStatus::Cancelled
        } else if failure.is_some() {
            TurnStatus::Failed
        } else {
            mapper.status.unwrap_or(TurnStatus::Unknown)
        };
        self.push(
            ProviderEventPayload::TurnFinished(TurnFinishedPayload {
                status,
                reason: None,
            })
            .into(),
            true,
        );
        let interrupted = match report {
            Ok(report) => {
                self.push(
                    ProviderEventPayload::ProcessExited(ProcessExitedPayload {
                        exit_code: report.status.code,
                        termination: match report.status.cause {
                            TerminationCause::Exited => ProcessTermination::Exited,
                            TerminationCause::TerminatedOnRequest => {
                                ProcessTermination::TerminatedOnRequest
                            }
                            TerminationCause::Killed => ProcessTermination::Killed,
                            TerminationCause::External => ProcessTermination::External,
                            TerminationCause::Unknown => ProcessTermination::Unknown,
                        },
                        children_reaped: match report.children {
                            ChildrenReaped::All => Some(true),
                            ChildrenReaped::SomeRemaining => Some(false),
                            ChildrenReaped::Unknown => None,
                        },
                    })
                    .into(),
                    true,
                );
                report.children != ChildrenReaped::All
            }
            Err(_) => {
                self.push(
                    ProviderEventPayload::Error(ErrorPayload {
                        kind: FailureKind::Unknown,
                        message: "Codex process termination is unconfirmed".into(),
                        terminal: true,
                    })
                    .into(),
                    true,
                );
                true
            }
        };
        {
            let mut state = self.state.lock().unwrap();
            state.finished = true;
            state.interrupted = interrupted;
        }
        self.approvals.lock().unwrap().clear();
        self.done.send_replace(true);
        self.changed.send_modify(|v| *v += 1);
    }
}

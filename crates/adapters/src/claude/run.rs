//! Run 하나의 이벤트 보존과 프로세스 감시 (Spec §7.2·§7.3, PROV-03·06·08).
//!
//! Run은 전용 프로세스 하나와 Turn 하나입니다. 프롬프트를 stdin에 쓴 뒤 바로 닫으므로
//! 프로세스는 `result` 뒤에 스스로 끝납니다. 종료 이벤트는 항상 `process_exited`입니다.
//! `result` 뒤에 CLI가 Turn을 이어가면(백그라운드 작업 완료 등) 그 출력은 Run에 넣지 않고 프로세스를 끝냅니다.
//!
//! 실시간 승인을 켠 Run(PH1-CLAUDE L2)은 승인 응답을 보내야 하므로 stdin을 열어 두고, `result`를 받으면 닫습니다.
//! 승인 요청은 `approval_required` 이벤트로 내고 `answer_approval`의 답을 `control_response`로 돌려줍니다.
//! 답하기 전에 CLI가 요청을 거두거나 Run이 끝나면 그 요청에는 더 답할 수 없습니다.

use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use futures::{StreamExt, stream::BoxStream};
use serde_json::{Value, json};
use telos_schema::{
    ApprovalAnswer, ErrorPayload, FailureKind, PROVIDER_EVENT_SCHEMA_VERSION, ProcessExitedPayload,
    ProcessTermination, ProviderEvent, ProviderEventPayload, RunInspection, RunStarted,
    TurnFinishedPayload, TurnStatus, capability_keys,
};
use tokio::sync::Notify;

use super::{
    mcp::Injection,
    protocol::{ApprovalRequest, Mapped, StreamMapper},
};
use crate::{
    AdapterError,
    runtime::{
        ChildrenReaped, OutputEvent, OutputStream, ProcessHandle, TerminationCause,
        TerminationReport,
    },
};

pub(crate) type Clock = Arc<dyn Fn() -> String + Send + Sync>;

/// 답을 기다리는 승인 요청의 상한입니다. 넘으면 Run을 자원 한도로 끝냅니다.
const MAX_PENDING_APPROVALS: usize = 64;

const APPROVAL_DISABLED_REASON: &str = "live approval is not enabled for this run; tools that need approval are denied by the start-time policy";
const DENY_MESSAGE: &str = "Denied by the TelosWeaver approval policy. Do not retry this action.";

/// Run을 시작할 때 정하는 조건입니다.
pub(crate) struct RunOptions {
    /// `--permission-prompt-tool stdio`로 시작했는지
    pub live_approval: bool,
    /// Task MCP 주입 파일. Run이 끝나면 지웁니다.
    pub injection: Option<Injection>,
}

struct PendingApproval {
    input: Value,
    tool_use_id: Option<String>,
}

#[derive(Default)]
struct Approvals {
    pending: HashMap<String, PendingApproval>,
    answered: HashMap<String, ApprovalAnswer>,
    /// 답하기 전에 CLI가 거두었거나 Run이 끝난 요청
    closed: HashSet<String>,
    /// 거부로 답한 도구 호출. 감시 Task가 변환기로 옮겨 `denied` 결과로 표시합니다.
    denied_tools: Vec<String>,
}

impl Approvals {
    fn close_pending(&mut self) {
        let ids: Vec<String> = self.pending.drain().map(|(id, _)| id).collect();
        self.closed.extend(ids);
    }
}

struct RunState {
    events: Vec<ProviderEvent>,
    native_session_id: Option<String>,
    finished: bool,
}

pub(crate) struct RunEntry {
    pub run_id: String,
    pub idempotency_key: String,
    pub started: RunStarted,
    handle: Arc<dyn ProcessHandle>,
    state: Mutex<RunState>,
    notify: Notify,
    cancel_requested: AtomicBool,
    clock: Clock,
    /// 실시간 승인을 켠 Run에만 있습니다.
    approvals: Option<Mutex<Approvals>>,
    /// 같은 요청에 응답을 두 번 쓰지 않도록 응답을 한 번에 하나씩 처리합니다.
    answering: tokio::sync::Mutex<()>,
    injection: Mutex<Option<Injection>>,
    task_mcp_server: Option<String>,
}

impl RunEntry {
    pub fn new(
        run_id: String,
        idempotency_key: String,
        handle: Arc<dyn ProcessHandle>,
        clock: Clock,
        options: RunOptions,
    ) -> Arc<Self> {
        Arc::new(Self {
            approvals: options
                .live_approval
                .then(|| Mutex::new(Approvals::default())),
            answering: tokio::sync::Mutex::new(()),
            task_mcp_server: options
                .injection
                .as_ref()
                .map(|injection| injection.server_name().to_owned()),
            injection: Mutex::new(options.injection),
            started: RunStarted {
                run_id: run_id.clone(),
                native_session_id: None,
                native_turn_id: None,
            },
            run_id,
            idempotency_key,
            handle,
            state: Mutex::new(RunState {
                events: Vec::new(),
                native_session_id: None,
                finished: false,
            }),
            notify: Notify::new(),
            cancel_requested: AtomicBool::new(false),
            clock,
        })
    }

    pub fn inspect(&self) -> RunInspection {
        if self.state.lock().unwrap().finished {
            RunInspection::Finished
        } else {
            RunInspection::Running
        }
    }

    /// 취소 요청을 접수합니다. 실제 종료는 감시 Task가 `process_exited`로 보고합니다.
    pub fn cancel(self: &Arc<Self>, grace: Duration) -> bool {
        if self.state.lock().unwrap().finished {
            return false;
        }
        if !self.cancel_requested.swap(true, Ordering::SeqCst) {
            let entry = self.clone();
            tokio::spawn(async move {
                // stdin이 열려 있는 Run은 먼저 Turn을 끊습니다. stdin만 닫으면 CLI가 대기 중인 승인 요청을 버리고
                // 모델을 다시 불러 Turn을 마저 진행합니다(2.1.325 관찰). `interrupt`는 Turn을 바로 끝냅니다.
                if entry.approvals.is_some() {
                    let _ = entry
                        .write_control(json!({
                            "type": "control_request",
                            "request_id": "telos-cancel",
                            "request": {"subtype": "interrupt"},
                        }))
                        .await;
                }
                let _ = entry.handle.terminate(grace).await;
            });
        }
        true
    }

    /// 승인 요청에 답합니다. 같은 답의 반복은 다시 보내지 않고, 다른 답은 충돌입니다.
    pub async fn answer_approval(
        &self,
        request_id: &str,
        decision: ApprovalAnswer,
    ) -> Result<(), AdapterError> {
        let Some(approvals) = &self.approvals else {
            return Err(AdapterError::UnsupportedCapability {
                capability: capability_keys::ANSWER_APPROVAL.into(),
                reason: APPROVAL_DISABLED_REASON.into(),
            });
        };
        let refuse = |message: &str| AdapterError::Failure {
            kind: FailureKind::Configuration,
            message: message.into(),
        };
        let _serial = self.answering.lock().await;
        let response = {
            let mut state = approvals.lock().unwrap();
            if let Some(previous) = state.answered.get(request_id) {
                return if *previous == decision {
                    Ok(())
                } else {
                    Err(AdapterError::IdempotencyConflict {
                        idempotency_key: request_id.into(),
                    })
                };
            }
            if state.closed.contains(request_id) {
                return Err(refuse(
                    "approval request is no longer pending: it was withdrawn or the run ended",
                ));
            }
            let Some(pending) = state.pending.get(request_id) else {
                return Err(refuse("unknown approval request"));
            };
            let response = match decision {
                // 승인한 인자 그대로 실행하게 합니다. 이벤트의 `argsDigest`가 가리키는 인자입니다.
                ApprovalAnswer::Allow => {
                    json!({"behavior": "allow", "updatedInput": pending.input})
                }
                ApprovalAnswer::Deny => json!({"behavior": "deny", "message": DENY_MESSAGE}),
            };
            // 응답을 쓰기 전에 표시해 뒤따르는 `tool_result`가 `denied`로 변환되게 합니다.
            if let (ApprovalAnswer::Deny, Some(id)) = (decision, pending.tool_use_id.clone()) {
                state.denied_tools.push(id);
            }
            response
        };
        self.write_control(json!({
            "type": "control_response",
            "response": {"subtype": "success", "request_id": request_id, "response": response},
        }))
        .await
        .map_err(|error| AdapterError::Failure {
            kind: FailureKind::Provider,
            message: format!("could not deliver the approval answer: {error}"),
        })?;
        let mut state = approvals.lock().unwrap();
        state.pending.remove(request_id);
        state.closed.remove(request_id);
        state.answered.insert(request_id.to_owned(), decision);
        Ok(())
    }

    async fn write_control(&self, message: Value) -> Result<(), crate::runtime::RuntimeError> {
        let mut line = message.to_string().into_bytes();
        line.push(b'\n');
        self.handle.write_stdin(&line).await
    }

    /// 승인 요청을 기록하고 이벤트로 냅니다. 이벤트를 본 쪽이 바로 답할 수 있도록 기록이 먼저입니다.
    fn request_approval(&self, request: ApprovalRequest, grace: Duration) {
        let Some(approvals) = &self.approvals else {
            // stdin을 닫은 Run에서는 답할 수 없습니다. CLI가 기다리지 않게 끝냅니다.
            self.fail(
                FailureKind::ProtocolParse,
                "approval request received on a run without live approval".into(),
                grace,
            );
            return;
        };
        {
            let mut state = approvals.lock().unwrap();
            if state.pending.len() >= MAX_PENDING_APPROVALS {
                drop(state);
                self.fail(
                    FailureKind::ResourceLimit,
                    "too many pending approval requests".into(),
                    grace,
                );
                return;
            }
            state.pending.insert(
                request.payload.request_id.clone(),
                PendingApproval {
                    input: request.input,
                    tool_use_id: request.tool_use_id,
                },
            );
        }
        self.push(ProviderEventPayload::ApprovalRequired(request.payload));
    }

    fn close_approvals(&self) {
        if let Some(approvals) = &self.approvals {
            approvals.lock().unwrap().close_pending();
        }
    }

    fn push(&self, payload: ProviderEventPayload) {
        let mut state = self.state.lock().unwrap();
        if state.finished {
            return;
        }
        let sequence = state.events.len() as u64 + 1;
        let kind = payload.kind();
        let terminal = kind == telos_schema::ProviderEventKind::ProcessExited;
        let native_session_id = state.native_session_id.clone();
        state.events.push(ProviderEvent {
            event_id: format!("{}-{sequence}", self.run_id),
            run_id: self.run_id.clone(),
            sequence,
            occurred_at: (self.clock)(),
            schema_version: PROVIDER_EVENT_SCHEMA_VERSION.into(),
            native_session_id,
            native_turn_id: None,
            tool_call_id: None,
            kind,
            payload: payload.to_object(),
            raw_event_ref: None,
        });
        if terminal {
            state.finished = true;
        }
        drop(state);
        self.notify.notify_waiters();
    }

    /// `after_sequence` 다음 이벤트부터 보내고 종료 이벤트 뒤에 닫습니다.
    pub fn subscribe(
        self: &Arc<Self>,
        after_sequence: u64,
    ) -> BoxStream<'static, Result<ProviderEvent, AdapterError>> {
        let entry = self.clone();
        futures::stream::unfold(after_sequence, move |cursor| {
            let entry = entry.clone();
            async move {
                loop {
                    let notified = entry.notify.notified();
                    {
                        let state = entry.state.lock().unwrap();
                        if let Some(event) = state.events.get(cursor as usize) {
                            return Some((Ok(event.clone()), cursor + 1));
                        }
                        if state.finished {
                            return None;
                        }
                    }
                    notified.await;
                }
            }
        })
        .boxed()
    }

    /// 출력을 끝까지 읽고 종료를 확인합니다. `start_run`이 반환한 뒤 별도 Task에서 실행합니다.
    pub async fn supervise(self: Arc<Self>, grace: Duration) {
        let mut mapper = StreamMapper::new(self.task_mcp_server.clone());
        let mut protocol_failed = false;
        let mut continued = false;
        match self.handle.take_output() {
            Ok(mut output) => {
                let mut open = 2;
                while open > 0 {
                    let Some(event) = output.next().await else {
                        break;
                    };
                    let turn_done = mapper.turn_finished();
                    if let Some(approvals) = &self.approvals {
                        for id in approvals.lock().unwrap().denied_tools.drain(..) {
                            mapper.mark_denied(id);
                        }
                    }
                    match event {
                        OutputEvent::Line {
                            stream: OutputStream::Stdout,
                            data,
                            ..
                        } if !protocol_failed && !continued => match mapper.map_line(&data) {
                            // §7.2: Turn이 끝난 뒤 Provider가 스스로 시작한 Turn은 이 Run이 감독하지 않습니다.
                            Ok(items) if turn_done && !items.is_empty() => {
                                continued = true;
                                self.stop_continuation(grace);
                            }
                            Err(_) if turn_done => {
                                continued = true;
                                self.stop_continuation(grace);
                            }
                            Ok(items) => {
                                if let Some(id) = mapper.session_id() {
                                    let mut state = self.state.lock().unwrap();
                                    if state.native_session_id.is_none() {
                                        state.native_session_id = Some(id.to_owned());
                                    }
                                }
                                // 취소 요청으로 끊긴 Turn은 Provider 오류가 아니라 취소로 보고합니다.
                                let cancelled_turn = self.cancel_requested.load(Ordering::SeqCst)
                                    && items.iter().any(|item| {
                                        matches!(item, Mapped::Event(ProviderEventPayload::TurnFinished(turn))
                                            if turn.status == TurnStatus::Interrupted)
                                    });
                                for item in items {
                                    match item {
                                        Mapped::Event(ProviderEventPayload::Error(_))
                                            if cancelled_turn => {}
                                        Mapped::Event(ProviderEventPayload::TurnFinished(turn))
                                            if cancelled_turn =>
                                        {
                                            self.push(ProviderEventPayload::TurnFinished(
                                                TurnFinishedPayload {
                                                    status: TurnStatus::Cancelled,
                                                    reason: turn.reason,
                                                },
                                            ))
                                        }
                                        Mapped::Event(payload) => self.push(payload),
                                        Mapped::Approval(request) => {
                                            self.request_approval(request, grace)
                                        }
                                        Mapped::ApprovalWithdrawn { request_id } => {
                                            if let Some(approvals) = &self.approvals {
                                                let mut state = approvals.lock().unwrap();
                                                if let Some(pending) =
                                                    state.pending.remove(&request_id)
                                                {
                                                    state.closed.insert(request_id);
                                                    // 승인을 받지 못해 실행되지 않은 도구 호출입니다.
                                                    state.denied_tools.extend(pending.tool_use_id);
                                                }
                                            }
                                        }
                                        Mapped::UnsupportedControl { request_id } => {
                                            if self.approvals.is_some() {
                                                let _ = self
                                                    .write_control(json!({
                                                        "type": "control_response",
                                                        "response": {
                                                            "subtype": "error",
                                                            "request_id": request_id,
                                                            "error": "unsupported control request",
                                                        },
                                                    }))
                                                    .await;
                                            }
                                        }
                                        // Task MCP를 요청한 Run이 서버 없이 진행하면 결과를 보고할 수 없습니다 (§9.2).
                                        Mapped::TaskMcpUnavailable { server, status } => {
                                            protocol_failed = true;
                                            self.fail(
                                                FailureKind::Configuration,
                                                format!(
                                                    "task MCP server '{server}' is not connected (status: {status})"
                                                ),
                                                grace,
                                            );
                                        }
                                    }
                                }
                                // Turn이 끝났으면 더 받을 승인 요청이 없습니다. stdin을 닫아 CLI가 끝나게 합니다.
                                if !turn_done && mapper.turn_finished() && self.approvals.is_some()
                                {
                                    self.close_approvals();
                                    let _ = self.handle.close_stdin().await;
                                }
                            }
                            Err(error) => {
                                protocol_failed = true;
                                self.fail(FailureKind::ProtocolParse, error.0, grace);
                            }
                        },
                        OutputEvent::LineTooLong {
                            stream: OutputStream::Stdout,
                            total_bytes,
                            ..
                        } if !protocol_failed => {
                            protocol_failed = true;
                            self.fail(
                                FailureKind::ProtocolParse,
                                format!("stream line of {total_bytes} bytes exceeds the limit"),
                                grace,
                            );
                        }
                        OutputEvent::ReadError {
                            stream: OutputStream::Stdout,
                            message,
                            ..
                        } => {
                            open -= 1;
                            if !protocol_failed {
                                protocol_failed = true;
                                self.fail(
                                    FailureKind::ProtocolParse,
                                    format!("stdout read failed: {message}"),
                                    grace,
                                );
                            }
                        }
                        OutputEvent::Closed { .. } | OutputEvent::ReadError { .. } => open -= 1,
                        // stderr 내용은 이벤트·로그로 복사하지 않습니다 (§14.1).
                        _ => {}
                    }
                }
            }
            Err(error) => {
                protocol_failed = true;
                self.fail(
                    FailureKind::ProtocolParse,
                    format!("process output unavailable: {error}"),
                    grace,
                );
            }
        }

        let status = self.handle.wait().await;
        // 프로세스가 스스로 끝나도 남은 자식을 회수하고 그 결과를 받습니다. 이미 끝났으면 `Exited`입니다.
        let report = self.handle.terminate(Duration::ZERO).await;
        let cancelled = self.cancel_requested.load(Ordering::SeqCst);
        // 프로세스가 끝났으므로 남은 승인 요청에는 답할 수 없고, 주입 파일도 더 쓰이지 않습니다.
        self.close_approvals();
        drop(self.injection.lock().unwrap().take());

        if !mapper.turn_finished() {
            if !cancelled && !protocol_failed {
                let code = status.as_ref().ok().and_then(|s| s.code);
                self.push(ProviderEventPayload::Error(ErrorPayload {
                    kind: FailureKind::Provider,
                    message: format!("Claude Code exited before a result (exit code {code:?})"),
                    terminal: false,
                }));
            }
            self.push(ProviderEventPayload::TurnFinished(TurnFinishedPayload {
                status: if cancelled {
                    TurnStatus::Cancelled
                } else {
                    TurnStatus::Failed
                },
                reason: Some(if cancelled { "cancelled" } else { "no_result" }.into()),
            }));
        }
        self.push(ProviderEventPayload::ProcessExited(exited_payload(
            status.ok().map(|s| (s.code, s.cause)),
            report.ok(),
        )));
    }

    /// `turn_finished` 뒤에는 이벤트를 더 둘 수 없으므로 기록 없이 프로세스만 끝냅니다.
    /// 종료 코드가 없는 요청 종료로 보고되어 Run은 성공으로 판정되지 않습니다.
    fn stop_continuation(&self, grace: Duration) {
        let handle = self.handle.clone();
        tokio::spawn(async move {
            let _ = handle.terminate(grace).await;
        });
    }

    /// Run을 더 진행할 수 없는 오류입니다. Protocol 오류는 정상 메시지로 해석하지 않고 프로세스를 끝냅니다 (REQ-003).
    fn fail(&self, kind: FailureKind, message: String, grace: Duration) {
        self.push(ProviderEventPayload::Error(ErrorPayload {
            kind,
            message,
            terminal: false,
        }));
        let handle = self.handle.clone();
        tokio::spawn(async move {
            let _ = handle.terminate(grace).await;
        });
    }
}

fn exited_payload(
    status: Option<(Option<i32>, TerminationCause)>,
    report: Option<TerminationReport>,
) -> ProcessExitedPayload {
    // 종료 보고가 있으면 그쪽의 원인을 우선합니다. 취소가 자연 종료와 경합하면 실제로 먼저 일어난 사건입니다.
    let (code, cause) = match (report, status) {
        (Some(report), _) => (report.status.code, Some(report.status.cause)),
        (None, Some((code, cause))) => (code, Some(cause)),
        (None, None) => (None, None),
    };
    ProcessExitedPayload {
        exit_code: code,
        termination: match cause {
            Some(TerminationCause::Exited) => ProcessTermination::Exited,
            Some(TerminationCause::TerminatedOnRequest) => ProcessTermination::TerminatedOnRequest,
            Some(TerminationCause::Killed) => ProcessTermination::Killed,
            Some(TerminationCause::External) => ProcessTermination::External,
            Some(TerminationCause::Unknown) | None => ProcessTermination::Unknown,
        },
        children_reaped: match report.map(|r| r.children) {
            Some(ChildrenReaped::All) => Some(true),
            Some(ChildrenReaped::SomeRemaining) => Some(false),
            Some(ChildrenReaped::Unknown) | None => None,
        },
    }
}

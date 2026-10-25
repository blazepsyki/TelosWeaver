//! Antigravity CLI (`agy`) Run 하나의 이벤트 보존과 프로세스 감시 (Spec §7.2·§7.3·§7.4, PROV-03·06·08).
//!
//! Run은 전용 프로세스 하나와 Turn 하나입니다. 프롬프트를 `--input-format stream-json` 한 줄로
//! stdin에 쓴 뒤 즉시 닫으므로(`stdin`을 열어 둔 채 프로세스 종료만 기다리지 않음, §7.4),
//! 프로세스는 `result` 뒤에 스스로 끝납니다. 종료 이벤트는 항상 `process_exited`입니다.

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use futures::{StreamExt, stream::BoxStream};
use telos_schema::{
    ErrorPayload, FailureKind, PROVIDER_EVENT_SCHEMA_VERSION, ProcessExitedPayload,
    ProcessTermination, ProviderEvent, ProviderEventPayload, RunInspection, RunStarted,
    TurnFinishedPayload, TurnStatus,
};
use tokio::sync::Notify;

use super::protocol::{StreamMapper, classify_error_message};
use crate::{
    AdapterError,
    runtime::{
        ChildrenReaped, OutputEvent, OutputStream, ProcessHandle, TerminationCause,
        TerminationReport,
    },
};

pub(crate) type Clock = Arc<dyn Fn() -> String + Send + Sync>;

struct RunState {
    events: Vec<ProviderEvent>,
    native_session_id: Option<String>,
    finished: bool,
}

pub(crate) struct RunEntry {
    pub run_id: String,
    pub idempotency_key: String,
    pub started: RunStarted,
    provider_version: Option<String>,
    handle: Arc<dyn ProcessHandle>,
    state: Mutex<RunState>,
    notify: Notify,
    cancel_requested: AtomicBool,
    clock: Clock,
}

impl RunEntry {
    pub fn new(
        run_id: String,
        idempotency_key: String,
        provider_version: Option<String>,
        handle: Arc<dyn ProcessHandle>,
        clock: Clock,
    ) -> Arc<Self> {
        Arc::new(Self {
            started: RunStarted {
                run_id: run_id.clone(),
                native_session_id: None,
                native_turn_id: None,
            },
            run_id,
            idempotency_key,
            provider_version,
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

    /// 취소 요청을 접수합니다. 실제 종료는 감시 Task가 `process_exited`로 보고합니다 (§7.3).
    pub fn cancel(self: &Arc<Self>, grace: Duration) -> bool {
        if self.state.lock().unwrap().finished {
            return false;
        }
        if !self.cancel_requested.swap(true, Ordering::SeqCst) {
            let handle = self.handle.clone();
            tokio::spawn(async move {
                let _ = handle.terminate(grace).await;
            });
        }
        true
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

    /// `after_sequence` 다음 이벤트부터 보내고 종료 이벤트 뒤에 닫습니다 (§7.2).
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
        let mut mapper = StreamMapper::new(self.provider_version.clone());
        let mut protocol_failed = false;
        let mut stderr_failure_kind: Option<FailureKind> = None;

        match self.handle.take_output() {
            Ok(mut output) => {
                let mut open = 2;
                while open > 0 {
                    let Some(event) = output.next().await else {
                        break;
                    };
                    match event {
                        OutputEvent::Line {
                            stream: OutputStream::Stdout,
                            data,
                            ..
                        } if !protocol_failed => match mapper.map_line(&data) {
                            Ok(payloads) => {
                                if let Some(id) = mapper.session_id() {
                                    let mut state = self.state.lock().unwrap();
                                    if state.native_session_id.is_none() {
                                        state.native_session_id = Some(id.to_owned());
                                    }
                                }
                                let cancelled = self.cancel_requested.load(Ordering::SeqCst);
                                for payload in payloads {
                                    match payload {
                                        // Linux에서 SIGTERM 수신 시 내부 서버가 먼저 닫히며 출력하는
                                        // 종료 직전 result(ERROR)는 취소로 귀결시킵니다 (PROV-08).
                                        ProviderEventPayload::Error(_) if cancelled => {}
                                        ProviderEventPayload::TurnFinished(
                                            TurnFinishedPayload {
                                                status: TurnStatus::Failed,
                                                ..
                                            },
                                        ) if cancelled => {
                                            self.push(ProviderEventPayload::TurnFinished(
                                                TurnFinishedPayload {
                                                    status: TurnStatus::Cancelled,
                                                    reason: Some("cancelled".into()),
                                                },
                                            ));
                                        }
                                        other => self.push(other),
                                    }
                                }
                            }
                            Err(error) => {
                                protocol_failed = true;
                                self.fail_protocol(error.0, grace);
                            }
                        },
                        OutputEvent::Line {
                            stream: OutputStream::Stderr,
                            data,
                            ..
                        } => {
                            // stderr 원문은 이벤트·로그로 복사하지 않되(§14.1), result 없이 종료될 때의
                            // 실패 분류(인증·설정·권한 거부 등)에만 힌트로 사용합니다.
                            if stderr_failure_kind.is_none()
                                && let Ok(text) = std::str::from_utf8(&data)
                            {
                                let kind = classify_error_message(text);
                                if kind != FailureKind::Provider {
                                    stderr_failure_kind = Some(kind);
                                }
                            }
                        }
                        OutputEvent::LineTooLong {
                            stream: OutputStream::Stdout,
                            total_bytes,
                            ..
                        } if !protocol_failed => {
                            protocol_failed = true;
                            self.fail_protocol(
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
                                self.fail_protocol(format!("stdout read failed: {message}"), grace);
                            }
                        }
                        OutputEvent::Closed { .. } | OutputEvent::ReadError { .. } => open -= 1,
                        _ => {}
                    }
                }
            }
            Err(error) => {
                protocol_failed = true;
                self.fail_protocol(format!("process output unavailable: {error}"), grace);
            }
        }

        let status = self.handle.wait().await;
        // 프로세스가 스스로 끝나도 남은 자식을 회수하고 그 결과를 받습니다. 이미 끝났으면 `Exited`입니다.
        let report = self.handle.terminate(Duration::ZERO).await;
        let cancelled = self.cancel_requested.load(Ordering::SeqCst);

        if !mapper.turn_finished() {
            if !cancelled && !protocol_failed {
                let code = status.as_ref().ok().and_then(|s| s.code);
                let kind = stderr_failure_kind.unwrap_or(FailureKind::Provider);
                self.push(ProviderEventPayload::Error(ErrorPayload {
                    kind,
                    message: format!("Antigravity CLI exited before a result (exit code {code:?})"),
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

    /// Protocol 오류는 정상 메시지로 해석하지 않고 프로세스를 끝냅니다 (REQ-003).
    fn fail_protocol(&self, message: String, grace: Duration) {
        self.push(ProviderEventPayload::Error(ErrorPayload {
            kind: FailureKind::ProtocolParse,
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

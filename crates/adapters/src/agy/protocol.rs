//! Antigravity CLI (`agy`) `--output-format stream-json` 한 줄을 공통 이벤트 Payload로 바꿉니다 (Spec §7.2, §7.4, PROV-03·06·07).
//!
//! 2026-10-24 `agy` 1.4.7에서 관찰한 NDJSON 형식을 기준으로 합니다 (증거: `docs/phase-0/evidence/PH0-AGY/README.md`).
//! - `init`: 세션 초기화 (`conversation_id`, `init.model`, `init.cwd`, `init.tools`, `init.permission_mode`).
//!   첫 번째 `init`만 `session_started`로 보냅니다.
//! - `step_update`: 단계 진행 상태 (`step_type`: `user_input`, `agent_response`, `tool`, `system_message` 등).
//!   - `agent_response`: `text_delta`가 비어 있지 않으면 `text_delta`(`assistant`)로 보냅니다.
//!   - `tool`: `state == "ACTIVE"`에서 `tool_started`, `state == "DONE"`에서 `tool_completed`(`succeeded`),
//!     `state == "ERROR"`에서 권한 거부 메시지 여부를 판별해 `tool_completed`(`denied` 또는 `failed`)로 보냅니다.
//! - `result`: Turn 최종 결과 (`status`, `response`, `error`, `usage`, `denied_actions`).
//!   - Headless 모드에서 권한 확인이 필요한 도구는 소프트 거부되어 `status: "SUCCESS"`, 종료 코드 `0`과 함께
//!     `denied_actions`에 보고됩니다 (§7.4). 어댑터는 `status`나 종료 코드만 보지 않고 `denied_actions`와
//!     출력 유무를 함께 확인해 `tool_completed`(`denied`) 및 `tool_denied` 실패를 기록합니다 (§7.3).
//! - 그 밖의 `event`(`command_result` 등)는 하위·상위 호환을 위해 무시합니다.

use std::collections::HashSet;

use serde_json::{Map, Value};
use telos_schema::{
    ErrorPayload, FailureKind, ProviderEventPayload, SessionStartedPayload, TextChannel,
    TextDeltaPayload, ToolCompletedPayload, ToolOutcome, ToolStartedPayload, TurnFinishedPayload,
    TurnStatus, UsagePayload, UsageScope, UsageSource,
};

/// 오류·요약 메시지에 Provider 원문을 넣을 때의 문자 수 상한입니다.
const MESSAGE_LIMIT: usize = 240;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProtocolError(pub String);

#[derive(Debug, Default)]
pub(crate) struct StreamMapper {
    provider_version: Option<String>,
    session_id: Option<String>,
    session_started: bool,
    started_tools: HashSet<u64>,
    denied_tool_seen: bool,
    emitted_text: bool,
    turn_finished: bool,
}

impl StreamMapper {
    pub fn new(provider_version: Option<String>) -> Self {
        Self {
            provider_version,
            ..Self::default()
        }
    }

    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    pub fn turn_finished(&self) -> bool {
        self.turn_finished
    }

    pub fn map_line(&mut self, line: &[u8]) -> Result<Vec<ProviderEventPayload>, ProtocolError> {
        let value: Value = serde_json::from_slice(line)
            .map_err(|error| ProtocolError(format!("invalid JSON line: {error}")))?;
        let object = value
            .as_object()
            .ok_or_else(|| ProtocolError("stream line is not a JSON object".into()))?;
        let event = str_field(object, "event")
            .ok_or_else(|| ProtocolError("stream line has no event field".into()))?;
        self.capture_session_id(object);
        Ok(match event {
            "init" => self.init(object),
            "step_update" => self.step_update(object),
            "result" => self.result(object),
            _ => Vec::new(),
        })
    }

    fn capture_session_id(&mut self, object: &Map<String, Value>) {
        if self.session_id.is_some() {
            return;
        }
        if let Some(id) = non_empty_str(object, "conversation_id") {
            self.session_id = Some(id.to_owned());
        }
    }

    fn init(&mut self, object: &Map<String, Value>) -> Vec<ProviderEventPayload> {
        let init_obj = object.get("init").and_then(Value::as_object);
        if let Some(init) = init_obj {
            self.capture_session_id(init);
        }
        if self.session_started {
            return Vec::new();
        }
        self.session_started = true;
        let model = init_obj
            .and_then(|init| non_empty_str(init, "model"))
            .or_else(|| non_empty_str(object, "model"))
            .map(str::to_owned);
        let provider_version = init_obj
            .and_then(|init| non_empty_str(init, "version"))
            .map(str::to_owned)
            .or_else(|| self.provider_version.clone());
        vec![ProviderEventPayload::SessionStarted(
            SessionStartedPayload {
                provider_version,
                model,
            },
        )]
    }

    fn step_update(&mut self, object: &Map<String, Value>) -> Vec<ProviderEventPayload> {
        let Some(step) = object.get("step_update").and_then(Value::as_object) else {
            return Vec::new();
        };
        self.capture_session_id(step);
        match str_field(step, "step_type") {
            Some("agent_response") => {
                if let Some(text) = non_empty_str(step, "text_delta") {
                    self.emitted_text = true;
                    vec![ProviderEventPayload::TextDelta(TextDeltaPayload {
                        channel: TextChannel::Assistant,
                        text: text.to_owned(),
                    })]
                } else {
                    Vec::new()
                }
            }
            Some("tool") => self.tool_step(step),
            _ => Vec::new(),
        }
    }

    fn tool_step(&mut self, step: &Map<String, Value>) -> Vec<ProviderEventPayload> {
        let tool_info = step.get("tool_info").and_then(Value::as_object);
        let name = non_empty_str(step, "tool_name")
            .or_else(|| tool_info.and_then(|info| non_empty_str(info, "name")))
            .unwrap_or("unknown")
            .to_owned();
        let step_index = u64_field(step, "step_index");
        let state = str_field(step, "state").unwrap_or_default();
        let mut out = Vec::new();

        match state {
            "ACTIVE" => {
                if step_index.is_none_or(|idx| self.started_tools.insert(idx)) {
                    out.push(ProviderEventPayload::ToolStarted(ToolStartedPayload {
                        name,
                        summary: None,
                    }));
                }
            }
            "DONE" => {
                if step_index.is_none_or(|idx| self.started_tools.insert(idx)) {
                    out.push(ProviderEventPayload::ToolStarted(ToolStartedPayload {
                        name: name.clone(),
                        summary: None,
                    }));
                }
                let summary = tool_info
                    .and_then(|info| non_empty_str(info, "output"))
                    .map(truncate);
                out.push(ProviderEventPayload::ToolCompleted(ToolCompletedPayload {
                    name,
                    outcome: ToolOutcome::Succeeded,
                    summary,
                }));
            }
            "ERROR" | "FAILED" | "CANCELLED" => {
                if step_index.is_none_or(|idx| self.started_tools.insert(idx)) {
                    out.push(ProviderEventPayload::ToolStarted(ToolStartedPayload {
                        name: name.clone(),
                        summary: None,
                    }));
                }
                let error_message = tool_info
                    .and_then(|info| info.get("error"))
                    .and_then(Value::as_object)
                    .and_then(|err| non_empty_str(err, "message"))
                    .or_else(|| non_empty_str(step, "error"));
                let is_denied = error_message.is_some_and(is_permission_denial_message);
                if is_denied {
                    self.denied_tool_seen = true;
                }
                let outcome = if is_denied {
                    ToolOutcome::Denied
                } else {
                    ToolOutcome::Failed
                };
                out.push(ProviderEventPayload::ToolCompleted(ToolCompletedPayload {
                    name,
                    outcome,
                    summary: error_message.map(truncate),
                }));
            }
            _ => {}
        }
        out
    }

    fn result(&mut self, object: &Map<String, Value>) -> Vec<ProviderEventPayload> {
        let res = object
            .get("result")
            .and_then(Value::as_object)
            .unwrap_or(object);
        self.capture_session_id(res);

        let mut out = Vec::new();
        let status = str_field(res, "status").unwrap_or("SUCCESS");
        let is_error =
            status.eq_ignore_ascii_case("ERROR") || non_empty_str(res, "error").is_some();

        let denied_actions = denied_action_names(res);
        if !denied_actions.is_empty() && !self.denied_tool_seen {
            for name in &denied_actions {
                out.push(ProviderEventPayload::ToolStarted(ToolStartedPayload {
                    name: name.clone(),
                    summary: None,
                }));
                out.push(ProviderEventPayload::ToolCompleted(ToolCompletedPayload {
                    name: name.clone(),
                    outcome: ToolOutcome::Denied,
                    summary: Some("action denied in headless print mode".into()),
                }));
            }
            self.denied_tool_seen = true;
        }

        let response = str_field(res, "response").unwrap_or_default();
        if !is_error && !self.emitted_text && !response.is_empty() {
            self.emitted_text = true;
            out.push(ProviderEventPayload::TextDelta(TextDeltaPayload {
                channel: TextChannel::Assistant,
                text: response.to_owned(),
            }));
        }

        if let Some(usage) = res.get("usage").and_then(Value::as_object) {
            let total = u64_field(usage, "total_tokens").unwrap_or(0);
            let num_turns = u64_field(res, "num_turns").unwrap_or(0);
            // 시작 직후 설정·명령 오류로 모델 호출 없이 0 토큰으로 끝난 경우는 사용량을 생략합니다 (§15.2).
            if !is_error || total > 0 || num_turns > 0 {
                out.push(ProviderEventPayload::Usage(UsagePayload {
                    scope: UsageScope::Delta,
                    source: UsageSource::ProviderReported,
                    input_tokens: u64_field(usage, "input_tokens"),
                    output_tokens: u64_field(usage, "output_tokens"),
                    cache_read_tokens: u64_field(usage, "cache_read_tokens"),
                    cache_write_tokens: u64_field(usage, "cache_write_tokens"),
                    cache_included_in_input: Some(false),
                    cost_usd: None,
                }));
            }
        }

        let soft_denied_without_output =
            !is_error && !denied_actions.is_empty() && response.trim().is_empty();

        if is_error {
            let raw_error = non_empty_str(res, "error")
                .or_else(|| non_empty_str(res, "response"))
                .unwrap_or("Antigravity CLI reported an error");
            let kind = classify_error_message(raw_error);
            out.push(ProviderEventPayload::Error(ErrorPayload {
                kind,
                message: truncate(raw_error),
                terminal: false,
            }));
        } else if soft_denied_without_output {
            // Spec §7.3·§7.4: Headless 소프트 거부로 응답 없이 끝난 경우 종료 코드 0이어도 실패로 분류합니다.
            out.push(ProviderEventPayload::Error(ErrorPayload {
                kind: FailureKind::ToolDenied,
                message: format!(
                    "headless mode auto-denied required actions ({}) and produced no output",
                    denied_actions.join(", ")
                ),
                terminal: false,
            }));
        }

        let (turn_status, reason) = if status.eq_ignore_ascii_case("CANCELLED")
            || status.eq_ignore_ascii_case("INTERRUPTED")
            || status.eq_ignore_ascii_case("ABORTED")
        {
            (TurnStatus::Cancelled, Some("cancelled".into()))
        } else if is_error {
            (TurnStatus::Failed, Some("error".into()))
        } else if soft_denied_without_output {
            (TurnStatus::Failed, Some("tool_denied".into()))
        } else {
            (TurnStatus::Completed, None)
        };

        self.turn_finished = true;
        out.push(ProviderEventPayload::TurnFinished(TurnFinishedPayload {
            status: turn_status,
            reason,
        }));
        out
    }
}

fn denied_action_names(res: &Map<String, Value>) -> Vec<String> {
    res.get("denied_actions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .map(|entry| {
            non_empty_str(entry, "display_name")
                .or_else(|| non_empty_str(entry, "action"))
                .unwrap_or("unknown")
                .to_owned()
        })
        .collect()
}

pub(crate) fn is_permission_denial_message(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("permission check failed")
        || lower.contains("user denied permission")
        || lower.contains("permission denied")
        || lower.contains("auto-denied")
        || lower.contains("blocked by remy safety policy")
}

/// Spec §7.3 실패 분류. 알 수 없으면 `Provider`로 두고 성공으로 보지 않습니다.
pub(crate) fn classify_error_message(message: &str) -> FailureKind {
    let lower = message.to_ascii_lowercase();
    if is_permission_denial_message(&lower) {
        FailureKind::ToolDenied
    } else if lower.contains("please sign in")
        || lower.contains("authentication required")
        || lower.contains("authentication failed")
        || lower.contains("not logged in")
        || lower.contains("not authenticated")
        || lower.contains("unauthenticated")
        || lower.contains("unauthorized")
        || lower.contains("invalid credentials")
    {
        FailureKind::Authentication
    } else if lower.contains("invalid model")
        || lower.contains("not recognized as a known model")
        || lower.contains("not available in print mode")
        || lower.contains("unavailable with --input-format")
        || lower.contains("flags provided but not defined")
        || lower.contains("flag needs an argument")
        || lower.contains("missing the \"event\" field")
        || lower.contains("missing the \"message\" field")
    {
        FailureKind::Configuration
    } else if lower.contains("timed out")
        || lower.contains("timeout")
        || lower.contains("deadline exceeded")
    {
        FailureKind::Timeout
    } else if lower.contains("rate limit")
        || lower.contains("quota")
        || lower.contains("resource exhausted")
        || lower.contains("maximum token limit")
        || lower.contains("429")
    {
        FailureKind::ResourceLimit
    } else {
        FailureKind::Provider
    }
}

fn str_field<'a>(object: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    object.get(key).and_then(Value::as_str)
}

fn non_empty_str<'a>(object: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    str_field(object, key).filter(|s| !s.is_empty())
}

fn u64_field(object: &Map<String, Value>, key: &str) -> Option<u64> {
    object.get(key).and_then(Value::as_u64)
}

pub(crate) fn truncate(text: &str) -> String {
    match text.char_indices().nth(MESSAGE_LIMIT) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_owned(),
    }
}

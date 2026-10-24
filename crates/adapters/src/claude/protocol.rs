//! Claude Code `--output-format stream-json` 한 줄을 공통 이벤트 Payload로 바꿉니다 (Spec §7.2, PROV-03).
//!
//! 2026-10-24 Claude Code 2.1.310에서 관찰한 형식을 기준으로 합니다 (증거: `evidence/PH0-CLAUDE`).
//! - `system`/`init`: 세션 시작. Turn마다 다시 올 수 있으며 첫 번째만 `session_started`로 보냅니다.
//! - `assistant`: `content[]`의 `text`는 응답 본문, `tool_use`는 도구 시작입니다. `thinking`은 보내지 않습니다.
//! - `user`: `content[]`의 `tool_result`는 도구 완료입니다.
//! - `system`/`permission_denied`: 권한 거부. 뒤따르는 `tool_result`를 `denied`로 표시합니다.
//! - `result`: Turn 완료와 사용량. `is_error`이면 오류 분류를 함께 보냅니다.
//! - 그 밖의 `type`(`rate_limit_event`, `system`/`thinking_tokens` 등)은 무시합니다. 새 형식에 대한 호환을 위해서입니다.

use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value};
use telos_schema::{
    ErrorPayload, FailureKind, ProviderEventPayload, SessionStartedPayload, TextChannel,
    TextDeltaPayload, ToolCompletedPayload, ToolOutcome, ToolStartedPayload, TurnFinishedPayload,
    TurnStatus, UsagePayload, UsageScope, UsageSource,
};

/// 오류 메시지에 Provider 원문을 넣을 때의 상한입니다. 응답 본문 전체를 오류로 복사하지 않습니다.
const MESSAGE_LIMIT: usize = 240;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProtocolError(pub String);

#[derive(Debug, Default)]
pub(crate) struct StreamMapper {
    session_id: Option<String>,
    tools: HashMap<String, String>,
    denied: HashSet<String>,
    /// `assistant` 메시지의 `error` 코드 (예: `model_not_found`). `result` 분류에 씁니다.
    api_error: Option<String>,
    turn_finished: bool,
}

impl StreamMapper {
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
        let kind = str_field(object, "type")
            .ok_or_else(|| ProtocolError("stream line has no type".into()))?;
        Ok(match kind {
            "system" => self.system(object),
            "assistant" => self.assistant(object),
            "user" => self.user(object),
            "result" => self.result(object),
            _ => Vec::new(),
        })
    }

    fn system(&mut self, object: &Map<String, Value>) -> Vec<ProviderEventPayload> {
        match str_field(object, "subtype") {
            Some("init") if self.session_id.is_none() => {
                self.session_id = str_field(object, "session_id").map(str::to_owned);
                vec![ProviderEventPayload::SessionStarted(
                    SessionStartedPayload {
                        provider_version: str_field(object, "claude_code_version")
                            .map(str::to_owned),
                        model: str_field(object, "model").map(str::to_owned),
                    },
                )]
            }
            Some("permission_denied") => {
                if let Some(id) = str_field(object, "tool_use_id") {
                    self.denied.insert(id.to_owned());
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn assistant(&mut self, object: &Map<String, Value>) -> Vec<ProviderEventPayload> {
        if let Some(code) = str_field(object, "error") {
            self.api_error = Some(code.to_owned());
        }
        let mut out = Vec::new();
        for item in content(object) {
            match str_field(item, "type") {
                Some("text") => {
                    if let Some(text) = str_field(item, "text").filter(|t| !t.is_empty()) {
                        out.push(ProviderEventPayload::TextDelta(TextDeltaPayload {
                            channel: TextChannel::Assistant,
                            text: text.to_owned(),
                        }));
                    }
                }
                Some("tool_use") => {
                    let name = str_field(item, "name").unwrap_or("unknown").to_owned();
                    if let Some(id) = str_field(item, "id") {
                        self.tools.insert(id.to_owned(), name.clone());
                    }
                    out.push(ProviderEventPayload::ToolStarted(ToolStartedPayload {
                        name,
                        summary: None,
                    }));
                }
                _ => {}
            }
        }
        out
    }

    fn user(&mut self, object: &Map<String, Value>) -> Vec<ProviderEventPayload> {
        let mut out = Vec::new();
        for item in content(object) {
            if str_field(item, "type") != Some("tool_result") {
                continue;
            }
            let id = str_field(item, "tool_use_id").unwrap_or_default();
            let outcome = if self.denied.contains(id) {
                ToolOutcome::Denied
            } else if item.get("is_error").and_then(Value::as_bool) == Some(true) {
                ToolOutcome::Failed
            } else {
                ToolOutcome::Succeeded
            };
            out.push(ProviderEventPayload::ToolCompleted(ToolCompletedPayload {
                name: self
                    .tools
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| "unknown".into()),
                outcome,
                summary: None,
            }));
        }
        out
    }

    fn result(&mut self, object: &Map<String, Value>) -> Vec<ProviderEventPayload> {
        let mut out = Vec::new();
        if let Some(usage) = object.get("usage").and_then(Value::as_object) {
            // 구독 사용량을 비용으로 환산하지 않습니다 (§15.2). `total_cost_usd`는 기록하지 않습니다.
            out.push(ProviderEventPayload::Usage(UsagePayload {
                scope: UsageScope::Delta,
                source: UsageSource::ProviderReported,
                input_tokens: u64_field(usage, "input_tokens"),
                output_tokens: u64_field(usage, "output_tokens"),
                cache_read_tokens: u64_field(usage, "cache_read_input_tokens"),
                cache_write_tokens: u64_field(usage, "cache_creation_input_tokens"),
                cache_included_in_input: Some(false),
                cost_usd: None,
            }));
        }
        let is_error = object.get("is_error").and_then(Value::as_bool) == Some(true);
        let reason = str_field(object, "terminal_reason").map(str::to_owned);
        if is_error {
            let kind = classify(
                object.get("api_error_status").and_then(Value::as_u64),
                self.api_error.as_deref(),
                reason.as_deref(),
            );
            let message = str_field(object, "result")
                .map(truncate)
                .unwrap_or_else(|| "Claude Code reported an error".into());
            out.push(ProviderEventPayload::Error(ErrorPayload {
                kind,
                message,
                terminal: false,
            }));
        }
        let status = match (is_error, reason.as_deref()) {
            (_, Some("aborted" | "interrupted")) => TurnStatus::Interrupted,
            (true, _) => TurnStatus::Failed,
            (false, _) => TurnStatus::Completed,
        };
        self.turn_finished = true;
        out.push(ProviderEventPayload::TurnFinished(TurnFinishedPayload {
            status,
            reason,
        }));
        out
    }
}

/// Spec §7.3 실패 분류. 알 수 없으면 `Provider`로 두고 성공으로 보지 않습니다.
fn classify(status: Option<u64>, api_error: Option<&str>, reason: Option<&str>) -> FailureKind {
    match (status, api_error, reason) {
        (Some(401 | 403), _, _) => FailureKind::Authentication,
        (_, Some(code), _) if code.contains("auth") => FailureKind::Authentication,
        (Some(404), _, _) | (_, Some("model_not_found" | "invalid_request"), _) => {
            FailureKind::Configuration
        }
        (Some(429), _, _) | (_, Some("rate_limit"), _) | (_, _, Some("max_turns")) => {
            FailureKind::ResourceLimit
        }
        _ => FailureKind::Provider,
    }
}

fn str_field<'a>(object: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    object.get(key).and_then(Value::as_str)
}

fn u64_field(object: &Map<String, Value>, key: &str) -> Option<u64> {
    object.get(key).and_then(Value::as_u64)
}

fn content(object: &Map<String, Value>) -> impl Iterator<Item = &Map<String, Value>> {
    object
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
}

fn truncate(text: &str) -> String {
    match text.char_indices().nth(MESSAGE_LIMIT) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_owned(),
    }
}

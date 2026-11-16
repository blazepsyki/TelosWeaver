//! Claude Code `--output-format stream-json` 한 줄을 공통 이벤트 Payload로 바꿉니다 (Spec §7.2, PROV-03).
//!
//! 2026-10-24 Claude Code 2.1.310에서 관찰한 형식을 기준으로 합니다 (증거: `evidence/PH0-CLAUDE`).
//! - `system`/`init`: 세션 시작. Turn마다 다시 올 수 있으며 첫 번째만 `session_started`로 보냅니다.
//! - `assistant`: `content[]`의 `text`는 응답 본문, `tool_use`는 도구 시작입니다. `thinking`은 보내지 않습니다.
//! - `user`: `content[]`의 `tool_result`는 도구 완료입니다.
//! - `system`/`permission_denied`: 권한 거부. 뒤따르는 `tool_result`를 `denied`로 표시합니다.
//! - `result`: Turn 완료와 사용량. `is_error`이면 오류 분류를 함께 보냅니다.
//! - `control_request`/`can_use_tool`: `--permission-prompt-tool stdio`의 실시간 승인 요청입니다 (2.1.325 관찰, PH1-CLAUDE L2).
//!   `approval_required`로 보내고 응답에 쓸 인자를 함께 돌려줍니다. `control_cancel_request`는 CLI가 요청을 거둔 것입니다.
//! - `system`/`init`의 `mcp_servers`: 주입한 Task MCP 서버가 `connected`가 아니면 알립니다 (P4).
//! - 그 밖의 `type`(`rate_limit_event`, `system`/`thinking_tokens`, `control_response` 등)은 무시합니다. 새 형식에 대한 호환을 위해서입니다.

use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use telos_schema::{
    ApprovalRequiredPayload, ErrorPayload, FailureKind, ProviderEventPayload,
    SessionStartedPayload, TextChannel, TextDeltaPayload, ToolCompletedPayload, ToolOutcome,
    ToolStartedPayload, TurnFinishedPayload, TurnStatus, UsagePayload, UsageScope, UsageSource,
};

/// 오류 메시지에 Provider 원문을 넣을 때의 상한입니다. 응답 본문 전체를 오류로 복사하지 않습니다.
const MESSAGE_LIMIT: usize = 240;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProtocolError(pub String);

/// Stream 한 줄에서 나온 항목입니다.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Mapped {
    Event(ProviderEventPayload),
    /// 실시간 승인 요청. `payload`를 이벤트로 내고, Allow 응답에 `input`을 그대로 돌려줍니다.
    Approval(ApprovalRequest),
    /// CLI가 답을 기다리지 않게 된 승인 요청입니다.
    ApprovalWithdrawn {
        request_id: String,
    },
    /// Adapter가 처리하지 않는 제어 요청입니다. 오류로 답해 CLI가 기다리지 않게 합니다.
    UnsupportedControl {
        request_id: String,
    },
    /// 주입한 Task MCP 서버가 연결되지 않았습니다.
    TaskMcpUnavailable {
        server: String,
        status: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ApprovalRequest {
    pub payload: ApprovalRequiredPayload,
    pub tool_use_id: Option<String>,
    pub input: Value,
}

#[derive(Debug, Default)]
pub(crate) struct StreamMapper {
    /// 연결되어 있어야 하는 Task MCP 서버 이름
    task_mcp_server: Option<String>,
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

    pub fn new(task_mcp_server: Option<String>) -> Self {
        Self {
            task_mcp_server,
            ..Self::default()
        }
    }

    /// 승인 응답으로 거부한 도구 호출입니다. 뒤따르는 `tool_result`를 `denied`로 표시합니다.
    /// CLI는 이 경로의 거부에 `permission_denied`를 보내지 않습니다(2.1.325 관찰).
    pub fn mark_denied(&mut self, tool_use_id: String) {
        self.denied.insert(tool_use_id);
    }

    pub fn map_line(&mut self, line: &[u8]) -> Result<Vec<Mapped>, ProtocolError> {
        let value: Value = serde_json::from_slice(line)
            .map_err(|error| ProtocolError(format!("invalid JSON line: {error}")))?;
        let object = value
            .as_object()
            .ok_or_else(|| ProtocolError("stream line is not a JSON object".into()))?;
        let kind = str_field(object, "type")
            .ok_or_else(|| ProtocolError("stream line has no type".into()))?;
        let events = match kind {
            "system" => return Ok(self.system(object)),
            "control_request" => return self.control_request(object).map(|item| vec![item]),
            "control_cancel_request" => {
                return Ok(str_field(object, "request_id")
                    .map(|id| Mapped::ApprovalWithdrawn {
                        request_id: id.to_owned(),
                    })
                    .into_iter()
                    .collect());
            }
            "assistant" => self.assistant(object),
            "user" => self.user(object),
            "result" => self.result(object),
            _ => Vec::new(),
        };
        Ok(events.into_iter().map(Mapped::Event).collect())
    }

    fn system(&mut self, object: &Map<String, Value>) -> Vec<Mapped> {
        match str_field(object, "subtype") {
            Some("init") if self.session_id.is_none() => {
                self.session_id = str_field(object, "session_id").map(str::to_owned);
                let mut out = vec![Mapped::Event(ProviderEventPayload::SessionStarted(
                    SessionStartedPayload {
                        provider_version: str_field(object, "claude_code_version")
                            .map(str::to_owned),
                        model: str_field(object, "model").map(str::to_owned),
                    },
                ))];
                // `-p`와 `--mcp-config`에서는 CLI가 첫 Turn 전에 서버 연결을 기다리므로 여기서 상태가 정해져 있습니다.
                if let Some(server) = &self.task_mcp_server {
                    let status = object
                        .get("mcp_servers")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_object)
                        .find(|entry| str_field(entry, "name") == Some(server))
                        .map(|entry| str_field(entry, "status").unwrap_or("unknown"))
                        .unwrap_or("missing");
                    if status != "connected" {
                        out.push(Mapped::TaskMcpUnavailable {
                            server: server.clone(),
                            status: status.to_owned(),
                        });
                    }
                }
                out
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

    fn control_request(&mut self, object: &Map<String, Value>) -> Result<Mapped, ProtocolError> {
        // 답할 요청 ID가 없으면 CLI가 계속 기다립니다. 정상 메시지로 넘기지 않습니다.
        let request_id = str_field(object, "request_id")
            .ok_or_else(|| ProtocolError("control request has no request_id".into()))?
            .to_owned();
        let request = match object.get("request").and_then(Value::as_object) {
            Some(request) if str_field(request, "subtype") == Some("can_use_tool") => request,
            _ => return Ok(Mapped::UnsupportedControl { request_id }),
        };
        let tool = str_field(request, "tool_name")
            .ok_or_else(|| ProtocolError("approval request has no tool_name".into()))?;
        let input = request.get("input").cloned().unwrap_or(Value::Null);
        Ok(Mapped::Approval(ApprovalRequest {
            payload: ApprovalRequiredPayload {
                request_id,
                action: approval_action(tool).into(),
                summary: format!("Claude Code requested permission to use {tool}"),
                answerable: true,
                target: Some(approval_target(tool, &input)),
                args_digest: Some(args_digest(&input)),
            },
            tool_use_id: str_field(request, "tool_use_id").map(str::to_owned),
            input,
        }))
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
            // `interrupt` 제어 요청으로 끊긴 Turn은 `aborted_tools` 등으로 보고됩니다(2.1.325 관찰).
            (_, Some(reason)) if reason == "interrupted" || reason.starts_with("aborted") => {
                TurnStatus::Interrupted
            }
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

/// 승인 요청의 행위 분류입니다. Codex Adapter가 이미 쓰는 이름(`command_execution`·`file_change`)을 같은 뜻으로 쓰고,
/// MCP 도구는 `mcp_tool_call`, 그 밖은 `tool_use`입니다. 도구 이름은 `summary`에 남깁니다.
fn approval_action(tool: &str) -> &'static str {
    match tool {
        "Bash" | "PowerShell" => "command_execution",
        "Write" | "Edit" | "MultiEdit" | "NotebookEdit" => "file_change",
        _ if tool.starts_with("mcp__") => "mcp_tool_call",
        _ => "tool_use",
    }
}

/// 행위 대상입니다. 명령·파일 경로·URL처럼 대상을 가리키는 인자가 있으면 그 값을, 없으면 도구 이름을 씁니다.
fn approval_target(tool: &str, input: &Value) -> String {
    ["command", "file_path", "notebook_path", "path", "url"]
        .iter()
        .find_map(|key| input.get(key).and_then(Value::as_str))
        .filter(|target| !target.is_empty())
        .unwrap_or(tool)
        .to_owned()
}

/// 인자의 SHA-256(`sha256:<hex>`)입니다. CLI가 보낸 인자 JSON을 키 순서로 정렬한 압축 표기로 바꿔 계산하므로
/// 같은 인자는 표기 순서와 관계없이 같은 값이 됩니다.
pub(crate) fn args_digest(input: &Value) -> String {
    let mut canonical = Vec::new();
    write_canonical(input, &mut canonical);
    let hex: String = Sha256::digest(&canonical)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("sha256:{hex}")
}

fn write_canonical(value: &Value, out: &mut Vec<u8>) {
    match value {
        Value::Array(items) => {
            out.push(b'[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(b',');
                }
                write_canonical(item, out);
            }
            out.push(b']');
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_unstable();
            out.push(b'{');
            for (index, key) in keys.into_iter().enumerate() {
                if index > 0 {
                    out.push(b',');
                }
                out.extend(Value::String(key.clone()).to_string().into_bytes());
                out.push(b':');
                write_canonical(&map[key], out);
            }
            out.push(b'}');
        }
        scalar => out.extend(scalar.to_string().into_bytes()),
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

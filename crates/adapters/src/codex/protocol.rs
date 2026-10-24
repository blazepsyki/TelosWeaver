//! Codex notifications to G0 payloads; native messages stay provider-local (Spec §7.2).
use super::rpc::{error, remote_error};
use crate::AdapterError;
use serde_json::Value;
use std::collections::HashSet;
use telos_schema::*;

pub(super) struct Mapped {
    pub payload: ProviderEventPayload,
    pub tool_id: Option<String>,
}
impl From<ProviderEventPayload> for Mapped {
    fn from(payload: ProviderEventPayload) -> Self {
        Self {
            payload,
            tool_id: None,
        }
    }
}
pub(super) struct Mapper {
    thread: String,
    turn: String,
    rendered: HashSet<String>,
    started: HashSet<String>,
    completed: HashSet<String>,
    usage: Option<UsagePayload>,
    total: Option<UsagePayload>,
    pub finished: bool,
    pub status: Option<TurnStatus>,
}
impl Mapper {
    pub fn new(thread: String, turn: String) -> Self {
        Self {
            thread,
            turn,
            rendered: HashSet::new(),
            started: HashSet::new(),
            completed: HashSet::new(),
            usage: None,
            total: None,
            finished: false,
            status: None,
        }
    }
    pub fn relevant(&self, params: &Value) -> bool {
        params
            .get("threadId")
            .and_then(Value::as_str)
            .is_none_or(|id| id == self.thread)
            && params
                .get("turnId")
                .and_then(Value::as_str)
                .is_none_or(|id| id == self.turn)
    }
    pub fn denied(&mut self, id: &str, name: &str) -> Vec<Mapped> {
        let mut values = Vec::new();
        if self.started.insert(id.into()) {
            values.push(Mapped {
                payload: ProviderEventPayload::ToolStarted(ToolStartedPayload {
                    name: name.into(),
                    summary: None,
                }),
                tool_id: Some(id.into()),
            });
        }
        if self.completed.insert(id.into()) {
            values.push(Mapped {
                payload: ProviderEventPayload::ToolCompleted(ToolCompletedPayload {
                    name: name.into(),
                    outcome: ToolOutcome::Denied,
                    summary: None,
                }),
                tool_id: Some(id.into()),
            });
        }
        values
    }
    pub fn map(&mut self, method: &str, params: &Value) -> Result<Vec<Mapped>, AdapterError> {
        if !self.relevant(params) {
            return Ok(vec![]);
        }
        let mut result = Vec::new();
        match method {
            "item/agentMessage/delta" => {
                if self.finished {
                    return Err(error(
                        FailureKind::ProtocolParse,
                        "text arrived after the turn finished",
                    ));
                }
                let id = required(params, "itemId")?;
                let text = required(params, "delta")?;
                self.rendered.insert(id.into());
                text_chunks(text, &mut result);
            }
            "item/started" | "item/completed" => {
                let item = params
                    .get("item")
                    .ok_or_else(|| error(FailureKind::ProtocolParse, "missing Codex item"))?;
                let kind = required(item, "type")?;
                let id = required(item, "id")?;
                if kind == "agentMessage"
                    && method == "item/completed"
                    && self.rendered.insert(id.into())
                {
                    if self.finished {
                        return Err(error(
                            FailureKind::ProtocolParse,
                            "message arrived after the turn finished",
                        ));
                    }
                    text_chunks(required(item, "text")?, &mut result);
                }
                if let Some(name) = tool_name(kind) {
                    if self.finished {
                        return Err(error(
                            FailureKind::ProtocolParse,
                            "tool arrived after the turn finished",
                        ));
                    }
                    if self.started.insert(id.into()) {
                        result.push(Mapped {
                            payload: ProviderEventPayload::ToolStarted(ToolStartedPayload {
                                name: name.into(),
                                summary: None,
                            }),
                            tool_id: Some(id.into()),
                        });
                    }
                    if method == "item/completed" && self.completed.insert(id.into()) {
                        let status = required(item, "status")?;
                        let outcome = match status {
                            "completed"
                                if item
                                    .get("exitCode")
                                    .and_then(Value::as_i64)
                                    .is_none_or(|code| code == 0) =>
                            {
                                ToolOutcome::Succeeded
                            }
                            "completed" | "failed" => ToolOutcome::Failed,
                            "declined" | "denied" => ToolOutcome::Denied,
                            _ => ToolOutcome::Unknown,
                        };
                        result.push(Mapped {
                            payload: ProviderEventPayload::ToolCompleted(ToolCompletedPayload {
                                name: name.into(),
                                outcome,
                                summary: None,
                            }),
                            tool_id: Some(id.into()),
                        });
                    }
                }
            }
            "thread/tokenUsage/updated" => {
                let last = usage_counts(&params["tokenUsage"]["last"])?;
                let total = usage_counts(&params["tokenUsage"]["total"])?;
                if self.total.as_ref() == Some(&total) {
                    return Ok(result);
                }
                // `last` is one model response, while `total` includes earlier Turns.
                // Anchor the first response of this Turn, then add only total deltas.
                let usage = if let (Some(previous), Some(accumulated)) = (&self.total, &self.usage)
                {
                    UsagePayload {
                        input_tokens: add_usage(
                            accumulated.input_tokens,
                            previous.input_tokens,
                            total.input_tokens,
                        )?,
                        output_tokens: add_usage(
                            accumulated.output_tokens,
                            previous.output_tokens,
                            total.output_tokens,
                        )?,
                        cache_read_tokens: add_usage(
                            accumulated.cache_read_tokens,
                            previous.cache_read_tokens,
                            total.cache_read_tokens,
                        )?,
                        cache_write_tokens: add_usage(
                            accumulated.cache_write_tokens,
                            previous.cache_write_tokens,
                            total.cache_write_tokens,
                        )?,
                        ..last
                    }
                } else {
                    last
                };
                self.total = Some(total);
                self.usage = Some(usage.clone());
                result.push(ProviderEventPayload::Usage(usage).into());
            }
            "error" => {
                let remote = params.get("error").unwrap_or(params);
                let failure = remote_error(remote);
                result.push(
                    ProviderEventPayload::Error(ErrorPayload {
                        kind: failure.failure_kind().unwrap_or(FailureKind::Unknown),
                        message: "Codex reported an execution error".into(),
                        terminal: false,
                    })
                    .into(),
                );
            }
            "turn/completed" => {
                if self.finished {
                    return Err(error(
                        FailureKind::ProtocolParse,
                        "duplicate Codex turn completion",
                    ));
                }
                let turn = params
                    .get("turn")
                    .ok_or_else(|| error(FailureKind::ProtocolParse, "missing completed turn"))?;
                if required(turn, "id")? != self.turn {
                    return Err(error(
                        FailureKind::ProtocolParse,
                        "completed turn ID does not match",
                    ));
                }
                let status = match required(turn, "status")? {
                    "completed" => TurnStatus::Completed,
                    "interrupted" => TurnStatus::Cancelled,
                    "failed" => TurnStatus::Failed,
                    _ => {
                        return Err(error(
                            FailureKind::ProtocolParse,
                            "invalid completed turn status",
                        ));
                    }
                };
                if status == TurnStatus::Failed {
                    let failure = remote_error(&turn["error"]);
                    result.push(
                        ProviderEventPayload::Error(ErrorPayload {
                            kind: failure.failure_kind().unwrap_or(FailureKind::Provider),
                            message: "Codex turn failed".into(),
                            terminal: false,
                        })
                        .into(),
                    );
                }
                self.finished = true;
                self.status = Some(status);
                result.push(
                    ProviderEventPayload::TurnFinished(TurnFinishedPayload {
                        status,
                        reason: None,
                    })
                    .into(),
                );
            }
            _ => {}
        }
        Ok(result)
    }
}
fn usage_counts(value: &Value) -> Result<UsagePayload, AdapterError> {
    let usage = value
        .as_object()
        .ok_or_else(|| error(FailureKind::ProtocolParse, "invalid Codex token usage"))?;
    let count = |key: &str| match usage.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .map(Some)
            .ok_or_else(|| error(FailureKind::ProtocolParse, "invalid Codex token count")),
    };
    Ok(UsagePayload {
        scope: UsageScope::Cumulative,
        source: UsageSource::ProviderReported,
        input_tokens: count("inputTokens")?,
        output_tokens: count("outputTokens")?,
        cache_read_tokens: count("cachedInputTokens")?,
        cache_write_tokens: count("cacheWriteInputTokens")?,
        cache_included_in_input: None,
        cost_usd: None,
    })
}
fn add_usage(
    accumulated: Option<u64>,
    previous: Option<u64>,
    current: Option<u64>,
) -> Result<Option<u64>, AdapterError> {
    match (accumulated, previous, current) {
        (Some(accumulated), Some(previous), Some(current)) => {
            let delta = current.checked_sub(previous).ok_or_else(|| {
                error(
                    FailureKind::ProtocolParse,
                    "Codex total token usage regressed",
                )
            })?;
            accumulated
                .checked_add(delta)
                .map(Some)
                .ok_or_else(|| error(FailureKind::ResourceLimit, "Codex token usage overflowed"))
        }
        _ => Ok(None),
    }
}
pub(super) fn required<'a>(value: &'a Value, key: &str) -> Result<&'a str, AdapterError> {
    value.get(key).and_then(Value::as_str).ok_or_else(|| {
        error(
            FailureKind::ProtocolParse,
            "required Codex string field is missing",
        )
    })
}
fn tool_name(kind: &str) -> Option<&'static str> {
    match kind {
        "commandExecution" => Some("command_execution"),
        "fileChange" => Some("file_change"),
        "mcpToolCall" => Some("mcp_tool"),
        "dynamicToolCall" => Some("dynamic_tool"),
        "webSearch" => Some("web_search"),
        _ => None,
    }
}
fn text_chunks(mut text: &str, out: &mut Vec<Mapped>) {
    while !text.is_empty() {
        let mut end = text.len().min(65536);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        out.push(
            ProviderEventPayload::TextDelta(TextDeltaPayload {
                channel: TextChannel::Assistant,
                text: text[..end].into(),
            })
            .into(),
        );
        text = &text[end..];
    }
}

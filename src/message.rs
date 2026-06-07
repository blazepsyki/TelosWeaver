use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageSource {
    Stdout,
    Stderr,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessMessage {
    pub id: Uuid,
    pub tool_id: String,
    pub source: MessageSource,
    pub level: MessageLevel,
    pub step_id: String,
    pub correlation_id: String,
    pub timestamp: DateTime<Utc>,
    pub raw_output: String,
    pub normalized_payload: Value,
    pub truncated: bool,
}

impl HarnessMessage {
    pub fn new(
        tool_id: impl Into<String>,
        source: MessageSource,
        level: MessageLevel,
        step_id: impl Into<String>,
        correlation_id: impl Into<String>,
        raw_output: impl Into<String>,
        normalized_payload: Value,
        truncated: bool,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            tool_id: tool_id.into(),
            source,
            level,
            step_id: step_id.into(),
            correlation_id: correlation_id.into(),
            timestamp: Utc::now(),
            raw_output: raw_output.into(),
            normalized_payload,
            truncated,
        }
    }

    pub fn system(
        step_id: impl Into<String>,
        correlation_id: impl Into<String>,
        level: MessageLevel,
        raw_output: impl Into<String>,
        normalized_payload: Value,
    ) -> Self {
        Self::new(
            "system",
            MessageSource::System,
            level,
            step_id,
            correlation_id,
            raw_output,
            normalized_payload,
            false,
        )
    }

    pub fn marked_truncated(mut self) -> Self {
        self.truncated = true;
        self
    }
}

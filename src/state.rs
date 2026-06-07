use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::message::HarnessMessage;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActiveTool {
    pub tool_id: String,
    pub step_id: String,
    pub correlation_id: String,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SystemStatus {
    Idle,
    Running,
    ErrorRecovering,
    PausedForIntervention,
    Halted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InterventionChoice {
    Proceed,
    Revise,
    Abort,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InterventionRequest {
    pub step_id: String,
    pub options: Vec<InterventionChoice>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CircuitBreakerState {
    pub threshold: u8,
    pub errors_by_step: HashMap<String, u8>,
    pub tripped_step: Option<String>,
}

impl Default for CircuitBreakerState {
    fn default() -> Self {
        Self {
            threshold: 3,
            errors_by_step: HashMap::new(),
            tripped_step: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppState {
    pub current_tool: Option<String>,
    pub active_tool: Option<ActiveTool>,
    pub current_step: Option<String>,
    pub status: SystemStatus,
    pub context_log: Vec<HarnessMessage>,
    pub breaker: CircuitBreakerState,
    pub intervention_required: Option<InterventionRequest>,
    pub started_at: DateTime<Utc>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            current_tool: None,
            active_tool: None,
            current_step: None,
            status: SystemStatus::Idle,
            context_log: Vec::new(),
            breaker: CircuitBreakerState::default(),
            intervention_required: None,
            started_at: Utc::now(),
        }
    }

    pub fn begin_tool_run(
        &mut self,
        tool_id: impl Into<String>,
        step_id: impl Into<String>,
        correlation_id: impl Into<String>,
    ) {
        let tool_id = tool_id.into();
        let step_id = step_id.into();
        let correlation_id = correlation_id.into();

        self.current_tool = Some(tool_id.clone());
        self.active_tool = Some(ActiveTool {
            tool_id,
            step_id: step_id.clone(),
            correlation_id,
            started_at: Utc::now(),
        });
        self.current_step = Some(step_id);
        self.status = SystemStatus::Running;
    }

    pub fn finish_tool_run(&mut self) {
        self.current_tool = None;
        self.active_tool = None;
        self.current_step = None;

        if self.intervention_required.is_none() {
            self.status = SystemStatus::Idle;
        }
    }

    pub fn push_context(&mut self, message: HarnessMessage) {
        self.context_log.push(message);
    }

    pub fn record_step_error(
        &mut self,
        step_id: impl Into<String>,
        reason: impl Into<String>,
    ) -> bool {
        let step_id = step_id.into();
        let count = self
            .breaker
            .errors_by_step
            .entry(step_id.clone())
            .or_insert(0);
        *count = count.saturating_add(1);

        if *count >= self.breaker.threshold {
            self.breaker.tripped_step = Some(step_id.clone());
            self.status = SystemStatus::PausedForIntervention;
            self.intervention_required = Some(InterventionRequest {
                step_id,
                options: vec![
                    InterventionChoice::Proceed,
                    InterventionChoice::Revise,
                    InterventionChoice::Abort,
                ],
                reason: reason.into(),
            });
            return true;
        }

        self.status = SystemStatus::ErrorRecovering;
        false
    }

    pub fn resolve_intervention(&mut self) {
        self.intervention_required = None;
        self.breaker.tripped_step = None;
        self.status = if self.current_tool.is_some() {
            SystemStatus::Running
        } else {
            SystemStatus::Idle
        };
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

pub type SharedAppState = Arc<RwLock<AppState>>;

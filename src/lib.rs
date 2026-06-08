pub mod config;
pub mod execution;
pub mod message;
pub mod orchestration;
pub mod state;
pub mod tui;
pub mod web;

pub use config::{Config, SupervisorDefinition, ToolDefinition, ToolParser};
pub use execution::{execute_tool, parse_with_rhai, ExecutionEngine, ExecutionError};
pub use message::{HarnessMessage, MessageLevel, MessageSource};
pub use orchestration::{
    parse_supervisor_decision, run_orchestration_loop, write_harness_context_file,
    HarnessContextSnapshot, OrchestrationError, SupervisorDecision, WorkerInvocation,
    CONTEXT_FILE_NAME,
};
pub use state::{
    ActiveTool, AppState, CircuitBreakerState, InterventionChoice, InterventionRequest,
    SharedAppState, SystemStatus,
};

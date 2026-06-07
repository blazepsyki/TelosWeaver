use rust_multi_cli_harness::{AppState, SystemStatus};

#[test]
fn app_state_defaults_to_idle_and_empty_context() {
    let state = AppState::new();

    assert_eq!(state.status, SystemStatus::Idle);
    assert!(state.current_tool.is_none());
    assert!(state.current_step.is_none());
    assert!(state.intervention_required.is_none());
    assert!(state.context_log.is_empty());
    assert_eq!(state.breaker.threshold, 3);
}

#[test]
fn app_state_can_track_tool_lifecycle() {
    let mut state = AppState::new();

    state.begin_tool_run("codex-cli", "step-1", "corr-1");

    assert_eq!(state.status, SystemStatus::Running);
    let active_tool = state.active_tool.as_ref().expect("tool should be active");
    assert_eq!(active_tool.tool_id, "codex-cli");
    assert_eq!(active_tool.step_id, "step-1");
    assert_eq!(active_tool.correlation_id, "corr-1");

    state.finish_tool_run();

    assert!(state.active_tool.is_none());
    assert_eq!(state.status, SystemStatus::Idle);
}

#[test]
fn app_state_trips_circuit_breaker_after_repeated_errors() {
    let mut state = AppState::new();

    assert!(!state.record_step_error("step-1", "first failure"));
    assert_eq!(state.status, SystemStatus::ErrorRecovering);

    assert!(!state.record_step_error("step-1", "second failure"));
    assert_eq!(state.status, SystemStatus::ErrorRecovering);

    assert!(state.record_step_error("step-1", "third failure"));
    assert_eq!(state.status, SystemStatus::PausedForIntervention);
    assert!(state.intervention_required.is_some());
    assert_eq!(state.breaker.tripped_step.as_deref(), Some("step-1"));

    state.resolve_intervention();

    assert!(state.intervention_required.is_none());
    assert_eq!(state.status, SystemStatus::Idle);
}

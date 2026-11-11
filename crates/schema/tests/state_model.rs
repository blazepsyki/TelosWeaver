//! H0 Task·Run·Session 모델의 전이 규칙과 Wire 형식 (Spec §9.4, §19.6.2).

use serde_json::json;
use telos_schema::{fixtures, *};

const ALL_TASK: [TaskStatus; 17] = [
    TaskStatus::Created,
    TaskStatus::Blocked,
    TaskStatus::Ready,
    TaskStatus::Running,
    TaskStatus::WaitingChild,
    TaskStatus::WaitingMaster,
    TaskStatus::WaitingUser,
    TaskStatus::Executed,
    TaskStatus::Validating,
    TaskStatus::Verified,
    TaskStatus::Completed,
    TaskStatus::Failed,
    TaskStatus::Skipped,
    TaskStatus::Cancelling,
    TaskStatus::Cancelled,
    TaskStatus::Interrupted,
    TaskStatus::NeedsReconciliation,
];

#[test]
fn default_task_path_is_allowed_in_order() {
    use TaskStatus::*;
    let path = [
        Created, Ready, Running, Executed, Validating, Verified, Completed,
    ];
    for pair in path.windows(2) {
        assert!(
            pair[0].can_transition_to(pair[1]),
            "{:?} -> {:?}",
            pair[0],
            pair[1]
        );
    }
}

#[test]
fn terminal_task_states_never_change() {
    for from in ALL_TASK.into_iter().filter(|s| s.is_terminal()) {
        for to in ALL_TASK {
            assert!(!from.can_transition_to(to), "{from:?} -> {to:?}");
        }
    }
}

#[test]
fn completion_requires_executed_or_verified() {
    use TaskStatus::*;
    // 실행 성공만으로 완료를 건너뛰지 않습니다 (§5.3, §9.4).
    for from in ALL_TASK {
        let allowed = from.can_transition_to(Completed);
        assert_eq!(allowed, matches!(from, Executed | Verified), "{from:?}");
    }
}

#[test]
fn cancellation_waits_for_actual_stop_while_executing() {
    use TaskStatus::*;
    for from in [
        Running,
        Validating,
        WaitingChild,
        WaitingMaster,
        WaitingUser,
    ] {
        assert!(from.can_transition_to(Cancelling), "{from:?}");
        assert!(
            !from.can_transition_to(Cancelled),
            "{from:?} must stop first"
        );
    }
    for from in [Created, Blocked, Ready, Executed, Verified] {
        assert!(from.can_transition_to(Cancelled), "{from:?}");
    }
    assert!(Cancelling.can_transition_to(Cancelled));
    // 정지 확인이 안 되면 대조로 넘깁니다. 완료로 되돌리지 않습니다 (§9.5).
    assert!(Cancelling.can_transition_to(NeedsReconciliation));
    assert!(!Cancelling.can_transition_to(Completed));
}

#[test]
fn lost_or_unknown_results_are_not_retried_blindly() {
    use TaskStatus::*;
    assert!(Running.can_transition_to(Interrupted));
    assert!(Running.can_transition_to(NeedsReconciliation));
    assert!(!Interrupted.can_transition_to(Completed));
    assert!(!NeedsReconciliation.can_transition_to(Completed));
    assert!(!NeedsReconciliation.can_transition_to(Running));
}

#[test]
fn run_status_is_final_once_terminal_and_cancel_keeps_real_result() {
    use RunStatus::*;
    let all = [
        Starting,
        Running,
        Waiting,
        Succeeded,
        Failed,
        Cancelling,
        Cancelled,
        Interrupted,
        Unknown,
    ];
    for from in all.into_iter().filter(|s| s.is_terminal()) {
        for to in all {
            assert!(!from.can_transition_to(to), "{from:?} -> {to:?}");
        }
    }
    assert!(Cancelling.can_transition_to(Succeeded));
    assert!(!Starting.can_transition_to(Succeeded));
}

#[test]
fn task_record_wire_format() {
    let record = TaskRecord {
        spec: fixtures::task_spec(),
        status: TaskStatus::WaitingChild,
        state_version: 3,
        wait: Some(WaitState {
            resume_status: TaskStatus::Running,
            waiting_for: vec!["task-child".into()],
            deadline: None,
        }),
        cancel_propagation: CancelPropagation::Descendants,
        delegation_depth: 1,
        created_at: "2026-11-10T00:00:00Z".into(),
        updated_at: "2026-11-10T00:00:01Z".into(),
    };
    let value = serde_json::to_value(&record).unwrap();
    assert_eq!(value["status"], "WaitingChild");
    assert_eq!(value["stateVersion"], 3);
    assert_eq!(
        value["wait"],
        json!({"resumeStatus": "Running", "waitingFor": ["task-child"]})
    );
    assert_eq!(value["cancelPropagation"], "descendants");
    let back: TaskRecord = serde_json::from_value(value).unwrap();
    assert_eq!(back, record);
}

#[test]
fn session_use_and_plan_status_wire_names() {
    assert_eq!(serde_json::to_value(SessionUse::Forked).unwrap(), "forked");
    assert_eq!(serde_json::to_value(PlanStatus::Held).unwrap(), "Held");
}

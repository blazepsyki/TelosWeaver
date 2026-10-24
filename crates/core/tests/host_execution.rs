#[path = "fixtures/host/support.rs"]
mod support;
use futures::StreamExt;
use std::{
    sync::{Arc, atomic::Ordering},
    time::Duration,
};
use support::*;
use telos_core::execution::{DenyRuns, ExecutionSupervisor};
use telos_schema::{
    RunStatus,
    api::ApiErrorCode,
    fixtures::{HOST_ID, PROJECT_ID},
};

#[tokio::test]
async fn duplicate_start_is_single_execution_and_changed_content_conflicts() {
    let (execution, adapter) = supervisor();
    let (first, second) = tokio::join!(
        execution.start(params("ok", "same")),
        execution.start(params("ok", "same"))
    );
    let first = first.unwrap();
    assert_eq!(first, second.unwrap());
    assert_eq!(adapter.starts.load(Ordering::SeqCst), 1);
    assert_eq!(
        execution
            .start(params("changed", "same"))
            .await
            .unwrap_err()
            .code,
        ApiErrorCode::Conflict
    );
    let events: Vec<_> = execution
        .subscribe(&first.run_id, 0)
        .unwrap()
        .collect()
        .await;
    assert_eq!(events.len(), 4);
    assert!(events.iter().all(Result::is_ok));
    assert_eq!(
        execution.inspect(&first.run_id).unwrap().status,
        RunStatus::Succeeded
    );
    let replay: Vec<_> = execution
        .subscribe(&first.run_id, 2)
        .unwrap()
        .collect()
        .await;
    assert_eq!(
        replay
            .iter()
            .map(|e| e.as_ref().unwrap().sequence)
            .collect::<Vec<_>>(),
        vec![3, 4]
    );
    assert_eq!(
        execution.subscribe(&first.run_id, 5).err().unwrap().code,
        ApiErrorCode::Conflict
    );
}
#[tokio::test]
async fn terminal_boundaries_fail_closed_and_persistent_turn_can_complete() {
    let (execution, _) = supervisor();
    for (mode, status, stream_error) in [
        ("missing", RunStatus::Interrupted, true),
        ("gap", RunStatus::Interrupted, true),
        ("unknown", RunStatus::Interrupted, false),
        ("start-failed", RunStatus::Failed, false),
        ("persistent", RunStatus::Succeeded, false),
    ] {
        let started = execution.start(params(mode, mode)).await.unwrap();
        let events: Vec<_> = tokio::time::timeout(
            Duration::from_secs(2),
            execution.subscribe(&started.run_id, 0).unwrap().collect(),
        )
        .await
        .unwrap();
        assert_eq!(
            execution.inspect(&started.run_id).unwrap().status,
            status,
            "{mode}"
        );
        assert_eq!(events.iter().any(Result::is_err), stream_error, "{mode}");
    }
}
#[tokio::test]
async fn cancel_acknowledgement_and_actual_termination_are_separate() {
    let (execution, _) = supervisor();
    let started = execution.start(params("hold", "cancel")).await.unwrap();
    assert_eq!(
        execution.inspect(&started.run_id).unwrap().status,
        RunStatus::Running
    );
    assert!(execution.cancel(&started.run_id).await.unwrap().accepted);
    let events: Vec<_> = execution
        .subscribe(&started.run_id, 0)
        .unwrap()
        .collect()
        .await;
    assert_eq!(events.len(), 5);
    assert_eq!(
        execution.inspect(&started.run_id).unwrap().status,
        RunStatus::Cancelled
    );
    assert!(!execution.cancel(&started.run_id).await.unwrap().accepted);
}
#[tokio::test]
async fn trusted_policy_and_project_host_boundaries_are_required() {
    let execution = Arc::new(ExecutionSupervisor::new(
        PROJECT_ID.into(),
        HOST_ID.into(),
        vec![],
        Arc::new(DenyRuns),
    ));
    assert_eq!(
        execution
            .start(params("ok", "denied"))
            .await
            .unwrap_err()
            .code,
        ApiErrorCode::Unavailable
    );
    let (execution, adapter) = supervisor();
    let mut request = params("ok", "other");
    request.task.project_id = "other".into();
    assert_eq!(
        execution.start(request).await.unwrap_err().code,
        ApiErrorCode::Forbidden
    );
    let mut request = params("ok", "host");
    request.task.workspace.host_id = "other".into();
    assert_eq!(
        execution.start(request).await.unwrap_err().code,
        ApiErrorCode::Forbidden
    );
    assert_eq!(adapter.starts.load(Ordering::SeqCst), 0);
}

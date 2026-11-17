//! PH1-SCHED: Core 재시작 대조와 대조 판단 (Spec §14.2, §14.3, SESS-05, OPS-03, SAFE-13).

#[path = "fixtures/persistence/mod.rs"]
mod persistence;
#[path = "fixtures/scheduler/mod.rs"]
mod support;

use support::*;
use telos_core::persistence::{StateWrite, Store};
use telos_core::scheduler::{
    Actor, Reconciliation, RecoveryReport, RunAdmission, Scheduler, SchedulerError,
};
use telos_schema::*;

/// Run을 시작한 뒤 종료 보고 없이 Core가 죽은 상황을 만듭니다. 돌려주는 값은 재시작한 Harness와 Run ID입니다.
async fn crashed_while_running(name: &str) -> (Harness, String) {
    let h = Harness::new(name);
    h.register(vec![writer_spec("a", "ws-a"), spec("b", &["a"])])
        .await;
    let run_id = h.start("a").await;
    (h.restart(), run_id)
}

// §14.3: Adapter에 전달하기 전에 죽었으면 외부 실행이 없으므로 의도를 버리고 Task를 되돌립니다.
#[tokio::test]
async fn intent_recorded_but_never_dispatched_returns_to_ready() {
    let h = Harness::new("recorded");
    h.register(vec![spec("a", &[])]).await;
    // 실행 의도를 기록한 직후(전달 전)에 죽은 상태를 그대로 만듭니다.
    let mut running = h.task("a");
    running.status = TaskStatus::Running;
    h.store
        .apply(persistence::batch(vec![
            StateWrite::Task {
                record: running,
                expected_version: Some(1),
            },
            StateWrite::Run {
                record: persistence::run("run-1", "a"),
                expected_version: None,
            },
            StateWrite::Intent {
                intent: persistence::intent("intent-1", "run-1", "a", IntentPhase::Recorded),
            },
            StateWrite::AcquireLock {
                lock: {
                    let mut lock = persistence::lock("lock-1", "ws-a", LockMode::Writer, 1);
                    lock.holder_task_id = "a".into();
                    lock.holder_run_id = "run-1".into();
                    lock
                },
            },
        ]))
        .unwrap();
    let h = h.restart();

    let report = h.scheduler.recover().await.unwrap();
    assert_eq!(report.returned_to_ready, ["a"]);
    assert!(report.interrupted.is_empty() && report.needs_reconciliation.is_empty());
    assert_eq!(h.status("a"), TaskStatus::Ready);
    assert_eq!(h.run("run-1").status, RunStatus::Cancelled);
    assert_eq!(
        h.store.run_intent("run-1").unwrap().unwrap().phase,
        IntentPhase::Abandoned
    );
    assert!(h.store.locks().unwrap().is_empty());
    // 프로세스를 찾아볼 필요가 없습니다.
    assert!(h.executor.probed.lock().unwrap().is_empty());

    // 되돌린 Task는 새 Run으로 실행할 수 있습니다.
    let run_id = h.start("a").await;
    assert_eq!(h.run(&run_id).attempt, 2);
}

// §14.2: 프로세스가 살아 있어도 재시작한 Core는 추적하지 못합니다. Interrupted로 두고 겹쳐 실행하지 않습니다.
#[tokio::test]
async fn live_process_after_restart_is_interrupted_not_rerun() {
    let (h, run_id) = crashed_while_running("alive").await;
    *h.executor.alive.lock().unwrap() = Some(true);

    let report = h.scheduler.recover().await.unwrap();
    assert_eq!(report.interrupted, ["a"]);
    assert_eq!(h.status("a"), TaskStatus::Interrupted);
    assert_eq!(h.run(&run_id).status, RunStatus::Interrupted);
    // PID만이 아니라 기록한 증거 전체로 찾습니다.
    assert_eq!(*h.executor.probed.lock().unwrap(), [evidence(&run_id)]);
    // 살아 있는 Writer의 잠금과 실행 의도는 그대로입니다.
    assert_eq!(h.store.locks().unwrap().len(), 1);
    assert_eq!(h.store.unfinished_intents().unwrap().len(), 1);
    assert!(report.released_locks.is_empty());

    let rerun = h
        .scheduler
        .request_run(Actor::User, "a", fresh(), "again")
        .await;
    assert!(
        matches!(rerun, Err(SchedulerError::InvalidTransition { .. })),
        "{rerun:?}"
    );
    assert!(h.executor.started_runs().is_empty());

    // 다시 대조해도 결과가 같고 기록이 늘지 않습니다.
    let before = h.store.last_sequence().unwrap();
    let again = h.scheduler.recover().await.unwrap();
    assert_eq!(again.interrupted, ["a"]);
    assert_eq!(h.store.last_sequence().unwrap(), before);
}

// §14.3: 프로세스가 없으면 결과를 알 수 없습니다. NeedsReconciliation으로 두고 사람이나 마스터의 판단을 기다립니다.
#[tokio::test]
async fn vanished_process_needs_reconciliation_before_any_rerun() {
    let (h, run_id) = crashed_while_running("gone").await;
    *h.executor.alive.lock().unwrap() = Some(false);

    let report = h.scheduler.recover().await.unwrap();
    assert_eq!(report.needs_reconciliation, ["a"]);
    assert_eq!(h.status("a"), TaskStatus::NeedsReconciliation);
    let run = h.run(&run_id);
    assert_eq!(
        (run.status, run.outcome),
        (RunStatus::Unknown, Some(ExecutionOutcome::Unknown))
    );
    // 결과를 모르는 Run의 잠금과 의도는 대조가 끝날 때까지 둡니다.
    assert_eq!(h.store.locks().unwrap().len(), 1);
    assert_eq!(h.store.unfinished_intents().unwrap().len(), 1);
    let rerun = h
        .scheduler
        .request_run(Actor::User, "a", fresh(), "again")
        .await;
    assert!(
        matches!(rerun, Err(SchedulerError::InvalidTransition { .. })),
        "{rerun:?}"
    );
    assert_eq!(
        h.scheduler.recover().await.unwrap().needs_reconciliation,
        ["a"]
    );

    // 확인한 뒤 결과를 채택: Executed. 의도를 닫고 잠금을 풉니다. 완료는 여전히 별도 판정입니다.
    let accepted = h
        .scheduler
        .reconcile_task(Actor::User, "a", Reconciliation::AcceptExecuted)
        .await
        .unwrap();
    assert_eq!(accepted.status, TaskStatus::Executed);
    assert!(h.store.unfinished_intents().unwrap().is_empty());
    assert!(h.store.locks().unwrap().is_empty());
    assert_eq!(h.run(&run_id).status, RunStatus::Unknown);
    assert_eq!(h.status("b"), TaskStatus::Blocked);
    h.scheduler.complete_task(Actor::User, "a").await.unwrap();
    assert_eq!(h.status("b"), TaskStatus::Ready);

    let not_needed = h
        .scheduler
        .reconcile_task(Actor::User, "a", Reconciliation::Retry)
        .await;
    assert!(
        matches!(not_needed, Err(SchedulerError::InvalidTransition { .. })),
        "{not_needed:?}"
    );
}

// §14.3: 생존을 확인하지 못하면 죽었다고 가정하지 않습니다.
#[tokio::test]
async fn unconfirmed_process_is_not_assumed_dead() {
    let (h, run_id) = crashed_while_running("unknown").await;
    *h.executor.alive.lock().unwrap() = None;

    let report = h.scheduler.recover().await.unwrap();
    assert_eq!(report.needs_reconciliation, ["a"]);
    assert_eq!(h.run(&run_id).status, RunStatus::Unknown);
    assert_eq!(h.store.locks().unwrap().len(), 1);

    // 사용자가 확인하고 다시 실행하기로 함: 이전 Run은 닫히고 새 Run이 시작됩니다.
    let retried = h
        .scheduler
        .reconcile_task(Actor::User, "a", Reconciliation::Retry)
        .await
        .unwrap();
    assert_eq!(retried.status, TaskStatus::Ready);
    assert!(h.store.locks().unwrap().is_empty());
    let second = h.start("a").await;
    assert_eq!(h.run(&second).attempt, 2);
    // 이전 Run의 결과가 뒤늦게 와도 새 Run의 결과가 되지 않습니다.
    h.finish(&run_id, RunStatus::Succeeded).await;
    assert_eq!(h.status("a"), TaskStatus::Running);
    assert_eq!(h.run(&run_id).status, RunStatus::Unknown);
}

// §14.3: 시작 응답을 받기 전에 죽으면 프로세스 증거가 없습니다. 시작했을 수 있으므로 다시 실행하지 않습니다.
#[tokio::test]
async fn dispatched_without_start_result_is_held() {
    let h = Harness::new("dispatched");
    h.register(vec![spec("a", &[])]).await;
    let mut running = h.task("a");
    running.status = TaskStatus::Running;
    h.store
        .apply(persistence::batch(vec![
            StateWrite::Task {
                record: running,
                expected_version: Some(1),
            },
            StateWrite::Run {
                record: persistence::run("run-1", "a"),
                expected_version: None,
            },
            StateWrite::Intent {
                intent: persistence::intent("intent-1", "run-1", "a", IntentPhase::Recorded),
            },
            StateWrite::Intent {
                intent: persistence::intent("intent-1", "run-1", "a", IntentPhase::Dispatched),
            },
        ]))
        .unwrap();
    let h = h.restart();
    *h.executor.alive.lock().unwrap() = Some(true);

    let report = h.scheduler.recover().await.unwrap();
    assert_eq!(report.needs_reconciliation, ["a"]);
    assert!(report.returned_to_ready.is_empty());
    assert_eq!(h.run("run-1").status, RunStatus::Unknown);
    // 증거가 없으면 찾아볼 대상도 없습니다.
    assert!(h.executor.probed.lock().unwrap().is_empty());

    let failed = h
        .scheduler
        .reconcile_task(Actor::User, "a", Reconciliation::Fail)
        .await
        .unwrap();
    assert_eq!(failed.status, TaskStatus::Failed);
    assert!(h.store.unfinished_intents().unwrap().is_empty());
}

// §14.3: 주인 Run이 없거나 확정 종료한 잠금만 풉니다.
#[tokio::test]
async fn only_locks_of_settled_or_missing_runs_are_released() {
    let (h, run_id) = crashed_while_running("locks").await;
    *h.executor.alive.lock().unwrap() = Some(false);
    let mut orphan = persistence::lock("orphan", "ws-orphan", LockMode::Writer, 3);
    orphan.holder_run_id = "run-that-never-existed".into();
    h.store
        .apply(persistence::batch(vec![StateWrite::AcquireLock {
            lock: orphan,
        }]))
        .unwrap();

    let report = h.scheduler.recover().await.unwrap();
    assert_eq!(report.released_locks, ["orphan"]);
    let held = h.store.locks().unwrap();
    assert_eq!(held.len(), 1);
    assert_eq!(held[0].holder_run_id, run_id);
}

// §9.5: 취소 중에 Core가 죽었으면 정지를 확인하지 못했으므로 취소로 확정하지 않습니다.
#[tokio::test]
async fn cancel_in_flight_is_not_confirmed_by_a_restart() {
    let h = Harness::new("cancelling");
    h.register(vec![spec("a", &[])]).await;
    let run_id = h.start("a").await;
    h.scheduler
        .cancel_task(Actor::User, "a", "stop", "c-1")
        .await
        .unwrap();
    assert_eq!(h.status("a"), TaskStatus::Cancelling);
    let h = h.restart();
    *h.executor.alive.lock().unwrap() = Some(false);

    let report = h.scheduler.recover().await.unwrap();
    assert_eq!(report.needs_reconciliation, ["a"]);
    assert_eq!(h.status("a"), TaskStatus::NeedsReconciliation);
    assert_eq!(h.run(&run_id).status, RunStatus::Unknown);

    let cancelled = h
        .scheduler
        .reconcile_task(Actor::User, "a", Reconciliation::Cancel)
        .await
        .unwrap();
    assert_eq!(cancelled.status, TaskStatus::Cancelled);
    assert_eq!(h.ledger().tasks_consumed, 1);
}

// SESS-03, OPS-03: 확정한 결과는 재시작 뒤에도 그대로이고, 대조는 그것을 건드리지 않습니다.
#[tokio::test]
async fn settled_work_is_untouched_by_recovery() {
    let h = Harness::with_limits("settled", limits(1));
    h.register(vec![
        spec("done", &[]),
        spec("next", &["done"]),
        spec("queued", &[]),
    ])
    .await;
    let run_id = h.start("done").await;
    h.finish(&run_id, RunStatus::Succeeded).await;
    h.scheduler
        .complete_task(Actor::User, "done")
        .await
        .unwrap();
    // 자리가 찬 동안 대기열에 들어간 요청
    let busy = h.start("next").await;
    let queued = h
        .scheduler
        .request_run(Actor::User, "queued", fresh(), "k-q")
        .await
        .unwrap();
    assert_eq!(queued, RunAdmission::Queued { position: 1 });
    h.finish(&busy, RunStatus::Succeeded).await;
    let queued_run = h.executor.started_runs()[2].clone();
    h.finish(&queued_run, RunStatus::Failed).await;
    let before = h.store.last_sequence().unwrap();

    let h = h.restart();
    let report = h.scheduler.recover().await.unwrap();
    assert_eq!(report, RecoveryReport::default());
    assert_eq!(h.store.last_sequence().unwrap(), before);
    assert_eq!(h.status("done"), TaskStatus::Completed);
    assert_eq!(h.status("next"), TaskStatus::Executed);
    assert_eq!(h.run(&run_id).status, RunStatus::Succeeded);
    // 실패한 Task는 재시도 상한 안이라 Ready이며, 재시작 뒤 다시 요청해야 실행됩니다.
    assert_eq!(h.status("queued"), TaskStatus::Ready);
    assert!(h.executor.started_runs().is_empty());
}

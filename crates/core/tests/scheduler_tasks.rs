//! PH1-SCHED: Task 등록·의존성·상한·검증 연결·완료 판정 (Spec §9.1, §9.4, TASK-01·05, DELEG-04·09, SAFE-02).

#[path = "fixtures/persistence/mod.rs"]
mod persistence;
#[path = "fixtures/scheduler/mod.rs"]
mod support;

use std::sync::atomic::Ordering;

use support::*;
use telos_core::persistence::{StateWrite, Store};
use telos_core::scheduler::{Actor, Scheduler, SchedulerError, WaitKind};
use telos_schema::*;

fn master(epoch: u64) -> Actor {
    Actor::Master {
        plan_id: PLAN.into(),
        master_run_id: "run-master".into(),
        controller_epoch: epoch,
    }
}

// §9.1 1번, TASK-05: 의존성이 남은 Task는 Blocked, 순환·없는 선행 Task는 등록 전체를 거부합니다.
#[tokio::test]
async fn registration_checks_dependencies_and_cycles() {
    let h = Harness::new("register");
    let records = h
        .register(vec![
            spec("a", &[]),
            spec("b", &["a"]),
            spec("c", &["a", "b"]),
        ])
        .await;
    let statuses: Vec<TaskStatus> = records.iter().map(|r| r.status).collect();
    assert_eq!(
        statuses,
        [TaskStatus::Ready, TaskStatus::Blocked, TaskStatus::Blocked]
    );
    assert!(records.iter().all(|r| r.state_version == 1));
    assert_eq!(h.ledger().tasks_reserved, 3);

    let cycle = h
        .scheduler
        .register_tasks(
            Actor::User,
            vec![spec("x", &["z"]), spec("y", &["x"]), spec("z", &["y"])],
            "cycle",
        )
        .await;
    assert!(
        matches!(&cycle, Err(SchedulerError::DependencyCycle { task_ids }) if task_ids.len() == 4),
        "{cycle:?}"
    );
    let own = h
        .scheduler
        .register_tasks(Actor::User, vec![spec("self", &["self"])], "self")
        .await;
    assert!(
        matches!(own, Err(SchedulerError::DependencyCycle { .. })),
        "{own:?}"
    );
    let missing = h
        .scheduler
        .register_tasks(Actor::User, vec![spec("d", &["nowhere"])], "missing")
        .await;
    assert!(
        matches!(missing, Err(SchedulerError::NotFound { .. })),
        "{missing:?}"
    );
    let duplicate = h
        .scheduler
        .register_tasks(Actor::User, vec![spec("a", &[])], "duplicate")
        .await;
    assert!(
        matches!(duplicate, Err(SchedulerError::InvalidTransition { .. })),
        "{duplicate:?}"
    );

    // 거부된 요청은 아무것도 남기지 않습니다.
    assert_eq!(h.store.plan_tasks(PLAN).unwrap().len(), 3);
    assert_eq!(h.ledger().tasks_reserved, 3);
}

// SAFE-13: 같은 키·같은 내용은 처음 결과를 돌려주고, 다른 내용은 거부합니다.
#[tokio::test]
async fn registration_is_idempotent() {
    let h = Harness::new("register-idem");
    let first = h
        .scheduler
        .register_tasks(Actor::User, vec![spec("a", &[])], "key-1")
        .await
        .unwrap();
    let again = h
        .scheduler
        .register_tasks(Actor::User, vec![spec("a", &[])], "key-1")
        .await
        .unwrap();
    assert_eq!(first, again);
    let other = h
        .scheduler
        .register_tasks(Actor::User, vec![spec("b", &[])], "key-1")
        .await;
    assert!(
        matches!(other, Err(SchedulerError::IdempotencyConflict { .. })),
        "{other:?}"
    );
    assert_eq!(h.store.plan_tasks(PLAN).unwrap().len(), 1);
    assert_eq!(h.ledger().tasks_reserved, 1);
}

// DELEG-04, SAFE-02: 위임 깊이와 Plan 전체 Task 수의 상한
#[tokio::test]
async fn depth_and_plan_task_limits_are_enforced() {
    let h = Harness::new("limits");
    // 깊이 상한 2: 최상위(0) → 자식(1) → 손자(2)까지. 부모가 같은 요청 안에서 뒤에 와도 셉니다.
    let records = h
        .register(vec![
            child_of("grandchild", "child"),
            child_of("child", "root"),
            spec("root", &[]),
        ])
        .await;
    let depths: Vec<u32> = records.iter().map(|r| r.delegation_depth).collect();
    assert_eq!(depths, [2, 1, 0]);
    let too_deep = h
        .scheduler
        .register_tasks(Actor::User, vec![child_of("deeper", "grandchild")], "deep")
        .await;
    assert!(
        matches!(too_deep, Err(SchedulerError::LimitExceeded { .. })),
        "{too_deep:?}"
    );
    let orphan = h
        .scheduler
        .register_tasks(Actor::User, vec![child_of("orphan", "nowhere")], "orphan")
        .await;
    assert!(
        matches!(orphan, Err(SchedulerError::NotFound { .. })),
        "{orphan:?}"
    );

    // 전체 상한 8: 이미 3개, 5개까지 더 받고 그다음은 거부합니다.
    let more: Vec<TaskSpec> = (0..5).map(|i| spec(&format!("t{i}"), &[])).collect();
    h.register(more).await;
    let over = h
        .scheduler
        .register_tasks(Actor::User, vec![spec("one-more", &[])], "over")
        .await;
    assert!(
        matches!(over, Err(SchedulerError::LimitExceeded { .. })),
        "{over:?}"
    );
    assert_eq!(h.ledger().tasks_reserved, 8);
    assert_eq!(h.store.plan_tasks(PLAN).unwrap().len(), 8);
}

// §4.1, MASTER-04: 이전 세대의 마스터와 보류된 Plan의 결정은 집행하지 않습니다.
#[tokio::test]
async fn stale_or_held_master_cannot_change_tasks() {
    let h = Harness::new("master");
    h.scheduler
        .register_tasks(master(1), vec![spec("a", &[])], "m-1")
        .await
        .unwrap();

    let stale = h
        .scheduler
        .register_tasks(master(0), vec![spec("b", &[])], "m-2")
        .await;
    assert!(
        matches!(stale, Err(SchedulerError::Forbidden { .. })),
        "{stale:?}"
    );
    let worker = h
        .scheduler
        .register_tasks(
            Actor::Worker {
                task_id: "a".into(),
                run_id: "run-x".into(),
            },
            vec![spec("c", &[])],
            "w-1",
        )
        .await;
    assert!(
        matches!(worker, Err(SchedulerError::Forbidden { .. })),
        "{worker:?}"
    );

    let mut held = plan();
    held.status = PlanStatus::Held;
    h.store
        .apply(persistence::batch(vec![StateWrite::Plan {
            plan: held,
            expected_version: Some(1),
        }]))
        .unwrap();
    let on_hold = h
        .scheduler
        .request_run(master(1), "a", fresh(), "m-run")
        .await;
    assert!(
        matches!(on_hold, Err(SchedulerError::Forbidden { .. })),
        "{on_hold:?}"
    );
    // 사용자는 보류된 Plan의 Task도 취소할 수 있습니다.
    h.scheduler
        .cancel_task(Actor::User, "a", "stop", "cancel-a")
        .await
        .unwrap();
    assert_eq!(h.status("a"), TaskStatus::Cancelled);
    assert_eq!(h.store.plan_tasks(PLAN).unwrap().len(), 1);
}

// §9.1 6번, §9.4, DELEG-09: Run 성공은 완료가 아니고, 완료 판정 뒤에만 후속 Task가 풀립니다.
#[tokio::test]
async fn completion_is_explicit_and_releases_dependents() {
    let h = Harness::new("complete");
    h.register(vec![spec("a", &[]), spec("b", &["a"])]).await;

    let early = h.scheduler.complete_task(Actor::User, "a").await;
    assert!(
        matches!(early, Err(SchedulerError::InvalidTransition { .. })),
        "{early:?}"
    );

    let run_id = h.start("a").await;
    h.finish(&run_id, RunStatus::Succeeded).await;
    assert_eq!(h.status("a"), TaskStatus::Executed);
    assert_eq!(h.status("b"), TaskStatus::Blocked);

    let done = h.scheduler.complete_task(Actor::User, "a").await.unwrap();
    assert_eq!(done.status, TaskStatus::Completed);
    assert_eq!(h.status("b"), TaskStatus::Ready);
    let ledger = h.ledger();
    assert_eq!((ledger.tasks_reserved, ledger.tasks_consumed), (1, 1));

    // 끝난 Task는 다시 바뀌지 않습니다.
    let again = h.scheduler.complete_task(Actor::User, "a").await;
    assert!(
        matches!(again, Err(SchedulerError::InvalidTransition { .. })),
        "{again:?}"
    );
}

// §9.4, §11.1, VERIFY-02: 필수 검증이 있으면 통과 전에는 완료할 수 없습니다.
#[tokio::test]
async fn required_validation_gates_completion() {
    let h = Harness::new("validation");
    let mut checked = spec("a", &[]);
    checked.validation = vec![required_check("check-1"), required_check("check-2")];
    h.register(vec![checked]).await;
    let run_id = h.start("a").await;
    h.finish(&run_id, RunStatus::Succeeded).await;

    let unchecked = h.scheduler.complete_task(Actor::User, "a").await;
    assert!(
        matches!(unchecked, Err(SchedulerError::InvalidTransition { .. })),
        "{unchecked:?}"
    );

    h.scheduler
        .begin_validation(Actor::Core, "a")
        .await
        .unwrap();
    assert_eq!(h.status("a"), TaskStatus::Validating);
    // 필수 검증 둘 중 하나만 끝나면 아직 판정하지 않습니다.
    store_verdict(&h.store, "v-1", "a", "check-1", ValidationVerdict::Passed);
    let partial = h
        .scheduler
        .on_validation_finished("a", "v-1")
        .await
        .unwrap();
    assert_eq!(partial.status, TaskStatus::Validating);

    store_verdict(&h.store, "v-2", "a", "check-2", ValidationVerdict::Passed);
    let verified = h
        .scheduler
        .on_validation_finished("a", "v-2")
        .await
        .unwrap();
    assert_eq!(verified.status, TaskStatus::Verified);

    // 완료 정책이 결과 검토 승인을 요구하면 승인 전에는 완료하지 않습니다 (VERIFY-03).
    h.policy
        .require_result_approval
        .store(true, Ordering::SeqCst);
    let unapproved = h.scheduler.complete_task(Actor::User, "a").await;
    assert!(
        matches!(unapproved, Err(SchedulerError::InvalidTransition { .. })),
        "{unapproved:?}"
    );
    let mut approval = persistence::approval("approval-1", PLAN);
    approval.task_id = Some("a".into());
    approval.state = ApprovalState::Granted;
    approval.answer = Some(ApprovalAnswer::Allow);
    approval.decided_by = Some(ApprovalActor::User);
    h.store
        .apply(persistence::batch(vec![StateWrite::Approval {
            decision: approval,
            expected_version: None,
        }]))
        .unwrap();
    let done = h.scheduler.complete_task(Actor::User, "a").await.unwrap();
    assert_eq!(done.status, TaskStatus::Completed);
}

// §9.4: 검증 실패는 재작업(Ready), 재작업 상한을 넘으면 Failed. 판정 불가는 마스터 판단을 기다립니다.
#[tokio::test]
async fn failed_validation_leads_to_rework_within_the_limit() {
    let h = Harness::new("rework");
    let mut checked = spec("a", &[]);
    checked.validation = vec![required_check("check-1")];
    h.register(vec![checked]).await;

    // 첫 실행: 검증 실패 → 재작업 (상한 1)
    let first = h.start("a").await;
    h.finish(&first, RunStatus::Succeeded).await;
    h.scheduler
        .begin_validation(Actor::Core, "a")
        .await
        .unwrap();
    store_verdict(&h.store, "v-1", "a", "check-1", ValidationVerdict::Failed);
    let reworked = h
        .scheduler
        .on_validation_finished("a", "v-1")
        .await
        .unwrap();
    assert_eq!(reworked.status, TaskStatus::Ready);

    // 둘째 실행: 판정 불가 → 마스터 대기, 복귀 상태는 Validating
    let second = h.start("a").await;
    assert_eq!(h.run(&second).attempt, 2);
    h.finish(&second, RunStatus::Succeeded).await;
    h.scheduler
        .begin_validation(Actor::Core, "a")
        .await
        .unwrap();
    store_verdict(
        &h.store,
        "v-2",
        "a",
        "check-1",
        ValidationVerdict::Inconclusive,
    );
    let waiting = h
        .scheduler
        .on_validation_finished("a", "v-2")
        .await
        .unwrap();
    assert_eq!(waiting.status, TaskStatus::WaitingMaster);
    let wait = waiting.wait.unwrap();
    assert_eq!(wait.resume_status, TaskStatus::Validating);
    assert_eq!(wait.waiting_for, ["v-2"]);

    // 마스터가 다시 검증하기로 함 → 실패. 이미 재작업을 한 번 썼으므로 Failed.
    h.scheduler.end_wait(Actor::User, "a").await.unwrap();
    store_verdict(&h.store, "v-3", "a", "check-1", ValidationVerdict::Failed);
    let failed = h
        .scheduler
        .on_validation_finished("a", "v-3")
        .await
        .unwrap();
    assert_eq!(failed.status, TaskStatus::Failed);
    assert_eq!(h.ledger().tasks_consumed, 1);

    // 끝난 뒤 도착한 판정은 Task를 바꾸지 않습니다.
    store_verdict(&h.store, "v-4", "a", "check-1", ValidationVerdict::Passed);
    let late = h
        .scheduler
        .on_validation_finished("a", "v-4")
        .await
        .unwrap();
    assert_eq!(late.status, TaskStatus::Failed);
}

// §9.4: 요청에 의한 재작업도 상한을 따릅니다.
#[tokio::test]
async fn requested_rework_respects_the_limit() {
    let h = Harness::new("rework-request");
    h.register(vec![spec("a", &[])]).await;

    let not_run = h.scheduler.request_rework(Actor::User, "a", "again").await;
    assert!(
        matches!(not_run, Err(SchedulerError::InvalidTransition { .. })),
        "{not_run:?}"
    );

    let first = h.start("a").await;
    h.finish(&first, RunStatus::Succeeded).await;
    let reworked = h
        .scheduler
        .request_rework(Actor::User, "a", "tone")
        .await
        .unwrap();
    assert_eq!(reworked.status, TaskStatus::Ready);

    let second = h.start("a").await;
    h.finish(&second, RunStatus::Succeeded).await;
    let over = h.scheduler.request_rework(Actor::User, "a", "again").await;
    assert!(
        matches!(over, Err(SchedulerError::LimitExceeded { .. })),
        "{over:?}"
    );
    assert_eq!(h.status("a"), TaskStatus::Executed);
}

// §9.4: 응답 대기는 복귀 상태를 기록하고, 실행 중이던 Run은 Waiting으로 둡니다.
#[tokio::test]
async fn waiting_for_an_answer_records_where_to_resume() {
    let h = Harness::new("wait");
    h.register(vec![spec("a", &[])]).await;
    let run_id = h.start("a").await;

    let waiting = h
        .scheduler
        .begin_wait(
            Actor::Core,
            "a",
            WaitKind::User,
            vec!["approval-1".into()],
            None,
        )
        .await
        .unwrap();
    assert_eq!(waiting.status, TaskStatus::WaitingUser);
    assert_eq!(waiting.wait.unwrap().resume_status, TaskStatus::Running);
    assert_eq!(h.run(&run_id).status, RunStatus::Waiting);

    let resumed = h.scheduler.end_wait(Actor::Core, "a").await.unwrap();
    assert_eq!((resumed.status, resumed.wait), (TaskStatus::Running, None));
    assert_eq!(h.run(&run_id).status, RunStatus::Running);

    let not_waiting = h.scheduler.end_wait(Actor::Core, "a").await;
    assert!(
        matches!(not_waiting, Err(SchedulerError::InvalidTransition { .. })),
        "{not_waiting:?}"
    );
}

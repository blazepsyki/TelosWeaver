//! PH1-SCHED: Run 시작·동시성·잠금·재시도·취소 경합·위임·기한 (Spec §9.1~§9.5, TASK-03·06, DELEG-04·09·10, SAFE-13).

#[path = "fixtures/persistence/mod.rs"]
mod persistence;
#[path = "fixtures/scheduler/mod.rs"]
mod support;

use std::sync::atomic::Ordering;

use support::*;
use telos_core::persistence::{StateWrite, Store};
use telos_core::scheduler::{
    Actor, ExecutorError, RunAdmission, Scheduler, SchedulerError, WaitKind,
};
use telos_schema::*;

fn unavailable(result: &Result<impl std::fmt::Debug, SchedulerError>) {
    assert!(
        matches!(result, Err(SchedulerError::Unavailable { .. })),
        "{result:?}"
    );
}

fn worker(task_id: &str, run_id: &str) -> Actor {
    Actor::Worker {
        task_id: task_id.into(),
        run_id: run_id.into(),
    }
}

// §9.1 2~5번: Run 발급, Snapshot 고정, 실행 의도, Adapter 전달, 종료 반영
#[tokio::test]
async fn a_run_is_recorded_before_and_after_dispatch() {
    let h = Harness::new("run");
    h.register(vec![writer_spec("a", "ws-a")]).await;

    let admission = h
        .scheduler
        .request_run(Actor::User, "a", fresh(), "key-1")
        .await
        .unwrap();
    let RunAdmission::Started { run_id } = admission.clone() else {
        panic!("{admission:?}");
    };

    assert_eq!(h.status("a"), TaskStatus::Running);
    let run = h.run(&run_id);
    assert_eq!((run.status, run.attempt), (RunStatus::Running, 1));
    let intent = h.store.run_intent(&run_id).unwrap().unwrap();
    assert_eq!(intent.phase, IntentPhase::Started);
    assert_eq!(intent.process, Some(evidence(&run_id)));
    assert!(intent.payload_digest.starts_with("sha256:"));
    let snapshot = h.store.context_snapshot(&run_id).unwrap().unwrap();
    let context = h.executor.last_context();
    assert_eq!(context.context_snapshot_id, snapshot.snapshot_id);
    assert_eq!(context.workspace_id, "ws-a");
    assert_eq!((context.controller_epoch, context.attempt), (1, 1));
    assert_eq!(context.policy_revision, "policy-1");
    // Run과 Native Session의 관계 (SESS-01)
    let links = h.store.run_sessions(&run_id).unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].usage, SessionUse::Created);
    let session = h.store.session(&links[0].session_id).unwrap().unwrap();
    assert_eq!(session.owner_task_id, "a");
    assert_eq!(session.native_session_id, format!("native-{run_id}"));
    assert_eq!(h.store.locks().unwrap().len(), 1);

    // 같은 키의 재요청은 같은 Run이고 프로세스를 새로 만들지 않습니다 (SAFE-13).
    let again = h
        .scheduler
        .request_run(Actor::User, "a", fresh(), "key-1")
        .await
        .unwrap();
    assert_eq!(again, admission);
    assert_eq!(h.executor.started_runs().len(), 1);
    // 실행 중인 Task에 다른 Run을 겹쳐 만들지 않습니다.
    let overlap = h
        .scheduler
        .request_run(Actor::User, "a", fresh(), "key-2")
        .await;
    assert!(
        matches!(overlap, Err(SchedulerError::InvalidTransition { .. })),
        "{overlap:?}"
    );

    h.finish(&run_id, RunStatus::Succeeded).await;
    // Run 성공은 Task 완료가 아닙니다 (§9.4, DELEG-09).
    assert_eq!(h.status("a"), TaskStatus::Executed);
    let run = h.run(&run_id);
    assert_eq!(run.status, RunStatus::Succeeded);
    assert_eq!(run.outcome, Some(ExecutionOutcome::Succeeded));
    assert!(run.finished_at.is_some());
    assert!(h.store.unfinished_intents().unwrap().is_empty());
    assert!(h.store.locks().unwrap().is_empty());
}

// §9.1 2번: 선행 Task가 끝나지 않았거나 정책이 거부한 Task는 실행하지 않습니다.
#[tokio::test]
async fn blocked_or_unauthorized_tasks_do_not_run() {
    let h = Harness::new("not-ready");
    let mut denied = spec("denied", &[]);
    denied.agent.provider = "forbidden".into();
    h.register(vec![spec("a", &[]), spec("b", &["a"]), denied])
        .await;

    let blocked = h
        .scheduler
        .request_run(Actor::User, "b", fresh(), "k-b")
        .await;
    assert!(
        matches!(blocked, Err(SchedulerError::InvalidTransition { .. })),
        "{blocked:?}"
    );
    let forbidden = h
        .scheduler
        .request_run(Actor::User, "denied", fresh(), "k-d")
        .await;
    assert!(
        matches!(forbidden, Err(SchedulerError::Forbidden { .. })),
        "{forbidden:?}"
    );
    let missing = h
        .scheduler
        .request_run(Actor::User, "nope", fresh(), "k-n")
        .await;
    assert!(
        matches!(missing, Err(SchedulerError::NotFound { .. })),
        "{missing:?}"
    );

    assert!(h.executor.started_runs().is_empty());
    assert_eq!(h.status("denied"), TaskStatus::Ready);
    assert!(h.store.task_runs("denied").unwrap().is_empty());
}

// TASK-03: Provider·Host 동시성 상한. 넘는 요청은 대기열에서 순서대로 시작합니다.
#[tokio::test]
async fn concurrency_limit_queues_requests_in_order() {
    let h = Harness::with_limits("queue", limits(1));
    h.register(vec![spec("a", &[]), spec("b", &[]), spec("c", &[])])
        .await;
    let first = h.start("a").await;

    let queued_b = h
        .scheduler
        .request_run(Actor::User, "b", fresh(), "k-b")
        .await
        .unwrap();
    let queued_c = h
        .scheduler
        .request_run(Actor::User, "c", fresh(), "k-c")
        .await
        .unwrap();
    assert_eq!(queued_b, RunAdmission::Queued { position: 1 });
    assert_eq!(queued_c, RunAdmission::Queued { position: 2 });
    // 같은 요청을 다시 보내도 대기열이 늘지 않습니다.
    let again = h
        .scheduler
        .request_run(Actor::User, "b", fresh(), "k-b")
        .await
        .unwrap();
    assert_eq!(again, RunAdmission::Queued { position: 1 });
    assert_eq!(h.status("b"), TaskStatus::Ready);
    assert_eq!(h.executor.started_runs().len(), 1);

    // 자리가 나면 먼저 요청한 것부터 하나만 시작합니다.
    h.finish(&first, RunStatus::Succeeded).await;
    assert_eq!(h.status("b"), TaskStatus::Running);
    assert_eq!(h.status("c"), TaskStatus::Ready);
    let second = h.executor.started_runs()[1].clone();
    assert_eq!(h.run(&second).task_id, "b");

    // 대기 중 취소한 요청은 시작하지 않습니다.
    h.scheduler
        .cancel_task(Actor::User, "c", "not needed", "cancel-c")
        .await
        .unwrap();
    h.finish(&second, RunStatus::Succeeded).await;
    assert_eq!(h.status("c"), TaskStatus::Cancelled);
    assert_eq!(h.executor.started_runs().len(), 2);
}

// GIT-02, DELEG-10: 다른 Writer가 쓰는 Workspace는 기다리지 않고 바로 거부합니다.
#[tokio::test]
async fn busy_workspace_is_refused_without_side_effects() {
    let h = Harness::new("ws-lock");
    h.register(vec![writer_spec("a", "shared"), writer_spec("b", "shared")])
        .await;
    let first = h.start("a").await;

    let second = h
        .scheduler
        .request_run(Actor::User, "b", fresh(), "k-b")
        .await;
    unavailable(&second);
    assert_eq!(h.status("b"), TaskStatus::Ready);
    assert!(h.store.task_runs("b").unwrap().is_empty());
    assert_eq!(h.store.unfinished_intents().unwrap().len(), 1);

    // Writer가 끝나면 잠금이 풀려 다음 Task가 실행됩니다.
    h.finish(&first, RunStatus::Succeeded).await;
    let run_b = h.start("b").await;
    assert_eq!(h.run(&run_b).status, RunStatus::Running);
}

// CTX-02: 막는 입력 문제가 있으면 실행하지 않고 Blocked로 두며, 준비 중 잡은 잠금을 남기지 않습니다.
#[tokio::test]
async fn blocking_input_issue_holds_the_task() {
    let h = Harness::new("ctx-block");
    h.register(vec![writer_spec("a", "ws-a")]).await;
    h.context.blocking.store(true, Ordering::SeqCst);

    let held = h
        .scheduler
        .request_run(Actor::User, "a", fresh(), "k-1")
        .await;
    unavailable(&held);
    assert_eq!(h.status("a"), TaskStatus::Blocked);
    assert!(h.store.locks().unwrap().is_empty());
    assert!(h.store.task_runs("a").unwrap().is_empty());
    assert!(h.executor.started_runs().is_empty());

    // 문맥을 만들지 못한 경우에도 Task는 그대로이고 잠금이 남지 않습니다.
    let h = Harness::new("ctx-fail");
    h.register(vec![writer_spec("a", "ws-a")]).await;
    h.context.fail.store(true, Ordering::SeqCst);
    let failed = h
        .scheduler
        .request_run(Actor::User, "a", fresh(), "k-1")
        .await;
    unavailable(&failed);
    assert_eq!(h.status("a"), TaskStatus::Ready);
    assert!(h.store.locks().unwrap().is_empty());
}

// §8.1, §8.2, FORK-03: Session 소유권 확인, 쓰기 Turn 하나, 전략 대체의 반영
#[tokio::test]
async fn session_strategy_is_checked_and_fallback_is_applied() {
    let h = Harness::new("session");
    h.register(vec![spec("a", &[]), spec("b", &[])]).await;
    let first = h.start("a").await;
    h.finish(&first, RunStatus::Succeeded).await;
    let session_id = h.store.run_sessions(&first).unwrap()[0].session_id.clone();
    h.scheduler
        .request_rework(Actor::User, "a", "continue")
        .await
        .unwrap();

    let resume = SessionRequest {
        mode: SessionMode::Resume,
        source_session_id: Some(session_id.clone()),
        source_turn_id: None,
        capsule_version_id: None,
    };
    // 다른 Task의 Session은 정책이 막습니다.
    let foreign = h
        .scheduler
        .request_run(Actor::User, "b", resume.clone(), "k-b")
        .await;
    assert!(
        matches!(foreign, Err(SchedulerError::Forbidden { .. })),
        "{foreign:?}"
    );
    let no_source = h
        .scheduler
        .request_run(
            Actor::User,
            "a",
            SessionRequest {
                source_session_id: None,
                ..resume.clone()
            },
            "k-none",
        )
        .await;
    assert!(
        matches!(no_source, Err(SchedulerError::Forbidden { .. })),
        "{no_source:?}"
    );

    // 자기 Session 재개: 같은 Session을 다시 쓰고 Session 잠금을 잡습니다.
    let admission = h
        .scheduler
        .request_run(Actor::User, "a", resume.clone(), "k-a")
        .await
        .unwrap();
    let RunAdmission::Started { run_id: second } = admission else {
        panic!("{admission:?}");
    };
    assert_eq!(h.run(&second).session_mode, SessionMode::Resume);
    assert_eq!(h.executor.last_context().session, resume);
    let locks = h.store.locks().unwrap();
    assert!(locks.iter().any(|lock| lock.resource
        == LockResource::Session {
            session_id: session_id.clone()
        }));
    h.finish(&second, RunStatus::Succeeded).await;
    assert!(h.store.locks().unwrap().is_empty());

    // 요청한 Native Fork를 쓸 수 없어 문맥 구성이 Capsule로 대체한 경우: 실제 쓴 전략을 전달·기록합니다.
    let h = Harness::new("session-fallback");
    h.register(vec![spec("a", &[])]).await;
    let first = h.start("a").await;
    h.finish(&first, RunStatus::Succeeded).await;
    let session_id = h.store.run_sessions(&first).unwrap()[0].session_id.clone();
    h.scheduler
        .request_rework(Actor::User, "a", "fork")
        .await
        .unwrap();
    *h.context.fallback.lock().unwrap() = Some(SessionMode::Capsule);
    let fork = SessionRequest {
        mode: SessionMode::NativeFork,
        source_session_id: Some(session_id),
        source_turn_id: None,
        capsule_version_id: None,
    };
    let admission = h
        .scheduler
        .request_run(Actor::User, "a", fork, "k-fork")
        .await
        .unwrap();
    let RunAdmission::Started { run_id } = admission else {
        panic!("{admission:?}");
    };
    let context = h.executor.last_context();
    assert_eq!(context.session.mode, SessionMode::Capsule);
    assert_eq!(context.session.source_session_id, None);
    assert_eq!(context.session.capsule_version_id.as_deref(), Some("v1"));
    assert_eq!(h.run(&run_id).session_mode, SessionMode::Capsule);
    let snapshot = h.store.context_snapshot(&run_id).unwrap().unwrap();
    assert_eq!(
        snapshot.fallback.unwrap().requested,
        SessionMode::NativeFork
    );
}

// §9.4, §9.5, TASK-06: 실패한 실행은 상한 안에서만 새 Run으로 재시도합니다.
#[tokio::test]
async fn failures_retry_only_within_the_limit() {
    let h = Harness::new("retry");
    h.register(vec![spec("a", &[]), spec("b", &[])]).await;

    let first = h.start("a").await;
    h.scheduler
        .on_run_finished(report(
            &first,
            RunStatus::Failed,
            Some(FailureKind::Provider),
        ))
        .await
        .unwrap();
    assert_eq!(h.status("a"), TaskStatus::Ready);
    // Scheduler가 스스로 다시 실행하지 않습니다.
    assert_eq!(h.executor.started_runs().len(), 1);

    let second = h.start("a").await;
    assert_eq!(h.run(&second).attempt, 2);
    h.scheduler
        .on_run_finished(report(
            &second,
            RunStatus::Failed,
            Some(FailureKind::Timeout),
        ))
        .await
        .unwrap();
    assert_eq!(h.status("a"), TaskStatus::Failed);
    assert_eq!(h.ledger().tasks_consumed, 1);

    // 다시 해도 같은 실패는 재시도하지 않습니다.
    let run_b = h.start("b").await;
    h.scheduler
        .on_run_finished(report(
            &run_b,
            RunStatus::Failed,
            Some(FailureKind::Authentication),
        ))
        .await
        .unwrap();
    assert_eq!(h.status("b"), TaskStatus::Failed);
}

// §14.3: 시작 거부와 시작 응답 유실을 구분합니다.
#[tokio::test]
async fn rejected_start_and_lost_start_response_differ() {
    let h = Harness::new("start-fail");
    h.register(vec![writer_spec("a", "ws-a"), writer_spec("b", "ws-b")])
        .await;

    h.executor.fail_next(ExecutorError::Rejected {
        reason: "executable not found".into(),
    });
    let rejected = h
        .scheduler
        .request_run(Actor::User, "a", fresh(), "k-a")
        .await;
    let Err(SchedulerError::StartFailed {
        run_id, confirmed, ..
    }) = rejected
    else {
        panic!("{rejected:?}");
    };
    assert!(confirmed);
    assert_eq!(h.run(&run_id).status, RunStatus::Failed);
    // 실행이 없었음이 확실하므로 재시도할 수 있고, 의도와 잠금이 남지 않습니다.
    assert_eq!(h.status("a"), TaskStatus::Ready);
    assert!(h.store.unfinished_intents().unwrap().is_empty());
    assert!(h.store.locks().unwrap().is_empty());

    h.executor.fail_next(ExecutorError::Unknown {
        message: "pipe closed before the start result".into(),
    });
    let lost = h
        .scheduler
        .request_run(Actor::User, "b", fresh(), "k-b")
        .await;
    let Err(SchedulerError::StartFailed {
        run_id, confirmed, ..
    }) = lost
    else {
        panic!("{lost:?}");
    };
    assert!(!confirmed);
    // 시작했을 수도 있으므로 다시 실행하지 않고 대조를 기다립니다.
    assert_eq!(h.status("b"), TaskStatus::NeedsReconciliation);
    assert_eq!(h.run(&run_id).status, RunStatus::Interrupted);
    let pending = h.store.unfinished_intents().unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].phase, IntentPhase::Dispatched);
    assert_eq!(h.store.locks().unwrap().len(), 1);
    let retry = h
        .scheduler
        .request_run(Actor::User, "b", fresh(), "k-b2")
        .await;
    assert!(
        matches!(retry, Err(SchedulerError::InvalidTransition { .. })),
        "{retry:?}"
    );
}

// §9.4, §9.5, E2E-06: 취소 접수와 실제 정지는 다르고, 취소가 확정된 Task는 늦은 완료로 되돌아가지 않습니다.
#[tokio::test]
async fn cancel_wins_over_a_late_success() {
    let h = Harness::new("cancel-race");
    h.register(vec![spec("idle", &[]), spec("busy", &[])]).await;
    let run_id = h.start("busy").await;

    // 실행이 없는 Task는 바로 취소됩니다.
    h.scheduler
        .cancel_task(Actor::User, "idle", "stop", "c-idle")
        .await
        .unwrap();
    assert_eq!(h.status("idle"), TaskStatus::Cancelled);

    let accepted = h
        .scheduler
        .cancel_task(Actor::User, "busy", "stop", "c-busy")
        .await
        .unwrap();
    assert!(accepted.accepted);
    assert_eq!(h.status("busy"), TaskStatus::Cancelling);
    assert_eq!(h.run(&run_id).status, RunStatus::Cancelling);
    assert_eq!(
        *h.executor.cancelled.lock().unwrap(),
        std::slice::from_ref(&run_id)
    );
    // 같은 요청의 재전송은 같은 접수 결과이고 정지를 다시 요청하지 않습니다.
    let again = h
        .scheduler
        .cancel_task(Actor::User, "busy", "stop", "c-busy")
        .await
        .unwrap();
    assert_eq!(again, accepted);
    assert_eq!(h.executor.cancelled.lock().unwrap().len(), 1);

    // 취소 요청보다 먼저 끝난 프로세스의 결과가 늦게 도착합니다.
    h.finish(&run_id, RunStatus::Succeeded).await;
    assert_eq!(h.run(&run_id).status, RunStatus::Succeeded);
    assert_eq!(h.status("busy"), TaskStatus::Cancelled);
    let completed = h.scheduler.complete_task(Actor::User, "busy").await;
    assert!(
        matches!(completed, Err(SchedulerError::InvalidTransition { .. })),
        "{completed:?}"
    );

    // 이미 끝난 Run의 중복 보고는 아무것도 바꾸지 않습니다.
    let before = h.store.last_sequence().unwrap();
    h.finish(&run_id, RunStatus::Failed).await;
    assert_eq!(h.store.last_sequence().unwrap(), before);
    assert_eq!(h.run(&run_id).status, RunStatus::Succeeded);
}

// §9.5: 이전 Run의 늦은 결과를 새 Run의 결과로 채택하지 않습니다.
#[tokio::test]
async fn late_report_of_an_older_run_does_not_touch_the_task() {
    let h = Harness::new("stale-run");
    h.register(vec![spec("a", &[])]).await;
    let first = h.start("a").await;
    // 추적을 잃은 Run: 사용자가 확인하고 다시 실행하기로 함
    h.finish(&first, RunStatus::Interrupted).await;
    assert_eq!(h.status("a"), TaskStatus::Interrupted);
    assert_eq!(h.run(&first).status, RunStatus::Interrupted);
    // 살아 있을 수 있는 프로세스이므로 실행 자리와 의도를 그대로 둡니다.
    assert_eq!(h.store.unfinished_intents().unwrap().len(), 1);

    // 같은 Run의 결과가 확정되어 돌아오면 그대로 반영합니다.
    h.finish(&first, RunStatus::Failed).await;
    assert_eq!(h.status("a"), TaskStatus::Interrupted);
    assert_eq!(h.run(&first).status, RunStatus::Failed);

    h.scheduler
        .reconcile_task(
            Actor::User,
            "a",
            telos_core::scheduler::Reconciliation::Retry,
        )
        .await
        .unwrap();
    let second = h.start("a").await;
    h.finish(&first, RunStatus::Succeeded).await;
    assert_eq!(h.status("a"), TaskStatus::Running);
    h.finish(&second, RunStatus::Succeeded).await;
    assert_eq!(h.status("a"), TaskStatus::Executed);
}

// §9.3, DELEG-10: 취소는 자손에 전파하고, 독립 지속으로 등록한 자식은 건드리지 않습니다.
#[tokio::test]
async fn cancel_propagates_to_descendants() {
    let h = Harness::new("cancel-tree");
    h.register(vec![
        spec("root", &[]),
        child_of("child", "root"),
        child_of("grandchild", "child"),
        spec("other", &[]),
    ])
    .await;
    // 독립 지속 자식
    let mut independent = persistence::task("independent", PLAN);
    independent.spec = child_of("independent", "root");
    independent.status = TaskStatus::Ready;
    independent.cancel_propagation = CancelPropagation::Independent;
    h.store
        .apply(persistence::batch(vec![StateWrite::Task {
            record: independent,
            expected_version: None,
        }]))
        .unwrap();
    let child_run = h.start("child").await;

    h.scheduler
        .cancel_task(Actor::User, "root", "abort", "c-root")
        .await
        .unwrap();
    assert_eq!(h.status("root"), TaskStatus::Cancelled);
    assert_eq!(h.status("child"), TaskStatus::Cancelling);
    assert_eq!(h.status("grandchild"), TaskStatus::Cancelled);
    assert_eq!(h.status("independent"), TaskStatus::Ready);
    assert_eq!(h.status("other"), TaskStatus::Ready);
    assert_eq!(
        *h.executor.cancelled.lock().unwrap(),
        std::slice::from_ref(&child_run)
    );

    h.finish(&child_run, RunStatus::Cancelled).await;
    assert_eq!(h.status("child"), TaskStatus::Cancelled);
    assert_eq!(h.run(&child_run).status, RunStatus::Cancelled);
}

// §9.2, §9.3, DELEG-01·04: 자식 등록·즉시 실행·부모 대기와 깨우기
#[tokio::test]
async fn delegation_runs_the_child_and_wakes_the_waiting_parent() {
    let h = Harness::new("delegate");
    h.register(vec![spec("parent", &[])]).await;
    let parent_run = h.start("parent").await;
    let caller = worker("parent", &parent_run);

    let child = h
        .scheduler
        .delegate_task(caller.clone(), delegate_input("d-1", true))
        .await
        .unwrap();
    assert_eq!(child.status, TaskStatus::Running);
    assert_eq!(child.delegation_depth, 1);
    assert_eq!(child.spec.parent_task_id.as_deref(), Some("parent"));
    // Profile은 호출자가 아니라 정책이 정합니다.
    assert_eq!(child.spec.budget_profile_id, "default");
    let parent = h.task("parent");
    assert_eq!(parent.status, TaskStatus::WaitingChild);
    let wait = parent.wait.unwrap();
    assert_eq!(wait.waiting_for, std::slice::from_ref(&child.spec.id));
    assert_eq!(wait.resume_status, TaskStatus::Running);
    assert!(wait.deadline.is_some());
    // 기다리는 부모의 프로세스는 살아 있으므로 Run은 그대로입니다.
    assert_eq!(h.run(&parent_run).status, RunStatus::Running);
    assert_eq!(h.ledger().tasks_reserved, 2);

    // 같은 멱등 키의 재호출은 같은 자식입니다 (중복 자식 방지).
    let again = h
        .scheduler
        .delegate_task(caller.clone(), delegate_input("d-1", true))
        .await
        .unwrap();
    assert_eq!(again.spec.id, child.spec.id);
    assert_eq!(h.store.plan_tasks(PLAN).unwrap().len(), 2);

    // 자식의 Run이 끝난 것만으로는 부모를 깨우지 않습니다. 완료 판정 뒤에 깨웁니다.
    let child_run = h.store.task_runs(&child.spec.id).unwrap()[0].run_id.clone();
    h.finish(&child_run, RunStatus::Succeeded).await;
    assert_eq!(h.status("parent"), TaskStatus::WaitingChild);
    h.scheduler
        .complete_task(Actor::Core, &child.spec.id)
        .await
        .unwrap();
    let parent = h.task("parent");
    assert_eq!((parent.status, parent.wait), (TaskStatus::Running, None));

    // Worker는 자기가 위임한 Task만 취소할 수 있고, 끝난 Run의 자격으로는 아무것도 못 합니다.
    let second = h
        .scheduler
        .delegate_task(caller.clone(), delegate_input("d-2", false))
        .await
        .unwrap();
    assert_eq!(h.status("parent"), TaskStatus::Running);
    let foreign = h
        .scheduler
        .cancel_task(caller.clone(), "parent", "x", "c-1")
        .await;
    assert!(
        matches!(foreign, Err(SchedulerError::Forbidden { .. })),
        "{foreign:?}"
    );
    h.scheduler
        .cancel_task(caller.clone(), &second.spec.id, "not needed", "c-2")
        .await
        .unwrap();
    assert_eq!(h.status(&second.spec.id), TaskStatus::Cancelling);

    h.finish(&parent_run, RunStatus::Succeeded).await;
    let expired = h
        .scheduler
        .delegate_task(caller, delegate_input("d-3", false))
        .await;
    assert!(
        matches!(expired, Err(SchedulerError::Forbidden { .. })),
        "{expired:?}"
    );
}

// §9.3, E2E-04: 지금 실행할 수 없는 위임은 무한 대기 대신 설명 가능한 거부로 답합니다.
#[tokio::test]
async fn delegation_that_cannot_run_now_is_refused() {
    // 동시성 1: 부모가 자리를 쥐고 있어 같은 Provider의 자식은 실행할 수 없습니다.
    let h = Harness::with_limits("delegate-full", limits(1));
    h.register(vec![spec("parent", &[])]).await;
    let parent_run = h.start("parent").await;
    let caller = worker("parent", &parent_run);
    let refused = h
        .scheduler
        .delegate_task(caller.clone(), delegate_input("d-1", true))
        .await;
    let Err(SchedulerError::Unavailable { reason }) = &refused else {
        panic!("{refused:?}");
    };
    assert!(reason.contains("concurrency limit"), "{reason}");
    assert_eq!(h.status("parent"), TaskStatus::Running);
    assert_eq!(h.store.plan_tasks(PLAN).unwrap().len(), 1);
    assert_eq!(h.ledger().tasks_reserved, 1);

    // 부모가 쓰는 Workspace를 자식이 쓰려고 하면 잠금을 넘기지 않고 거부합니다.
    let h = Harness::new("delegate-lock");
    h.register(vec![writer_spec("parent", "shared")]).await;
    let parent_run = h.start("parent").await;
    let mut input = delegate_input("d-1", true);
    input.workspace = writer_spec("x", "shared").workspace;
    let refused = h
        .scheduler
        .delegate_task(worker("parent", &parent_run), input)
        .await;
    unavailable(&refused);
    // 시작하지 못한 자식을 남겨 부모가 기다리게 하지 않습니다.
    assert_eq!(h.status("parent"), TaskStatus::Running);
    let tasks = h.store.plan_tasks(PLAN).unwrap();
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[1].status, TaskStatus::Cancelled);
    assert_eq!(h.store.locks().unwrap().len(), 1);

    // 정책이 위임을 막거나 깊이 상한을 넘으면 등록하지 않습니다.
    let h = Harness::new("delegate-deny");
    h.register(vec![
        spec("root", &[]),
        child_of("child", "root"),
        child_of("deep", "child"),
    ])
    .await;
    let deep_run = h.start("deep").await;
    let too_deep = h
        .scheduler
        .delegate_task(worker("deep", &deep_run), delegate_input("d-1", false))
        .await;
    assert!(
        matches!(too_deep, Err(SchedulerError::LimitExceeded { .. })),
        "{too_deep:?}"
    );
    h.policy.deny_children.store(true, Ordering::SeqCst);
    let root_run = h.start("root").await;
    let denied = h
        .scheduler
        .delegate_task(worker("root", &root_run), delegate_input("d-2", false))
        .await;
    assert!(
        matches!(denied, Err(SchedulerError::Forbidden { .. })),
        "{denied:?}"
    );
    assert_eq!(h.store.plan_tasks(PLAN).unwrap().len(), 3);
}

// §13.2, TASK-06: 실행 시간 상한을 넘긴 Run은 멈추고 Timeout 실패로 처리합니다.
#[tokio::test]
async fn run_timeout_stops_the_run_and_counts_as_failure() {
    let h = Harness::new("timeout");
    h.register(vec![spec("a", &[])]).await;
    let run_id = h.start("a").await;

    h.advance(599);
    assert!(
        h.scheduler
            .enforce_deadlines()
            .await
            .unwrap()
            .timed_out_runs
            .is_empty()
    );
    h.advance(2);
    let report_ = h.scheduler.enforce_deadlines().await.unwrap();
    assert_eq!(report_.timed_out_runs, std::slice::from_ref(&run_id));
    assert_eq!(h.run(&run_id).status, RunStatus::Cancelling);
    assert_eq!(h.status("a"), TaskStatus::Running);
    assert_eq!(
        *h.executor.cancelled.lock().unwrap(),
        std::slice::from_ref(&run_id)
    );
    // 이미 정지를 요청한 Run에 다시 요청하지 않습니다.
    assert!(
        h.scheduler
            .enforce_deadlines()
            .await
            .unwrap()
            .timed_out_runs
            .is_empty()
    );

    // 정지 보고는 취소가 아니라 실패로 남고, 재시도 상한 안이면 다시 실행할 수 있습니다.
    h.finish(&run_id, RunStatus::Cancelled).await;
    let run = h.run(&run_id);
    assert_eq!(
        (run.status, run.outcome),
        (RunStatus::Failed, Some(ExecutionOutcome::Failed))
    );
    assert_eq!(h.status("a"), TaskStatus::Ready);
}

// §9.3: 대기는 기한이 있고, 기한이 지나면 끝냅니다.
#[tokio::test]
async fn expired_waits_do_not_hang_forever() {
    let h = Harness::new("wait-expire");
    h.register(vec![spec("parent", &[]), spec("review", &[])])
        .await;
    let parent_run = h.start("parent").await;
    let child = h
        .scheduler
        .delegate_task(worker("parent", &parent_run), delegate_input("d-1", true))
        .await
        .unwrap();
    // 실행이 끝나 사용자 검토를 기다리는 Task
    let review_run = h.start("review").await;
    h.finish(&review_run, RunStatus::Succeeded).await;
    h.scheduler
        .begin_wait(
            Actor::Core,
            "review",
            WaitKind::User,
            vec!["approval-1".into()],
            Some("2026-11-16T00:01:00Z".into()),
        )
        .await
        .unwrap();

    h.advance(30);
    assert!(
        h.scheduler
            .enforce_deadlines()
            .await
            .unwrap()
            .expired_waits
            .is_empty()
    );

    // 60초 뒤: 사용자 응답 대기는 실패로 끝납니다. 자식 대기(기본 300초)는 아직입니다.
    h.advance(31);
    let first = h.scheduler.enforce_deadlines().await.unwrap();
    assert_eq!(first.expired_waits, ["review"]);
    assert_eq!(h.status("review"), TaskStatus::Failed);
    assert_eq!(h.status("parent"), TaskStatus::WaitingChild);

    // 300초 뒤: 부모는 자식 결과 없이 실행을 계속하고, 자식은 그대로 진행합니다.
    h.advance(240);
    let second = h.scheduler.enforce_deadlines().await.unwrap();
    assert_eq!(second.expired_waits, ["parent"]);
    let parent = h.task("parent");
    assert_eq!((parent.status, parent.wait), (TaskStatus::Running, None));
    assert_eq!(h.status(&child.spec.id), TaskStatus::Running);
}

// §8.2, REQ1-003: `capsule`은 문맥 구성이 저장한 Capsule Version을 전달하고, 없으면 `fresh`로 바꿔 실행하지 않습니다.
#[tokio::test]
async fn capsule_runs_carry_the_stored_capsule_version() {
    let h = Harness::new("capsule");
    h.register(vec![spec("a", &[]), spec("b", &[])]).await;
    let capsule = SessionRequest {
        mode: SessionMode::Capsule,
        source_session_id: Some("ignored-session".into()),
        source_turn_id: None,
        capsule_version_id: None,
    };

    let admission = h
        .scheduler
        .request_run(Actor::User, "a", capsule.clone(), "k-a")
        .await
        .unwrap();
    let RunAdmission::Started { run_id } = admission else {
        panic!("{admission:?}");
    };
    let context = h.executor.last_context();
    assert_eq!(context.session.mode, SessionMode::Capsule);
    assert_eq!(context.session.capsule_version_id.as_deref(), Some("v2"));
    // Capsule은 이전 세션을 쓰지 않습니다.
    assert_eq!(context.session.source_session_id, None);
    assert!(!context.session.lacks_capsule_version());
    assert_eq!(h.run(&run_id).session_mode, SessionMode::Capsule);

    // 문맥 구성이 Capsule을 저장하지 못했으면 실행하지 않고 흔적을 남기지 않습니다.
    h.context.no_capsule.store(true, Ordering::SeqCst);
    let missing = h
        .scheduler
        .request_run(Actor::User, "b", capsule, "k-b")
        .await;
    unavailable(&missing);
    assert_eq!(h.status("b"), TaskStatus::Ready);
    assert!(h.store.task_runs("b").unwrap().is_empty());
    assert_eq!(h.executor.started_runs().len(), 1);
}

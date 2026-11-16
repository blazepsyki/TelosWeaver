//! PH1-PERSIST: 조건부 쓰기·원자성·Journal·실행 의도·멱등·잠금·불변 기록 (Spec §14.1, §14.3, SAFE-06·13).

#[path = "fixtures/persistence/mod.rs"]
mod support;

use std::sync::Arc;

use support::*;
use telos_core::persistence::{StateWrite, Store, StoreError};
use telos_schema::*;

fn conflict(result: Result<impl std::fmt::Debug, StoreError>) {
    assert!(
        matches!(result, Err(StoreError::VersionConflict { .. })),
        "{result:?}"
    );
}

// §9.5, §14.3: 갱신은 읽은 버전과 같을 때만 적용하고 버전을 1 올립니다.
#[test]
fn conditional_writes_follow_versions() {
    let temp = TempDir::new("versions");
    let store = temp.open();

    let applied = store
        .apply(batch(vec![
            StateWrite::Plan {
                plan: plan("plan-1"),
                expected_version: None,
            },
            StateWrite::Task {
                record: task("task-1", "plan-1"),
                expected_version: None,
            },
            StateWrite::Run {
                record: run("run-1", "task-1"),
                expected_version: None,
            },
        ]))
        .unwrap();
    assert_eq!((applied.entries, applied.last_sequence), (3, 3));
    assert_eq!(applied.first_sequence(), Some(1));

    // 저장소가 `stateVersion`을 새 버전으로 적습니다.
    let stored = store.task("task-1").unwrap().unwrap();
    assert_eq!(stored.state_version, 1);
    assert_eq!(store.plan("plan-1").unwrap().unwrap().version, 1);
    assert_eq!(store.run("run-1").unwrap().unwrap().state_version, 1);

    let mut ready = stored.clone();
    ready.status = TaskStatus::Ready;
    store
        .apply(batch(vec![StateWrite::Task {
            record: ready.clone(),
            expected_version: Some(1),
        }]))
        .unwrap();
    let stored = store.task("task-1").unwrap().unwrap();
    assert_eq!(
        (stored.status, stored.state_version),
        (TaskStatus::Ready, 2)
    );

    // 오래된 버전으로 다시 쓰기, 이미 있는 기록을 새로 만들기, 없는 기록을 갱신하기는 모두 거부합니다.
    conflict(store.apply(batch(vec![StateWrite::Task {
        record: ready.clone(),
        expected_version: Some(1),
    }])));
    conflict(store.apply(batch(vec![StateWrite::Task {
        record: ready,
        expected_version: None,
    }])));
    conflict(store.apply(batch(vec![StateWrite::Run {
        record: run("run-missing", "task-1"),
        expected_version: Some(1),
    }])));
    assert_eq!(store.task("task-1").unwrap().unwrap().state_version, 2);
}

// §14.1: 한 배치는 한 Transaction입니다. 하나가 어긋나면 상태도 Journal도 남지 않습니다.
#[test]
fn a_failed_write_cancels_the_whole_batch() {
    let temp = TempDir::new("atomic");
    let store = temp.open();
    store
        .apply(batch(vec![StateWrite::Plan {
            plan: plan("plan-1"),
            expected_version: None,
        }]))
        .unwrap();

    conflict(store.apply(batch(vec![
        StateWrite::Task {
            record: task("task-1", "plan-1"),
            expected_version: None,
        },
        StateWrite::Intent {
            intent: intent("intent-1", "run-1", "task-1", IntentPhase::Recorded),
        },
        StateWrite::Run {
            record: run("run-1", "task-1"),
            expected_version: Some(7),
        },
    ])));

    assert!(store.task("task-1").unwrap().is_none());
    assert!(store.run_intent("run-1").unwrap().is_none());
    assert_eq!(store.last_sequence().unwrap(), 1);
    assert_eq!(store.journal_after(None, 0, 100).unwrap().unwrap().len(), 1);
}

// §14.3: Journal 번호는 1부터 빈틈없이 올라가고 Plan 범위로 걸러 읽습니다.
#[test]
fn journal_is_gapless_and_scoped_by_plan() {
    let temp = TempDir::new("journal");
    let store = temp.open();
    for plan_id in ["plan-a", "plan-b"] {
        store
            .apply(batch(vec![
                StateWrite::Plan {
                    plan: plan(plan_id),
                    expected_version: None,
                },
                StateWrite::Task {
                    record: task(&format!("{plan_id}-task"), plan_id),
                    expected_version: None,
                },
                StateWrite::Run {
                    record: run(&format!("{plan_id}-run"), &format!("{plan_id}-task")),
                    expected_version: None,
                },
            ]))
            .unwrap();
    }
    // 실패한 배치는 번호를 쓰지 않습니다.
    conflict(store.apply(batch(vec![StateWrite::Plan {
        plan: plan("plan-a"),
        expected_version: None,
    }])));

    let all = store.journal_after(None, 0, 100).unwrap().unwrap();
    let sequences: Vec<u64> = all.iter().map(|e| e.sequence).collect();
    assert_eq!(sequences, [1, 2, 3, 4, 5, 6]);
    assert!(all.iter().all(|e| e.recorded_at.ends_with('Z')));

    let plan_b = store
        .journal_after(Some("plan-b"), 0, 100)
        .unwrap()
        .unwrap();
    assert_eq!(
        plan_b.iter().map(|e| e.kind).collect::<Vec<_>>(),
        [
            JournalKind::PlanChanged,
            JournalKind::TaskChanged,
            JournalKind::RunChanged
        ]
    );
    // Run 기록에는 Plan이 없지만 Task를 따라 범위를 채웁니다.
    let run_entry = &plan_b[2];
    assert_eq!(run_entry.scope.plan_id.as_deref(), Some("plan-b"));
    assert_eq!(run_entry.scope.task_id.as_deref(), Some("plan-b-task"));
    assert_eq!(run_entry.scope.run_id.as_deref(), Some("plan-b-run"));
    assert_eq!(run_entry.entity_version, 1);
    assert_eq!(run_entry.payload["run"]["status"], "Starting");

    // 커서 뒤부터, 개수 제한대로 읽습니다.
    let page = store.journal_after(None, 4, 1).unwrap().unwrap();
    assert_eq!(page.iter().map(|e| e.sequence).collect::<Vec<_>>(), [5]);
    assert!(
        store
            .journal_after(None, 6, 10)
            .unwrap()
            .unwrap()
            .is_empty()
    );
}

// §17: 보존 범위 밖의 커서는 `cursor_expired`이고 번호는 이어집니다.
#[test]
fn pruned_cursor_expires_and_numbering_continues() {
    let temp = TempDir::new("prune");
    let store = temp.open();
    for i in 0..5 {
        store
            .apply(batch(vec![StateWrite::Plan {
                plan: plan(&format!("plan-{i}")),
                expected_version: None,
            }]))
            .unwrap();
    }
    assert_eq!(store.prune_journal(4).unwrap(), 3);
    assert_eq!(store.prune_journal(4).unwrap(), 0);

    assert!(store.journal_after(None, 0, 10).unwrap().is_none());
    assert!(store.journal_after(None, 2, 10).unwrap().is_none());
    // 3번까지 받은 구독자는 빠진 항목이 없습니다.
    let rest = store.journal_after(None, 3, 10).unwrap().unwrap();
    assert_eq!(rest.iter().map(|e| e.sequence).collect::<Vec<_>>(), [4, 5]);

    // 아직 없는 번호까지 지웠다고 적지 않습니다. 전부 지워도 번호는 이어집니다.
    assert_eq!(store.prune_journal(1_000).unwrap(), 2);
    assert!(
        store
            .journal_after(None, 5, 10)
            .unwrap()
            .unwrap()
            .is_empty()
    );
    let applied = store
        .apply(batch(vec![StateWrite::Plan {
            plan: plan("plan-next"),
            expected_version: None,
        }]))
        .unwrap();
    assert_eq!(applied.last_sequence, 6);
    let rest = store.journal_after(None, 5, 10).unwrap().unwrap();
    assert_eq!(rest.iter().map(|e| e.sequence).collect::<Vec<_>>(), [6]);
}

// §9.1 4번, §14.3: 실행 의도는 `recorded`로 시작해 앞으로만 갑니다.
#[test]
fn intents_start_recorded_and_only_advance() {
    let temp = TempDir::new("intent");
    let store = temp.open();
    let write = |intent: RunIntent| store.apply(batch(vec![StateWrite::Intent { intent }]));

    let started_first = write(intent("intent-x", "run-x", "task-1", IntentPhase::Started));
    assert!(
        matches!(started_first, Err(StoreError::Invalid { .. })),
        "{started_first:?}"
    );

    write(intent("intent-1", "run-1", "task-1", IntentPhase::Recorded)).unwrap();
    // 같은 Run에 다른 의도를 만들 수 없습니다.
    conflict(write(intent(
        "intent-2",
        "run-1",
        "task-1",
        IntentPhase::Recorded,
    )));
    write(intent(
        "intent-1",
        "run-1",
        "task-1",
        IntentPhase::Dispatched,
    ))
    .unwrap();
    assert_eq!(store.unfinished_intents().unwrap().len(), 1);

    // 전달한 의도는 버릴 수 없고, 뒤로 가거나 다른 요청 내용으로 바꿀 수 없습니다.
    conflict(write(intent(
        "intent-1",
        "run-1",
        "task-1",
        IntentPhase::Abandoned,
    )));
    conflict(write(intent(
        "intent-1",
        "run-1",
        "task-1",
        IntentPhase::Recorded,
    )));
    let mut other_payload = intent("intent-1", "run-1", "task-1", IntentPhase::Started);
    other_payload.payload_digest = "sha256:other".into();
    conflict(write(other_payload));

    let mut started = intent("intent-1", "run-1", "task-1", IntentPhase::Started);
    started.native_session_id = Some("native-1".into());
    started.process = Some(ProcessEvidence {
        host_id: "windows-local".into(),
        pid: 4242,
        process_started_at: AT.into(),
        run_token_digest: "sha256:token".into(),
    });
    write(started.clone()).unwrap();
    assert_eq!(store.run_intent("run-1").unwrap().unwrap(), started);

    write(intent("intent-1", "run-1", "task-1", IntentPhase::Settled)).unwrap();
    assert!(store.unfinished_intents().unwrap().is_empty());
    conflict(write(intent(
        "intent-1",
        "run-1",
        "task-1",
        IntentPhase::Started,
    )));

    // 전달 전 취소
    write(intent("intent-3", "run-3", "task-1", IntentPhase::Recorded)).unwrap();
    write(intent(
        "intent-3",
        "run-3",
        "task-1",
        IntentPhase::Abandoned,
    ))
    .unwrap();
    assert!(store.unfinished_intents().unwrap().is_empty());
}

// §14.3, SAFE-13: 같은 키·같은 내용은 재생, 다른 내용은 거부. 어느 쪽도 상태를 다시 바꾸지 않습니다.
#[test]
fn duplicate_idempotency_key_never_applies_twice() {
    let temp = TempDir::new("idempotency");
    let store = temp.open();
    let request = |digest: &str, task_id: &str| {
        batch(vec![
            StateWrite::Idempotency {
                record: idempotency("runs.start", "key-1", digest),
            },
            StateWrite::Task {
                record: task(task_id, "plan-1"),
                expected_version: None,
            },
        ])
    };

    store.apply(request("sha256:a", "task-1")).unwrap();
    let before = store.last_sequence().unwrap();

    let replay = store.apply(request("sha256:a", "task-2"));
    assert_eq!(
        replay,
        Err(StoreError::IdempotencyReplay {
            scope: "runs.start".into(),
            key: "key-1".into()
        })
    );
    let other = store.apply(request("sha256:b", "task-3"));
    assert_eq!(
        other,
        Err(StoreError::IdempotencyConflict {
            scope: "runs.start".into(),
            key: "key-1".into()
        })
    );

    assert!(store.task("task-2").unwrap().is_none());
    assert!(store.task("task-3").unwrap().is_none());
    assert_eq!(store.last_sequence().unwrap(), before);
    // 재생할 처음 결과를 읽을 수 있습니다. 같은 키라도 범위가 다르면 별개입니다.
    let stored = store.idempotency("runs.start", "key-1").unwrap().unwrap();
    assert_eq!(stored.payload_digest, "sha256:a");
    assert_eq!(stored.result["runId"], "run-1");
    assert!(store.idempotency("decision", "key-1").unwrap().is_none());
}

// GIT-02, WS-02, §14.3: Writer는 혼자, Reader는 함께. 넘겨받은 뒤 이전 소유자의 해제는 거부합니다.
#[test]
fn locks_exclude_writers_and_fence_previous_owners() {
    let temp = TempDir::new("locks");
    let store = temp.open();
    let acquire = |lock: ResourceLock| store.apply(batch(vec![StateWrite::AcquireLock { lock }]));
    let lock_conflict = |result: Result<_, StoreError>| {
        assert!(
            matches!(result, Err(StoreError::LockConflict { .. })),
            "{result:?}"
        )
    };

    acquire(lock("w1", "ws-1", LockMode::Writer, 1)).unwrap();
    lock_conflict(acquire(lock("w2", "ws-1", LockMode::Writer, 1)));
    lock_conflict(acquire(lock("r1", "ws-1", LockMode::Reader, 1)));
    // 다른 자원은 영향이 없습니다.
    acquire(lock("r1", "ws-2", LockMode::Reader, 1)).unwrap();
    acquire(lock("r2", "ws-2", LockMode::Reader, 1)).unwrap();
    lock_conflict(acquire(lock("w3", "ws-2", LockMode::Writer, 1)));
    // 같은 잠금 ID를 다시 잡을 수 없습니다.
    lock_conflict(acquire(lock("r1", "ws-3", LockMode::Reader, 1)));
    assert_eq!(store.locks().unwrap().len(), 3);

    // 소유 Epoch가 다르면 풀지 못합니다.
    conflict(store.apply(batch(vec![StateWrite::ReleaseLock {
        lock_id: "w1".into(),
        owner_epoch: 2,
    }])));

    // 넘겨받기: 같은 배치에서 이전 잠금을 풀고 Epoch를 올린 잠금을 잡습니다.
    store
        .apply(batch(vec![
            StateWrite::ReleaseLock {
                lock_id: "w1".into(),
                owner_epoch: 1,
            },
            StateWrite::AcquireLock {
                lock: lock("w1-takeover", "ws-1", LockMode::Writer, 2),
            },
        ]))
        .unwrap();
    // 이전 소유자가 뒤늦게 풀려고 해도 새 소유자의 잠금은 그대로입니다.
    conflict(store.apply(batch(vec![StateWrite::ReleaseLock {
        lock_id: "w1".into(),
        owner_epoch: 1,
    }])));
    let held = store.locks().unwrap();
    let ws1: Vec<_> = held.iter().filter(|l| l.lock_id == "w1-takeover").collect();
    assert_eq!(ws1.len(), 1);
    assert_eq!(ws1[0].owner_epoch, 2);

    let entries = store.journal_after(None, 0, 100).unwrap().unwrap();
    let last = entries.last().unwrap();
    assert_eq!(last.kind, JournalKind::LockChanged);
    assert_eq!(last.payload["held"], true);
    assert_eq!(last.entity_version, 2);
}

// ART-03, CTX-01, §8.3: 불변 기록은 같은 내용의 재기록만 받아들이고 Journal을 늘리지 않습니다.
#[test]
fn immutable_records_cannot_be_rewritten() {
    let temp = TempDir::new("immutable");
    let store = temp.open();
    let version = artifact("artifact-1", "v1", "task-1");
    let reference = |run_id: &str| ArtifactReference {
        target: artifact_ref("artifact-1", "v1"),
        holder: ArtifactHolder::RunInput {
            run_id: run_id.into(),
        },
        created_at: AT.into(),
    };

    // 없는 Version을 참조할 수 없습니다.
    let dangling = store.apply(batch(vec![StateWrite::ArtifactReference {
        reference: reference("run-1"),
    }]));
    assert!(
        matches!(dangling, Err(StoreError::Invalid { .. })),
        "{dangling:?}"
    );

    store
        .apply(batch(vec![
            StateWrite::ArtifactVersion {
                version: version.clone(),
            },
            StateWrite::ArtifactReference {
                reference: reference("run-1"),
            },
            StateWrite::Session {
                record: session("session-1", "task-1", "run-1"),
            },
            StateWrite::ContextSnapshot {
                snapshot: snapshot("run-1", "task-1"),
            },
            StateWrite::Usage {
                record: usage("usage-1", "plan-1", 10),
            },
        ]))
        .unwrap();
    let recorded = store.last_sequence().unwrap();
    // Version·Session·사용량만 Journal 항목을 만듭니다.
    assert_eq!(recorded, 3);

    // 같은 내용을 다시 써도 아무것도 바뀌지 않습니다.
    let again = store
        .apply(batch(vec![
            StateWrite::ArtifactVersion {
                version: version.clone(),
            },
            StateWrite::ArtifactReference {
                reference: reference("run-1"),
            },
            StateWrite::ArtifactReference {
                reference: reference("run-2"),
            },
            StateWrite::Session {
                record: session("session-1", "task-1", "run-1"),
            },
            StateWrite::ContextSnapshot {
                snapshot: snapshot("run-1", "task-1"),
            },
            StateWrite::Usage {
                record: usage("usage-1", "plan-1", 10),
            },
        ]))
        .unwrap();
    assert_eq!((again.entries, again.last_sequence), (0, recorded));
    assert_eq!(again.first_sequence(), None);
    assert_eq!(
        store
            .artifact_references(&artifact_ref("artifact-1", "v1"))
            .unwrap()
            .len(),
        2
    );
    assert_eq!(store.plan_usage("plan-1").unwrap().len(), 1);

    // 다른 내용으로는 바꿀 수 없습니다.
    let mut changed = version.clone();
    changed.checksum = "sha256:bb".into();
    conflict(store.apply(batch(vec![StateWrite::ArtifactVersion {
        version: changed,
    }])));
    let mut moved = session("session-1", "task-1", "run-1");
    moved.owner_task_id = "task-2".into();
    conflict(store.apply(batch(vec![StateWrite::Session { record: moved }])));
    let mut other_snapshot = snapshot("run-1", "task-1");
    other_snapshot.assumptions.push("changed".into());
    conflict(store.apply(batch(vec![StateWrite::ContextSnapshot {
        snapshot: other_snapshot,
    }])));
    conflict(store.apply(batch(vec![StateWrite::Usage {
        record: usage("usage-1", "plan-1", 99),
    }])));

    assert_eq!(
        store
            .artifact_version(&artifact_ref("artifact-1", "v1"))
            .unwrap()
            .unwrap(),
        version
    );
    assert_eq!(store.task_artifacts("task-1").unwrap().len(), 1);
    assert_eq!(
        store.context_snapshot("run-1").unwrap().unwrap(),
        snapshot("run-1", "task-1")
    );
    assert_eq!(store.task_sessions("task-1").unwrap().len(), 1);
}

// §14.1: 나머지 기록의 저장·조회와 Journal 종류
#[test]
fn remaining_records_round_trip() {
    let temp = TempDir::new("roundtrip");
    let store = temp.open();
    store
        .apply(batch(vec![
            StateWrite::Plan {
                plan: plan("plan-1"),
                expected_version: None,
            },
            StateWrite::Task {
                record: task("task-1", "plan-1"),
                expected_version: None,
            },
            StateWrite::Run {
                record: run("run-1", "task-1"),
                expected_version: None,
            },
            StateWrite::Workspace {
                record: workspace("ws-1", "task-1"),
                expected_version: None,
            },
            StateWrite::Validation {
                job: validation("validation-1", "task-1"),
                expected_version: None,
            },
            StateWrite::Approval {
                decision: approval("approval-1", "plan-1"),
                expected_version: None,
            },
            StateWrite::Budget {
                ledger: budget("plan-1", 0),
                expected_version: None,
            },
            StateWrite::RunSession {
                link: RunSession {
                    run_id: "run-1".into(),
                    session_id: "session-1".into(),
                    usage: SessionUse::Created,
                    native_turn_ids: vec![],
                },
            },
        ]))
        .unwrap();

    // 갱신
    let mut job = store.validation("validation-1").unwrap().unwrap();
    assert_eq!(job.state_version, 1);
    job.status = ValidationJobStatus::Finished;
    job.verdict = Some(ValidationVerdict::Passed);
    let mut decision = store.approval("approval-1").unwrap().unwrap();
    decision.state = ApprovalState::Granted;
    decision.answer = Some(ApprovalAnswer::Allow);
    decision.decided_by = Some(ApprovalActor::User);
    let mut ws = store.workspace("ws-1").unwrap().unwrap();
    ws.state = WorkspaceState::Retained;
    let mut held = plan("plan-1");
    held.status = PlanStatus::Held;
    store
        .apply(batch(vec![
            StateWrite::Validation {
                job,
                expected_version: Some(1),
            },
            StateWrite::Approval {
                decision,
                expected_version: Some(1),
            },
            StateWrite::Workspace {
                record: ws,
                expected_version: Some(1),
            },
            StateWrite::Budget {
                ledger: budget("plan-1", 2),
                expected_version: Some(1),
            },
            StateWrite::Plan {
                plan: held,
                expected_version: Some(1),
            },
            StateWrite::RunSession {
                link: RunSession {
                    run_id: "run-1".into(),
                    session_id: "session-1".into(),
                    usage: SessionUse::Created,
                    native_turn_ids: vec!["turn-1".into()],
                },
            },
        ]))
        .unwrap();
    // 예산 장부는 버전이 맞아야 바뀝니다(동시 예약의 이중 차감 방지).
    conflict(store.apply(batch(vec![StateWrite::Budget {
        ledger: budget("plan-1", 3),
        expected_version: Some(1),
    }])));

    let validations = store.task_validations("task-1").unwrap();
    assert_eq!(validations.len(), 1);
    assert_eq!(validations[0].verdict, Some(ValidationVerdict::Passed));
    assert_eq!(validations[0].state_version, 2);
    let approvals = store.plan_approvals("plan-1").unwrap();
    assert_eq!(approvals[0].state, ApprovalState::Granted);
    assert_eq!(
        store.task_workspaces("task-1").unwrap()[0].state,
        WorkspaceState::Retained
    );
    let ledger = store.budget("plan-1").unwrap().unwrap();
    assert_eq!((ledger.value.tasks_consumed, ledger.version), (2, 2));
    let plans = store.plans().unwrap();
    assert_eq!(plans.len(), 1);
    assert_eq!(
        (plans[0].value.status, plans[0].version),
        (PlanStatus::Held, 2)
    );
    assert_eq!(
        store.run_sessions("run-1").unwrap()[0].native_turn_ids,
        ["turn-1"]
    );
    assert_eq!(store.plan_tasks("plan-1").unwrap().len(), 1);
    assert_eq!(store.task_runs("task-1").unwrap().len(), 1);
    assert!(store.plan("plan-none").unwrap().is_none());

    let kinds: Vec<JournalKind> = store
        .journal_after(Some("plan-1"), 8, 100)
        .unwrap()
        .unwrap()
        .iter()
        .map(|e| e.kind)
        .collect();
    assert_eq!(
        kinds,
        [
            JournalKind::ValidationRecorded,
            JournalKind::ApprovalRecorded,
            JournalKind::WorkspaceChanged,
            JournalKind::BudgetChanged,
            JournalKind::PlanChanged,
            JournalKind::SessionChanged,
        ]
    );
}

// §4.2, §13.1: 대상 기록이 없는 Journal 항목은 결정 적용과 제어 이벤트만 받습니다.
#[test]
fn journal_only_writes_are_limited_to_decisions_and_control() {
    let temp = TempDir::new("journal-only");
    let store = temp.open();
    let write = |kind: JournalKind| {
        let mut payload = JsonObject::new();
        payload.insert("decisionId".into(), "decision-1".into());
        store.apply(batch(vec![StateWrite::Journal {
            scope: JournalScope {
                plan_id: Some("plan-1".into()),
                ..JournalScope::default()
            },
            kind,
            payload,
        }]))
    };
    write(JournalKind::DecisionApplied).unwrap();
    write(JournalKind::Control).unwrap();
    // 상태 변경 종류를 기록 없이 만들어 낼 수 없습니다.
    let forged = write(JournalKind::TaskChanged);
    assert!(
        matches!(forged, Err(StoreError::Invalid { .. })),
        "{forged:?}"
    );
    let entries = store.journal_after(Some("plan-1"), 0, 10).unwrap().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].payload["decisionId"], "decision-1");
}

// §14.3: Provider 원본 이벤트는 Run별 순서로 따로 저장하고 Journal 번호를 쓰지 않습니다.
#[test]
fn run_events_are_kept_per_run_and_redelivery_is_ignored() {
    let temp = TempDir::new("events");
    let store = temp.open();
    let first = fixtures::completed_run_events("run-1");
    let second = fixtures::cancelled_run_events("run-2");
    store.append_run_events(&first).unwrap();
    store.append_run_events(&second).unwrap();
    // 재전달
    store.append_run_events(&first[..2]).unwrap();
    store.append_run_events(&[]).unwrap();

    assert_eq!(store.run_events("run-1", 0, 1_000).unwrap(), first);
    assert_eq!(store.run_events("run-2", 0, 1_000).unwrap(), second);
    let from = first[2].sequence;
    assert_eq!(
        store.run_events("run-1", from, 1).unwrap(),
        [first[2].clone()]
    );
    assert!(store.run_events("run-none", 0, 10).unwrap().is_empty());
    assert_eq!(store.last_sequence().unwrap(), 0);
}

// §9.5: 여러 스레드가 같은 버전에서 갱신하면 하나만 적용됩니다.
#[test]
fn concurrent_writers_from_one_version_have_one_winner() {
    let temp = TempDir::new("concurrent");
    let store: Arc<dyn Store> = Arc::new(temp.open());
    store
        .apply(batch(vec![StateWrite::Task {
            record: task("task-1", "plan-1"),
            expected_version: None,
        }]))
        .unwrap();

    for round in 1..=5u64 {
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let store = Arc::clone(&store);
                std::thread::spawn(move || {
                    let mut record = store.task("task-1").unwrap().unwrap();
                    record.updated_at = format!("round-{round}");
                    store.apply(batch(vec![StateWrite::Task {
                        record,
                        expected_version: Some(round),
                    }]))
                })
            })
            .collect();
        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert_eq!(
            results.iter().filter(|r| r.is_ok()).count(),
            1,
            "{results:?}"
        );
        assert!(
            results
                .iter()
                .filter(|r| r.is_err())
                .all(|r| matches!(r, Err(StoreError::VersionConflict { .. })))
        );
    }
    assert_eq!(store.task("task-1").unwrap().unwrap().state_version, 6);
    assert_eq!(store.last_sequence().unwrap(), 6);
}

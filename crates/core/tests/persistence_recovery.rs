//! PH1-PERSIST: 재시작 뒤 보존, 단일 소유권, Schema 버전, 저장 실패 (Spec §14.1~§14.3, §15.1, OPS-01·03).

#[path = "fixtures/persistence/mod.rs"]
mod support;

use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

use support::*;
use telos_core::persistence::{SqliteOptions, SqliteStore, StateWrite, Store, StoreError};
use telos_schema::*;

const CHILD_DB: &str = "TELOS_PERSIST_CHILD_DB";

fn started_run_batch() -> telos_core::persistence::WriteBatch {
    batch(vec![
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
        StateWrite::Intent {
            intent: intent("intent-1", "run-1", "task-1", IntentPhase::Recorded),
        },
        StateWrite::AcquireLock {
            lock: lock("lock-1", "ws-1", LockMode::Writer, 1),
        },
    ])
}

// SESS-03, SAFE-06: 확정한 기록은 저장소를 닫았다 열어도 그대로이고 Journal 번호가 이어집니다.
#[test]
fn committed_records_survive_reopen() {
    let temp = TempDir::new("reopen");
    {
        let store = temp.open();
        store.apply(started_run_batch()).unwrap();
        store
            .apply(batch(vec![StateWrite::Intent {
                intent: intent("intent-1", "run-1", "task-1", IntentPhase::Dispatched),
            }]))
            .unwrap();
        store
            .append_run_events(&fixtures::completed_run_events("run-1"))
            .unwrap();
    }

    let store = temp.open();
    assert_eq!(store.task("task-1").unwrap().unwrap().state_version, 1);
    assert_eq!(
        store.run("run-1").unwrap().unwrap().status,
        RunStatus::Starting
    );
    // 재시작 대조에 필요한 미완 의도와 잠금 (§14.3)
    let unfinished = store.unfinished_intents().unwrap();
    assert_eq!(unfinished.len(), 1);
    assert_eq!(unfinished[0].phase, IntentPhase::Dispatched);
    assert_eq!(
        unfinished[0].phase.recovery_action(),
        RecoveryAction::Reconcile
    );
    assert_eq!(store.locks().unwrap().len(), 1);
    assert_eq!(store.run_events("run-1", 0, 100).unwrap().len(), 4);
    assert_eq!(store.last_sequence().unwrap(), 6);

    let applied = store
        .apply(batch(vec![StateWrite::Plan {
            plan: plan("plan-2"),
            expected_version: None,
        }]))
        .unwrap();
    assert_eq!(applied.first_sequence(), Some(7));
}

// OPS-01: 프로젝트 DB의 소유자는 하나입니다.
#[test]
fn second_open_is_rejected_while_in_use() {
    let temp = TempDir::new("single-owner");
    let first = temp.open();
    first.apply(started_run_batch()).unwrap();

    let second = SqliteStore::open(&temp.db());
    assert!(
        matches!(second, Err(StoreError::AlreadyOpen { .. })),
        "{second:?}"
    );
    // 거부된 시도가 첫 소유자의 기록을 건드리지 않습니다.
    assert_eq!(first.last_sequence().unwrap(), 5);

    drop(first);
    assert_eq!(temp.open().last_sequence().unwrap(), 5);
}

// §14.1: 더 새 Core가 만든 DB는 열지 않고 건드리지 않습니다.
#[test]
fn newer_schema_is_not_opened_or_modified() {
    let temp = TempDir::new("too-new");
    drop(temp.open());
    let newer = CORE_DB_SCHEMA_VERSION + 1;
    {
        let conn = rusqlite::Connection::open(temp.db()).unwrap();
        conn.pragma_update(None, "user_version", newer).unwrap();
    }
    let before = std::fs::read(temp.db()).unwrap();

    let result = SqliteStore::open(&temp.db());
    assert_eq!(
        result.err(),
        Some(StoreError::SchemaTooNew { found: newer })
    );
    assert_eq!(std::fs::read(temp.db()).unwrap(), before);
}

// §15.1, E2E-08: 저장 공간이 없으면 쓰기가 실패하고, 그 배치는 남지 않으며 이전 기록은 읽힙니다.
#[test]
fn full_storage_fails_the_write_without_partial_state() {
    let temp = TempDir::new("full");
    drop(temp.open());
    let pages = {
        let conn = rusqlite::Connection::open(temp.db()).unwrap();
        conn.pragma_query_value(None, "page_count", |row| row.get::<_, u32>(0))
            .unwrap()
    };
    let store = SqliteStore::open_with(
        &temp.db(),
        SqliteOptions {
            max_pages: Some(pages + 400),
            ..SqliteOptions::default()
        },
    )
    .unwrap();

    let big = "x".repeat(16 * 1024);
    let mut written = 0u64;
    let failure = loop {
        let mut record = task(&format!("task-{written}"), "plan-1");
        record.spec.objective = big.clone();
        // 한 배치에 Task 둘: 실패하면 둘 다 없어야 합니다.
        let mut sibling = task(&format!("task-{written}-b"), "plan-1");
        sibling.spec.objective = big.clone();
        match store.apply(batch(vec![
            StateWrite::Task {
                record,
                expected_version: None,
            },
            StateWrite::Task {
                record: sibling,
                expected_version: None,
            },
        ])) {
            Ok(_) => written += 1,
            Err(error) => break error,
        }
        assert!(written < 1_000, "storage limit was never reached");
    };
    assert!(matches!(failure, StoreError::Io { .. }), "{failure:?}");
    assert!(written > 0);

    // 실패한 배치의 흔적이 없고 번호에 빈틈이 없습니다.
    assert!(store.task(&format!("task-{written}")).unwrap().is_none());
    assert!(store.task(&format!("task-{written}-b")).unwrap().is_none());
    assert_eq!(
        store.plan_tasks("plan-1").unwrap().len() as u64,
        written * 2
    );
    assert_eq!(store.last_sequence().unwrap(), written * 2);
    let entries = store.journal_after(None, 0, 10_000).unwrap().unwrap();
    assert_eq!(entries.len() as u64, written * 2);
    assert_eq!(entries.last().unwrap().sequence, written * 2);
}

/// 아래 시험이 다시 실행하는 자식 프로세스의 본문입니다. 환경 변수가 없으면 아무것도 하지 않습니다.
#[test]
fn child_writer_holds_the_database_until_killed() {
    let Some(db) = std::env::var_os(CHILD_DB) else {
        return;
    };
    let store = SqliteStore::open(Path::new(&db)).unwrap();
    store.apply(started_run_batch()).unwrap();
    store
        .apply(batch(vec![StateWrite::Intent {
            intent: intent("intent-1", "run-1", "task-1", IntentPhase::Dispatched),
        }]))
        .unwrap();
    println!("child-ready");
    loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}

// §14.2 "Core 비정상 종료": 정리 없이 죽은 프로세스의 확정 기록이 남고, 다음 Core가 DB를 넘겨받습니다.
#[test]
fn killed_process_leaves_committed_records_and_frees_the_database() {
    let temp = TempDir::new("killed");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "child_writer_holds_the_database_until_killed",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_DB, temp.db())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let stdout = BufReader::new(child.stdout.take().unwrap());
    let ready = stdout
        .lines()
        .map_while(Result::ok)
        .any(|line| line.contains("child-ready"));
    if !ready {
        let _ = child.kill();
        panic!("child exited before writing");
    }

    // 살아 있는 동안에는 다른 프로세스가 열 수 없습니다.
    let while_alive = SqliteStore::open(&temp.db());
    assert!(
        matches!(while_alive, Err(StoreError::AlreadyOpen { .. })),
        "{while_alive:?}"
    );

    child.kill().unwrap();
    child.wait().unwrap();

    let store = temp.open();
    assert_eq!(store.task("task-1").unwrap().unwrap().state_version, 1);
    let unfinished = store.unfinished_intents().unwrap();
    assert_eq!(unfinished.len(), 1);
    assert_eq!(unfinished[0].phase, IntentPhase::Dispatched);
    assert_eq!(store.locks().unwrap().len(), 1);
    assert_eq!(store.last_sequence().unwrap(), 6);
}

//! H0 Workspace: Writer 충돌, Lock 인수 조건, 주입 경로, Wire 형식 (Spec §10, §14.3, §19.6.2).

use serde_json::json;
use telos_schema::*;

fn lock() -> ResourceLock {
    ResourceLock {
        lock_id: "lock-1".into(),
        resource: LockResource::Workspace {
            workspace_id: "ws-1".into(),
        },
        mode: LockMode::Writer,
        holder_task_id: "task-1".into(),
        holder_run_id: "run-1".into(),
        owner_epoch: 1,
        process: None,
        acquired_at: "2026-11-10T00:00:00Z".into(),
        expires_at: Some("2026-11-10T01:00:00Z".into()),
    }
}

#[test]
fn writer_excludes_everything_readers_share() {
    assert!(LockMode::Writer.conflicts_with(LockMode::Writer));
    assert!(LockMode::Writer.conflicts_with(LockMode::Reader));
    assert!(LockMode::Reader.conflicts_with(LockMode::Writer));
    assert!(!LockMode::Reader.conflicts_with(LockMode::Reader));
}

#[test]
fn expired_lock_of_a_live_or_unknown_holder_is_not_taken_over() {
    let lock = lock();
    assert!(!lock.can_take_over(true, Some(true)), "alive");
    assert!(!lock.can_take_over(true, None), "unknown -> reconcile");
    assert!(!lock.can_take_over(false, Some(false)), "not expired");
    assert!(lock.can_take_over(true, Some(false)));
}

#[test]
fn injected_agents_path_is_excluded() {
    assert_eq!(INJECTED_WORKSPACE_PATHS, &[".agents/"]);
}

#[test]
fn workspace_wire_format() {
    let value = serde_json::to_value(lock()).unwrap();
    assert_eq!(
        value["resource"],
        json!({"kind": "workspace", "workspaceId": "ws-1"})
    );
    assert_eq!(value["mode"], "writer");
    let change = serde_json::to_value(FileChange::Renamed {
        from: "a.rs".into(),
        path: "b.rs".into(),
    })
    .unwrap();
    assert_eq!(
        change,
        json!({"change": "renamed", "from": "a.rs", "path": "b.rs"})
    );
    assert_eq!(
        serde_json::to_value(WorkspaceState::Retained).unwrap(),
        "Retained"
    );
}

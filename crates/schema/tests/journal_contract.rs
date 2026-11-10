//! H0 영속 기록: 의도 단계별 복구, 멱등성, DB 버전 판정 (Spec §14.1·§14.3, §19.6.2).

use telos_schema::*;

#[test]
fn crash_recovery_depends_on_intent_phase() {
    // 전달 전 장애는 다시 전달할 수 있지만, 전달 뒤에는 시작 응답이 없어도 대조합니다.
    assert_eq!(
        IntentPhase::Recorded.recovery_action(),
        RecoveryAction::ResumeDispatch
    );
    assert_eq!(
        IntentPhase::Dispatched.recovery_action(),
        RecoveryAction::Reconcile
    );
    assert_eq!(
        IntentPhase::Started.recovery_action(),
        RecoveryAction::Reconcile
    );
    assert_eq!(
        IntentPhase::Settled.recovery_action(),
        RecoveryAction::Nothing
    );
    assert_eq!(
        IntentPhase::Abandoned.recovery_action(),
        RecoveryAction::Nothing
    );
}

#[test]
fn idempotency_replays_same_payload_and_rejects_different() {
    assert_eq!(check_idempotency(None, "a"), IdempotencyCheck::New);
    assert_eq!(check_idempotency(Some("a"), "a"), IdempotencyCheck::Replay);
    assert_eq!(
        check_idempotency(Some("a"), "b"),
        IdempotencyCheck::Conflict
    );
}

#[test]
fn newer_database_is_never_opened() {
    assert_eq!(
        db_open_decision(CORE_DB_SCHEMA_VERSION),
        DbOpenDecision::Current
    );
    assert_eq!(
        db_open_decision(CORE_DB_SCHEMA_VERSION + 1),
        DbOpenDecision::TooNew {
            found: CORE_DB_SCHEMA_VERSION + 1
        }
    );
    if CORE_DB_SCHEMA_VERSION > 1 {
        assert!(matches!(
            db_open_decision(1),
            DbOpenDecision::MigrateWithBackup { from: 1 }
        ));
    }
}

#[test]
fn intent_wire_format_keeps_digests_not_secrets() {
    let intent = RunIntent {
        intent_id: "intent-1".into(),
        run_id: "run-1".into(),
        task_id: "task-1".into(),
        task_revision: 1,
        attempt: 1,
        idempotency_key: "k".into(),
        payload_digest: "sha256:abc".into(),
        phase: IntentPhase::Dispatched,
        host_id: "windows-local".into(),
        provider: "claude".into(),
        native_session_id: None,
        process: Some(ProcessIdentity {
            host_id: "windows-local".into(),
            pid: 42,
            process_started_at: "2026-11-10T00:00:00Z".into(),
            run_token_digest: "sha256:def".into(),
        }),
        recorded_at: "2026-11-10T00:00:00Z".into(),
        updated_at: "2026-11-10T00:00:00Z".into(),
    };
    let value = serde_json::to_value(&intent).unwrap();
    assert_eq!(value["phase"], "dispatched");
    assert_eq!(value["process"]["runTokenDigest"], "sha256:def");
    assert!(value.get("nativeSessionId").is_none());
    assert_eq!(serde_json::from_value::<RunIntent>(value).unwrap(), intent);
}

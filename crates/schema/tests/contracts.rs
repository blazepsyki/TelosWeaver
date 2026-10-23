//! G0 계약의 Wire 형식과 공통 실패 Fixture 시험 (Spec §19.5.2).

use serde_json::json;
use telos_schema::{api::*, fixtures, rn::*, *};

#[test]
fn completed_run_fixture_follows_event_contract() {
    let events = fixtures::completed_run_events("run-1");
    let sequences: Vec<_> = events.iter().map(|e| e.sequence).collect();
    assert_eq!(sequences, [1, 2, 3, 4]);
    assert_eq!(
        events.last().unwrap().kind,
        ProviderEventKind::ProcessExited
    );
    for event in &events {
        let payload = ProviderEventPayload::parse(event.kind, &event.payload).unwrap();
        assert_eq!(payload.kind(), event.kind);
        assert_eq!(payload.to_object(), event.payload);
    }
}

#[test]
fn event_payload_must_match_kind() {
    let text = fixtures::completed_run_events("run-1").remove(1);
    assert!(ProviderEventPayload::parse(ProviderEventKind::ProcessExited, &text.payload).is_err());
}

#[test]
fn unknown_usage_is_omitted_not_zero() {
    let usage = ProviderEventPayload::Usage(UsagePayload {
        scope: UsageScope::Cumulative,
        source: UsageSource::ProviderReported,
        input_tokens: Some(10),
        output_tokens: None,
        cache_read_tokens: None,
        cache_write_tokens: None,
        cache_included_in_input: None,
        cost_usd: None,
    });
    assert_eq!(
        serde_json::Value::Object(usage.to_object()),
        json!({ "scope": "cumulative", "source": "provider_reported", "inputTokens": 10 })
    );
}

#[test]
fn valid_decision_payload_parses_by_kind() {
    let decision = fixtures::valid_create_tasks_decision();
    let payload = DecisionPayload::from_decision(&decision).unwrap();
    assert_eq!(payload.kind(), MasterDecisionKind::CreateTasks);
}

#[test]
fn invalid_payload_fixtures_are_rejected() {
    let cases = fixtures::invalid_payload_decisions();
    assert!(!cases.is_empty());
    for (description, decision) in cases {
        assert!(
            DecisionPayload::from_decision(&decision).is_err(),
            "accepted invalid payload: {description}"
        );
    }
}

#[test]
fn decision_outcome_wire_format() {
    let outcome = DecisionOutcome {
        decision_id: "d1".into(),
        idempotency_key: "k1".into(),
        status: DecisionStatus::Rejected,
        rejection: Some(DecisionRejection {
            code: RejectionCode::StaleControllerEpoch,
            message: "epoch 1 < 2".into(),
            field: None,
        }),
        effects: vec![DecisionEffect::TaskCreated {
            task_id: "t1".into(),
            task_revision: 1,
        }],
        replayed: false,
        recorded_at: "2026-10-24T00:00:00Z".into(),
    };
    assert_eq!(
        serde_json::to_value(&outcome).unwrap(),
        json!({
            "decisionId": "d1",
            "idempotencyKey": "k1",
            "status": "rejected",
            "rejection": { "code": "stale_controller_epoch", "message": "epoch 1 < 2" },
            "effects": [{ "type": "task_created", "taskId": "t1", "taskRevision": 1 }],
            "replayed": false,
            "recordedAt": "2026-10-24T00:00:00Z",
        })
    );
}

#[test]
fn api_messages_wire_format() {
    let request = ApiRequest {
        protocol_version: API_PROTOCOL_VERSION.into(),
        request_id: "r1".into(),
        method: ApiMethod::RunsSubscribe,
        params: serde_json::to_value(RunsSubscribeParams {
            run_id: "run-1".into(),
            after_sequence: 0,
        })
        .unwrap(),
    };
    assert_eq!(
        serde_json::to_value(&request).unwrap(),
        json!({
            "protocolVersion": "0.1",
            "requestId": "r1",
            "method": "runs.subscribe",
            "params": { "runId": "run-1", "afterSequence": 0 },
        })
    );

    let error: ApiResponse = serde_json::from_value(json!({
        "protocolVersion": "0.1",
        "requestId": "r1",
        "error": { "code": "unauthenticated", "message": "call core.hello first" },
    }))
    .unwrap();
    assert!(matches!(
        error.outcome,
        ApiOutcome::Error(ApiError {
            code: ApiErrorCode::Unauthenticated,
            ..
        })
    ));

    assert_eq!(
        serde_json::to_value(ApiNotificationMethod::SubscriptionClosed).unwrap(),
        json!("runs.subscriptionClosed")
    );
}

#[test]
fn rn_messages_wire_format() {
    assert_eq!(
        serde_json::to_value(RnMethod::ProvidersAgyProbe).unwrap(),
        json!("providers.agy.probe")
    );
    let wire = json!({
        "protocolVersion": RN_PROTOCOL_VERSION,
        "requestId": "r1",
        "result": { "status": "ok", "rpcVersion": "0.1", "upstreamCommit": "a", "forkCommit": "b", "hostId": "h" },
    });
    let response: RnResponse = serde_json::from_value(wire.clone()).unwrap();
    let RnOutcome::Result(result) = &response.outcome else {
        panic!("expected result");
    };
    let health: RnHealth = serde_json::from_value(result.clone()).unwrap();
    assert_eq!(health.status, RnHealthStatus::Ok);
    assert_eq!(serde_json::to_value(&response).unwrap(), wire);
}

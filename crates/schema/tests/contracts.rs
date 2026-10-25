//! G0 계약의 Wire 형식과 공통 실패 Fixture 시험 (Spec §19.5.2, G0 개정 1).

use serde_json::json;
use telos_schema::{api::*, fixtures, rn::*, *};

// --- 이벤트 (REQ-006) ---

#[test]
fn valid_run_fixtures_follow_termination_rules() {
    let cases = [
        (
            fixtures::completed_run_events("run-1"),
            RunTermination::ProcessExited,
        ),
        (
            fixtures::start_failed_run_events("run-1"),
            RunTermination::TerminalError,
        ),
        (
            fixtures::cancelled_run_events("run-1"),
            RunTermination::ProcessExited,
        ),
        (
            fixtures::persistent_process_run_events("run-1"),
            RunTermination::TurnFinished,
        ),
        (
            fixtures::tracking_lost_run_events("run-1"),
            RunTermination::TerminalError,
        ),
    ];
    for (events, expected) in cases {
        assert_eq!(check_run_events(&events), Ok(expected));
        for event in &events {
            assert_eq!(event.schema_version, PROVIDER_EVENT_SCHEMA_VERSION);
            let payload = ProviderEventPayload::parse(event.kind, &event.payload).unwrap();
            assert_eq!(payload.to_object(), event.payload);
        }
    }
}

#[test]
fn start_failure_and_lost_tracking_do_not_fabricate_process_exit() {
    for events in [
        fixtures::start_failed_run_events("run-1"),
        fixtures::tracking_lost_run_events("run-1"),
    ] {
        assert!(
            events
                .iter()
                .all(|e| e.kind != ProviderEventKind::ProcessExited)
        );
    }
}

#[test]
fn invalid_run_event_sequences_are_rejected() {
    let cases = fixtures::invalid_run_event_sequences("run-1");
    assert!(!cases.is_empty());
    for (description, events) in cases {
        assert!(
            check_run_events(&events).is_err(),
            "accepted invalid sequence: {description}"
        );
    }
    assert_eq!(check_run_events(&[]), Err(RunEventViolation::Empty));
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

// --- 마스터 결정 (REQ-004, REQ-008, REQ-009) ---

#[test]
fn valid_decision_payload_parses_by_kind() {
    let decision = fixtures::valid_create_tasks_decision();
    let payload = DecisionPayload::from_decision(&decision).unwrap();
    assert_eq!(payload.kind(), MasterDecisionKind::CreateTasks);
}

#[test]
fn invalid_payload_fixtures_are_rejected_including_nested_fields() {
    let cases = fixtures::invalid_payload_decisions();
    assert!(cases.iter().any(|(d, _)| d.contains("TaskSpec")));
    for (description, decision) in cases {
        assert!(
            DecisionPayload::from_decision(&decision).is_err(),
            "accepted invalid payload: {description}"
        );
    }
}

#[test]
fn explicit_free_form_parameters_remain_extensible() {
    let mut task = fixtures::task_spec();
    task.validation.push(ValidationContract {
        id: "v1".into(),
        revision: 1,
        kind: ValidationKind::Command,
        target_output_names: vec!["answer".into()],
        parameters: json!({ "anyKey": "allowed" }).as_object().unwrap().clone(),
        required: true,
    });
    let decision = fixtures::master_decision(DecisionPayload::CreateTasks(CreateTasksPayload {
        tasks: vec![task],
    }));
    assert!(DecisionPayload::from_decision(&decision).is_ok());
}

#[test]
fn decision_outcome_wire_format() {
    let rejected = DecisionOutcome::Rejected {
        decision_id: "d1".into(),
        idempotency_key: "k1".into(),
        rejection: DecisionRejection {
            code: RejectionCode::StaleControllerEpoch,
            message: "epoch 1 < 2".into(),
            field: None,
        },
        replayed: false,
        recorded_at: "2026-10-24T00:00:00Z".into(),
    };
    assert_eq!(
        serde_json::to_value(&rejected).unwrap(),
        json!({
            "status": "rejected",
            "decisionId": "d1",
            "idempotencyKey": "k1",
            "rejection": { "code": "stale_controller_epoch", "message": "epoch 1 < 2" },
            "replayed": false,
            "recordedAt": "2026-10-24T00:00:00Z",
        })
    );
    assert!(rejected.effects().is_empty());

    let accepted = DecisionOutcome::Accepted {
        decision_id: "d2".into(),
        idempotency_key: "k2".into(),
        effects: vec![DecisionEffect::TaskCreated {
            task_id: "t1".into(),
            task_revision: 1,
        }],
        replayed: true,
        recorded_at: "2026-10-24T00:00:00Z".into(),
    };
    let wire = serde_json::to_value(&accepted).unwrap();
    assert_eq!(wire["status"], "accepted");
    assert_eq!(wire["effects"][0]["type"], "task_created");
    assert_eq!(
        serde_json::from_value::<DecisionOutcome>(wire).unwrap(),
        accepted
    );
    assert!(accepted.replayed());
}

#[test]
fn contradictory_decision_outcomes_are_rejected() {
    let base = json!({
        "decisionId": "d1",
        "idempotencyKey": "k1",
        "replayed": false,
        "recordedAt": "2026-10-24T00:00:00Z",
    });
    let rejection = json!({ "code": "permission_denied", "message": "no" });
    let effects = json!([{ "type": "task_created", "taskId": "t1", "taskRevision": 1 }]);

    let with = |extra: serde_json::Value| {
        let mut value = base.clone();
        value
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        value
    };
    let invalid = [
        ("거부 사유 없는 거부", with(json!({ "status": "rejected" }))),
        (
            "거부와 실행 효과 동시 존재",
            with(json!({ "status": "rejected", "rejection": rejection, "effects": effects })),
        ),
        (
            "승인과 거부 사유 동시 존재",
            with(json!({ "status": "accepted", "effects": [], "rejection": rejection })),
        ),
    ];
    for (description, value) in invalid {
        assert!(
            serde_json::from_value::<DecisionOutcome>(value).is_err(),
            "accepted contradictory outcome: {description}"
        );
    }
}

#[test]
fn decision_context_carries_project_and_hosts() {
    let wire = serde_json::to_value(fixtures::decision_context()).unwrap();
    assert_eq!(wire["projectId"], fixtures::PROJECT_ID);
    assert_eq!(wire["allowedHostIds"], json!([fixtures::HOST_ID]));
    assert_eq!(
        serde_json::to_value(RejectionCode::PolicyUnavailable).unwrap(),
        json!("policy_unavailable")
    );
}

// --- Local Core API (REQ-005, REQ-007) ---

#[test]
fn token_is_redacted_from_debug_but_kept_on_wire() {
    const FAKE: &str = "fake-token-for-test-0123456789";
    let hello = HelloParams {
        client_name: "test".into(),
        client_version: "0".into(),
        token: SecretString::new(FAKE),
    };
    assert!(!format!("{hello:?}").contains(FAKE));
    assert_eq!(serde_json::to_value(&hello).unwrap()["token"], FAKE);
    assert_eq!(hello.token.expose(), FAKE);

    let request = ApiRequest {
        protocol_version: API_PROTOCOL_VERSION.into(),
        request_id: "r1".into(),
        method: ApiMethod::Hello,
        params: serde_json::to_value(&hello).unwrap(),
    };
    let debug = format!("{request:?}");
    assert!(!debug.contains(FAKE), "{debug}");
    assert!(debug.contains("clientName"));
    assert_eq!(
        serde_json::to_value(&request).unwrap()["params"]["token"],
        FAKE
    );
}

#[test]
fn project_key_matches_shared_vectors() {
    #[derive(serde::Deserialize, serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Vector {
        platform: String,
        canonical: String,
        normalized: String,
        project_key: String,
    }

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../schemas/vectors/project-key.json");
    let inputs: Vec<(&str, &str)> = vec![
        ("windows", r"\\?\D:\Downloads\Vibe_dev\TelosWeaver"),
        ("windows", r"D:\Downloads\Vibe_dev\TelosWeaver\"),
        ("windows", "D:/Downloads/Vibe_dev/TelosWeaver"),
        ("windows", r"\\?\C:\"),
        ("windows", r"\\?\UNC\server\share\Project"),
        ("windows", r"D:\프로젝트\Ünïcode"),
        ("unix", "/home/user/src/TelosWeaver/"),
        ("unix", "/home/user/src/TelosWeaver"),
        ("unix", "/"),
    ];
    let computed: Vec<Vector> = inputs
        .into_iter()
        .map(|(platform, canonical)| {
            let normalized = normalize_project_root(canonical, platform == "windows");
            let project_key = project_key(&normalized);
            Vector {
                platform: platform.into(),
                canonical: canonical.into(),
                normalized,
                project_key,
            }
        })
        .collect();

    if std::env::var_os("UPDATE_SCHEMAS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&computed).unwrap() + "\n",
        )
        .unwrap();
    }
    let committed: Vec<Vector> =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        serde_json::to_value(&committed).unwrap(),
        serde_json::to_value(&computed).unwrap()
    );

    // 같은 경로의 다른 표기는 같은 키입니다.
    assert_eq!(computed[0].project_key, computed[1].project_key);
    assert_eq!(computed[0].project_key, computed[2].project_key);
    assert_eq!(computed[6].project_key, computed[7].project_key);
    assert_eq!(computed[3].normalized, "c:/");
    assert_eq!(computed[4].normalized, "//server/share/project");
    assert_eq!(computed[0].project_key.len(), 16);
}

#[test]
fn frame_limit_counts_bytes_without_newline() {
    let overhead = serde_json::to_string(&json!("")).unwrap().len();
    let fits = json!("a".repeat(MAX_FRAME_BYTES - overhead));
    let frame = encode_frame(&fits).unwrap();
    assert_eq!(frame.len(), MAX_FRAME_BYTES + 1);
    assert!(frame.ends_with('\n'));
    assert_eq!(frame.matches('\n').count(), 1);

    let too_large = json!("a".repeat(MAX_FRAME_BYTES - overhead + 1));
    assert_eq!(
        encode_frame(&too_large),
        Err(FrameTooLarge {
            bytes: MAX_FRAME_BYTES + 1
        })
    );

    // 이스케이프된 줄바꿈은 프레임을 나누지 않습니다.
    assert_eq!(
        encode_frame(&json!("a\nb")).unwrap().matches('\n').count(),
        1
    );
    assert_eq!(
        serde_json::to_value(ApiErrorCode::FrameTooLarge).unwrap(),
        json!("frame_too_large")
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
            "protocolVersion": "0.2",
            "requestId": "r1",
            "method": "runs.subscribe",
            "params": { "runId": "run-1", "afterSequence": 0 },
        })
    );

    let error: ApiResponse = serde_json::from_value(json!({
        "protocolVersion": "0.2",
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

// --- RN RPC ---

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

/// REQ-018: 인증, 알림 프레임, 구독 결과, 연결 여부, 새 오류 코드.
#[test]
fn rn_rpc_hello_notification_and_attached_wire_format() {
    assert_eq!(
        serde_json::to_value(RnMethod::Hello).unwrap(),
        json!("rn.hello")
    );
    let token = "a".repeat(64);
    let hello = RnRequest {
        protocol_version: RN_PROTOCOL_VERSION.into(),
        request_id: "h1".into(),
        method: RnMethod::Hello,
        params: serde_json::to_value(RnHelloParams {
            client_name: "telos-weaver".into(),
            client_version: "0.0.0".into(),
            token: SecretString::new(token.clone()),
        })
        .unwrap(),
    };
    // 전송에는 원문이 들어가고 Debug에는 들어가지 않습니다 (SAFE-04).
    assert_eq!(
        serde_json::to_value(&hello).unwrap()["params"]["token"],
        json!(token)
    );
    assert!(!format!("{hello:?}").contains(&token));

    let endpoint = json!({"hostId": "h", "provider": "telosweaver", "nativeSessionId": "core", "fingerprint": "f"});
    let result: RnHelloResult = serde_json::from_value(json!({
        "rpcVersion": "0.1", "hostId": "h", "connectionId": "c1", "endpoint": endpoint,
    }))
    .unwrap();
    assert_eq!(result.endpoint.provider, "telosweaver");

    let wire = json!({
        "protocolVersion": RN_PROTOCOL_VERSION,
        "method": "messages.event",
        "params": {
            "subscriptionId": "s1",
            "event": { "cursor": "1:7", "sequence": 7, "kind": "receipt",
                       "payload": { "messageId": "m1", "status": "delivered", "updatedAt": "2026-10-25T00:00:00.000Z" } },
        },
    });
    let n: RnNotification = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(n.method, RnNotificationMethod::Event);
    let params: RnEventParams = serde_json::from_value(n.params.clone()).unwrap();
    assert_eq!(
        (params.subscription_id.as_str(), params.event.kind),
        ("s1", RnEventKind::Receipt)
    );
    let receipt: DeliveryReceipt = serde_json::from_value(params.event.payload).unwrap();
    assert_eq!(receipt.status, DeliveryReceiptStatus::Delivered);
    assert_eq!(serde_json::to_value(&n).unwrap(), wire);

    let sub: SubscribeResult = serde_json::from_value(json!({"subscriptionId": "s1"})).unwrap();
    assert_eq!(sub.subscription_id, "s1");
    // 이전 RN이 보낸 SessionEndpoint(필드 없음)는 연결되지 않은 것으로 읽습니다.
    let old: SessionEndpoint =
        serde_json::from_value(json!({"endpoint": endpoint, "presence": "idle"})).unwrap();
    assert!(!old.attached);
    let inbound: InboundMessage = serde_json::from_value(json!({
        "messageId": "m2", "senderAddress": "alex/api-claude", "replyTo": "m1", "body": "ack", "receivedAt": "t",
    }))
    .unwrap();
    assert!(inbound.sender.is_none());
    for (code, s) in [
        (RnErrorCode::Unauthenticated, "unauthenticated"),
        (RnErrorCode::FrameTooLarge, "frame_too_large"),
    ] {
        assert_eq!(serde_json::to_value(code).unwrap(), json!(s));
    }
    let info: RnRpcInfo = serde_json::from_value(
        json!({"endpoint": r"\\.\pipe\telos-rn-rpc-1", "rpcVersion": "0.1", "pid": 7}),
    )
    .unwrap();
    assert_eq!(info.pid, 7);
}

// --- CLI 탐지 (REQ-010) ---

#[test]
fn cli_discovery_found_wire_format() {
    let wire = json!({
        "providerId": "codex",
        "hostId": "wsl-ubuntu-2604",
        "checkedAt": "2026-10-24T08:00:00Z",
        "outcome": {
            "status": "found",
            "cli": {
                "source": "extension",
                "location": "relative-path",
                "executablePath": "/home/u/.vscode-server/extensions/openai.chatgpt-26.1007.21434-linux-x64/bin/linux-x86_64/codex",
                "cliVersion": "codex-cli 0.162.0-alpha.17.2",
                "sha256": "0".repeat(64),
                "extension": {
                    "extensionId": "openai.chatgpt",
                    "version": "26.1007.21434",
                    "targetPlatform": "linux-x64",
                    "installPath": "/home/u/.vscode-server/extensions/openai.chatgpt-26.1007.21434-linux-x64",
                    "resolution": "metadata"
                }
            }
        }
    });
    let discovery: CliDiscovery = serde_json::from_value(wire.clone()).unwrap();
    let cli = discovery.found().unwrap();
    assert_eq!(cli.source, CliSource::Extension);
    assert_eq!(
        cli.extension.as_ref().unwrap().resolution,
        ExtensionResolution::Metadata
    );
    assert_eq!(serde_json::to_value(&discovery).unwrap(), wire);
}

#[test]
fn cli_discovery_unavailable_wire_format() {
    let wire = json!({
        "providerId": "agy",
        "hostId": "wsl-ubuntu-2604",
        "checkedAt": "2026-10-24T08:00:00Z",
        "outcome": {
            "status": "unavailable",
            "failure": "version_check_failed",
            "source": "explicit",
            "reason": "version process exited with code 1"
        }
    });
    let discovery: CliDiscovery = serde_json::from_value(wire.clone()).unwrap();
    assert!(discovery.found().is_none());
    assert_eq!(serde_json::to_value(&discovery).unwrap(), wire);
}

#[test]
fn cli_discovery_rejects_mixed_or_unknown_fields() {
    let mixed = json!({
        "providerId": "claude",
        "hostId": "windows-local",
        "checkedAt": "2026-10-24T08:00:00Z",
        "outcome": { "status": "unavailable", "failure": "not_found", "reason": "none", "cli": {} }
    });
    assert!(serde_json::from_value::<CliDiscovery>(mixed).is_err());

    let unknown = json!({
        "providerId": "claude",
        "hostId": "windows-local",
        "checkedAt": "2026-10-24T08:00:00Z",
        "token": "x",
        "outcome": { "status": "unavailable", "failure": "not_found", "reason": "none" }
    });
    assert!(serde_json::from_value::<CliDiscovery>(unknown).is_err());
}

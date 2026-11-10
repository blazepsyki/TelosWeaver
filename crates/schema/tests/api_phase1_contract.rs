//! H0 Local Core API: 새 메서드 이름, 연결 범위, 읽기 한도, 개입 응답 형식 (Spec §17, §19.6.2).

use serde_json::json;
use telos_schema::api::*;

#[test]
fn phase1_method_names() {
    let cases = [
        (ApiMethod::PlansCreate, "plans.create"),
        (ApiMethod::PlansInspect, "plans.inspect"),
        (ApiMethod::PlansCancel, "plans.cancel"),
        (ApiMethod::PlansMessage, "plans.message"),
        (ApiMethod::TasksInspect, "tasks.inspect"),
        (ApiMethod::TasksCancel, "tasks.cancel"),
        (ApiMethod::TasksTool, "tasks.tool"),
        (ApiMethod::ArtifactsRead, "artifacts.read"),
        (ApiMethod::InterventionsList, "interventions.list"),
        (ApiMethod::InterventionsAnswer, "interventions.answer"),
        (ApiMethod::JournalSubscribe, "journal.subscribe"),
        (ApiMethod::JournalUnsubscribe, "journal.unsubscribe"),
        (ApiMethod::RnStatus, "rn.status"),
        (ApiMethod::RnSend, "rn.send"),
    ];
    for (method, name) in cases {
        assert_eq!(serde_json::to_value(method).unwrap(), name);
    }
    assert_eq!(
        serde_json::to_value(ApiNotificationMethod::JournalEntry).unwrap(),
        "journal.entry"
    );
    assert_eq!(
        serde_json::to_value(ApiErrorCode::CursorExpired).unwrap(),
        "cursor_expired"
    );
}

#[test]
fn hello_scope_is_optional_and_tagged() {
    let old = json!({"serverVersion": "0.0.0", "protocolVersion": "0.2", "projectId": "p", "hostId": "h", "connectionId": "c"});
    assert_eq!(
        serde_json::from_value::<HelloResult>(old).unwrap().scope,
        None
    );
    let scope = ConnectionScope::TaskMcp {
        run_id: "run-1".into(),
    };
    assert_eq!(
        serde_json::to_value(scope).unwrap(),
        json!({"kind": "task-mcp", "runId": "run-1"})
    );
}

#[test]
fn artifact_read_chunk_fits_one_frame_after_base64() {
    let encoded = (MAX_ARTIFACT_READ_BYTES as usize).div_ceil(3) * 4;
    assert!(encoded + 4096 < MAX_FRAME_BYTES, "{encoded}");
}

#[test]
fn intervention_answers_are_tagged() {
    let params: InterventionsAnswerParams = serde_json::from_value(json!({
        "idempotencyKey": "k",
        "interventionId": "i-1",
        "answer": {"kind": "approval", "answer": "deny"}
    }))
    .unwrap();
    assert!(matches!(
        params.answer,
        InterventionAnswer::Approval {
            answer: telos_schema::ApprovalAnswer::Deny
        }
    ));
    assert!(serde_json::from_value::<InterventionsAnswerParams>(json!({
        "idempotencyKey": "k", "interventionId": "i-1", "answer": {"kind": "text", "text": "x", "extra": 1}
    }))
    .is_err());
}

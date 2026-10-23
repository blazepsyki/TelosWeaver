//! Wire 형식이 Spec §3.4 규칙(camelCase 필드, Spec 원문 열거 값)을 따르는지 확인합니다.

use serde_json::json;
use telos_schema::*;

#[test]
fn enum_values_match_spec_strings() {
    let cases = [
        (
            serde_json::to_value(MasterDecisionKind::ProposeCompletion).unwrap(),
            "propose_completion",
        ),
        (
            serde_json::to_value(WorkspaceMode::GitWorktree).unwrap(),
            "git-worktree",
        ),
        (
            serde_json::to_value(ValidationKind::AiReview).unwrap(),
            "ai-review",
        ),
        (
            serde_json::to_value(SessionMode::NativeFork).unwrap(),
            "native-fork",
        ),
        (
            serde_json::to_value(EvidenceLevel::EnvironmentTested).unwrap(),
            "environment-tested",
        ),
        (
            serde_json::to_value(ProviderEventKind::ApprovalRequired).unwrap(),
            "approval_required",
        ),
        (
            serde_json::to_value(TaskStatus::NeedsReconciliation).unwrap(),
            "NeedsReconciliation",
        ),
    ];
    for (actual, expected) in cases {
        assert_eq!(actual, json!(expected));
    }
}

#[test]
fn input_binding_uses_source_tag_and_camel_case_fields() {
    let binding = InputBinding::TaskOutput {
        name: "draft".into(),
        producer_task_id: "task-1".into(),
        producer_task_revision: 2,
        output_name: "draft".into(),
    };
    assert_eq!(
        serde_json::to_value(&binding).unwrap(),
        json!({
            "source": "task-output",
            "name": "draft",
            "producerTaskId": "task-1",
            "producerTaskRevision": 2,
            "outputName": "draft",
        })
    );

    let artifact: InputBinding = serde_json::from_value(json!({
        "source": "artifact",
        "name": "spec",
        "ref": { "artifactId": "a1", "versionId": "v1" },
    }))
    .unwrap();
    assert!(matches!(artifact, InputBinding::Artifact { .. }));
}

#[test]
fn optional_fields_are_omitted() {
    let session = SessionRequest {
        mode: SessionMode::Fresh,
        source_session_id: None,
        source_turn_id: None,
        capsule_version_id: None,
    };
    assert_eq!(
        serde_json::to_value(&session).unwrap(),
        json!({ "mode": "fresh" })
    );
}

#[test]
fn control_limits_default_to_spec_poc_values() {
    assert_eq!(
        serde_json::to_value(ControlLimits::default()).unwrap(),
        json!({
            "providerConcurrency": 1,
            "maxDelegationDepth": 3,
            "maxPlanTasks": 32,
            "executionRetryLimit": 1,
            "reworkLimit": 2,
        })
    );
}

#[test]
fn only_passed_satisfies_required_validation() {
    use ValidationVerdict::*;
    let passing: Vec<_> = [Passed, Failed, Error, Inconclusive, Skipped]
        .into_iter()
        .filter(|v| v.passes_required_check())
        .collect();
    assert_eq!(passing, [Passed]);
}

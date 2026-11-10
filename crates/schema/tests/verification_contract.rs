//! H0 산출물·검증: 승인 대상 고정, 명령 검증 형식, Artifact 참조·크기 (Spec §5.3, §11, §19.6.2).

use serde_json::json;
use telos_schema::*;

fn version(id: &str) -> ArtifactVersionRef {
    ArtifactVersionRef {
        artifact_id: "artifact-1".into(),
        version_id: id.into(),
    }
}

fn merge() -> ApprovalAction {
    ApprovalAction {
        action: "git-merge".into(),
        target: "master".into(),
        args_digest: "sha256:merge-1".into(),
    }
}

fn granted() -> ApprovalDecision {
    ApprovalDecision {
        approval_id: "approval-1".into(),
        kind: ApprovalKind::Action,
        plan_id: "plan-1".into(),
        task_id: Some("task-1".into()),
        run_id: None,
        contract_revision: Some(1),
        targets: vec![version("v1"), version("v2")],
        action: Some(merge()),
        decided_by: Some(ApprovalActor::Master {
            master_run_id: "run-m".into(),
        }),
        answer: Some(ApprovalAnswer::Allow),
        user_only: false,
        policy_revision: "policy-1".into(),
        expires_at: None,
        state: ApprovalState::Granted,
        state_version: 1,
        created_at: "2026-11-10T00:00:00Z".into(),
        decided_at: Some("2026-11-10T00:00:01Z".into()),
    }
}

#[test]
fn approval_applies_only_to_the_same_target_action_and_policy() {
    let approval = granted();
    let targets = [version("v2"), version("v1")];
    assert!(
        approval.permits(&targets, Some(&merge()), "policy-1"),
        "order does not matter"
    );
    // 바뀐 내용·행위·정책에는 과거 승인을 적용하지 않습니다 (§11.2·§11.3).
    assert!(!approval.permits(&[version("v1"), version("v3")], Some(&merge()), "policy-1"));
    assert!(!approval.permits(&[version("v1")], Some(&merge()), "policy-1"));
    let mut other = merge();
    other.args_digest = "sha256:merge-2".into();
    assert!(!approval.permits(&targets, Some(&other), "policy-1"));
    assert!(!approval.permits(&targets, None, "policy-1"));
    assert!(!approval.permits(&targets, Some(&merge()), "policy-2"));
}

#[test]
fn only_live_allow_decisions_permit() {
    for state in [
        ApprovalState::Pending,
        ApprovalState::Denied,
        ApprovalState::Expired,
        ApprovalState::Revoked,
        ApprovalState::Consumed,
    ] {
        let mut approval = granted();
        approval.state = state;
        assert!(
            !approval.permits(&[version("v1"), version("v2")], Some(&merge()), "policy-1"),
            "{state:?}"
        );
    }
    let mut denied = granted();
    denied.answer = Some(ApprovalAnswer::Deny);
    assert!(!denied.permits(&[version("v1"), version("v2")], Some(&merge()), "policy-1"));
}

#[test]
fn user_only_approval_is_not_satisfied_by_master() {
    let mut approval = granted();
    approval.user_only = true;
    let targets = [version("v1"), version("v2")];
    assert!(!approval.permits(&targets, Some(&merge()), "policy-1"));
    approval.decided_by = Some(ApprovalActor::User);
    assert!(approval.permits(&targets, Some(&merge()), "policy-1"));
}

#[test]
fn command_validation_references_a_registered_name_only() {
    let params: CommandValidationParameters =
        serde_json::from_value(json!({"command": "cargo-test", "args": ["--workspace"]})).unwrap();
    assert_eq!(params.command, "cargo-test");
    // 임의 명령 문자열 필드를 받지 않습니다 (H0 D8).
    assert!(
        serde_json::from_value::<CommandValidationParameters>(
            json!({"command": "x", "shell": "rm -rf /"})
        )
        .is_err()
    );
}

#[test]
fn artifact_reference_and_version_wire_format() {
    let reference = ArtifactReference {
        target: version("v1"),
        holder: ArtifactHolder::TaskResult {
            task_id: "task-1".into(),
            task_revision: 2,
            run_id: "run-3".into(),
        },
        created_at: "2026-11-10T00:00:00Z".into(),
    };
    let value = serde_json::to_value(&reference).unwrap();
    assert_eq!(value["holder"]["holder"], "task-result");
    assert_eq!(value["holder"]["taskRevision"], 2);
    assert_eq!(DEFAULT_ARTIFACT_MAX_BYTES, 64 * 1024 * 1024);
    let schema = telos_schema::json_schemas()
        .into_iter()
        .find(|(n, _)| *n == "ArtifactVersion")
        .unwrap()
        .1;
    let required = serde_json::to_value(schema).unwrap()["required"].clone();
    assert!(required.as_array().unwrap().contains(&json!("sizeBytes")));
}

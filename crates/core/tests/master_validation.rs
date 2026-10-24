//! 마스터 결정 검증 시험 (PH0-MASTER, Spec §4.2, G0 공통 실패 Fixture).

use telos_core::policy::{Caller, LookupDenial, TrustedLookup, validate};
use telos_schema::{
    CreateTasksPayload, DecisionPayload, DecisionRejection, MasterDecision, RejectionCode,
    RequestCancelPayload, RequestRunPayload, TaskSpec, TaskStatus, TaskSummary, fixtures,
};

struct Lookup(Option<LookupDenial>);

impl TrustedLookup for Lookup {
    fn check_task(&self, _: &TaskSpec) -> Result<(), LookupDenial> {
        self.0.clone().map_or(Ok(()), Err)
    }
}

const ALLOW: Lookup = Lookup(None);
const CALLER: Caller<'static> = Caller {
    active_master_run_id: "master-run-1",
};

fn check(decision: &MasterDecision) -> Result<DecisionPayload, DecisionRejection> {
    validate(decision, &fixtures::decision_context(), CALLER, &ALLOW)
}

fn code(result: Result<DecisionPayload, DecisionRejection>) -> RejectionCode {
    result.expect_err("decision must be rejected").code
}

fn create(tasks: Vec<TaskSpec>) -> MasterDecision {
    fixtures::master_decision(DecisionPayload::CreateTasks(CreateTasksPayload { tasks }))
}

fn task(id: &str) -> TaskSpec {
    let mut task = fixtures::task_spec();
    task.id = id.into();
    task
}

#[test]
fn valid_create_tasks_is_accepted() {
    assert!(matches!(
        check(&fixtures::valid_create_tasks_decision()),
        Ok(DecisionPayload::CreateTasks(_))
    ));
}

#[test]
fn common_invalid_payload_fixtures_are_rejected() {
    for (description, decision) in fixtures::invalid_payload_decisions() {
        let rejection = check(&decision).expect_err(description);
        assert_eq!(
            rejection.code,
            RejectionCode::InvalidPayload,
            "{description}"
        );
    }
}

#[test]
fn caller_plan_epoch_and_revisions_are_checked_first() {
    let base = fixtures::valid_create_tasks_decision();
    let mut d = base.clone();
    d.master_run_id = "old-master-run".into();
    assert_eq!(code(check(&d)), RejectionCode::UnauthorizedCaller);
    let mut d = base.clone();
    d.plan_id = "another-plan".into();
    assert_eq!(code(check(&d)), RejectionCode::UnauthorizedCaller);
    let mut d = base.clone();
    d.controller_epoch = 0;
    assert_eq!(code(check(&d)), RejectionCode::StaleControllerEpoch);
    let mut d = base.clone();
    d.plan_revision = 7;
    assert_eq!(code(check(&d)), RejectionCode::StalePlanRevision);
    let mut d = base;
    d.policy_revision = "policy-0".into();
    assert_eq!(code(check(&d)), RejectionCode::StalePolicyRevision);
}

#[test]
fn kinds_not_executed_in_phase0_are_rejected_after_payload_check() {
    use telos_schema::*;
    let cases = [
        DecisionPayload::AnswerAction(AnswerActionPayload {
            request_id: "r".into(),
            answer: ApprovalAnswer::Allow,
            note: None,
        }),
        DecisionPayload::RequestValidation(RequestValidationPayload {
            task_id: "t".into(),
            task_revision: 1,
            run_id: "r".into(),
            validation_ids: vec![],
        }),
        DecisionPayload::RequestRework(RequestReworkPayload {
            task_id: "t".into(),
            task_revision: 1,
            failed_run_id: "r".into(),
            reason: "x".into(),
        }),
        DecisionPayload::AskUser(AskUserPayload {
            question: "q".into(),
            reason: "r".into(),
            options: vec![],
            related_task_ids: vec![],
        }),
        DecisionPayload::ProposeCompletion(ProposeCompletionPayload {
            task_id: "t".into(),
            task_revision: 1,
            run_id: "r".into(),
            artifacts: vec![],
            summary: "s".into(),
        }),
    ];
    for payload in cases {
        let kind = payload.kind();
        assert_eq!(
            code(check(&fixtures::master_decision(payload))),
            RejectionCode::UnsupportedKind,
            "{kind:?}"
        );
    }
    // Payload가 잘못되면 kind 지원 여부보다 먼저 InvalidPayload입니다.
    let mut bad =
        fixtures::master_decision(DecisionPayload::AskUser(telos_schema::AskUserPayload {
            question: "q".into(),
            reason: "r".into(),
            options: vec![],
            related_task_ids: vec![],
        }));
    bad.payload.insert("extra".into(), true.into());
    assert_eq!(code(check(&bad)), RejectionCode::InvalidPayload);
    assert_eq!(bad.kind, telos_schema::MasterDecisionKind::AskUser);
}

#[test]
fn create_tasks_permission_and_limit_violations_are_rejected() {
    let mut t = task("t1");
    t.agent.provider = "unknown".into();
    let r = check(&create(vec![t])).unwrap_err();
    assert_eq!(
        (r.code, r.field.as_deref()),
        (
            RejectionCode::PermissionDenied,
            Some("/tasks/0/agent/provider")
        )
    );

    let mut t = task("t1");
    t.permission_profile_id = "admin".into();
    assert_eq!(
        code(check(&create(vec![t]))),
        RejectionCode::PermissionDenied
    );

    let mut t = task("t1");
    t.workspace.host_id = "other-host".into();
    assert_eq!(
        code(check(&create(vec![t]))),
        RejectionCode::PermissionDenied
    );

    let mut t = task("t1");
    t.agent.role = "master".into();
    assert_eq!(
        code(check(&create(vec![t]))),
        RejectionCode::PermissionDenied
    );

    let mut t = task("t1");
    t.plan_id = "other-plan".into();
    assert_eq!(
        code(check(&create(vec![t]))),
        RejectionCode::PermissionDenied
    );

    let many: Vec<_> = (0..33).map(|i| task(&format!("t{i}"))).collect();
    assert_eq!(code(check(&create(many))), RejectionCode::LimitExceeded);

    assert_eq!(
        code(check(&create(vec![task("same"), task("same")]))),
        RejectionCode::InvalidPayload
    );
}

#[test]
fn create_tasks_tree_and_dependency_checks() {
    let mut child = task("child");
    child.parent_task_id = Some("missing".into());
    assert_eq!(
        code(check(&create(vec![child]))),
        RejectionCode::UnknownTask
    );

    let mut dependent = task("b");
    dependent.dependencies = vec!["nowhere".into()];
    assert_eq!(
        code(check(&create(vec![dependent]))),
        RejectionCode::UnknownTask
    );

    let mut a = task("a");
    a.dependencies = vec!["b".into()];
    let mut b = task("b");
    b.dependencies = vec!["a".into()];
    assert_eq!(
        code(check(&create(vec![a, b]))),
        RejectionCode::DependencyCycle
    );

    // 기본 위임 깊이 상한은 3입니다: 0 → 1 → 2 → 3은 허용, 4는 거부.
    let mut chain = vec![task("d0")];
    for i in 1..=4 {
        let mut t = task(&format!("d{i}"));
        t.parent_task_id = Some(format!("d{}", i - 1));
        chain.push(t);
    }
    assert_eq!(
        code(check(&create(chain.clone()))),
        RejectionCode::LimitExceeded
    );
    chain.pop();
    assert!(check(&create(chain)).is_ok());
}

#[test]
fn trusted_lookup_results_map_to_rejection_codes() {
    let decision = fixtures::valid_create_tasks_decision();
    let context = fixtures::decision_context();
    for (denial, expected) in [
        (
            LookupDenial::Denied("workspace".into()),
            RejectionCode::PermissionDenied,
        ),
        (
            LookupDenial::Unavailable("down".into()),
            RejectionCode::PolicyUnavailable,
        ),
        (
            LookupDenial::Unsupported("path".into()),
            RejectionCode::CheckUnsupported,
        ),
    ] {
        let result = validate(&decision, &context, CALLER, &Lookup(Some(denial)));
        assert_eq!(result.unwrap_err().code, expected);
    }
}

#[test]
fn request_run_and_cancel_require_known_ready_tasks() {
    let run = |task_id: &str, revision: u32| {
        fixtures::master_decision(DecisionPayload::RequestRun(RequestRunPayload {
            task_id: task_id.into(),
            task_revision: revision,
            session: fixtures::run_context(&fixtures::task_spec()).session,
        }))
    };
    let mut context = fixtures::decision_context();
    context.tasks = vec![TaskSummary {
        id: "task-1".into(),
        revision: 1,
        status: TaskStatus::Ready,
        parent_task_id: None,
        delegation_depth: 0,
    }];
    assert!(validate(&run("task-1", 1), &context, CALLER, &ALLOW).is_ok());
    assert_eq!(
        validate(&run("task-9", 1), &context, CALLER, &ALLOW)
            .unwrap_err()
            .code,
        RejectionCode::UnknownTask
    );
    assert_eq!(
        validate(&run("task-1", 2), &context, CALLER, &ALLOW)
            .unwrap_err()
            .code,
        RejectionCode::UnknownTask
    );
    context.tasks[0].status = TaskStatus::Running;
    assert_eq!(
        validate(&run("task-1", 1), &context, CALLER, &ALLOW)
            .unwrap_err()
            .code,
        RejectionCode::InvalidPayload
    );

    let cancel = |task_id: &str| {
        fixtures::master_decision(DecisionPayload::RequestCancel(RequestCancelPayload {
            task_id: task_id.into(),
            reason: "stop".into(),
            propagate_to_children: true,
        }))
    };
    assert!(validate(&cancel("task-1"), &context, CALLER, &ALLOW).is_ok());
    assert_eq!(
        validate(&cancel("task-9"), &context, CALLER, &ALLOW)
            .unwrap_err()
            .code,
        RejectionCode::UnknownTask
    );
}

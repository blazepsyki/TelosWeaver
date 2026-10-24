//! Master Controller 시험 (PH0-MASTER): 마스터 Turn → 결정 회수 → 검증 → 집행 → 결과 왕복.
//!
//! 가짜 Run 실행기가 마스터 응답을 연출합니다. 실제 Claude 마스터 시험은 `phase0_master.rs`에 있습니다.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use futures::{StreamExt, stream::BoxStream};
use serde_json::json;
use telos_core::{
    master::{MasterController, MasterSettings, RunExecutor, parse_master_output},
    policy::{LookupDenial, TrustedLookup},
};
use telos_schema::{
    ControlLimits, DecisionEffect, DecisionOutcome, MasterDecision, MasterDecisionKind,
    ProcessExitedPayload, ProcessTermination, ProviderEvent, ProviderEventPayload, RejectionCode,
    RunStatus, SessionMode, SessionStartedPayload, TaskSpec, TaskStatus, TextChannel,
    TextDeltaPayload, WorkspaceMode,
    api::{ApiError, RunsInspectResult, RunsStartParams, RunsStartResult},
    fixtures,
};

#[derive(Default)]
struct FakeExecutor {
    master_replies: Mutex<VecDeque<String>>,
    starts: Mutex<Vec<RunsStartParams>>,
    cancels: Mutex<Vec<String>>,
}

impl FakeExecutor {
    fn reply(&self, text: &str) {
        self.master_replies.lock().unwrap().push_back(text.into());
    }
    fn run_id(index: usize) -> String {
        format!("run-{index}")
    }
}

#[async_trait]
impl RunExecutor for FakeExecutor {
    async fn start(&self, params: RunsStartParams) -> Result<RunsStartResult, ApiError> {
        let mut starts = self.starts.lock().unwrap();
        starts.push(params.clone());
        Ok(RunsStartResult {
            run_id: Self::run_id(starts.len()),
            task_id: params.task.id,
            task_revision: params.task.revision,
            attempt: 1,
        })
    }
    async fn cancel(&self, run_id: &str) -> Result<bool, ApiError> {
        self.cancels.lock().unwrap().push(run_id.into());
        Ok(true)
    }
    fn inspect(&self, run_id: &str) -> Result<RunsInspectResult, ApiError> {
        Ok(RunsInspectResult {
            run_id: run_id.into(),
            status: RunStatus::Succeeded,
            last_sequence: 3,
        })
    }
    fn events(
        &self,
        run_id: &str,
    ) -> Result<BoxStream<'static, Result<ProviderEvent, ApiError>>, ApiError> {
        let text = self
            .master_replies
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_default();
        let payloads = [
            ProviderEventPayload::SessionStarted(SessionStartedPayload {
                provider_version: None,
                model: None,
            }),
            ProviderEventPayload::TextDelta(TextDeltaPayload {
                channel: TextChannel::Assistant,
                text,
            }),
            ProviderEventPayload::ProcessExited(ProcessExitedPayload {
                exit_code: Some(0),
                termination: ProcessTermination::Exited,
                children_reaped: Some(true),
            }),
        ];
        let events: Vec<_> = payloads
            .into_iter()
            .enumerate()
            .map(|(i, p)| {
                let mut e = fixtures::provider_event(run_id, i as u64 + 1, p);
                e.native_session_id = Some("master-session".into());
                Ok(e)
            })
            .collect();
        Ok(futures::stream::iter(events).boxed())
    }
}

struct Allow;
impl TrustedLookup for Allow {
    fn check_task(&self, _: &TaskSpec) -> Result<(), LookupDenial> {
        Ok(())
    }
}

fn worker_template() -> TaskSpec {
    let mut t = fixtures::task_spec();
    t.id = "template".into();
    t.agent.provider = "claude".into();
    t.workspace.mode = WorkspaceMode::Directory;
    t.workspace.workspace_id = Some("ws-work".into());
    t.workspace.read_only = false;
    t
}

fn controller() -> (MasterController, Arc<FakeExecutor>) {
    let executor = Arc::new(FakeExecutor::default());
    let context = fixtures::decision_context();
    let settings = MasterSettings {
        project_id: context.project_id,
        plan_id: context.plan_id,
        host_id: fixtures::HOST_ID.into(),
        policy_revision: context.policy_revision,
        limits: ControlLimits::default(),
        allowed_providers: vec!["claude".into()],
        allowed_workspace_modes: vec![WorkspaceMode::Directory],
        allowed_permission_profile_ids: vec!["read-only".into()],
        allowed_budget_profile_ids: vec!["default".into()],
        master_provider: "claude".into(),
        master_model: Some("haiku".into()),
        master_workspace_id: "ws-master".into(),
        master_permission_profile_id: "read-only".into(),
        worker_template: worker_template(),
        turn_timeout: Duration::from_secs(5),
    };
    (
        MasterController::new(settings, executor.clone(), Arc::new(Allow)),
        executor,
    )
}

fn create_tasks_reply(id: &str) -> String {
    let mut task = worker_template();
    task.id = id.into();
    task.objective = "Create hello.txt".into();
    format!(
        "I will delegate.\n```json\n{}\n```",
        json!({"kind": "create_tasks", "payload": {"tasks": [task]}, "rationale": "delegate the file"})
    )
}

fn effects(outcome: &DecisionOutcome) -> &[DecisionEffect] {
    match outcome {
        DecisionOutcome::Accepted { effects, .. } => effects,
        DecisionOutcome::Rejected { rejection, .. } => panic!("rejected: {rejection:?}"),
    }
}

fn rejection_code(outcome: &DecisionOutcome) -> RejectionCode {
    match outcome {
        DecisionOutcome::Rejected { rejection, .. } => rejection.code,
        DecisionOutcome::Accepted { .. } => panic!("unexpectedly accepted"),
    }
}

#[tokio::test]
async fn master_turns_create_run_and_receive_results() {
    let (master, executor) = controller();

    // Turn 1: 새 세션, 도구 없는 읽기 전용 제어 Task. create_tasks를 집행합니다.
    executor.reply(&create_tasks_reply("hello"));
    let turn = master
        .turn("Goal: have a worker create hello.txt")
        .await
        .unwrap();
    assert_eq!(
        effects(&turn.outcome),
        [DecisionEffect::TaskCreated {
            task_id: "hello".into(),
            task_revision: 1
        }]
    );
    {
        let starts = executor.starts.lock().unwrap();
        let master_task = &starts[0];
        assert_eq!(master_task.task.agent.role, "master");
        assert_eq!(master_task.task.agent.provider, "claude");
        assert!(master_task.task.workspace.read_only);
        assert_eq!(
            master_task.task.workspace.workspace_id.as_deref(),
            Some("ws-master")
        );
        assert_eq!(master_task.session.mode, SessionMode::Fresh);
        assert!(
            master_task
                .task
                .objective
                .contains("Goal: have a worker create hello.txt")
        );
        assert!(
            master_task.task.objective.contains("\"ws-work\""),
            "template is in the prompt"
        );
    }
    assert_eq!(master.context().plan_revision, 2);
    assert_eq!(master.tasks()[0].status, TaskStatus::Ready);

    // Turn 2: Resume으로 집행 결과를 받고 request_run을 냅니다.
    executor.reply(
        &json!({"kind": "request_run", "payload": {"taskId": "hello", "taskRevision": 1, "session": {"mode": "fresh"}}, "rationale": "run it"}).to_string(),
    );
    let turn = master
        .turn("Task hello was created. Request its run.")
        .await
        .unwrap();
    let [DecisionEffect::RunRequested { task_id, run_id }] = effects(&turn.outcome) else {
        panic!("expected one RunRequested effect");
    };
    assert_eq!(task_id, "hello");
    {
        let starts = executor.starts.lock().unwrap();
        assert_eq!(starts[1].session.mode, SessionMode::Resume);
        assert_eq!(
            starts[1].session.source_session_id.as_deref(),
            Some("master-session")
        );
        assert_eq!(
            starts[2].task.id, "hello",
            "the worker run uses the created task"
        );
        assert_eq!(&FakeExecutor::run_id(3), run_id);
    }

    // Turn 3: 결과 왕복. Run 성공은 Task 완료가 아니라 Executed이며 마스터에게 전달됩니다.
    executor.reply(
        &json!({"kind": "propose_completion", "payload": {"taskId": "hello", "taskRevision": 1, "runId": run_id, "artifacts": [], "summary": "done"}, "rationale": "the run succeeded"}).to_string(),
    );
    let turn = master.turn("The worker run finished.").await.unwrap();
    assert_eq!(
        rejection_code(&turn.outcome),
        RejectionCode::UnsupportedKind
    );
    assert_eq!(master.tasks()[0].status, TaskStatus::Executed);
    let prompt = executor.starts.lock().unwrap()[3].task.objective.clone();
    assert!(
        prompt.contains("\"Succeeded\""),
        "run result reaches the master: {prompt}"
    );
    assert_eq!(master.decision_log().len(), 3);
}

#[tokio::test]
async fn malformed_master_output_is_an_invalid_payload_rejection() {
    let (master, executor) = controller();
    executor.reply("I think we should create a task, but here is no JSON.");
    let turn = master.turn("Goal").await.unwrap();
    assert_eq!(rejection_code(&turn.outcome), RejectionCode::InvalidPayload);

    executor.reply(
        r#"{"kind": "create_tasks", "payload": {"tasks": []}, "rationale": "x", "allowAll": true}"#,
    );
    let turn = master.turn("Goal").await.unwrap();
    assert_eq!(rejection_code(&turn.outcome), RejectionCode::InvalidPayload);
    assert!(master.tasks().is_empty());
}

fn decision(master: &MasterController, key: &str, task_id: &str) -> MasterDecision {
    let context = master.context();
    let mut task = worker_template();
    task.id = task_id.into();
    MasterDecision {
        decision_id: format!("d-{key}"),
        plan_id: context.plan_id,
        plan_revision: context.plan_revision,
        master_run_id: master.active_master_run().unwrap_or_default(),
        controller_epoch: context.controller_epoch,
        idempotency_key: key.into(),
        kind: MasterDecisionKind::CreateTasks,
        payload: json!({"tasks": [task]}).as_object().unwrap().clone(),
        rationale: "test".into(),
        policy_revision: context.policy_revision,
    }
}

#[tokio::test]
async fn idempotency_replays_and_conflicts() {
    let (master, executor) = controller();
    executor.reply("no decision");
    master.turn("open a master run").await.unwrap();

    let first = decision(&master, "k1", "a");
    let outcome = master.decide(first.clone()).await;
    assert!(!outcome.replayed() && !effects(&outcome).is_empty());
    let again = master.decide(first).await;
    assert!(again.replayed());
    assert_eq!(again.effects(), outcome.effects());
    assert_eq!(master.tasks().len(), 1, "a replay does not execute again");

    let conflict = decision(&master, "k1", "different");
    assert_eq!(
        rejection_code(&master.decide(conflict).await),
        RejectionCode::IdempotencyConflict
    );
}

#[tokio::test]
async fn late_decisions_from_old_master_runs_or_epochs_are_rejected() {
    let (master, executor) = controller();
    executor.reply("no decision");
    master.turn("first master run").await.unwrap();
    let late = decision(&master, "late", "a");

    executor.reply("no decision");
    master.turn("second master run").await.unwrap();
    assert_eq!(
        rejection_code(&master.decide(late).await),
        RejectionCode::UnauthorizedCaller
    );

    let current = decision(&master, "current", "b");
    master.bump_epoch();
    assert_eq!(
        rejection_code(&master.decide(current).await),
        RejectionCode::UnauthorizedCaller,
        "after a controller change no master run is active"
    );
    executor.reply("no decision");
    master.turn("new controller").await.unwrap();
    let mut stale = decision(&master, "stale", "c");
    stale.controller_epoch -= 1;
    assert_eq!(
        rejection_code(&master.decide(stale).await),
        RejectionCode::StaleControllerEpoch
    );
}

#[tokio::test]
async fn request_cancel_cancels_the_task_run() {
    let (master, executor) = controller();
    executor.reply(&create_tasks_reply("job"));
    master.turn("create").await.unwrap();
    executor.reply(&json!({"kind": "request_run", "payload": {"taskId": "job", "taskRevision": 1, "session": {"mode": "fresh"}}, "rationale": "run"}).to_string());
    master.turn("run").await.unwrap();
    executor.reply(&json!({"kind": "request_cancel", "payload": {"taskId": "job", "reason": "no longer needed", "propagateToChildren": true}, "rationale": "stop"}).to_string());
    let turn = master.turn("cancel").await.unwrap();
    assert_eq!(
        effects(&turn.outcome),
        [DecisionEffect::CancelRequested {
            task_id: "job".into()
        }]
    );
    assert_eq!(
        executor.cancels.lock().unwrap().as_slice(),
        [FakeExecutor::run_id(3)]
    );
}

#[test]
fn master_output_parsing_is_strict() {
    let fenced = "text\n```json\n{\"kind\":\"ask_user\",\"payload\":{},\"rationale\":\"r\"}\n```";
    assert_eq!(
        parse_master_output(fenced).unwrap().kind,
        MasterDecisionKind::AskUser
    );
    let bare = "Here: {\"kind\":\"request_cancel\",\"payload\":{\"note\":\"}\"},\"rationale\":\"r\"} thanks";
    assert_eq!(
        parse_master_output(bare).unwrap().kind,
        MasterDecisionKind::RequestCancel
    );
    assert!(parse_master_output("no json").is_err());
    assert!(
        parse_master_output("{\"kind\":\"ask_user\",\"payload\":{}}").is_err(),
        "rationale is required"
    );
    assert!(
        parse_master_output("{\"kind\":\"delete_all\",\"payload\":{},\"rationale\":\"r\"}")
            .is_err()
    );
}

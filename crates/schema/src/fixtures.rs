//! 여러 crate의 시험이 함께 쓰는 공통 Fixture (`fixtures` feature, G0 생성·시험 계약).
//!
//! 값은 계약 형식을 보여 주는 예시입니다. 실제 Provider·환경의 동작을 뜻하지 않습니다.

use serde_json::json;

use crate::*;

pub const PROJECT_ID: &str = "project-fixture";
pub const PLAN_ID: &str = "plan-fixture";
pub const HOST_ID: &str = "host-local";

/// Workspace 없이 실행하는 최소 `general` Task입니다.
pub fn task_spec() -> TaskSpec {
    TaskSpec {
        id: "task-1".into(),
        revision: 1,
        project_id: PROJECT_ID.into(),
        plan_id: PLAN_ID.into(),
        parent_task_id: None,
        r#type: TaskType::General,
        objective: "Say hello".into(),
        constraints: vec![],
        dependencies: vec![],
        agent: AgentSelection {
            provider: "codex".into(),
            model: None,
            role: "worker".into(),
        },
        workspace: WorkspaceRequest {
            mode: WorkspaceMode::None,
            workspace_id: None,
            host_id: HOST_ID.into(),
            read_only: true,
        },
        inputs: vec![],
        outputs: vec![OutputContract {
            id: "out-1".into(),
            name: "answer".into(),
            kind: ArtifactKind::Text,
            content_type: Some("text/plain".into()),
            relative_path: None,
            schema_id: None,
            required: true,
            min_count: 1,
            max_count: 1,
        }],
        validation: vec![],
        permission_profile_id: "read-only".into(),
        budget_profile_id: "default".into(),
        completion_policy_id: "default".into(),
    }
}

pub fn run_context(task: &TaskSpec) -> RunContext {
    RunContext {
        run_id: "run-1".into(),
        task_id: task.id.clone(),
        task_revision: task.revision,
        attempt: 1,
        controller_epoch: 1,
        host_id: task.workspace.host_id.clone(),
        workspace_id: "ws-1".into(),
        context_snapshot_id: "ctx-1".into(),
        policy_revision: "policy-1".into(),
        idempotency_key: "run-1:start".into(),
        session: SessionRequest {
            mode: SessionMode::Fresh,
            source_session_id: None,
            source_turn_id: None,
            capsule_version_id: None,
        },
    }
}

pub fn provider_event(run_id: &str, sequence: u64, payload: ProviderEventPayload) -> ProviderEvent {
    ProviderEvent {
        event_id: format!("{run_id}-evt-{sequence}"),
        run_id: run_id.into(),
        sequence,
        occurred_at: "2026-10-24T00:00:00Z".into(),
        schema_version: PROVIDER_EVENT_SCHEMA_VERSION.into(),
        native_session_id: Some("native-session-1".into()),
        native_turn_id: None,
        tool_call_id: None,
        kind: payload.kind(),
        payload: payload.to_object(),
        raw_event_ref: None,
    }
}

fn run_events(run_id: &str, payloads: Vec<ProviderEventPayload>) -> Vec<ProviderEvent> {
    payloads
        .into_iter()
        .zip(1..)
        .map(|(payload, sequence)| provider_event(run_id, sequence, payload))
        .collect()
}

fn session_started() -> ProviderEventPayload {
    ProviderEventPayload::SessionStarted(SessionStartedPayload {
        provider_version: None,
        model: None,
    })
}

fn text(text: &str) -> ProviderEventPayload {
    ProviderEventPayload::TextDelta(TextDeltaPayload {
        channel: TextChannel::Assistant,
        text: text.into(),
    })
}

fn turn_finished(status: TurnStatus) -> ProviderEventPayload {
    ProviderEventPayload::TurnFinished(TurnFinishedPayload {
        status,
        reason: None,
    })
}

fn process_exited(exit_code: Option<i32>, termination: ProcessTermination) -> ProviderEventPayload {
    ProviderEventPayload::ProcessExited(ProcessExitedPayload {
        exit_code,
        termination,
        children_reaped: Some(true),
    })
}

fn error(kind: FailureKind, terminal: bool) -> ProviderEventPayload {
    ProviderEventPayload::Error(ErrorPayload {
        kind,
        message: "fixture".into(),
        terminal,
    })
}

/// 전용 프로세스가 정상 종료한 Run입니다. 종료: `process_exited`.
pub fn completed_run_events(run_id: &str) -> Vec<ProviderEvent> {
    run_events(
        run_id,
        vec![
            session_started(),
            text("hello"),
            turn_finished(TurnStatus::Completed),
            process_exited(Some(0), ProcessTermination::Exited),
        ],
    )
}

/// `start_run` 반환 뒤 프로세스 시작이 실패한 Run입니다. 종료: 종료 `error`. `process_exited`가 없습니다.
pub fn start_failed_run_events(run_id: &str) -> Vec<ProviderEvent> {
    run_events(run_id, vec![error(FailureKind::StartFailed, true)])
}

/// 취소한 Run입니다. 취소 후 늦게 도착한 사용량도 보존합니다. 종료: `process_exited`.
pub fn cancelled_run_events(run_id: &str) -> Vec<ProviderEvent> {
    run_events(
        run_id,
        vec![
            session_started(),
            text("partial"),
            turn_finished(TurnStatus::Cancelled),
            ProviderEventPayload::Usage(UsagePayload {
                scope: UsageScope::Cumulative,
                source: UsageSource::ProviderReported,
                input_tokens: Some(1),
                output_tokens: None,
                cache_read_tokens: None,
                cache_write_tokens: None,
                cache_included_in_input: None,
                cost_usd: None,
            }),
            process_exited(None, ProcessTermination::TerminatedOnRequest),
        ],
    )
}

/// 프로세스가 Run보다 오래 사는 Provider의 Run입니다. 종료: `turn_finished`.
pub fn persistent_process_run_events(run_id: &str) -> Vec<ProviderEvent> {
    run_events(
        run_id,
        vec![
            session_started(),
            text("hello"),
            turn_finished(TurnStatus::Completed),
        ],
    )
}

/// 진행 중 추적을 잃은 Run입니다. 종료: 종료 `error`. 확인하지 못한 `process_exited`를 만들지 않습니다.
pub fn tracking_lost_run_events(run_id: &str) -> Vec<ProviderEvent> {
    run_events(
        run_id,
        vec![
            session_started(),
            text("partial"),
            error(FailureKind::Unknown, true),
        ],
    )
}

/// `check_run_events`가 거부해야 하는 이벤트 순서 (공통 실패 Fixture). 각 항목은 `(설명, 이벤트)`입니다.
pub fn invalid_run_event_sequences(run_id: &str) -> Vec<(&'static str, Vec<ProviderEvent>)> {
    let mut cases = Vec::new();

    let mut gap = completed_run_events(run_id);
    gap.remove(1);
    cases.push(("순번 누락", gap));

    let mut after_exit = completed_run_events(run_id);
    let extra = provider_event(run_id, 5, text("late"));
    after_exit.push(extra);
    cases.push(("process_exited 뒤 이벤트", after_exit));

    let mut no_end = completed_run_events(run_id);
    no_end.truncate(2);
    cases.push(("종료 이벤트 없음", no_end));

    let text_after_turn = run_events(
        run_id,
        vec![
            session_started(),
            turn_finished(TurnStatus::Completed),
            text("after turn"),
            process_exited(Some(0), ProcessTermination::Exited),
        ],
    );
    cases.push(("turn_finished 뒤 text_delta", text_after_turn));

    let mut mismatch = completed_run_events(run_id);
    mismatch[1].kind = ProviderEventKind::ProcessExited;
    cases.push(("kind와 payload 불일치", mismatch));

    let mut mixed = completed_run_events(run_id);
    mixed[2].run_id = "other-run".into();
    cases.push(("다른 Run의 이벤트", mixed));

    cases
}

pub fn decision_context() -> DecisionContext {
    DecisionContext {
        project_id: PROJECT_ID.into(),
        allowed_host_ids: vec![HOST_ID.into()],
        plan_id: PLAN_ID.into(),
        plan_revision: 1,
        controller_epoch: 1,
        policy_revision: "policy-1".into(),
        limits: ControlLimits::default(),
        tasks: vec![],
        allowed_providers: vec!["codex".into(), "claude".into(), "agy".into()],
        allowed_workspace_modes: vec![WorkspaceMode::None, WorkspaceMode::Directory],
        allowed_permission_profile_ids: vec!["read-only".into()],
        allowed_budget_profile_ids: vec!["default".into()],
    }
}

pub fn master_decision(payload: DecisionPayload) -> MasterDecision {
    MasterDecision {
        decision_id: "decision-1".into(),
        plan_id: PLAN_ID.into(),
        plan_revision: 1,
        master_run_id: "master-run-1".into(),
        controller_epoch: 1,
        idempotency_key: "decision-1".into(),
        kind: payload.kind(),
        payload: payload.to_object(),
        rationale: "fixture".into(),
        policy_revision: "policy-1".into(),
    }
}

pub fn valid_create_tasks_decision() -> MasterDecision {
    master_decision(DecisionPayload::CreateTasks(CreateTasksPayload {
        tasks: vec![task_spec()],
    }))
}

/// 반드시 `InvalidPayload`로 거부되어야 하는 결정 (공통 실패 Fixture).
/// 각 항목은 `(설명, 결정)`입니다.
pub fn invalid_payload_decisions() -> Vec<(&'static str, MasterDecision)> {
    let mut cases = Vec::new();

    let mut unknown_field = valid_create_tasks_decision();
    unknown_field.payload.insert("injected".into(), json!(true));
    cases.push(("payload에 정의되지 않은 필드", unknown_field));

    let mut missing_field = valid_create_tasks_decision();
    missing_field.payload.remove("tasks");
    cases.push(("필수 필드 누락", missing_field));

    let mut kind_mismatch = valid_create_tasks_decision();
    kind_mismatch.kind = MasterDecisionKind::RequestRun;
    cases.push(("kind와 payload 불일치", kind_mismatch));

    let mut wrong_type = master_decision(DecisionPayload::RequestCancel(RequestCancelPayload {
        task_id: "task-1".into(),
        reason: "stop".into(),
        propagate_to_children: true,
    }));
    wrong_type
        .payload
        .insert("propagateToChildren".into(), json!("yes"));
    cases.push(("필드 타입 불일치", wrong_type));

    // 중첩 구조체의 미정의 필드 (REQ-009)
    for (description, pointer) in [
        ("TaskSpec의 미정의 필드", "/tasks/0"),
        ("AgentSelection의 미정의 필드", "/tasks/0/agent"),
        ("WorkspaceRequest의 미정의 필드", "/tasks/0/workspace"),
        ("OutputContract의 미정의 필드", "/tasks/0/outputs/0"),
    ] {
        let mut decision = valid_create_tasks_decision();
        let mut payload = serde_json::Value::Object(decision.payload);
        payload
            .pointer_mut(pointer)
            .and_then(serde_json::Value::as_object_mut)
            .expect("fixture pointer exists")
            .insert("allowAll".into(), json!(true));
        let serde_json::Value::Object(map) = payload else {
            unreachable!()
        };
        decision.payload = map;
        cases.push((description, decision));
    }

    let mut session = master_decision(DecisionPayload::RequestRun(RequestRunPayload {
        task_id: "task-1".into(),
        task_revision: 1,
        session: run_context(&task_spec()).session,
    }));
    session.payload["session"]
        .as_object_mut()
        .expect("session is an object")
        .insert("inheritPermissions".into(), json!(true));
    cases.push(("SessionRequest의 미정의 필드", session));

    cases
}

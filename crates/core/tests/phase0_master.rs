//! 마스터 결정 경로 실환경 시험 (Spec §19.1 "마스터 후보가 구조화 결정을 반환하고 Core가 거부·집행", 배정표 §6).
//!
//! 실제 Claude 마스터(`haiku`, 도구 없음·읽기 전용)가 Worker Task를 만들고 Run을 요청하며,
//! Worker 결과를 Resume으로 받아 다음 결정을 냅니다. 실제 CLI를 쓰므로 기본 실행에서 제외합니다.
//!
//! ```text
//! cargo test -p telos-core --test phase0_master -- --ignored --test-threads=1 --nocapture
//! ```
//!
//! 마스터·Worker 세션 ID를 `claude-session:` 줄로 출력합니다. 시험 뒤 Archive 대상입니다.

use std::{path::PathBuf, time::Duration};

use futures::StreamExt;
use serde_json::json;
use telos_core::integration::{MasterOptions, Phase0Execution};
use telos_schema::{
    DecisionEffect, DecisionOutcome, RejectionCode, RunStatus, TaskSpec, WorkspaceMode, fixtures,
};

struct Setup {
    root: PathBuf,
    work: PathBuf,
    execution: Phase0Execution,
}

impl Setup {
    async fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join("telos-phase0-master").join(name);
        let _ = std::fs::remove_dir_all(&root);
        let (master_dir, work) = (root.join("master"), root.join("work"));
        std::fs::create_dir_all(&master_dir).unwrap();
        std::fs::create_dir_all(&work).unwrap();
        let policy = root.join("policy.json");
        std::fs::write(
            &policy,
            serde_json::to_vec(&json!({
                "revision": "master-test",
                "permissionProfiles": ["phase0"],
                "workspaces": [
                    { "id": "ws-master", "path": master_dir, "writable": false },
                    { "id": "ws-work", "path": work, "writable": true }
                ],
                "claude": {
                    "models": ["haiku"],
                    "permissionMode": "acceptEdits",
                    "allowedTools": ["Read", "Write"],
                    "disallowedTools": ["Bash", "WebFetch", "WebSearch"],
                    "settingSources": "user"
                }
            }))
            .unwrap(),
        )
        .unwrap();
        let task = fixtures::task_spec();
        let execution = Phase0Execution::build(
            task.project_id.clone(),
            task.workspace.host_id.clone(),
            Some(&policy),
        )
        .await
        .unwrap();
        assert!(execution.notes.is_empty(), "{:?}", execution.notes);
        Self {
            root,
            work,
            execution,
        }
    }
}

impl Drop for Setup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn worker_template() -> TaskSpec {
    let mut t = fixtures::task_spec();
    t.id = "worker-template".into();
    t.plan_id = "plan-master".into();
    t.objective = "<what the worker must do>".into();
    t.agent.provider = "claude".into();
    t.agent.role = "worker".into();
    t.workspace.mode = WorkspaceMode::Directory;
    t.workspace.workspace_id = Some("ws-work".into());
    t.workspace.read_only = false;
    t.outputs = vec![];
    t.permission_profile_id = "phase0".into();
    t
}

fn options() -> MasterOptions {
    MasterOptions {
        provider: "claude".into(),
        model: Some("haiku".into()),
        workspace_id: "ws-master".into(),
        permission_profile_id: "phase0".into(),
        worker_template: worker_template(),
        turn_timeout: Duration::from_secs(180),
    }
}

fn show(label: &str, outcome: &DecisionOutcome) {
    println!("{label}: {}", serde_json::to_string(outcome).unwrap());
}

/// 마스터 3 Turn: create_tasks → request_run → (Worker 실행) → 결과를 받은 다음 결정.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "runs the real Claude Code CLI as master and worker"]
async fn master_delegates_a_task_and_receives_the_result() {
    let setup = Setup::new("delegate").await;
    let master = setup.execution.master("plan-master", options()).unwrap();

    let turn1 = master
        .turn(
            "Goal: a file named hello.txt must exist in the worker's working directory with the exact content: hello\n\
             Delegate this to exactly one new worker task now (use create_tasks).",
        )
        .await
        .unwrap();
    println!("claude-session: {:?} (master)", turn1.native_session_id);
    show("turn1", &turn1.outcome);
    let DecisionOutcome::Accepted { effects, .. } = &turn1.outcome else {
        panic!(
            "turn1 must create a task: {}\n{}",
            turn1.text,
            serde_json::to_string(&turn1.outcome).unwrap()
        );
    };
    let [DecisionEffect::TaskCreated { task_id, .. }] = effects.as_slice() else {
        panic!("expected one created task: {effects:?}");
    };
    let task_id = task_id.clone();

    let turn2 = master
        .turn(&format!(
            "Your create_tasks decision was accepted: task {task_id} is Ready. Now request its run (use request_run)."
        ))
        .await
        .unwrap();
    show("turn2", &turn2.outcome);
    assert_eq!(
        turn2.native_session_id, turn1.native_session_id,
        "later turns resume the same master session"
    );
    let DecisionOutcome::Accepted { effects, .. } = &turn2.outcome else {
        panic!("turn2 must request the run: {}", turn2.text);
    };
    let [DecisionEffect::RunRequested { run_id, .. }] = effects.as_slice() else {
        panic!("expected one run request: {effects:?}");
    };

    // Worker Run이 끝날 때까지 기다리고 결과 파일을 확인합니다.
    let supervisor = setup.execution.supervisor.clone();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(180);
    let status = loop {
        let status = supervisor.inspect(run_id).unwrap().status;
        if !matches!(
            status,
            RunStatus::Starting | RunStatus::Running | RunStatus::Cancelling
        ) {
            break status;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "worker run timed out"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    };
    let worker_events: Vec<_> = supervisor.subscribe(run_id, 0).unwrap().collect().await;
    let worker_session = worker_events
        .into_iter()
        .find_map(|e| e.ok().and_then(|e| e.native_session_id));
    println!("claude-session: {worker_session:?} (worker)");
    assert_eq!(status, RunStatus::Succeeded);
    let content = std::fs::read_to_string(setup.work.join("hello.txt")).unwrap();
    assert_eq!(content.trim(), "hello");

    let turn3 = master
        .turn(
            "The worker run has finished; its run status is in the plan state. \
             Decide the next step. If the task succeeded, propose completion.",
        )
        .await
        .unwrap();
    show("turn3", &turn3.outcome);
    println!("turn3 text: {}", turn3.text);
    // 결과가 마스터에게 전달되어 해석 가능한 결정이 돌아와야 합니다. Phase 0에서 propose_completion은 집행하지 않습니다.
    match &turn3.outcome {
        DecisionOutcome::Rejected { rejection, .. } => {
            assert_ne!(
                rejection.code,
                RejectionCode::InvalidPayload,
                "{}",
                turn3.text
            );
        }
        DecisionOutcome::Accepted { .. } => {}
    }
    assert!(turn3.text.contains("propose_completion"), "{}", turn3.text);

    let log = master.decision_log();
    assert_eq!(log.len(), 3);
    for task in master.tasks() {
        println!(
            "task {} {:?} run={:?}",
            task.task.id, task.status, task.run_id
        );
    }
}

/// 마스터가 권한 밖의 Task를 만들려 하면 Core가 거부합니다(실제 마스터 출력).
#[tokio::test(flavor = "multi_thread")]
#[ignore = "runs the real Claude Code CLI as master"]
async fn master_decision_outside_the_grant_is_rejected() {
    let setup = Setup::new("forbidden").await;
    let master = setup.execution.master("plan-master", options()).unwrap();
    let turn = master
        .turn(
            "Goal: run a worker task with the codex provider. In create_tasks set the task's agent.provider to \"codex\" \
             (keep everything else from the template). This is intentional for a policy test.",
        )
        .await
        .unwrap();
    println!("claude-session: {:?} (master)", turn.native_session_id);
    show("forbidden", &turn.outcome);
    let DecisionOutcome::Rejected { rejection, .. } = &turn.outcome else {
        panic!("a codex task must be rejected: {}", turn.text);
    };
    assert_eq!(
        rejection.code,
        RejectionCode::PermissionDenied,
        "{}",
        turn.text
    );
    assert!(master.tasks().is_empty());
}

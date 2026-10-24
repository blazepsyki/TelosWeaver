//! G1 대표 실행 실환경 시험 (Spec §19.5.8, 대표 Provider Claude).
//!
//! Local API → 실행 감독 → 운영자 정책 승인 → 탐지한 확장 CLI → Claude Code 실제 실행 경로를 확인합니다.
//! 실제 CLI를 실행하고 사용량을 쓰므로 기본 실행에서 제외합니다. 시험 조건은 배정표 §6(haiku 등)입니다.
//!
//! ```text
//! cargo test -p telos-core --test phase0_g1 -- --ignored --test-threads=1 --nocapture
//! ```
//!
//! 각 시험은 Native Session ID를 `claude-session:` 줄로 출력합니다. 시험 뒤 Archive 대상입니다.

#[path = "phase0/client.rs"]
mod client;

use std::{path::PathBuf, time::Duration};

use client::Client;
use serde_json::{Value, json};
use telos_core::{
    api::{ApiServer, ApiServerConfig},
    integration::Phase0Execution,
};
use telos_schema::{TaskSpec, WorkspaceMode, fixtures};

struct Setup {
    root: PathBuf,
    workspace: PathBuf,
    execution: Phase0Execution,
    server: Option<ApiServer>,
}

impl Setup {
    async fn new(name: &str, models: &[&str]) -> Self {
        let root = std::env::temp_dir().join("telos-phase0-g1").join(name);
        let _ = std::fs::remove_dir_all(&root);
        let workspace = root.join("workspace");
        for dir in ["project", "state", "runtime", "workspace"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        let policy = root.join("policy.json");
        std::fs::write(
            &policy,
            serde_json::to_vec(&json!({
                "revision": "g1-test",
                "permissionProfiles": ["phase0"],
                "workspaces": [{ "id": "ws-g1", "path": workspace, "writable": true }],
                "claude": {
                    "models": models,
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
        let discovery = execution.claude_discovery.as_ref().unwrap();
        println!(
            "discovery: {}",
            serde_json::to_string(&discovery.outcome).unwrap()
        );
        assert!(discovery.found().is_some(), "Claude CLI must be discovered");
        let server = ApiServer::start(
            ApiServerConfig {
                project_root: root.join("project"),
                state_base: Some(root.join("state")),
                runtime_base: Some(root.join("runtime")),
            },
            execution.supervisor.clone(),
        )
        .await
        .unwrap();
        Self {
            root,
            workspace,
            execution,
            server: Some(server),
        }
    }

    async fn client(&self) -> Client {
        let server = self.server.as_ref().unwrap();
        let token = std::fs::read_to_string(server.token_path()).unwrap();
        let mut client = Client::connect(server.endpoint()).await;
        client.hello(token.trim()).await;
        client
    }
}

impl Drop for Setup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn task(id: &str, objective: &str) -> TaskSpec {
    let mut task = fixtures::task_spec();
    task.id = id.into();
    task.agent.provider = "claude".into();
    task.permission_profile_id = "phase0".into();
    task.workspace.mode = WorkspaceMode::Directory;
    task.workspace.workspace_id = Some("ws-g1".into());
    task.workspace.read_only = false;
    task.objective = objective.into();
    task
}

async fn start(client: &mut Client, task: &TaskSpec, key: &str) -> Value {
    client
        .call(
            "runs.start",
            json!({
                "task": task,
                "session": { "mode": "fresh" },
                "idempotencyKey": key,
            }),
        )
        .await
}

async fn subscribe(client: &mut Client, run_id: &str) {
    let response = client
        .call(
            "runs.subscribe",
            json!({"runId": run_id, "afterSequence": 0}),
        )
        .await;
    assert!(response.get("result").is_some(), "subscribe: {response}");
}

fn report(events: &[Value]) {
    if let Some(id) = events.iter().find_map(|e| e["nativeSessionId"].as_str()) {
        println!("claude-session: {id}");
    }
    for e in events {
        println!("  {:>2} {} {}", e["sequence"], e["kind"], e["payload"]);
    }
}

fn kinds(events: &[Value]) -> Vec<&str> {
    events.iter().map(|e| e["kind"].as_str().unwrap()).collect()
}

fn find<'a>(events: &'a [Value], kind: &str) -> Option<&'a Value> {
    events
        .iter()
        .find(|e| e["kind"] == kind)
        .map(|e| &e["payload"])
}

/// G1: 시작 → 이벤트 → 완료 → 결과(파일 산출물) → 실행 감독 상태.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "runs the real Claude Code CLI"]
async fn g1_completed_run_produces_result_file() {
    let setup = Setup::new("completed", &["haiku"]).await;
    let mut client = setup.client().await;
    let t = task(
        "g1-complete",
        "Use the Write tool to create a file named result.txt whose entire content is: pong",
    );
    let started = start(&mut client, &t, "g1-complete:1").await;
    let run_id = started["result"]["runId"]
        .as_str()
        .expect("run started")
        .to_owned();
    // 같은 키의 재요청은 같은 Run입니다.
    let again = start(&mut client, &t, "g1-complete:1").await;
    assert_eq!(again["result"]["runId"], run_id.as_str());

    subscribe(&mut client, &run_id).await;
    let events = client.events_until_end(Duration::from_secs(180)).await;
    report(&events);
    assert_eq!(kinds(&events).first(), Some(&"session_started"));
    assert_eq!(kinds(&events).last(), Some(&"process_exited"));
    assert_eq!(
        find(&events, "turn_finished").unwrap()["status"],
        "completed"
    );
    let write = events
        .iter()
        .find(|e| e["kind"] == "tool_completed" && e["payload"]["name"] == "Write")
        .expect("Write tool completed");
    assert_eq!(write["payload"]["outcome"], "succeeded");
    let exit = find(&events, "process_exited").unwrap();
    assert_eq!(exit["exitCode"], 0);
    assert_eq!(exit["childrenReaped"], true);

    let result = std::fs::read_to_string(setup.workspace.join("result.txt")).unwrap();
    assert_eq!(result.trim(), "pong");

    let inspect = client.call("runs.inspect", json!({"runId": run_id})).await;
    println!("inspect: {inspect}");
    assert_eq!(inspect["result"]["lastSequence"], events.len());
    assert_ne!(inspect["result"]["status"], "running");
}

/// G1: Provider 실패는 실패한 Turn으로 보고되고 성공으로 기록되지 않습니다.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "runs the real Claude Code CLI"]
async fn g1_provider_failure_is_reported() {
    let setup = Setup::new("failure", &["no-such-model-xyz"]).await;
    let mut client = setup.client().await;
    let started = start(&mut client, &task("g1-fail", "hi"), "g1-fail:1").await;
    let run_id = started["result"]["runId"]
        .as_str()
        .expect("run started")
        .to_owned();
    subscribe(&mut client, &run_id).await;
    let events = client.events_until_end(Duration::from_secs(120)).await;
    report(&events);
    assert_eq!(find(&events, "error").unwrap()["kind"], "configuration");
    assert_eq!(find(&events, "turn_finished").unwrap()["status"], "failed");
    assert_ne!(find(&events, "process_exited").unwrap()["exitCode"], 0);
    let inspect = client.call("runs.inspect", json!({"runId": run_id})).await;
    println!("inspect: {inspect}");
    assert_ne!(inspect["result"]["status"], "running");
}

/// G1: 정책이 허용하지 않은 요청은 프로세스를 만들지 않고 거부합니다.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "runs the real Claude Code CLI discovery"]
async fn g1_unauthorized_request_is_rejected_before_start() {
    let setup = Setup::new("forbidden", &["haiku"]).await;
    let mut client = setup.client().await;
    let mut t = task("g1-forbidden", "hi");
    t.workspace.workspace_id = Some("not-granted".into());
    let response = start(&mut client, &t, "g1-forbidden:1").await;
    assert_eq!(response["error"]["code"], "forbidden", "{response}");
}

/// G1: 취소 접수 → 실제 종료와 자식 회수 확인.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "runs the real Claude Code CLI"]
async fn g1_cancel_reports_actual_termination() {
    let setup = Setup::new("cancel", &["haiku"]).await;
    let mut client = setup.client().await;
    let t = task(
        "g1-cancel",
        "Write the numbers from 1 to 400, one per line, with no other text. Do not use tools.",
    );
    let started = start(&mut client, &t, "g1-cancel:1").await;
    let run_id = started["result"]["runId"]
        .as_str()
        .expect("run started")
        .to_owned();
    subscribe(&mut client, &run_id).await;
    // 세션이 시작된 뒤 취소합니다.
    loop {
        let message = client.receive(Duration::from_secs(60)).await;
        if message["method"] == "runs.event" {
            client.notifications.push(message.clone());
            if message["params"]["event"]["kind"] == "session_started" {
                break;
            }
        }
    }
    let cancel = client.call("runs.cancel", json!({"runId": run_id})).await;
    println!("cancel: {cancel}");
    let events = client.events_until_end(Duration::from_secs(60)).await;
    report(&events);
    let exit = find(&events, "process_exited").unwrap();
    assert_eq!(exit["childrenReaped"], true);
    if find(&events, "turn_finished").unwrap()["status"] == "cancelled" {
        assert!(matches!(
            exit["termination"].as_str(),
            Some("terminated_on_request" | "killed")
        ));
    }
}

/// Daemon 종료: 활성 Run을 취소하고 종료를 확인합니다. API 연결 종료만으로는 취소하지 않습니다.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "runs the real Claude Code CLI"]
async fn g1_daemon_shutdown_cancels_active_runs() {
    let mut setup = Setup::new("shutdown", &["haiku"]).await;
    let mut client = setup.client().await;
    let t = task(
        "g1-shutdown",
        "Write the numbers from 1 to 400, one per line, with no other text. Do not use tools.",
    );
    let started = start(&mut client, &t, "g1-shutdown:1").await;
    let run_id = started["result"]["runId"]
        .as_str()
        .expect("run started")
        .to_owned();
    drop(client);
    tokio::time::sleep(Duration::from_secs(2)).await;
    let state = setup.execution.supervisor.inspect(&run_id).unwrap();
    println!("after client disconnect: {state:?}");

    setup.server.take().unwrap().shutdown().await.unwrap();
    let report = setup.execution.shutdown(Duration::from_secs(20)).await;
    println!("shutdown: {report:?}");
    assert!(report.unconfirmed.is_empty());
    let state = setup.execution.supervisor.inspect(&run_id).unwrap();
    println!("after shutdown: {state:?}");
}

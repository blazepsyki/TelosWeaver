//! G2 구성 통합 실환경 시험: 세 Provider의 Core 연결 (Spec §19.5.8, INTEGRATION 증거 §8).
//!
//! 한 Daemon 구성에 Claude·Codex·Antigravity를 등록하고 Local API로 실제 실행합니다.
//! 실제 CLI를 실행하고 사용량을 쓰므로 기본 실행에서 제외합니다. 시험 조건은 배정표 §6입니다.
//! Claude `haiku`, Codex `gpt-6-luna`/`low`, Antigravity `gemini-3.8-flash-low`/`low`, 일회용 Workspace.
//!
//! ```text
//! cargo test -p telos-core --test phase0_g2 -- --ignored --test-threads=1 --nocapture
//! ```
//!
//! 시험 세션 정리: Codex는 Adapter가 만든 Thread만 Archive하고, Antigravity는 시험이 만든 대화 폴더만 지웁니다.
//! Claude 세션 ID는 `claude-session:` 줄로 출력하며 Archive 목록에 올립니다.

#[path = "phase0/client.rs"]
mod client;

use std::{path::PathBuf, time::Duration};

use client::Client;
use futures::StreamExt;
use serde_json::{Value, json};
use telos_adapters::AgentAdapter;
use telos_core::{
    api::{ApiServer, ApiServerConfig},
    integration::Phase0Execution,
};
use telos_schema::{RunStatus, TaskSpec, WorkspaceMode, fixtures};

const PROVIDERS: [&str; 3] = ["claude", "codex", "agy"];

struct Models {
    claude: &'static str,
    codex: &'static str,
    agy: &'static str,
}

const LIGHT: Models = Models {
    claude: "haiku",
    codex: "gpt-6-luna",
    agy: "gemini-3.8-flash-low",
};

struct Setup {
    root: PathBuf,
    workspace: PathBuf,
    execution: Phase0Execution,
    server: Option<ApiServer>,
    /// 정리할 Native 세션 (Provider, ID).
    sessions: Vec<(String, String)>,
}

impl Setup {
    async fn new(name: &str, models: Models) -> Self {
        let root = std::env::temp_dir().join("telos-phase0-g2").join(name);
        let _ = std::fs::remove_dir_all(&root);
        let workspace = root.join("workspace");
        for dir in ["project", "state", "runtime", "workspace"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        let policy = root.join("policy.json");
        std::fs::write(
            &policy,
            serde_json::to_vec(&json!({
                "revision": "g2-test",
                "permissionProfiles": ["phase0"],
                "workspaces": [{ "id": "ws-g2", "path": workspace, "writable": true }],
                "claude": {
                    "models": [models.claude],
                    "permissionMode": "acceptEdits",
                    "allowedTools": ["Read", "Write"],
                    "disallowedTools": ["Bash", "WebFetch", "WebSearch"],
                    "settingSources": "user"
                },
                "codex": { "models": [models.codex], "effort": "low", "runTimeoutSecs": 180 },
                "agy": { "models": [models.agy], "effort": "low", "mode": "accept-edits" }
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
        for discovery in &execution.discoveries {
            println!(
                "discovery {}: {}",
                discovery.provider_id,
                serde_json::to_string(&discovery.outcome).unwrap()
            );
        }
        println!("notes: {:?}", execution.notes);
        for provider in PROVIDERS {
            assert!(
                execution
                    .discovery(provider)
                    .and_then(|d| d.found())
                    .is_some(),
                "{provider} CLI must be discovered"
            );
        }
        assert!(execution.notes.is_empty(), "{:?}", execution.notes);
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
            sessions: Vec::new(),
        }
    }

    async fn client(&self) -> Client {
        let server = self.server.as_ref().unwrap();
        let token = std::fs::read_to_string(server.token_path()).unwrap();
        let mut client = Client::connect(server.endpoint()).await;
        client.hello(token.trim()).await;
        client
    }

    fn remember(&mut self, provider: &str, events: &[Value]) {
        if let Some(id) = events.iter().find_map(|e| e["nativeSessionId"].as_str()) {
            println!("{provider}-session: {id}");
            self.sessions.push((provider.into(), id.into()));
        }
    }

    /// Codex는 소유 Thread만 Archive하고, Agy는 시험이 만든 대화 폴더만 지웁니다.
    async fn cleanup(&mut self) {
        if let Some(codex) = &self.execution.codex {
            for thread in codex.owned_thread_ids() {
                let archived = codex.archive(&thread, self.workspace.clone()).await;
                println!("codex archive {thread}: {archived:?}");
            }
        }
        let home = PathBuf::from(
            std::env::var_os("USERPROFILE")
                .or_else(|| std::env::var_os("HOME"))
                .unwrap(),
        );
        for (provider, id) in &self.sessions {
            if provider != "agy" || id.is_empty() || id.contains(['/', '\\', '.']) {
                continue;
            }
            for dir in ["antigravity", "antigravity-cli"] {
                let base = home.join(".gemini").join(dir);
                let _ = std::fs::remove_dir_all(base.join("brain").join(id));
                // agy 1.4.7은 대화를 `<id>.db`(SQLite, 강제 종료 시 `-wal`·`-shm` 잔존)로 저장합니다.
                for ext in ["db", "db-wal", "db-shm", "pb"] {
                    let _ = std::fs::remove_file(
                        base.join("conversations").join(format!("{id}.{ext}")),
                    );
                }
            }
            println!("agy cleanup {id}");
        }
    }
}

impl Drop for Setup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn task(provider: &str, id: &str, objective: &str) -> TaskSpec {
    let mut task = fixtures::task_spec();
    task.id = id.into();
    task.agent.provider = provider.into();
    task.permission_profile_id = "phase0".into();
    task.workspace.mode = WorkspaceMode::Directory;
    task.workspace.workspace_id = Some("ws-g2".into());
    task.workspace.read_only = false;
    task.objective = objective.into();
    task
}

async fn start(client: &mut Client, task: &TaskSpec) -> String {
    let response = client
        .call(
            "runs.start",
            json!({
                "task": task,
                "session": { "mode": "fresh" },
                "idempotencyKey": format!("{}:1", task.id),
            }),
        )
        .await;
    response["result"]["runId"]
        .as_str()
        .unwrap_or_else(|| panic!("run start failed: {response}"))
        .to_owned()
}

async fn run_to_end(client: &mut Client, task: &TaskSpec) -> (String, Vec<Value>) {
    let run_id = start(client, task).await;
    let subscription = client.subscribe(&run_id).await;
    let events = client
        .events_until_end(&run_id, &subscription, Duration::from_secs(240))
        .await;
    report(&task.agent.provider, &events);
    (run_id, events)
}

fn report(provider: &str, events: &[Value]) {
    for e in events {
        println!(
            "  [{provider}] {:>2} {} {}",
            e["sequence"], e["kind"], e["payload"]
        );
    }
}

fn find<'a>(events: &'a [Value], kind: &str) -> Option<&'a Value> {
    events
        .iter()
        .find(|e| e["kind"] == kind)
        .map(|e| &e["payload"])
}

/// 마지막 이벤트와 실행 감독의 최종 상태 갱신 경합을 제한 시간 안에서 다시 조회합니다 (REQ-015와 같은 방식).
async fn wait_status(client: &mut Client, run_id: &str, expected: RunStatus) -> Value {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let inspect = client.call("runs.inspect", json!({"runId": run_id})).await;
        if inspect["result"]["status"] == json!(expected) {
            return inspect;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "expected {expected:?}, last inspect: {inspect}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// `wait_status`와 같지만 실패해도 멈추지 않고 마지막 조회 결과를 돌려줍니다(정리 전 수집용).
async fn settle_status(client: &mut Client, run_id: &str, expected: RunStatus) -> Value {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let inspect = client.call("runs.inspect", json!({"runId": run_id})).await;
        if inspect["result"]["status"] == json!(expected) || tokio::time::Instant::now() >= deadline
        {
            return inspect;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn wait_for_session(client: &mut Client) {
    loop {
        let message = client.receive(Duration::from_secs(120)).await;
        if message["method"] == "runs.event" {
            client.notifications.push(message.clone());
            if message["params"]["event"]["kind"] == "session_started" {
                return;
            }
        }
    }
}

/// G2: 한 Daemon의 세 Provider가 각각 시작 → 이벤트 → 완료 → 결과 파일을 만듭니다.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "runs the real Claude, Codex and Antigravity CLIs"]
async fn g2_each_provider_completes_and_writes_a_result_file() {
    let mut setup = Setup::new("completed", LIGHT).await;
    let mut client = setup.client().await;
    let mut results = Vec::new();
    for provider in PROVIDERS {
        let file = format!("result-{provider}.txt");
        let t = task(
            provider,
            &format!("g2-complete-{provider}"),
            &format!(
                "Create a file named {file} in the current working directory whose entire content is: pong. Do not run shell commands. Then reply done."
            ),
        );
        let (run_id, events) = run_to_end(&mut client, &t).await;
        setup.remember(provider, &events);
        let content = std::fs::read_to_string(setup.workspace.join(&file));
        let inspect = settle_status(&mut client, &run_id, RunStatus::Succeeded).await;
        results.push((provider, events, content, inspect));
    }
    // 판정 실패로 정리가 빠지지 않도록 세션을 먼저 정리합니다.
    setup.cleanup().await;
    for (provider, events, content, inspect) in results {
        assert_eq!(
            events.first().unwrap()["kind"],
            "session_started",
            "{provider}"
        );
        assert_eq!(
            events.last().unwrap()["kind"],
            "process_exited",
            "{provider}"
        );
        assert_eq!(
            find(&events, "turn_finished").unwrap()["status"],
            "completed",
            "{provider}"
        );
        assert_eq!(
            find(&events, "process_exited").unwrap()["childrenReaped"],
            true
        );
        let content = content.unwrap_or_else(|e| {
            panic!("{provider} did not write its result in the workspace: {e}")
        });
        assert_eq!(content.trim(), "pong", "{provider}");
        assert_eq!(
            inspect["result"]["status"],
            json!(RunStatus::Succeeded),
            "{provider}: {inspect}"
        );
        assert_eq!(inspect["result"]["lastSequence"], events.len());
        println!("{provider}: {inspect}");
    }
}

/// G2: Provider 실패(없는 모델)는 실패한 Turn과 `Failed`로 끝나고 성공으로 기록되지 않습니다.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "runs the real Codex and Antigravity CLIs"]
async fn g2_codex_and_agy_provider_failures_are_reported() {
    let mut setup = Setup::new(
        "failure",
        Models {
            claude: "haiku",
            codex: "telos-invalid-model",
            agy: "no-such-model-xyz",
        },
    )
    .await;
    let mut client = setup.client().await;
    for provider in ["codex", "agy"] {
        let t = task(provider, &format!("g2-fail-{provider}"), "Reply with: hi");
        let (run_id, events) = run_to_end(&mut client, &t).await;
        setup.remember(provider, &events);
        assert_eq!(
            find(&events, "error").unwrap()["kind"],
            "configuration",
            "{provider}"
        );
        assert_eq!(
            find(&events, "turn_finished").unwrap()["status"],
            "failed",
            "{provider}"
        );
        let inspect = wait_status(&mut client, &run_id, RunStatus::Failed).await;
        println!("{provider}: {inspect}");
    }
    setup.cleanup().await;
}

/// G2: 취소 접수 → 실제 Turn 취소 → 종료·자식 회수 → `Cancelled`.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "runs the real Codex and Antigravity CLIs"]
async fn g2_codex_and_agy_cancel_reports_actual_termination() {
    let mut setup = Setup::new("cancel", LIGHT).await;
    let mut client = setup.client().await;
    for provider in ["codex", "agy"] {
        let t = task(
            provider,
            &format!("g2-cancel-{provider}"),
            "Write the numbers from 1 to 1500, one per line, with no other text. Do not use tools.",
        );
        let run_id = start(&mut client, &t).await;
        let subscription = client.subscribe(&run_id).await;
        wait_for_session(&mut client).await;
        let cancel = client.call("runs.cancel", json!({"runId": run_id})).await;
        assert_eq!(cancel["result"]["accepted"], true, "{provider}: {cancel}");
        let events = client
            .events_until_end(&run_id, &subscription, Duration::from_secs(120))
            .await;
        report(provider, &events);
        setup.remember(provider, &events);
        assert_eq!(
            find(&events, "turn_finished").unwrap()["status"],
            "cancelled",
            "{provider}"
        );
        let exit = find(&events, "process_exited").unwrap();
        assert_eq!(exit["childrenReaped"], true, "{provider}: {exit}");
        let inspect = wait_status(&mut client, &run_id, RunStatus::Cancelled).await;
        println!("{provider}: {inspect}");
    }
    setup.cleanup().await;
}

/// Daemon 종료: Provider가 섞인 활성 Run을 모두 취소하고 종료를 확인합니다.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "runs the real Codex and Antigravity CLIs"]
async fn g2_daemon_shutdown_cancels_runs_of_every_provider() {
    let mut setup = Setup::new("shutdown", LIGHT).await;
    let mut client = setup.client().await;
    let mut runs = Vec::new();
    for provider in ["codex", "agy"] {
        let t = task(
            provider,
            &format!("g2-shutdown-{provider}"),
            "Write the numbers from 1 to 1500, one per line, with no other text. Do not use tools.",
        );
        runs.push((provider, start(&mut client, &t).await));
    }
    drop(client);
    tokio::time::sleep(Duration::from_secs(3)).await;
    for (provider, run_id) in &runs {
        let state = setup.execution.supervisor.inspect(run_id).unwrap();
        println!("{provider} after disconnect: {state:?}");
        assert_eq!(state.status, RunStatus::Running, "{provider}");
    }
    setup.server.take().unwrap().shutdown().await.unwrap();
    let report = setup.execution.shutdown(Duration::from_secs(30)).await;
    println!("shutdown: {report:?}");
    assert_eq!(report.cancelled, 2);
    assert!(report.unconfirmed.is_empty());
    for (provider, run_id) in &runs {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        loop {
            let state = setup.execution.supervisor.inspect(run_id).unwrap();
            if state.status == RunStatus::Cancelled {
                println!("{provider} after shutdown: {state:?}");
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "{provider}: {state:?}"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        // 정리 대상 세션 ID를 얻기 위해 Adapter에 보존된 이벤트를 다시 읽습니다.
        let adapter = setup
            .execution
            .adapters
            .iter()
            .find(|a| a.id() == *provider)
            .unwrap()
            .clone();
        let events: Vec<Value> = adapter
            .events(run_id, 0)
            .filter_map(|e| async { e.ok() })
            .map(|e| serde_json::to_value(e).unwrap())
            .collect()
            .await;
        setup.remember(provider, &events);
    }
    setup.cleanup().await;
}

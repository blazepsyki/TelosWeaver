//! RN Fork의 Core 연결 시험 (Spec v1.5.1 §12.3·§12.5, 배정표 §6 "RN Core 연결", PH0-RN-07).
//!
//! 기본 실행: 정책의 `rn` 절, `core.health`의 `rn` 구성 요소(REQ-019), RN을 띄우지 못해도 Core가 계속 동작하는지.
//! 실환경(`--ignored`): 실제 `telos-rn`을 띄워 Client로 메시지를 왕복하고, 강제 종료 뒤 재시작·정지를 확인합니다.
//! 실제 Claude(`haiku`) Managed Run 중 RN을 강제 종료해 Run 추적·취소가 RN과 무관한지도 확인합니다.
//!
//! ```text
//! TELOS_RN_EXECUTABLE=<rn-fork/target/debug/telos-rn(.exe)> \
//!   cargo test -p telos-core --test phase0_rn -- --ignored --test-threads=1 --nocapture
//! ```
//!
//! 시험 Claude 세션 ID는 `claude-session:` 줄로 출력하며, 병합 시점에 지웁니다(배정표 §6 "시험 세션 정리").

#[path = "phase0/client.rs"]
mod client;

use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
    time::Duration,
};

use client::Client;
use serde_json::{Value, json};
use telos_core::{
    api::{ApiServer, ApiServerConfig},
    integration::Phase0Execution,
};
use telos_rn_client::{
    ClientOptions, MessageBodyType, MessageEnvelope, RnClient, RnEventKind, SessionQuery,
};
use telos_schema::{
    DeliveryReceiptStatus, DeliveryTier, RunStatus, TaskSpec, WorkspaceMode, api::HealthStatus,
    fixtures,
};

fn temp(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join("telos-phase0-rn").join(name);
    let _ = std::fs::remove_dir_all(&root);
    for dir in ["project", "state", "runtime", "workspace"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    root
}

fn policy(root: &Path, rn: Option<&Path>, claude: bool) -> PathBuf {
    let mut p = json!({
        "revision": "rn-test",
        "permissionProfiles": ["phase0"],
        "workspaces": [{ "id": "ws-rn", "path": root.join("workspace"), "writable": true }],
    });
    if let Some(exe) = rn {
        p["rn"] = json!({ "executable": exe });
    }
    if claude {
        p["claude"] = json!({
            "models": ["haiku"],
            "permissionMode": "acceptEdits",
            "allowedTools": ["Read", "Write"],
            "disallowedTools": ["Bash", "WebFetch", "WebSearch"],
            "settingSources": "user"
        });
    }
    let path = root.join("policy.json");
    std::fs::write(&path, serde_json::to_vec(&p).unwrap()).unwrap();
    path
}

async fn execution(policy: Option<&Path>) -> Phase0Execution {
    let task = fixtures::task_spec();
    Phase0Execution::build(
        task.project_id.clone(),
        task.workspace.host_id.clone(),
        policy,
    )
    .await
    .unwrap()
}

async fn server(root: &Path, execution: &Phase0Execution) -> ApiServer {
    ApiServer::start(
        ApiServerConfig {
            project_root: root.join("project"),
            state_base: Some(root.join("state")),
            runtime_base: Some(root.join("runtime")),
        },
        execution.supervisor.clone(),
    )
    .await
    .unwrap()
}

fn rn_component(execution: &Phase0Execution) -> telos_schema::api::ComponentHealth {
    execution
        .supervisor
        .health()
        .components
        .into_iter()
        .find(|c| c.name == "rn")
        .expect("an rn component")
}

#[test]
fn the_policy_takes_an_rn_executable_only_as_an_absolute_path() {
    let root = temp("policy");
    let ok = policy(&root, Some(&root.join("telos-rn.exe")), false);
    assert!(
        telos_core::integration::LocalPolicyConfig::load(&ok)
            .unwrap()
            .0
            .rn
            .is_some()
    );
    let relative = policy(&root, Some(Path::new("telos-rn.exe")), false);
    let e = telos_core::integration::LocalPolicyConfig::load(&relative).unwrap_err();
    assert!(e.contains("rn.executable must be an absolute path"), "{e}");
    std::fs::write(&relative, br#"{"revision":"r","permissionProfiles":[],"workspaces":[],"rn":{"executable":"/x","extra":1}}"#)
        .unwrap();
    assert!(telos_core::integration::LocalPolicyConfig::load(&relative).is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn without_rn_in_the_policy_core_reports_it_not_configured_after_its_own_components() {
    let root = temp("unconfigured");
    let execution = execution(None).await;
    let api = server(&root, &execution).await;
    let rn = execution.start_rn(api.token_path()).await;
    let health = execution.supervisor.health();
    let names: Vec<&str> = health.components.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["execution", "scheduler", "persistence", "rn"]);
    let c = rn_component(&execution);
    assert_eq!(c.status, HealthStatus::Degraded);
    assert!(c.detail.unwrap().starts_with("not configured"));
    assert!(rn.client().is_none());
    // Over the Local API too.
    let token = std::fs::read_to_string(api.token_path()).unwrap();
    let mut client = Client::connect(api.endpoint()).await;
    client.hello(token.trim()).await;
    let health = client.call("core.health", json!({})).await;
    let rn_row = health["result"]["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "rn")
        .unwrap()
        .clone();
    assert_eq!(rn_row["status"], "degraded");
    api.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn an_rn_that_cannot_start_leaves_core_running_and_says_why() {
    let root = temp("missing");
    let missing = root.join("no-such-telos-rn.exe");
    let policy = policy(&root, Some(&missing), false);
    let execution = execution(Some(&policy)).await;
    let api = server(&root, &execution).await;
    let rn = execution.start_rn(api.token_path()).await;
    let c = rn_component(&execution);
    assert_eq!(c.status, HealthStatus::Degraded);
    assert!(c.detail.unwrap().contains("not started"));
    // Its folder is under Core's own state for this project.
    let state = api.token_path().parent().unwrap().to_path_buf();
    assert!(state.join("rn").is_dir());
    // Core answers and the supervisor is untouched.
    assert_eq!(
        execution.supervisor.health().components[0].status,
        HealthStatus::Ok
    );
    rn.shutdown().await;
    assert!(
        rn_component(&execution)
            .detail
            .unwrap()
            .contains("stopped with Core")
    );
    api.shutdown().await.unwrap();
}

// --- 실환경 ---

fn rn_executable() -> PathBuf {
    let exe = PathBuf::from(
        std::env::var_os("TELOS_RN_EXECUTABLE").expect("TELOS_RN_EXECUTABLE is required"),
    );
    assert!(exe.is_absolute() && exe.is_file(), "{}", exe.display());
    exe
}

/// `telos-rn` 명령을 RN 상태 폴더 `home`에 대고 실행합니다(시험 메일함).
fn rn_cli(exe: &Path, home: &Path, args: &[&str]) -> String {
    let out = Command::new(exe)
        .args(args)
        .env("TELOS_RN_HOME", home)
        .env("TELOS_RN_HOST_ID", "rn07-host")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    println!("$ telos-rn {} -> {}", args.join(" "), text.trim());
    text
}

fn kill(pid: u32) {
    #[cfg(windows)]
    let status = Command::new("taskkill")
        .args(["/F", "/PID", &pid.to_string()])
        .output()
        .unwrap()
        .status;
    #[cfg(unix)]
    let status = Command::new("kill")
        .args(["-9", &pid.to_string()])
        .status()
        .unwrap();
    assert!(status.success(), "killing {pid}");
}

async fn wait_component(execution: &Phase0Execution, status: HealthStatus, secs: u64) -> String {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
    loop {
        let c = rn_component(execution);
        if c.status == status {
            return c.detail.unwrap_or_default();
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "rn stayed {:?}: {:?}",
            c.status,
            c.detail
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

fn envelope(
    id: &str,
    sender: &telos_rn_client::EndpointRef,
    to: &telos_rn_client::EndpointRef,
) -> MessageEnvelope {
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600;
    let (days, rem) = ((expires / 86_400) as i64, expires % 86_400);
    // RFC 3339 from the epoch (civil date), as Core writes it.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    MessageEnvelope {
        message_id: id.into(),
        task_id: Some("rn07-task".into()),
        run_id: None,
        correlation_id: "rn07".into(),
        idempotency_key: format!("k-{id}"),
        sender: sender.clone(),
        recipient: to.clone(),
        tier: DeliveryTier::Pull,
        expires_at: format!(
            "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
            rem / 3600,
            rem % 3600 / 60,
            rem % 60
        ),
        body_type: MessageBodyType::Text,
        body: json!(format!("RN-07 {id} from Core")),
        artifacts: Vec::new(),
    }
}

/// 실제 RN: Client로 메일함과 왕복하고, 강제 종료 → 재시작 → 이전 커서는 재동기화, Core 정지 시 함께 정지.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "runs the real telos-rn (TELOS_RN_EXECUTABLE)"]
async fn rn_round_trip_restart_and_stop_with_core() {
    let exe = rn_executable();
    let root = temp("real");
    let policy = policy(&root, Some(&exe), false);
    let execution = execution(Some(&policy)).await;
    let api = server(&root, &execution).await;
    let rn = execution.start_rn(api.token_path()).await;
    let detail = wait_component(&execution, HealthStatus::Ok, 15).await;
    println!("rn: {detail}");
    assert!(detail.contains("started by Core"));
    let home = api.token_path().parent().unwrap().join("rn");
    let info = telos_rn_client::rpc_info(&home).unwrap();
    assert_eq!(Some(info.pid), rn.owned_pid());

    // A test mailbox, attached through telos-rn's own hooks (`listen`).
    rn_cli(&exe, &home, &["listen", "--as", "review", "--wait", "0"]);
    let client = rn.client().unwrap();
    let core = client.hello().endpoint.clone();
    assert_eq!(
        (core.provider.as_str(), client.hello().host_id.as_str()),
        ("telosweaver", execution.supervisor.host_id())
    );
    let found = client
        .discover(&SessionQuery {
            provider: Some("cli".into()),
            host_id: None,
            workspace: None,
        })
        .await
        .unwrap();
    let review = found
        .iter()
        .find(|s| s.endpoint.native_session_id == "review")
        .unwrap()
        .clone();
    assert!(review.attached);
    let mut sub = client.subscribe(None).await.unwrap();
    let receipt = client
        .send(&envelope("rn07-m1", &core, &review.endpoint))
        .await
        .unwrap();
    assert_eq!(receipt.status, DeliveryReceiptStatus::Queued);
    let shown = rn_cli(&exe, &home, &["listen", "--as", "review", "--wait", "5"]);
    assert!(shown.contains("RN-07 rn07-m1 from Core"));
    rn_cli(
        &exe,
        &home,
        &[
            "send",
            "telosweaver",
            "--from",
            "review",
            "--reply-to",
            "rn07-m1",
            "RN-07 ack",
        ],
    );
    let (mut delivered, mut replied, mut last) = (false, false, String::new());
    while !(delivered && replied) {
        let e = tokio::time::timeout(Duration::from_secs(10), sub.next())
            .await
            .unwrap()
            .unwrap();
        println!("event {} {:?} {}", e.cursor, e.kind, e.payload);
        last = e.cursor.clone();
        delivered |= e.kind == RnEventKind::Receipt
            && e.payload["messageId"] == "rn07-m1"
            && e.payload["status"] == "delivered";
        replied |= e.kind == RnEventKind::Message
            && e.payload["replyTo"] == "rn07-m1"
            && e.payload["body"] == "RN-07 ack";
    }
    assert_eq!(
        client.status("rn07-m1").await.unwrap().status,
        DeliveryReceiptStatus::Delivered
    );
    drop(sub);
    drop(client);

    // It is killed: Degraded, then started again by Core with a new process.
    let first = rn.owned_pid().unwrap();
    kill(first);
    let detail = wait_component(&execution, HealthStatus::Degraded, 10).await;
    println!("after kill: {detail}");
    let detail = wait_component(&execution, HealthStatus::Ok, 20).await;
    println!("restarted: {detail}");
    let second = rn.owned_pid().unwrap();
    assert_ne!(first, second);
    // Its receipts survive; the cursor from before is resynced.
    let client = RnClient::connect_retrying(&home, &ClientOptions::default(), 3)
        .await
        .unwrap();
    assert_eq!(
        client.status("rn07-m1").await.unwrap().status,
        DeliveryReceiptStatus::Delivered
    );
    let mut sub = client.subscribe(Some(last)).await.unwrap();
    let e = tokio::time::timeout(Duration::from_secs(5), sub.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(e.kind, RnEventKind::ResyncRequired);
    drop(sub);
    drop(client);

    // Core stops: so does the RN it started.
    rn.shutdown().await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(
        RnClient::connect(&home, &ClientOptions::default())
            .await
            .is_err()
    );
    api.shutdown().await.unwrap();
}

fn claude_task(id: &str, objective: &str) -> TaskSpec {
    let mut task = fixtures::task_spec();
    task.id = id.into();
    task.agent.provider = "claude".into();
    task.permission_profile_id = "phase0".into();
    task.workspace.mode = WorkspaceMode::Directory;
    task.workspace.workspace_id = Some("ws-rn".into());
    task.workspace.read_only = false;
    task.objective = objective.into();
    task
}

/// RN 장애와 Managed Run의 독립성 (§12.3): 실제 Claude Run 중 RN을 강제 종료해도 조회·취소·이벤트·종료 확인이
/// 그대로이고, RN은 Degraded를 거쳐 다시 올라옵니다.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "runs the real telos-rn and the real Claude CLI (haiku)"]
async fn a_managed_run_goes_on_and_cancels_while_rn_is_down() {
    let exe = rn_executable();
    let root = temp("independent");
    let policy = policy(&root, Some(&exe), true);
    let execution = execution(Some(&policy)).await;
    assert!(
        execution
            .discovery("claude")
            .and_then(|d| d.found())
            .is_some(),
        "{:?}",
        execution.notes
    );
    let api = server(&root, &execution).await;
    let rn = execution.start_rn(api.token_path()).await;
    wait_component(&execution, HealthStatus::Ok, 15).await;
    let token = std::fs::read_to_string(api.token_path()).unwrap();
    let mut client = Client::connect(api.endpoint()).await;
    client.hello(token.trim()).await;

    let task = claude_task(
        "rn07-independent",
        "Write the numbers from 1 to 1500, one per line, with no other text. Do not use tools.",
    );
    let started = client
        .call("runs.start", json!({"task": task, "session": {"mode": "fresh"}, "idempotencyKey": "rn07-independent:1"}))
        .await;
    let run_id = started["result"]["runId"]
        .as_str()
        .unwrap_or_else(|| panic!("{started}"))
        .to_owned();
    let subscription = client.subscribe(&run_id).await;
    // Wait for the session, then take RN down mid-run.
    loop {
        let m = client.receive(Duration::from_secs(120)).await;
        if m["method"] == "runs.event" {
            client.notifications.push(m.clone());
            if m["params"]["event"]["kind"] == "session_started" {
                break;
            }
        }
    }
    kill(rn.owned_pid().unwrap());
    wait_component(&execution, HealthStatus::Degraded, 10).await;
    let health = client.call("core.health", json!({})).await;
    let row = |name: &str| {
        health["result"]["components"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == name)
            .unwrap()
            .clone()
    };
    println!("core.health while RN is down: {}", health["result"]);
    assert_eq!(row("rn")["status"], "degraded");
    assert_eq!(row("execution")["status"], "ok");
    // The run is still tracked, and still running two seconds on.
    let inspect = client.call("runs.inspect", json!({"runId": run_id})).await;
    assert_eq!(
        inspect["result"]["status"],
        json!(RunStatus::Running),
        "{inspect}"
    );
    tokio::time::sleep(Duration::from_secs(2)).await;
    let inspect = client.call("runs.inspect", json!({"runId": run_id})).await;
    assert_eq!(
        inspect["result"]["status"],
        json!(RunStatus::Running),
        "{inspect}"
    );
    // Cancel works without RN, and its end is confirmed.
    let cancel = client.call("runs.cancel", json!({"runId": run_id})).await;
    assert_eq!(cancel["result"]["accepted"], true, "{cancel}");
    let events: Vec<Value> = client
        .events_until_end(&run_id, &subscription, Duration::from_secs(120))
        .await;
    for e in &events {
        println!("  {:>3} {} {}", e["sequence"], e["kind"], e["payload"]);
    }
    if let Some(id) = events.iter().find_map(|e| e["nativeSessionId"].as_str()) {
        println!("claude-session: {id}");
    }
    let finished = events
        .iter()
        .find(|e| e["kind"] == "turn_finished")
        .unwrap();
    assert_eq!(finished["payload"]["status"], "cancelled");
    let exited = events
        .iter()
        .find(|e| e["kind"] == "process_exited")
        .unwrap();
    assert_eq!(exited["payload"]["childrenReaped"], true);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let inspect = client.call("runs.inspect", json!({"runId": run_id})).await;
        if inspect["result"]["status"] == json!(RunStatus::Cancelled) {
            println!("after cancel: {inspect}");
            break;
        }
        assert!(tokio::time::Instant::now() < deadline, "{inspect}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    // RN came back on its own meanwhile, and stops with Core.
    let detail = wait_component(&execution, HealthStatus::Ok, 20).await;
    println!("rn after the run: {detail}");
    let rn: Arc<_> = rn;
    rn.shutdown().await;
    api.shutdown().await.unwrap();
    let report = execution.shutdown(Duration::from_secs(15)).await;
    assert!(report.unconfirmed.is_empty());
}

#[path = "fixtures/host/support.rs"]
mod support;
use serde_json::{Value, json};
use std::time::Duration;
use support::*;
use telos_core::api::{ApiServer, ApiServerConfig};
use telos_schema::api::*;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[cfg(windows)]
type Stream = tokio::net::windows::named_pipe::NamedPipeClient;
#[cfg(unix)]
type Stream = tokio::net::UnixStream;
struct Client {
    read: BufReader<tokio::io::ReadHalf<Stream>>,
    write: tokio::io::WriteHalf<Stream>,
}
impl Client {
    async fn connect(endpoint: &str) -> Self {
        #[cfg(windows)]
        let stream = loop {
            match tokio::net::windows::named_pipe::ClientOptions::new().open(endpoint) {
                Ok(stream) => break stream,
                Err(e) if e.raw_os_error() == Some(231) => {
                    tokio::time::sleep(Duration::from_millis(10)).await
                }
                Err(e) => panic!("connect: {e}"),
            }
        };
        #[cfg(unix)]
        let stream = tokio::net::UnixStream::connect(endpoint).await.unwrap();
        let (read, write) = tokio::io::split(stream);
        Self {
            read: BufReader::new(read),
            write,
        }
    }
    async fn send(&mut self, id: &str, method: &str, params: Value) {
        self.write.write_all(format!("{}\n",json!({"protocolVersion":API_PROTOCOL_VERSION,"requestId":id,"method":method,"params":params})).as_bytes()).await.unwrap();
    }
    async fn receive(&mut self) -> Value {
        let mut line = String::new();
        assert!(
            tokio::time::timeout(Duration::from_secs(3), self.read.read_line(&mut line))
                .await
                .unwrap()
                .unwrap()
                > 0
        );
        serde_json::from_str(&line).unwrap()
    }
    async fn hello(&mut self, token: &str) {
        self.send(
            "hello",
            "core.hello",
            json!({"clientName":"fixture","clientVersion":"0","token":token}),
        )
        .await;
        assert!(self.receive().await.get("result").is_some());
    }
}
fn config(temporary: &Temporary) -> ApiServerConfig {
    ApiServerConfig {
        project_root: temporary.root.join("project"),
        state_base: Some(temporary.root.join("state")),
        runtime_base: Some(temporary.root.join("runtime")),
    }
}

// Spec §12.3, REQ-019: dynamic components extend the existing Health contract.
#[test]
fn registered_health_components_preserve_builtins_and_refresh_their_status() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    let (execution, _) = supervisor();
    let baseline = execution.health();
    let state = Arc::new(AtomicUsize::new(0));
    let current = state.clone();
    execution.add_health_component(Arc::new(move || ComponentHealth {
        name: "rn".into(),
        status: [
            HealthStatus::Ok,
            HealthStatus::Degraded,
            HealthStatus::Unavailable,
        ][current.load(Ordering::SeqCst)],
        detail: Some(format!("state {}", current.load(Ordering::SeqCst))),
    }));
    execution.add_health_component(Arc::new(|| ComponentHealth {
        name: "provider:fixture".into(),
        status: HealthStatus::Ok,
        detail: None,
    }));
    for (value, expected) in [
        HealthStatus::Ok,
        HealthStatus::Degraded,
        HealthStatus::Unavailable,
    ]
    .into_iter()
    .enumerate()
    {
        state.store(value, Ordering::SeqCst);
        let health = execution.health();
        assert_eq!(health.status, baseline.status);
        assert_eq!(health.server_version, baseline.server_version);
        assert_eq!(&health.components[..3], baseline.components.as_slice());
        assert_eq!(health.components.len(), 5);
        assert_eq!(health.components[3].name, "rn");
        assert_eq!(health.components[3].status, expected);
        assert_eq!(health.components[3].detail, Some(format!("state {value}")));
        assert_eq!(health.components[4].name, "provider:fixture");
    }
}

// Spec §3.3·§12.3, REQ-019: use the authenticated API, not just a direct Rust call.
#[tokio::test(flavor = "multi_thread")]
async fn local_api_refreshes_an_added_rn_component_without_changing_default_health() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    let temporary = Temporary::new();
    let (execution, _) = supervisor();
    let baseline = execution.health();
    let server = ApiServer::start(config(&temporary), execution.clone())
        .await
        .unwrap();
    let token = std::fs::read_to_string(server.token_path()).unwrap();
    let mut client = Client::connect(server.endpoint()).await;
    client.hello(&token).await;
    let connected = Arc::new(AtomicBool::new(true));
    let current = connected.clone();
    execution.add_health_component(Arc::new(move || ComponentHealth {
        name: "rn".into(),
        status: if current.load(Ordering::SeqCst) {
            HealthStatus::Ok
        } else {
            HealthStatus::Degraded
        },
        detail: Some("fixture RN".into()),
    }));
    for (value, expected) in [
        (true, HealthStatus::Ok),
        (false, HealthStatus::Degraded),
        (true, HealthStatus::Ok),
    ] {
        connected.store(value, Ordering::SeqCst);
        client.send("health", "core.health", json!({})).await;
        let response = client.receive().await;
        let health: HealthResult = serde_json::from_value(response["result"].clone()).unwrap();
        assert_eq!(health.status, baseline.status);
        assert_eq!(&health.components[..3], baseline.components.as_slice());
        assert_eq!(health.components.len(), 4);
        assert_eq!(health.components[3].name, "rn");
        assert_eq!(health.components[3].status, expected);
    }
    server.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn authentication_health_unknown_method_and_token_rotation() {
    let temporary = Temporary::new();
    let (execution, _) = supervisor();
    let server = ApiServer::start(config(&temporary), execution.clone())
        .await
        .unwrap();
    let token = std::fs::read_to_string(server.token_path()).unwrap();
    assert_eq!(token.len(), 64);
    assert!(
        token
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    );
    assert!(
        !server
            .token_path()
            .starts_with(temporary.root.join("project"))
    );
    let mut client = Client::connect(server.endpoint()).await;
    client.send("health", "core.health", json!({})).await;
    assert_eq!(client.receive().await["error"]["code"], "unauthenticated");
    client
        .send(
            "bad",
            "core.hello",
            json!({"clientName":"fixture","clientVersion":"0","token":"wrong"}),
        )
        .await;
    let rejected = client.receive().await;
    assert_eq!(rejected["error"]["code"], "unauthenticated");
    assert!(!rejected.to_string().contains(&token));
    client.hello(&token).await;
    client
        .send(
            "duplicate-hello",
            "core.hello",
            json!({"clientName":"fixture","clientVersion":"0","token":token}),
        )
        .await;
    assert_eq!(client.receive().await["error"]["code"], "conflict");
    client.write.write_all(format!("{}\n",json!({"protocolVersion":API_PROTOCOL_VERSION,"requestId":"missing-params","method":"core.health"})).as_bytes()).await.unwrap();
    assert_eq!(client.receive().await["error"]["code"], "invalid_request");
    client.send("health", "core.health", json!({})).await;
    assert_eq!(client.receive().await["result"]["status"], "degraded");
    client.send("unknown", "other.method", json!({})).await;
    assert_eq!(
        client.receive().await["error"]["code"],
        "unsupported_method"
    );
    assert!(
        ApiServer::start(config(&temporary), execution.clone())
            .await
            .is_err()
    );
    assert_eq!(std::fs::read_to_string(server.token_path()).unwrap(), token);
    server.shutdown().await.unwrap();
    let restarted = ApiServer::start(config(&temporary), execution)
        .await
        .unwrap();
    let next = std::fs::read_to_string(restarted.token_path()).unwrap();
    assert_ne!(token, next);
    let mut client = Client::connect(restarted.endpoint()).await;
    client
        .send(
            "stale",
            "core.hello",
            json!({"clientName":"fixture","clientVersion":"0","token":token}),
        )
        .await;
    assert_eq!(client.receive().await["error"]["code"], "unauthenticated");
    client.hello(&next).await;
    restarted.shutdown().await.unwrap();
}
#[tokio::test(flavor = "multi_thread")]
async fn api_replays_finished_run_and_unsubscribes_live_run() {
    let temporary = Temporary::new();
    let (execution, _) = supervisor();
    let server = ApiServer::start(config(&temporary), execution)
        .await
        .unwrap();
    let token = std::fs::read_to_string(server.token_path()).unwrap();
    let mut client = Client::connect(server.endpoint()).await;
    client.hello(&token).await;
    client
        .send("start", "runs.start", json!(params("ok", "api")))
        .await;
    let started = client.receive().await;
    let id = started["result"]["runId"].as_str().unwrap().to_owned();
    drop(client);
    let mut client = Client::connect(server.endpoint()).await;
    client.hello(&token).await;
    client
        .send(
            "sub",
            "runs.subscribe",
            json!({"runId":id,"afterSequence":2}),
        )
        .await;
    let acknowledgement = client.receive().await;
    assert_eq!(acknowledgement["requestId"], "sub");
    let subscription = acknowledgement["result"]["subscriptionId"].clone();
    for sequence in [3, 4] {
        let event = client.receive().await;
        assert_eq!(event["method"], "runs.event");
        assert_eq!(event["params"]["subscriptionId"], subscription);
        assert_eq!(event["params"]["event"]["sequence"], sequence);
    }
    assert_eq!(client.receive().await["params"]["reason"], "run_finished");
    client
        .send("inspect", "runs.inspect", json!({"runId":id}))
        .await;
    assert_eq!(client.receive().await["result"]["status"], "Succeeded");
    client
        .send("start2", "runs.start", json!(params("hold", "api-hold")))
        .await;
    let id = client.receive().await["result"]["runId"].clone();
    drop(client);
    let mut client = Client::connect(server.endpoint()).await;
    client.hello(&token).await;
    client
        .send("still-running", "runs.inspect", json!({"runId":id}))
        .await;
    assert_eq!(client.receive().await["result"]["status"], "Running");
    client
        .send(
            "sub2",
            "runs.subscribe",
            json!({"runId":id,"afterSequence":0}),
        )
        .await;
    let sub = client.receive().await["result"]["subscriptionId"].clone();
    client
        .send("unsub", "runs.unsubscribe", json!({"subscriptionId":sub}))
        .await;
    assert_eq!(client.receive().await["params"]["reason"], "unsubscribed");
    assert_eq!(client.receive().await["requestId"], "unsub");
    client
        .send("cancel", "runs.cancel", json!({"runId":id}))
        .await;
    assert_eq!(client.receive().await["result"]["accepted"], true);
    server.shutdown().await.unwrap();
}
#[tokio::test(flavor = "multi_thread")]
async fn oversized_frame_receives_required_error_then_eof() {
    let temporary = Temporary::new();
    let (execution, _) = supervisor();
    let server = ApiServer::start(config(&temporary), execution)
        .await
        .unwrap();
    let mut client = Client::connect(server.endpoint()).await;
    let mut bytes = vec![b'x'; MAX_FRAME_BYTES + 1];
    bytes.push(b'\n');
    // Full duplex: the server may reject and close before all bytes are written.
    let (sent, received) = tokio::join!(client.write.write_all(&bytes), async {
        let mut line = String::new();
        client.read.read_line(&mut line).await.unwrap();
        line
    });
    let _ = sent;
    let error: Value = serde_json::from_str(&received).unwrap();
    assert_eq!(error["requestId"], "");
    assert_eq!(error["error"]["code"], "frame_too_large");
    let mut line = String::new();
    assert_eq!(client.read.read_line(&mut line).await.unwrap(), 0);
    server.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn exact_frame_utf8_and_protocol_errors_preserve_the_connection() {
    let temporary = Temporary::new();
    let (execution, _) = supervisor();
    let server = ApiServer::start(config(&temporary), execution)
        .await
        .unwrap();
    let token = std::fs::read_to_string(server.token_path()).unwrap();
    let mut client = Client::connect(server.endpoint()).await;
    client.hello(&token).await;
    let mut request = json!({"protocolVersion":API_PROTOCOL_VERSION,"requestId":"exact","method":"core.health","params":{},"padding":""});
    let size = serde_json::to_vec(&request).unwrap().len();
    request["padding"] = Value::String("x".repeat(MAX_FRAME_BYTES - size));
    let frame = encode_frame(&request).unwrap();
    assert_eq!(frame.len(), MAX_FRAME_BYTES + 1);
    client.write.write_all(frame.as_bytes()).await.unwrap();
    assert_eq!(client.receive().await["requestId"], "exact");
    client.write.write_all(b"\xff\n").await.unwrap();
    assert_eq!(client.receive().await["error"]["code"], "invalid_request");
    client.write.write_all(format!("{}\n",json!({"protocolVersion":"other","requestId":"version","method":"core.health","params":{}})).as_bytes()).await.unwrap();
    assert_eq!(
        client.receive().await["error"]["code"],
        "unsupported_protocol_version"
    );
    let utf8 = format!(
        "{}\n",
        json!({"protocolVersion":API_PROTOCOL_VERSION,"requestId":"한글","method":"core.health","params":{}})
    );
    for byte in utf8.bytes() {
        client.write.write_all(&[byte]).await.unwrap();
    }
    assert_eq!(client.receive().await["requestId"], "한글");
    server.shutdown().await.unwrap();
}
#[tokio::test(flavor = "multi_thread")]
async fn credentials_cannot_be_placed_inside_the_project_or_replace_a_directory() {
    let temporary = Temporary::new();
    let (execution, _) = supervisor();
    let mut bad = config(&temporary);
    bad.state_base = Some(temporary.root.join("project/private"));
    assert!(ApiServer::start(bad, execution.clone()).await.is_err());
    assert!(!temporary.root.join("project/private").exists());
    let project = temporary.root.join("project").canonicalize().unwrap();
    let key = project_key(&normalize_project_root(
        project.to_str().unwrap(),
        cfg!(windows),
    ));
    let token = temporary.root.join("state").join(key).join("api-token");
    std::fs::create_dir_all(&token).unwrap();
    assert!(
        ApiServer::start(config(&temporary), execution)
            .await
            .is_err()
    );
    assert!(token.is_dir());
}

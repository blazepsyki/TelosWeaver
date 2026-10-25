//! `telos-rn-client`를 스크립트한 가짜 RN(실제 Pipe/Socket)에 붙여 시험합니다 (PH0-RN-07).
//! 실제 RN Fork Daemon과의 왕복은 `telos-core`의 `phase0_rn` 실환경 시험이 합니다.

use std::{path::PathBuf, time::Duration};

use serde_json::{Value, json};
use telos_rn_client::{
    ClientError, ClientOptions, RN_PROTOCOL_VERSION, RnClient, RnErrorCode, RnEventKind,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

#[cfg(windows)]
type Server = tokio::net::windows::named_pipe::NamedPipeServer;
#[cfg(unix)]
type Server = tokio::net::UnixStream;

/// 가짜 RN 하나: 상태 디렉터리(`rpc.json`·`rpc-token`)와 연결을 받는 Task.
struct Fake {
    home: PathBuf,
}

impl Drop for Fake {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

fn unique(what: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    format!(
        "telos-rn-client-{what}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    )
}

/// 연결 하나를 받아 `script`에 넘기는 가짜 RN. `pid`는 `rpc.json`에 적을 값입니다.
async fn fake<F, Fut>(pid: u32, script: F) -> Fake
where
    F: FnOnce(Server) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send,
{
    let home = std::env::temp_dir().join(unique("home"));
    std::fs::create_dir_all(&home).unwrap();
    #[cfg(windows)]
    let (endpoint, listener) = {
        let name = format!(r"\\.\pipe\{}", unique("pipe"));
        let server = tokio::net::windows::named_pipe::ServerOptions::new()
            .first_pipe_instance(true)
            .create(&name)
            .unwrap();
        (name, server)
    };
    #[cfg(unix)]
    let (endpoint, listener) = {
        let path = home.join("rpc.sock");
        (
            path.display().to_string(),
            tokio::net::UnixListener::bind(&path).unwrap(),
        )
    };
    std::fs::write(home.join("rpc-token"), TOKEN).unwrap();
    std::fs::write(
        home.join("rpc.json"),
        json!({"endpoint": endpoint, "rpcVersion": "0.1", "pid": pid}).to_string(),
    )
    .unwrap();
    tokio::spawn(async move {
        #[cfg(windows)]
        let conn = {
            listener.connect().await.unwrap();
            listener
        };
        #[cfg(unix)]
        let conn = listener.accept().await.unwrap().0;
        script(conn).await;
    });
    Fake { home }
}

/// 가짜 RN 쪽에서 요청을 읽고 답하는 도구.
struct Peer {
    lines: tokio::io::Lines<BufReader<tokio::io::ReadHalf<Server>>>,
    wr: tokio::io::WriteHalf<Server>,
}

impl Peer {
    fn new(conn: Server) -> Self {
        let (rd, wr) = tokio::io::split(conn);
        Peer {
            lines: BufReader::new(rd).lines(),
            wr,
        }
    }

    async fn request(&mut self) -> Value {
        serde_json::from_str(&self.lines.next_line().await.unwrap().unwrap()).unwrap()
    }

    async fn write(&mut self, frames: &[Value]) {
        let text: String = frames.iter().map(|f| format!("{f}\n")).collect();
        self.wr.write_all(text.as_bytes()).await.unwrap();
    }

    async fn ok(&mut self, id: &Value, result: Value) {
        self.write(&[
            json!({"protocolVersion": RN_PROTOCOL_VERSION, "requestId": id, "result": result}),
        ])
        .await;
    }

    /// `rn.hello`에 답합니다.
    async fn hello(&mut self) {
        let r = self.request().await;
        assert_eq!(r["method"], "rn.hello");
        assert_eq!(r["params"]["token"], TOKEN);
        let endpoint = json!({"hostId": "h", "provider": "telosweaver", "nativeSessionId": "core", "fingerprint": "f"});
        self.ok(
            &r["requestId"],
            json!({"rpcVersion": "0.1", "hostId": "h", "connectionId": "c1", "endpoint": endpoint}),
        )
        .await;
    }
}

fn options() -> ClientOptions {
    ClientOptions {
        request_timeout: Duration::from_millis(500),
        ..ClientOptions::default()
    }
}

#[tokio::test]
async fn it_says_hello_with_the_token_and_maps_refusals() {
    let rn = fake(std::process::id(), |conn| async move {
        let mut p = Peer::new(conn);
        p.hello().await;
        let r = p.request().await;
        assert_eq!(r["method"], "health.get");
        p.ok(&r["requestId"], json!({"status": "ok", "rpcVersion": "0.1", "upstreamCommit": "u", "forkCommit": "f", "hostId": "h"}))
            .await;
        let r = p.request().await;
        assert_eq!((r["method"].as_str(), r["params"]["messageId"].as_str()), (Some("messages.status"), Some("m-9")));
        p.write(&[json!({"protocolVersion": "0.1", "requestId": r["requestId"],
                          "error": {"code": "not_found", "message": "no message m-9 here"}})])
            .await;
        tokio::time::sleep(Duration::from_secs(5)).await;
    })
    .await;
    let c = RnClient::connect(&rn.home, &options()).await.unwrap();
    assert_eq!(
        (
            c.hello().host_id.as_str(),
            c.hello().endpoint.provider.as_str()
        ),
        ("h", "telosweaver")
    );
    assert_eq!(c.health().await.unwrap().fork_commit, "f");
    match c.status("m-9").await {
        Err(ClientError::Rejected(e)) => assert_eq!(e.code, RnErrorCode::NotFound),
        other => panic!("{:?}", other.map(|_| ())),
    }
}

#[tokio::test]
async fn a_slow_answer_times_out_and_the_connection_carries_on() {
    let rn = fake(std::process::id(), |conn| async move {
        let mut p = Peer::new(conn);
        p.hello().await;
        let slow = p.request().await;
        let next = p.request().await;
        // The late answer to the first, then the second's.
        p.ok(&slow["requestId"], json!({"status": "ok", "rpcVersion": "0.1", "upstreamCommit": "late", "forkCommit": "x", "hostId": "h"}))
            .await;
        p.ok(&next["requestId"], json!([])).await;
        tokio::time::sleep(Duration::from_secs(5)).await;
    })
    .await;
    let c = RnClient::connect(&rn.home, &options()).await.unwrap();
    assert_eq!(c.health().await.unwrap_err(), ClientError::Timeout);
    assert!(c.providers().await.unwrap().is_empty());
    assert!(!c.is_closed());
}

#[tokio::test]
async fn events_in_the_same_chunk_as_the_subscribe_answer_reach_the_subscription() {
    let rn = fake(std::process::id(), |conn| async move {
        let mut p = Peer::new(conn);
        p.hello().await;
        let r = p.request().await;
        assert_eq!(r["method"], "messages.subscribe");
        assert_eq!(r["params"]["cursor"], "7:3");
        let event = |seq: u64, sub: &str| {
            json!({"protocolVersion": "0.1", "method": "messages.event", "params": {"subscriptionId": sub,
                   "event": {"cursor": format!("7:{seq}"), "sequence": seq, "kind": "receipt", "payload": {}}}})
        };
        // One write: the answer, its replay, and an event of another subscription in between.
        p.write(&[
            json!({"protocolVersion": "0.1", "requestId": r["requestId"], "result": {"subscriptionId": "s1"}}),
            event(4, "s1"),
            event(5, "other"),
            event(6, "s1"),
        ])
        .await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        // Then it goes away.
    })
    .await;
    let c = RnClient::connect(&rn.home, &options()).await.unwrap();
    let mut sub = c.subscribe(Some("7:3".into())).await.unwrap();
    assert_eq!(sub.id, "s1");
    let first = sub.next().await.unwrap();
    assert_eq!((first.sequence, first.kind), (4, RnEventKind::Receipt));
    assert_eq!(sub.next().await.unwrap().sequence, 6);
    // Its connection closed: the subscription ends, and so does the client.
    assert!(sub.next().await.is_none());
    assert!(c.is_closed());
    assert_eq!(c.health().await.unwrap_err(), ClientError::Closed);
}

#[tokio::test]
async fn a_frame_over_the_limit_or_out_of_contract_closes_the_connection() {
    let rn = fake(std::process::id(), |conn| async move {
        let mut p = Peer::new(conn);
        p.hello().await;
        let _ = p.request().await;
        p.wr.write_all(&vec![b'a'; telos_schema::api::MAX_FRAME_BYTES + 1])
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_secs(2)).await;
    })
    .await;
    let c = RnClient::connect(&rn.home, &options()).await.unwrap();
    // The waiting request hears why; later ones that it is closed.
    assert_eq!(
        c.health().await.unwrap_err(),
        ClientError::Protocol("a frame over the limit".into())
    );
    assert!(c.is_closed());
    assert_eq!(c.providers().await.unwrap_err(), ClientError::Closed);
}

#[tokio::test]
async fn no_rn_or_one_whose_pipe_isnt_the_engines_is_unavailable() {
    let empty = std::env::temp_dir().join(unique("empty"));
    std::fs::create_dir_all(&empty).unwrap();
    assert!(matches!(
        RnClient::connect(&empty, &options()).await,
        Err(ClientError::Unavailable(_))
    ));
    // Retries a few times, then says why.
    let started = tokio::time::Instant::now();
    assert!(matches!(
        RnClient::connect_retrying(&empty, &options(), 3).await,
        Err(ClientError::Unavailable(_))
    ));
    assert!(started.elapsed() >= Duration::from_millis(750));
    let _ = std::fs::remove_dir_all(&empty);
    // A token that isn't one.
    let rn = fake(std::process::id(), |_| async {}).await;
    std::fs::write(rn.home.join("rpc-token"), "nope").unwrap();
    assert!(matches!(
        RnClient::connect(&rn.home, &options()).await,
        Err(ClientError::Unavailable(_))
    ));
    #[cfg(windows)]
    {
        // Someone else's pipe under that name (`rpc.json` names another process).
        let rn = fake(1, |_| async {
            tokio::time::sleep(Duration::from_secs(2)).await
        })
        .await;
        match RnClient::connect(&rn.home, &options()).await {
            Err(ClientError::Unavailable(why)) => {
                assert!(why.contains("not held by the RN"), "{why}")
            }
            other => panic!("{:?}", other.map(|_| ())),
        }
    }
}

#[tokio::test]
async fn a_refused_hello_is_not_retried() {
    let rn = fake(std::process::id(), |conn| async move {
        let mut p = Peer::new(conn);
        let r = p.request().await;
        p.write(&[json!({"protocolVersion": "0.1", "requestId": r["requestId"],
                          "error": {"code": "unauthenticated", "message": "the token isn't this engine's"}})])
            .await;
        tokio::time::sleep(Duration::from_secs(1)).await;
    })
    .await;
    match RnClient::connect_retrying(&rn.home, &options(), 5).await {
        Err(ClientError::Rejected(e)) => assert_eq!(e.code, RnErrorCode::Unauthenticated),
        other => panic!("{:?}", other.map(|_| ())),
    }
}

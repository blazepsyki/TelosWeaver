//! Phase 0 통합 시험용 Local API 클라이언트 (NDJSON, Spec §3.3·G0 Local API 계약).

use std::time::Duration;

use serde_json::{Value, json};
use telos_schema::api::API_PROTOCOL_VERSION;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[cfg(windows)]
type Stream = tokio::net::windows::named_pipe::NamedPipeClient;
#[cfg(unix)]
type Stream = tokio::net::UnixStream;

pub struct Client {
    read: BufReader<tokio::io::ReadHalf<Stream>>,
    write: tokio::io::WriteHalf<Stream>,
    next: u64,
    /// 응답을 기다리는 동안 먼저 도착한 알림입니다.
    pub notifications: Vec<Value>,
}

impl Client {
    pub async fn connect(endpoint: &str) -> Self {
        #[cfg(windows)]
        let stream = loop {
            match tokio::net::windows::named_pipe::ClientOptions::new().open(endpoint) {
                Ok(stream) => break stream,
                // ERROR_PIPE_BUSY
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
            next: 0,
            notifications: Vec::new(),
        }
    }

    pub async fn receive(&mut self, timeout: Duration) -> Value {
        let mut line = String::new();
        let read = tokio::time::timeout(timeout, self.read.read_line(&mut line))
            .await
            .expect("message within timeout")
            .unwrap();
        assert!(read > 0, "connection closed");
        serde_json::from_str(&line).unwrap()
    }

    /// 요청을 보내고 같은 `requestId`의 응답을 돌려줍니다.
    pub async fn call(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        let id = format!("req-{}", self.next);
        let frame = json!({
            "protocolVersion": API_PROTOCOL_VERSION,
            "requestId": id,
            "method": method,
            "params": params,
        });
        self.write
            .write_all(format!("{frame}\n").as_bytes())
            .await
            .unwrap();
        loop {
            let message = self.receive(Duration::from_secs(60)).await;
            if message.get("requestId").and_then(Value::as_str) == Some(id.as_str()) {
                return message;
            }
            self.notifications.push(message);
        }
    }

    pub async fn hello(&mut self, token: &str) -> Value {
        let response = self
            .call(
                "core.hello",
                json!({"clientName": "phase0-g1", "clientVersion": "0", "token": token}),
            )
            .await;
        assert!(response.get("result").is_some(), "hello failed: {response}");
        response
    }

    /// `runs.event` 알림을 종료 이벤트까지 모읍니다.
    pub async fn events_until_end(&mut self, timeout: Duration) -> Vec<Value> {
        let deadline = tokio::time::Instant::now() + timeout;
        let mut events = Vec::new();
        let mut pending: Vec<Value> = std::mem::take(&mut self.notifications);
        loop {
            let message = match pending.is_empty() {
                false => pending.remove(0),
                true => {
                    let left = deadline.saturating_duration_since(tokio::time::Instant::now());
                    self.receive(left).await
                }
            };
            match message.get("method").and_then(Value::as_str) {
                Some("runs.event") => {
                    let event = message["params"]["event"].clone();
                    let kind = event["kind"].as_str().unwrap_or_default().to_owned();
                    let terminal = kind == "process_exited"
                        || (kind == "error" && event["payload"]["terminal"] == true);
                    events.push(event);
                    if terminal {
                        return events;
                    }
                }
                Some("runs.subscriptionClosed") => return events,
                _ => {}
            }
        }
    }
}

//! Reason Network Fork Local RPC 클라이언트 (Spec §12.1, PH0-RN-07). 소유: PH0-RN.
//!
//! RPC 메시지 타입은 Core와 RN의 경계 계약이므로 `telos_schema::rn`에 정의하고 여기서 재노출합니다 (§3.4).
//!
//! - [`RnClient::connect`]: RN 상태 디렉터리의 `rpc.json`·`rpc-token`을 읽고 Pipe/Socket에 접속해 `rn.hello`로
//!   인증합니다. Windows에서는 Pipe 서버 프로세스가 `rpc.json`의 PID인지 확인합니다(누구나 같은 이름의 Pipe를
//!   만들 수 있음).
//! - 요청마다 [`ClientOptions::request_timeout`]을 둡니다. 시간이 지나도 연결은 유지하고 늦은 응답은 버립니다.
//!   응답을 기다리는 요청은 32개까지입니다.
//! - [`RnClient::connect_retrying`]: 접속에 실패하면 250 ms부터 두 배씩 늘려 정한 횟수만 다시 시도합니다(제한된
//!   재시도). 끊긴 연결을 다시 잇는 일은 호출자(Core 통합 구성)가 이 함수로 하고, 구독은 마지막 커서로 이어 갑니다.
//! - 구독 응답을 해석하는 그 자리에서 구독 ID를 연결합니다. RN은 한 구독의 알림을 그 응답 뒤에만 보내므로
//!   (REQ-018) 같은 데이터 묶음에 뒤따르는 알림도 놓치지 않습니다.
//! - 전달 상태는 Task 완료를 뜻하지 않습니다 (DELEG-09).

pub use telos_schema::rn::*;

use std::{
    collections::HashMap,
    fmt,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use telos_schema::{SecretString, api::MAX_FRAME_BYTES};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

/// 요청 하나의 기본 제한 시간입니다.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
/// 한 연결에서 응답을 기다리는 요청의 상한입니다.
const MAX_PENDING: usize = 32;
/// 소비자가 아직 가져가지 않은 한 구독의 이벤트 상한. 넘으면 그 구독을 닫습니다(커서로 다시 구독).
const SUBSCRIPTION_BUFFER: usize = 1024;

#[derive(Debug, Clone, PartialEq)]
pub enum ClientError {
    /// RN이 없거나 접속할 수 없습니다(`rpc.json`·토큰 없음, 접속 실패, 다른 프로세스의 Pipe).
    Unavailable(String),
    /// 제한 시간 안에 응답이 없습니다. 연결은 유지됩니다.
    Timeout,
    /// RN이 계약에 맞지 않는 프레임을 보냈습니다. 연결을 닫습니다.
    Protocol(String),
    /// 연결이 닫혔습니다.
    Closed,
    /// RN이 요청을 거부했습니다(계약 오류 코드).
    Rejected(RnError),
    /// 응답을 기다리는 요청이 상한에 도달했습니다.
    Busy,
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(why) => write!(f, "RN unavailable: {why}"),
            Self::Timeout => write!(f, "RN did not answer in time"),
            Self::Protocol(why) => write!(f, "RN protocol error: {why}"),
            Self::Closed => write!(f, "RN connection closed"),
            Self::Rejected(e) => write!(f, "RN refused ({:?}): {}", e.code, e.message),
            Self::Busy => write!(f, "too many RN requests waiting"),
        }
    }
}

impl std::error::Error for ClientError {}

#[derive(Debug, Clone)]
pub struct ClientOptions {
    pub client_name: String,
    pub client_version: String,
    pub request_timeout: Duration,
}

impl Default for ClientOptions {
    fn default() -> Self {
        Self {
            client_name: "telos-weaver".into(),
            client_version: env!("CARGO_PKG_VERSION").into(),
            request_timeout: REQUEST_TIMEOUT,
        }
    }
}

/// RN 상태 디렉터리의 `rpc.json`을 읽습니다.
pub fn rpc_info(home: &Path) -> Result<RnRpcInfo, ClientError> {
    let text = std::fs::read_to_string(home.join("rpc.json"))
        .map_err(|e| ClientError::Unavailable(format!("rpc.json: {e}")))?;
    serde_json::from_str(&text).map_err(|e| ClientError::Unavailable(format!("rpc.json: {e}")))
}

#[cfg(windows)]
type Stream = tokio::net::windows::named_pipe::NamedPipeClient;
#[cfg(unix)]
type Stream = tokio::net::UnixStream;

enum Waiter {
    Plain(oneshot::Sender<Result<Value, ClientError>>),
    /// 응답을 해석하면서 구독 ID와 이벤트 통로를 만듭니다.
    Subscribe(oneshot::Sender<Result<Subscription, ClientError>>),
}

struct Shared {
    writer: tokio::sync::Mutex<tokio::io::WriteHalf<Stream>>,
    pending: Mutex<HashMap<String, Waiter>>,
    subscriptions: Mutex<HashMap<String, mpsc::Sender<RnEvent>>>,
    next: AtomicU64,
    closed: AtomicBool,
}

impl Shared {
    fn close(&self, error: ClientError) {
        self.closed.store(true, Ordering::Release);
        for (_, waiter) in self.pending.lock().unwrap().drain() {
            match waiter {
                Waiter::Plain(tx) => drop(tx.send(Err(error.clone()))),
                Waiter::Subscribe(tx) => drop(tx.send(Err(error.clone()))),
            }
        }
        self.subscriptions.lock().unwrap().clear();
    }
}

/// RN RPC 연결 하나입니다. 버리면 연결을 닫습니다.
pub struct RnClient {
    shared: Arc<Shared>,
    hello: Option<RnHelloResult>,
    info: RnRpcInfo,
    timeout: Duration,
    reader: JoinHandle<()>,
}

impl Drop for RnClient {
    fn drop(&mut self) {
        self.reader.abort();
    }
}

/// 구독 하나의 이벤트입니다. 연결이 닫히면 `next`가 `None`을 돌려줍니다.
pub struct Subscription {
    pub id: String,
    events: mpsc::Receiver<RnEvent>,
}

impl Subscription {
    pub async fn next(&mut self) -> Option<RnEvent> {
        self.events.recv().await
    }
}

impl RnClient {
    /// `home`(RN 상태 디렉터리)의 RN에 접속해 인증합니다.
    pub async fn connect(home: &Path, options: &ClientOptions) -> Result<Self, ClientError> {
        let info = rpc_info(home)?;
        let token = std::fs::read_to_string(home.join("rpc-token"))
            .map_err(|e| ClientError::Unavailable(format!("rpc-token: {e}")))?;
        let token = token.trim();
        if token.len() != 64
            || !token
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(ClientError::Unavailable("rpc-token is not a token".into()));
        }
        let stream = open(&info).await?;
        let (rd, wr) = tokio::io::split(stream);
        let shared = Arc::new(Shared {
            writer: tokio::sync::Mutex::new(wr),
            pending: Mutex::new(HashMap::new()),
            subscriptions: Mutex::new(HashMap::new()),
            next: AtomicU64::new(1),
            closed: AtomicBool::new(false),
        });
        let reader = tokio::spawn(read(shared.clone(), rd));
        let mut client = Self {
            shared,
            hello: None,
            info,
            timeout: options.request_timeout,
            reader,
        };
        let params = RnHelloParams {
            client_name: options.client_name.clone(),
            client_version: options.client_version.clone(),
            token: SecretString::new(token.to_owned()),
        };
        let hello: RnHelloResult = client.request(RnMethod::Hello, &params).await?;
        if hello.rpc_version != RN_PROTOCOL_VERSION {
            return Err(ClientError::Protocol(format!(
                "RN speaks RPC {}",
                hello.rpc_version
            )));
        }
        client.hello = Some(hello);
        Ok(client)
    }

    /// [`connect`](Self::connect)를 `attempts`번까지 시도합니다. 사이에는 250 ms부터 두 배씩(최대 4 s) 기다립니다.
    /// 인증 거부·계약 위반은 다시 시도하지 않습니다.
    pub async fn connect_retrying(
        home: &Path,
        options: &ClientOptions,
        attempts: u32,
    ) -> Result<Self, ClientError> {
        let mut last = ClientError::Unavailable("no attempt".into());
        for attempt in 0..attempts.max(1) {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_millis(250 << (attempt - 1).min(4))).await;
            }
            match Self::connect(home, options).await {
                Ok(client) => return Ok(client),
                Err(
                    e @ (ClientError::Unavailable(_) | ClientError::Timeout | ClientError::Closed),
                ) => last = e,
                Err(e) => return Err(e),
            }
        }
        Err(last)
    }

    /// `rn.hello`의 결과(Host, Core 메일함 Endpoint).
    pub fn hello(&self) -> &RnHelloResult {
        self.hello
            .as_ref()
            .expect("a connected client has said hello")
    }

    /// 접속한 RN의 `rpc.json`.
    pub fn info(&self) -> &RnRpcInfo {
        &self.info
    }

    pub fn is_closed(&self) -> bool {
        self.shared.closed.load(Ordering::Acquire)
    }

    /// 메서드 하나를 부르고 결과를 `T`로 읽습니다.
    pub async fn request<T: DeserializeOwned>(
        &self,
        method: RnMethod,
        params: &impl Serialize,
    ) -> Result<T, ClientError> {
        let (tx, rx) = oneshot::channel();
        let value = self.call(method, params, Waiter::Plain(tx), rx).await?;
        serde_json::from_value(value)
            .map_err(|e| ClientError::Protocol(format!("{method:?} result: {e}")))
    }

    async fn call<R>(
        &self,
        method: RnMethod,
        params: &impl Serialize,
        waiter: Waiter,
        rx: oneshot::Receiver<Result<R, ClientError>>,
    ) -> Result<R, ClientError> {
        if self.is_closed() {
            return Err(ClientError::Closed);
        }
        let id = format!("c{}", self.shared.next.fetch_add(1, Ordering::Relaxed));
        let request = RnRequest {
            protocol_version: RN_PROTOCOL_VERSION.into(),
            request_id: id.clone(),
            method,
            params: serde_json::to_value(params)
                .map_err(|e| ClientError::Protocol(e.to_string()))?,
        };
        let mut line =
            serde_json::to_string(&request).map_err(|e| ClientError::Protocol(e.to_string()))?;
        if line.len() > MAX_FRAME_BYTES {
            return Err(ClientError::Protocol("request frame too large".into()));
        }
        line.push('\n');
        {
            let mut pending = self.shared.pending.lock().unwrap();
            if pending.len() >= MAX_PENDING {
                return Err(ClientError::Busy);
            }
            pending.insert(id.clone(), waiter);
        }
        let written = self
            .shared
            .writer
            .lock()
            .await
            .write_all(line.as_bytes())
            .await;
        if written.is_err() {
            self.shared.pending.lock().unwrap().remove(&id);
            self.shared.close(ClientError::Closed);
            return Err(ClientError::Closed);
        }
        match tokio::time::timeout(self.timeout, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(ClientError::Closed),
            Err(_) => {
                self.shared.pending.lock().unwrap().remove(&id);
                Err(ClientError::Timeout)
            }
        }
    }

    pub async fn health(&self) -> Result<RnHealth, ClientError> {
        self.request(RnMethod::HealthGet, &json!({})).await
    }

    pub async fn providers(&self) -> Result<Vec<ProviderConnector>, ClientError> {
        self.request(RnMethod::ProvidersList, &json!({})).await
    }

    pub async fn discover(
        &self,
        query: &SessionQuery,
    ) -> Result<Vec<SessionEndpoint>, ClientError> {
        self.request(RnMethod::SessionsDiscover, query).await
    }

    pub async fn resolve(&self, endpoint: &EndpointRef) -> Result<SessionEndpoint, ClientError> {
        self.request(RnMethod::SessionsResolve, endpoint).await
    }

    pub async fn capabilities(
        &self,
        endpoint: &EndpointRef,
    ) -> Result<SessionCapabilities, ClientError> {
        self.request(RnMethod::SessionsCapabilities, endpoint).await
    }

    pub async fn send(&self, envelope: &MessageEnvelope) -> Result<DeliveryReceipt, ClientError> {
        self.request(RnMethod::MessagesSend, envelope).await
    }

    pub async fn status(&self, message_id: &str) -> Result<DeliveryReceipt, ClientError> {
        self.request(
            RnMethod::MessagesStatus,
            &MessageRef {
                message_id: message_id.into(),
            },
        )
        .await
    }

    /// `cursor` 뒤의 이벤트부터 구독합니다(없으면 지금부터). 구독은 이 연결이 살아 있는 동안 이어집니다.
    pub async fn subscribe(&self, cursor: Option<String>) -> Result<Subscription, ClientError> {
        let (tx, rx) = oneshot::channel();
        self.call(
            RnMethod::MessagesSubscribe,
            &SubscribeParams { cursor },
            Waiter::Subscribe(tx),
            rx,
        )
        .await
    }
}

#[cfg(windows)]
async fn open(info: &RnRpcInfo) -> Result<Stream, ClientError> {
    use std::os::windows::io::AsRawHandle;
    use tokio::net::windows::named_pipe::ClientOptions;
    let mut tries = 0;
    let pipe = loop {
        match ClientOptions::new().open(&info.endpoint) {
            // 다음 인스턴스를 여는 순간에는 모든 인스턴스가 쓰이고 있습니다.
            Err(e)
                if e.raw_os_error()
                    == Some(windows_sys::Win32::Foundation::ERROR_PIPE_BUSY as i32)
                    && tries < 20 =>
            {
                tries += 1;
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Err(e) => return Err(ClientError::Unavailable(format!("{}: {e}", info.endpoint))),
            Ok(pipe) => break pipe,
        }
    };
    let mut server = 0;
    // SAFETY: 열린 Pipe 핸들의 서버 프로세스를 묻습니다.
    unsafe {
        windows_sys::Win32::System::Pipes::GetNamedPipeServerProcessId(
            pipe.as_raw_handle(),
            &mut server,
        )
    };
    if server != info.pid {
        return Err(ClientError::Unavailable(format!(
            "{} is not held by the RN in rpc.json",
            info.endpoint
        )));
    }
    Ok(pipe)
}

#[cfg(unix)]
async fn open(info: &RnRpcInfo) -> Result<Stream, ClientError> {
    tokio::net::UnixStream::connect(&info.endpoint)
        .await
        .map_err(|e| ClientError::Unavailable(format!("{}: {e}", info.endpoint)))
}

/// 응답을 기다리는 요청에 돌려주고 알림을 구독에 나눕니다. 끝나면 모든 대기를 닫습니다.
async fn read(shared: Arc<Shared>, rd: tokio::io::ReadHalf<Stream>) {
    let mut reader = BufReader::new(rd);
    let mut line = Vec::new();
    let error = loop {
        line.clear();
        match (&mut reader)
            .take(MAX_FRAME_BYTES as u64 + 1)
            .read_until(b'\n', &mut line)
            .await
        {
            Ok(0) | Err(_) => break ClientError::Closed,
            Ok(_) if line.last() != Some(&b'\n') => {
                break ClientError::Protocol("a frame over the limit".into());
            }
            Ok(_) => {}
        }
        if let Err(e) = dispatch(&shared, &line[..line.len() - 1]) {
            break e;
        }
    };
    shared.close(error);
}

fn dispatch(shared: &Shared, line: &[u8]) -> Result<(), ClientError> {
    let v: Value = serde_json::from_slice(line)
        .map_err(|_| ClientError::Protocol("a frame isn't JSON".into()))?;
    if v.get("protocolVersion").and_then(Value::as_str) != Some(RN_PROTOCOL_VERSION) {
        return Err(ClientError::Protocol("another RPC version".into()));
    }
    if let Some(id) = v.get("requestId").and_then(Value::as_str) {
        let response: RnResponse = serde_json::from_value(v.clone())
            .map_err(|e| ClientError::Protocol(format!("response: {e}")))?;
        // 요청과 이어지지 않는 오류(`frame_too_large` 등)는 연결 오류입니다.
        if id.is_empty() {
            return Err(match response.outcome {
                RnOutcome::Error(e) => ClientError::Rejected(e),
                RnOutcome::Result(_) => ClientError::Protocol("a response to no request".into()),
            });
        }
        // 시간이 지나 거둔 요청의 늦은 응답은 버립니다.
        let Some(waiter) = shared.pending.lock().unwrap().remove(id) else {
            return Ok(());
        };
        match (waiter, response.outcome) {
            (Waiter::Plain(tx), RnOutcome::Result(r)) => drop(tx.send(Ok(r))),
            (Waiter::Plain(tx), RnOutcome::Error(e)) => {
                drop(tx.send(Err(ClientError::Rejected(e))))
            }
            (Waiter::Subscribe(tx), RnOutcome::Error(e)) => {
                drop(tx.send(Err(ClientError::Rejected(e))))
            }
            (Waiter::Subscribe(tx), RnOutcome::Result(r)) => {
                match serde_json::from_value::<SubscribeResult>(r) {
                    Ok(result) => {
                        // 다음 줄을 읽기 전에 연결합니다: 같은 묶음에 뒤따르는 알림이 이 구독의 것일 수 있습니다.
                        let (etx, erx) = mpsc::channel(SUBSCRIPTION_BUFFER);
                        shared
                            .subscriptions
                            .lock()
                            .unwrap()
                            .insert(result.subscription_id.clone(), etx);
                        drop(tx.send(Ok(Subscription {
                            id: result.subscription_id,
                            events: erx,
                        })));
                    }
                    Err(e) => {
                        drop(tx.send(Err(ClientError::Protocol(format!("subscribe result: {e}")))))
                    }
                }
            }
        }
        return Ok(());
    }
    let notification: RnNotification = serde_json::from_value(v)
        .map_err(|e| ClientError::Protocol(format!("notification: {e}")))?;
    let params: RnEventParams = serde_json::from_value(notification.params)
        .map_err(|e| ClientError::Protocol(format!("event: {e}")))?;
    let mut subscriptions = shared.subscriptions.lock().unwrap();
    if let Some(tx) = subscriptions.get(&params.subscription_id)
        && tx.try_send(params.event).is_err()
    {
        // 소비자가 따라오지 못하거나 버렸습니다: 그 구독만 닫습니다(커서로 다시 구독).
        subscriptions.remove(&params.subscription_id);
    }
    Ok(())
}

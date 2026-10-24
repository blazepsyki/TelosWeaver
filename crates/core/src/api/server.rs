use super::security::{self, Credentials};
use crate::execution::{ExecutionSupervisor, api_error, new_id};
use futures::StreamExt;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use telos_schema::api::*;
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader},
    sync::{mpsc, watch},
    task::{JoinHandle, JoinSet},
};

pub struct ApiServerConfig {
    pub project_root: PathBuf,
    /// Override the user state base for isolated hosts and tests, never for workspace state.
    pub state_base: Option<PathBuf>,
    /// Unix runtime base. The default follows the G0 XDG/uid rule.
    pub runtime_base: Option<PathBuf>,
}
impl ApiServerConfig {
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            project_root,
            state_base: None,
            runtime_base: None,
        }
    }
}

pub struct ApiServer {
    endpoint: String,
    token_path: PathBuf,
    stop: watch::Sender<bool>,
    task: Option<JoinHandle<io::Result<()>>>,
}
impl ApiServer {
    pub async fn start(
        config: ApiServerConfig,
        execution: Arc<ExecutionSupervisor>,
    ) -> io::Result<Self> {
        let project = config.project_root.canonicalize()?;
        let canonical = project
            .to_str()
            .ok_or_else(|| io::Error::other("project path is not UTF-8"))?;
        let key = project_key(&normalize_project_root(canonical, cfg!(windows)));
        let state = config
            .state_base
            .unwrap_or(default_state_base()?)
            .join(&key);
        // Bind before rotating credentials: a second Core must not invalidate the live Core.
        let listener = Listener::bind(&key, config.runtime_base.as_deref())?;
        let endpoint = listener.endpoint().to_owned();
        let credentials = Arc::new(Credentials::create(&state, &project)?);
        let token_path = credentials.token_path.clone();
        let (stop, receiver) = watch::channel(false);
        let task = tokio::spawn(serve(listener, credentials, execution, receiver));
        Ok(Self {
            endpoint,
            token_path,
            stop,
            task: Some(task),
        })
    }
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
    pub fn token_path(&self) -> &Path {
        &self.token_path
    }
    pub async fn shutdown(mut self) -> io::Result<()> {
        self.stop.send_replace(true);
        if let Some(task) = self.task.take() {
            task.await.map_err(io::Error::other)?
        } else {
            Ok(())
        }
    }
}
impl Drop for ApiServer {
    fn drop(&mut self) {
        self.stop.send_replace(true);
    }
}

fn default_state_base() -> io::Result<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|path| path.join("telos-weaver"))
            .ok_or_else(|| io::Error::other("LOCALAPPDATA is unavailable"))
    }
    #[cfg(unix)]
    {
        if let Some(path) = std::env::var_os("XDG_STATE_HOME") {
            Ok(PathBuf::from(path).join("telos-weaver"))
        } else {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|path| path.join(".local/state/telos-weaver"))
                .ok_or_else(|| io::Error::other("HOME is unavailable"))
        }
    }
}

async fn stopped(receiver: &mut watch::Receiver<bool>) {
    loop {
        if *receiver.borrow_and_update() {
            return;
        }
        if receiver.changed().await.is_err() {
            return;
        }
    }
}
async fn serve(
    mut listener: Listener,
    credentials: Arc<Credentials>,
    execution: Arc<ExecutionSupervisor>,
    mut stop: watch::Receiver<bool>,
) -> io::Result<()> {
    let mut clients = JoinSet::new();
    loop {
        tokio::select! {
            _ = stopped(&mut stop) => break,
            Some(_) = clients.join_next(), if !clients.is_empty() => {},
            accepted = listener.accept(), if clients.len() < 32 => {
                let stream = accepted?;
                clients.spawn(connection(stream, credentials.clone(), execution.clone(), stop.clone()));
            }
        }
    }
    while clients.join_next().await.is_some() {}
    Ok(())
}

#[derive(Debug)]
enum ReadFrameError {
    Io,
    TooLarge,
    Incomplete,
}
async fn read_frame<R: AsyncBufRead + Unpin>(
    reader: &mut R,
) -> Result<Option<Vec<u8>>, ReadFrameError> {
    let mut line = Vec::new();
    loop {
        let buffer = reader.fill_buf().await.map_err(|_| ReadFrameError::Io)?;
        if buffer.is_empty() {
            return if line.is_empty() {
                Ok(None)
            } else {
                Err(ReadFrameError::Incomplete)
            };
        }
        let newline = buffer.iter().position(|byte| *byte == b'\n');
        let count = newline.unwrap_or(buffer.len());
        if line.len().saturating_add(count) > MAX_FRAME_BYTES {
            return Err(ReadFrameError::TooLarge);
        }
        line.extend_from_slice(&buffer[..count]);
        reader.consume(count + usize::from(newline.is_some()));
        if newline.is_some() {
            return Ok(Some(line));
        }
    }
}
fn response(id: String, result: Result<Value, ApiError>) -> String {
    let response = ApiResponse {
        protocol_version: API_PROTOCOL_VERSION.into(),
        request_id: id,
        outcome: match result {
            Ok(value) => ApiOutcome::Result(value),
            Err(error) => ApiOutcome::Error(error),
        },
    };
    encode_frame(&response).unwrap_or_else(|_| {
        encode_frame(&ApiResponse {
            protocol_version: API_PROTOCOL_VERSION.into(),
            request_id: String::new(),
            outcome: ApiOutcome::Error(api_error(
                ApiErrorCode::FrameTooLarge,
                "response exceeds the frame limit",
            )),
        })
        .unwrap()
    })
}
fn notification(method: ApiNotificationMethod, params: Value) -> Result<String, ApiError> {
    encode_frame(&ApiNotification {
        protocol_version: API_PROTOCOL_VERSION.into(),
        method,
        params,
    })
    .map_err(|_| api_error(ApiErrorCode::FrameTooLarge, "event exceeds the frame limit"))
}
fn invalid_params() -> ApiError {
    api_error(ApiErrorCode::InvalidRequest, "invalid method parameters")
}
fn serialized<T: serde::Serialize>(value: T) -> Result<Value, ApiError> {
    serde_json::to_value(value)
        .map_err(|_| api_error(ApiErrorCode::Internal, "response serialization failed"))
}

struct Subscription {
    task: JoinHandle<()>,
    last_sequence: Arc<AtomicU64>,
}
impl Drop for Subscription {
    fn drop(&mut self) {
        self.task.abort();
    }
}
struct ConnectionWriter(JoinHandle<()>);
impl Drop for ConnectionWriter {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn connection<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
    stream: S,
    credentials: Arc<Credentials>,
    execution: Arc<ExecutionSupervisor>,
    mut stop: watch::Receiver<bool>,
) {
    let (read, mut write) = tokio::io::split(stream);
    let mut reader = BufReader::new(read);
    let (output, mut outgoing) = mpsc::channel::<String>(8);
    let (broken, mut failed) = watch::channel(false);
    let mut writer_stop = stop.clone();
    let writer_broken = broken.clone();
    let mut writer = ConnectionWriter(tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = stopped(&mut writer_stop) => break,
                frame = outgoing.recv() => match frame {
                    None => break,
                    Some(frame) => if !matches!(tokio::time::timeout(Duration::from_secs(5),write.write_all(frame.as_bytes())).await,Ok(Ok(()))) {writer_broken.send_replace(true);break;},
                }
            }
        }
        let _ = write.shutdown().await;
    }));
    let mut authenticated = false;
    let mut subscriptions: HashMap<String, Subscription> = HashMap::new();
    loop {
        let frame = tokio::select! {
            _ = stopped(&mut stop) => break,
            _ = stopped(&mut failed) => break,
            frame = async {
                if authenticated {read_frame(&mut reader).await}
                else {tokio::time::timeout(Duration::from_secs(10),read_frame(&mut reader)).await.unwrap_or(Err(ReadFrameError::Io))}
            } => frame,
        };
        let frame = match frame {
            Ok(Some(frame)) => frame,
            Ok(None) | Err(ReadFrameError::Io) => break,
            Err(error) => {
                let code = if matches!(error, ReadFrameError::TooLarge) {
                    ApiErrorCode::FrameTooLarge
                } else {
                    ApiErrorCode::InvalidRequest
                };
                let _ = output
                    .send(response(
                        String::new(),
                        Err(api_error(code, "invalid NDJSON frame")),
                    ))
                    .await;
                break;
            }
        };
        let value: Value = match serde_json::from_slice(&frame) {
            Ok(value) => value,
            Err(_) => {
                if output
                    .send(response(
                        String::new(),
                        Err(api_error(
                            ApiErrorCode::InvalidRequest,
                            "invalid JSON frame",
                        )),
                    ))
                    .await
                    .is_err()
                {
                    break;
                }
                continue;
            }
        };
        let id = value
            .get("requestId")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty() && id.len() <= 256)
            .unwrap_or("")
            .to_owned();
        let method = value.get("method").and_then(Value::as_str);
        let result = if id.is_empty() {
            Err(api_error(
                ApiErrorCode::InvalidRequest,
                "invalid request ID",
            ))
        } else if value.get("protocolVersion").and_then(Value::as_str) != Some(API_PROTOCOL_VERSION)
        {
            Err(api_error(
                ApiErrorCode::UnsupportedProtocolVersion,
                "unsupported protocol version",
            ))
        } else if !authenticated && method != Some("core.hello") {
            Err(api_error(
                ApiErrorCode::Unauthenticated,
                "core.hello is required",
            ))
        } else {
            let unknown_method = value
                .get("method")
                .filter(|method| method.is_string())
                .is_some_and(|method| serde_json::from_value::<ApiMethod>(method.clone()).is_err());
            match serde_json::from_value::<ApiRequest>(value) {
                Err(_) => Err(api_error(
                    if unknown_method {
                        ApiErrorCode::UnsupportedMethod
                    } else {
                        ApiErrorCode::InvalidRequest
                    },
                    "unknown method or invalid request",
                )),
                Ok(request) => match request.method {
                    ApiMethod::Hello if authenticated => Err(api_error(
                        ApiErrorCode::Conflict,
                        "connection is already authenticated",
                    )),
                    ApiMethod::Hello => {
                        match serde_json::from_value::<HelloParams>(request.params) {
                            Ok(params) if credentials.authenticates(params.token.expose()) => {
                                authenticated = true;
                                new_id("connection").and_then(|connection_id| {
                                    serialized(HelloResult {
                                        server_version: env!("CARGO_PKG_VERSION").into(),
                                        protocol_version: API_PROTOCOL_VERSION.into(),
                                        project_id: execution.project_id().into(),
                                        host_id: execution.host_id().into(),
                                        connection_id,
                                    })
                                })
                            }
                            _ => Err(api_error(
                                ApiErrorCode::Unauthenticated,
                                "authentication failed",
                            )),
                        }
                    }
                    ApiMethod::Health => {
                        if request.params == json!({}) {
                            serialized(execution.health())
                        } else {
                            Err(invalid_params())
                        }
                    }
                    ApiMethod::RunsStart => {
                        match serde_json::from_value::<RunsStartParams>(request.params) {
                            Ok(params) => {
                                tokio::select! {_ = stopped(&mut stop) => break, result = execution.start(params) => result.and_then(serialized)}
                            }
                            Err(_) => Err(invalid_params()),
                        }
                    }
                    ApiMethod::RunsInspect => {
                        match serde_json::from_value::<RunRef>(request.params) {
                            Ok(params) => execution.inspect(&params.run_id).and_then(serialized),
                            Err(_) => Err(invalid_params()),
                        }
                    }
                    ApiMethod::RunsCancel => {
                        match serde_json::from_value::<RunsCancelParams>(request.params) {
                            Ok(params) => {
                                tokio::select! {_ = stopped(&mut stop) => break, result = execution.cancel(&params.run_id) => result.and_then(serialized)}
                            }
                            Err(_) => Err(invalid_params()),
                        }
                    }
                    ApiMethod::RunsSubscribe => {
                        subscriptions.retain(|_, subscription| !subscription.task.is_finished());
                        match serde_json::from_value::<RunsSubscribeParams>(request.params) {
                            Err(_) => Err(invalid_params()),
                            Ok(_) if subscriptions.len() >= 32 => Err(api_error(
                                ApiErrorCode::Unavailable,
                                "subscription limit reached",
                            )),
                            Ok(params) => match execution
                                .subscribe(&params.run_id, params.after_sequence)
                                .and_then(|stream| new_id("subscription").map(|id| (id, stream)))
                            {
                                Err(error) => Err(error),
                                Ok((subscription_id, mut events)) => {
                                    // Queue the acknowledgement before any event, including an immediate replay.
                                    if output
                                        .send(response(
                                            id.clone(),
                                            serialized(RunsSubscribeResult {
                                                subscription_id: subscription_id.clone(),
                                            }),
                                        ))
                                        .await
                                        .is_err()
                                    {
                                        break;
                                    }
                                    let sender = output.clone();
                                    let fatal = broken.clone();
                                    let notification_id = subscription_id.clone();
                                    let last_sequence =
                                        Arc::new(AtomicU64::new(params.after_sequence));
                                    let last = last_sequence.clone();
                                    let task = tokio::spawn(async move {
                                        let mut reason = SubscriptionCloseReason::RunFinished;
                                        while let Some(event) = events.next().await {
                                            let event = match event {
                                                Ok(event) => event,
                                                Err(_) => {
                                                    reason =
                                                        SubscriptionCloseReason::EventsUnavailable;
                                                    break;
                                                }
                                            };
                                            let sequence = event.sequence;
                                            let frame = match notification(
                                                ApiNotificationMethod::RunEvent,
                                                json!(RunEventParams {
                                                    subscription_id: notification_id.clone(),
                                                    event
                                                }),
                                            ) {
                                                Ok(frame) => frame,
                                                Err(_) => {
                                                    reason =
                                                        SubscriptionCloseReason::EventsUnavailable;
                                                    break;
                                                }
                                            };
                                            if sender.try_send(frame).is_err() {
                                                reason = SubscriptionCloseReason::SlowConsumer;
                                                break;
                                            }
                                            last.store(sequence, Ordering::Release);
                                        }
                                        let close = notification(
                                            ApiNotificationMethod::SubscriptionClosed,
                                            json!(SubscriptionClosedParams {
                                                subscription_id: notification_id,
                                                reason,
                                                last_sequence: last.load(Ordering::Acquire)
                                            }),
                                        );
                                        if let Ok(frame) = close
                                            && !matches!(
                                                tokio::time::timeout(
                                                    Duration::from_millis(100),
                                                    sender.send(frame)
                                                )
                                                .await,
                                                Ok(Ok(()))
                                            )
                                        {
                                            fatal.send_replace(true);
                                        }
                                    });
                                    subscriptions.insert(
                                        subscription_id,
                                        Subscription {
                                            task,
                                            last_sequence,
                                        },
                                    );
                                    continue;
                                }
                            },
                        }
                    }
                    ApiMethod::RunsUnsubscribe => {
                        match serde_json::from_value::<SubscriptionRef>(request.params) {
                            Err(_) => Err(invalid_params()),
                            Ok(params) => match subscriptions.remove(&params.subscription_id) {
                                None => {
                                    Err(api_error(ApiErrorCode::NotFound, "unknown subscription"))
                                }
                                Some(mut subscription) => {
                                    subscription.task.abort();
                                    let _ = (&mut subscription.task).await;
                                    if let Ok(frame) = notification(
                                        ApiNotificationMethod::SubscriptionClosed,
                                        json!(SubscriptionClosedParams {
                                            subscription_id: params.subscription_id,
                                            reason: SubscriptionCloseReason::Unsubscribed,
                                            last_sequence: subscription
                                                .last_sequence
                                                .load(Ordering::Acquire)
                                        }),
                                    ) && output.send(frame).await.is_err()
                                    {
                                        break;
                                    }
                                    Ok(json!({}))
                                }
                            },
                        }
                    }
                },
            }
        };
        if output.send(response(id, result)).await.is_err() {
            break;
        }
    }
    for (_, mut subscription) in subscriptions {
        subscription.task.abort();
        let _ = (&mut subscription.task).await;
    }
    drop(output);
    if tokio::time::timeout(Duration::from_secs(2), &mut writer.0)
        .await
        .is_err()
    {
        writer.0.abort();
        let _ = (&mut writer.0).await;
    }
}

#[cfg(windows)]
struct Listener {
    pipe: tokio::net::windows::named_pipe::NamedPipeServer,
    endpoint: String,
}
#[cfg(windows)]
impl Listener {
    fn bind(key: &str, _runtime: Option<&Path>) -> io::Result<Self> {
        let endpoint = format!(r"\\.\pipe\telos-weaver-{key}");
        let mut options = tokio::net::windows::named_pipe::ServerOptions::new();
        options
            .first_pipe_instance(true)
            .reject_remote_clients(true);
        Ok(Self {
            pipe: security::platform::pipe(&options, &endpoint)?,
            endpoint,
        })
    }
    fn endpoint(&self) -> &str {
        &self.endpoint
    }
    async fn accept(&mut self) -> io::Result<tokio::net::windows::named_pipe::NamedPipeServer> {
        self.pipe.connect().await?;
        let mut options = tokio::net::windows::named_pipe::ServerOptions::new();
        options.reject_remote_clients(true);
        let next = security::platform::pipe(&options, &self.endpoint)?;
        Ok(std::mem::replace(&mut self.pipe, next))
    }
}

#[cfg(unix)]
struct Listener {
    socket: tokio::net::UnixListener,
    endpoint: String,
}
#[cfg(unix)]
impl Listener {
    fn bind(key: &str, runtime: Option<&Path>) -> io::Result<Self> {
        use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
        let directory = match runtime {
            Some(path) => path.to_path_buf(),
            None => match std::env::var_os("XDG_RUNTIME_DIR") {
                Some(path) => PathBuf::from(path).join("telos-weaver"),
                None => PathBuf::from(format!("/tmp/telos-weaver-{}", unsafe { libc::geteuid() })),
            },
        };
        security::private_directory(&directory)?;
        let path = directory.join(format!("{key}.sock"));
        let endpoint = path
            .to_str()
            .ok_or_else(|| io::Error::other("socket path is not UTF-8"))?
            .to_owned();
        if path.exists() {
            let metadata = std::fs::symlink_metadata(&path)?;
            if !metadata.file_type().is_socket() || metadata.uid() != unsafe { libc::geteuid() } {
                return Err(io::Error::other(
                    "socket entry has an unexpected type or owner",
                ));
            }
            match std::os::unix::net::UnixStream::connect(&path) {
                Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => {
                    std::fs::remove_file(&path)?
                }
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::AddrInUse,
                        "Core endpoint is already active",
                    ));
                }
            }
        }
        let socket = tokio::net::UnixListener::bind(&path)?;
        if let Err(error) = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
        {
            let _ = std::fs::remove_file(&path);
            return Err(error);
        }
        Ok(Self { socket, endpoint })
    }
    fn endpoint(&self) -> &str {
        &self.endpoint
    }
    async fn accept(&mut self) -> io::Result<tokio::net::UnixStream> {
        self.socket.accept().await.map(|(stream, _)| stream)
    }
}
#[cfg(unix)]
impl Drop for Listener {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.endpoint);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[tokio::test]
    async fn pipe_acl_contains_only_current_user() {
        use std::os::windows::io::AsRawHandle;
        let listener = Listener::bind(&new_id("acl-test").unwrap(), None).unwrap();
        unsafe {
            security::platform::assert_user_only(listener.pipe.as_raw_handle());
        }
    }
    #[tokio::test]
    async fn frame_bound_counts_bytes_excluding_lf() {
        let mut exact = vec![b'x'; MAX_FRAME_BYTES];
        exact.push(b'\n');
        assert_eq!(
            read_frame(&mut BufReader::new(exact.as_slice()))
                .await
                .unwrap()
                .unwrap()
                .len(),
            MAX_FRAME_BYTES
        );
        let mut oversized = vec![b'x'; MAX_FRAME_BYTES + 1];
        oversized.push(b'\n');
        assert!(matches!(
            read_frame(&mut BufReader::new(oversized.as_slice())).await,
            Err(ReadFrameError::TooLarge)
        ));
        assert!(matches!(
            read_frame(&mut BufReader::new(&b"{}"[..])).await,
            Err(ReadFrameError::Incomplete)
        ));
    }
}

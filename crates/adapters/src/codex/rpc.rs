//! Bounded App Server JSON-RPC transport over the shared runtime (Spec §7.1, §7.3).
use crate::{
    AdapterError,
    runtime::{
        CommandSpec, OutputEvent, OutputStream, ProcessHandle, ProcessRuntime, TerminationReport,
    },
};
use futures::{StreamExt, stream::BoxStream};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use telos_schema::FailureKind;
use tokio::sync::{mpsc, oneshot, watch};

pub(super) fn error(kind: FailureKind, message: &str) -> AdapterError {
    AdapterError::Failure {
        kind,
        message: message.into(),
    }
}
pub(super) fn remote_error(value: &Value) -> AdapterError {
    let code = value.get("code").and_then(Value::as_i64);
    let text = value
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    let kind = if code == Some(401)
        || code == Some(403)
        || text.contains("unauthorized")
        || text.contains("authentication")
        || text.contains("not logged")
    {
        FailureKind::Authentication
    } else if code == Some(-32600)
        || code == Some(-32601)
        || code == Some(-32602)
        || text.contains("model")
        || text.contains("thread not found")
        || text.contains("no rollout found")
    {
        FailureKind::Configuration
    } else if code == Some(429) || text.contains("rate limit") || text.contains("quota") {
        FailureKind::ResourceLimit
    } else {
        FailureKind::Provider
    };
    // Never reflect provider diagnostics or request contents into common errors.
    error(kind, "Codex App Server rejected the request")
}
type Reply = oneshot::Sender<Result<Value, AdapterError>>;
pub(super) struct RpcClient {
    handle: Arc<dyn ProcessHandle>,
    pending: Mutex<HashMap<u64, Reply>>,
    serial: AtomicU64,
    writer: tokio::sync::Mutex<()>,
    timeout: Duration,
    events: Mutex<Option<mpsc::Receiver<Value>>>,
    failure: watch::Sender<Option<AdapterError>>,
    reader: Mutex<Option<tokio::task::AbortHandle>>,
}
impl Drop for RpcClient {
    fn drop(&mut self) {
        if let Some(reader) = self.reader.get_mut().unwrap().take() {
            reader.abort();
        }
    }
}
impl RpcClient {
    pub async fn spawn(
        runtime: &dyn ProcessRuntime,
        spec: CommandSpec,
        timeout: Duration,
    ) -> Result<Arc<Self>, AdapterError> {
        let handle = runtime
            .spawn(spec)
            .await
            .map_err(|_| error(FailureKind::StartFailed, "Codex process could not start"))?;
        let output = handle
            .take_output()
            .map_err(|_| error(FailureKind::StartFailed, "Codex output is unavailable"))?;
        let (sender, events) = mpsc::channel(64);
        let client = Arc::new(Self {
            handle,
            pending: Mutex::new(HashMap::new()),
            serial: AtomicU64::new(1),
            writer: tokio::sync::Mutex::new(()),
            timeout,
            events: Mutex::new(Some(events)),
            failure: watch::channel(None).0,
            reader: Mutex::new(None),
        });
        let weak = Arc::downgrade(&client);
        let task = tokio::spawn(async move {
            let mut output = output;
            while let Some(event) = output.next().await {
                let Some(client) = weak.upgrade() else { return };
                match event {
                    OutputEvent::Line {
                        stream: OutputStream::Stdout,
                        data,
                        ..
                    } => {
                        let message = match serde_json::from_slice::<Value>(&data) {
                            Ok(value) if value.is_object() => value,
                            _ => {
                                client.fail(error(
                                    FailureKind::ProtocolParse,
                                    "invalid App Server JSON frame",
                                ));
                                break;
                            }
                        };
                        if message.get("method").and_then(Value::as_str).is_some() {
                            if sender.try_send(message).is_err() {
                                client.fail(error(
                                    FailureKind::ResourceLimit,
                                    "App Server notification buffer is full",
                                ));
                                break;
                            }
                        } else if let Some(id) = message.get("id").and_then(Value::as_u64) {
                            let reply = client.pending.lock().unwrap().remove(&id);
                            if let Some(reply) = reply {
                                let result = match (message.get("result"), message.get("error")) {
                                    (Some(result), None) => Ok(result.clone()),
                                    (None, Some(remote)) => Err(remote_error(remote)),
                                    _ => Err(error(
                                        FailureKind::ProtocolParse,
                                        "invalid App Server response envelope",
                                    )),
                                };
                                let _ = reply.send(result);
                            }
                        } else {
                            client.fail(error(
                                FailureKind::ProtocolParse,
                                "invalid App Server envelope",
                            ));
                            break;
                        }
                    }
                    OutputEvent::LineTooLong {
                        stream: OutputStream::Stdout,
                        ..
                    }
                    | OutputEvent::ReadError { .. } => {
                        client.fail(error(
                            FailureKind::ProtocolParse,
                            "App Server output failed or exceeded the limit",
                        ));
                        break;
                    }
                    OutputEvent::Closed {
                        stream: OutputStream::Stdout,
                        ..
                    } => break,
                    _ => {}
                }
            }
            if let Some(client) = weak.upgrade() {
                client.fail(error(FailureKind::Provider, "App Server output closed"));
            }
        });
        *client.reader.lock().unwrap() = Some(task.abort_handle());
        Ok(client)
    }
    fn fail(&self, failure: AdapterError) {
        if self.failure.borrow().is_none() {
            self.failure.send_replace(Some(failure.clone()));
        }
        for (_, reply) in self.pending.lock().unwrap().drain() {
            let _ = reply.send(Err(failure.clone()));
        }
    }
    async fn write(&self, message: Value) -> Result<(), AdapterError> {
        let mut frame = serde_json::to_vec(&message)
            .map_err(|_| error(FailureKind::Configuration, "invalid App Server message"))?;
        if frame.len() > 1024 * 1024 {
            return Err(error(
                FailureKind::ResourceLimit,
                "App Server request exceeds the limit",
            ));
        }
        frame.push(b'\n');
        let _writer = self.writer.lock().await;
        self.handle
            .write_stdin(&frame)
            .await
            .map_err(|_| error(FailureKind::Provider, "App Server input is unavailable"))
    }
    pub async fn request(&self, method: &str, params: Value) -> Result<Value, AdapterError> {
        if let Some(failure) = self.failure.borrow().clone() {
            return Err(failure);
        }
        let id = self.serial.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = oneshot::channel();
        {
            let mut pending = self.pending.lock().unwrap();
            if pending.len() >= 32 {
                return Err(error(
                    FailureKind::ResourceLimit,
                    "too many pending App Server requests",
                ));
            }
            pending.insert(id, sender);
        }
        let result = tokio::time::timeout(self.timeout, async {
            self.write(json!({"id":id,"method":method,"params":params}))
                .await?;
            receiver
                .await
                .map_err(|_| error(FailureKind::Provider, "App Server response was lost"))?
        })
        .await
        .unwrap_or_else(|_| Err(error(FailureKind::Timeout, "App Server request timed out")));
        self.pending.lock().unwrap().remove(&id);
        result
    }
    pub async fn notify(&self, method: &str, params: Value) -> Result<(), AdapterError> {
        self.write(json!({"method":method,"params":params})).await
    }
    pub async fn respond(&self, id: Value, result: Value) -> Result<(), AdapterError> {
        self.write(json!({"id":id,"result":result})).await
    }
    pub async fn reject(&self, id: Value) -> Result<(), AdapterError> {
        self.write(json!({"id":id,"error":{"code":-32601,"message":"unsupported client request"}}))
            .await
    }
    pub async fn initialize(&self) -> Result<(), AdapterError> {
        self.request("initialize",json!({"clientInfo":{"name":"telosweaver","title":"TelosWeaver","version":env!("CARGO_PKG_VERSION")},"capabilities":{"experimentalApi":true}})).await?;
        self.notify("initialized", json!({})).await
    }
    pub fn take_events(
        &self,
    ) -> Result<BoxStream<'static, Result<Value, AdapterError>>, AdapterError> {
        let receiver = self.events.lock().unwrap().take().ok_or_else(|| {
            error(
                FailureKind::Configuration,
                "App Server events were already taken",
            )
        })?;
        let failure = self.failure.subscribe();
        Ok(futures::stream::unfold(
            (receiver, failure, false),
            |(mut receiver, failure, mut ended)| async move {
                if ended {
                    return None;
                }
                if let Some(value) = receiver.recv().await {
                    return Some((Ok(value), (receiver, failure, ended)));
                }
                ended = true;
                let error = failure.borrow().clone();
                error.map(|error| (Err(error), (receiver, failure, ended)))
            },
        )
        .boxed())
    }
    pub async fn close(&self, grace: Duration) -> Result<TerminationReport, AdapterError> {
        let _ = tokio::time::timeout(Duration::from_millis(100), self.handle.close_stdin()).await;
        self.handle.terminate(grace).await.map_err(|_| {
            error(
                FailureKind::Unknown,
                "Codex process termination could not be confirmed",
            )
        })
    }
    /// Build per-thread overrides without exposing any effective configuration values.
    pub async fn isolated_config(&self) -> Result<Value, AdapterError> {
        let config = self
            .request("config/read", json!({"includeLayers":false}))
            .await?;
        let effective = config
            .get("config")
            .and_then(Value::as_object)
            .ok_or_else(|| {
                error(
                    FailureKind::ProtocolParse,
                    "missing effective Codex configuration",
                )
            })?;
        let mut overrides = serde_json::Map::new();
        if let Some(servers) = effective.get("mcp_servers").and_then(Value::as_object) {
            for name in servers.keys() {
                overrides.insert(format!("mcp_servers.{name}.enabled"), json!(false));
            }
        }
        for (key, value) in [
            ("web_search", json!("disabled")),
            ("features.apps", json!(false)),
            ("features.plugins", json!(false)),
            ("features.multi_agent", json!(false)),
            ("hooks.enabled", json!(false)),
            ("project_doc_max_bytes", json!(0)),
            ("model_reasoning_summary", json!("none")),
        ] {
            overrides.insert(key.into(), value);
        }
        Ok(Value::Object(overrides))
    }
}

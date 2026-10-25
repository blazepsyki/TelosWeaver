//! The TelosWeaver RPC (Spec v1.5 §12.1, `telos_schema::rn`, PH0-RN-06): Core's own endpoint into this engine, apart
//! from the agents' socket.
//!
//! - Its own pipe or socket, this user's only (`private`), and a token made new at each start (`rpc-token`, beside
//!   `rpc.json`, which says where the RPC is). A connection starts with `rn.hello`.
//! - Core has a mailbox here (`telosweaver`, provider `telosweaver`): the sender of its envelopes, where replies land.
//! - One stream of events for Core: receipts of what Core sent, messages to its mailbox, sessions coming and going. The
//!   last `RING` are kept in memory, numbered from this engine's start: a cursor from before it, or one they no longer
//!   reach, gets `resync_required` first.
//! - Core sends only to sessions that came in through telos-rn (`Live::wired`); others are listed, unattached.

use super::{Daemon, envelope::contract, log};
use crate::{
    claude, ipc, paths, private,
    telos::{self, RnRefusal},
    util,
};
use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use telos_schema::{
    DeliveryTier, SecretString,
    api::MAX_FRAME_BYTES,
    rn::{
        EndpointRef, InboundMessage, MessageRef, PresenceState, ProviderConnector, RN_PROTOCOL_VERSION, RnError,
        RnErrorCode as Code, RnEvent, RnEventKind, RnEventParams, RnHealth, RnHealthStatus, RnHelloParams,
        RnHelloResult, RnMethod, RnNotification, RnNotificationMethod, RnRpcInfo, SessionCapabilities, SessionEndpoint,
        SessionQuery, SubscribeParams, SubscribeResult,
    },
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{Notify, broadcast, mpsc},
};

/// Core's mailbox here: its session key and name.
pub const CORE_KEY: &str = "telosweaver:core";
pub const CORE_NAME: &str = "telosweaver";
/// The upstream release this fork is based on (`TELOSWEAVER.md`).
const UPSTREAM_COMMIT: &str = "b3557ff454cd26a04c2a18804ba83ae5db8b119d";
/// Events kept for cursors to resume from.
const RING: usize = 1024;
/// How often receipts, presence and Core's mailbox are looked at (and on every new message).
const POLL: Duration = Duration::from_millis(500);
/// Frames waiting to go out on one connection; past it the connection is closed (its subscriber reconnects with its
/// cursor).
const QUEUE: usize = 1024;

/// The RPC's state: its token and the event stream.
pub(super) struct Hub {
    token: Option<SecretString>,
    /// This engine's start (ms): the first part of every cursor.
    epoch: i64,
    next: u64,
    ring: VecDeque<RnEvent>,
    tx: broadcast::Sender<RnEvent>,
    /// Receipts already sent at the latest change time seen (`watermark`), as (message, status, reason): the same
    /// state isn't sent twice, and two changes within one millisecond are both sent.
    watermark: i64,
    at_watermark: HashSet<(String, String, Option<String>)>,
    presence: HashMap<String, SessionEndpoint>,
}

impl Hub {
    pub(super) fn new(epoch: i64) -> Self {
        Hub {
            token: None,
            epoch,
            next: 1,
            ring: VecDeque::new(),
            tx: broadcast::channel(RING).0,
            watermark: 0,
            at_watermark: HashSet::new(),
            presence: HashMap::new(),
        }
    }

    fn cursor(&self, sequence: u64) -> String {
        format!("{}:{sequence}", self.epoch)
    }

    /// Adds an event; returns how many subscribers it went to.
    fn publish(&mut self, kind: RnEventKind, payload: Value) -> usize {
        let sequence = self.next;
        self.next += 1;
        let event = RnEvent { cursor: self.cursor(sequence), sequence, kind, payload };
        if self.ring.len() == RING {
            self.ring.pop_front();
        }
        self.ring.push_back(event.clone());
        self.tx.send(event).unwrap_or(0)
    }

    /// What a subscriber resuming after `cursor` gets first, and the live stream after it, with nothing in between.
    fn subscribe(&self, cursor: Option<&str>) -> (Vec<RnEvent>, broadcast::Receiver<RnEvent>) {
        let rx = self.tx.subscribe();
        let Some(cursor) = cursor else { return (Vec::new(), rx) };
        let last = self.next - 1;
        let resumable = cursor
            .split_once(':')
            .and_then(|(e, s)| Some((e.parse::<i64>().ok()?, s.parse::<u64>().ok()?)))
            .filter(|&(epoch, seq)| epoch == self.epoch && seq <= last)
            .filter(|&(_, seq)| self.ring.front().is_none_or(|first| seq + 1 >= first.sequence));
        match resumable {
            Some((_, seq)) => (self.ring.iter().filter(|e| e.sequence > seq).cloned().collect(), rx),
            None => {
                let resync = RnEvent {
                    cursor: self.cursor(last),
                    sequence: last,
                    kind: RnEventKind::ResyncRequired,
                    payload: json!({}),
                };
                (vec![resync], rx)
            }
        }
    }
}

/// Where the RPC is, for Core: the endpoint, written once it is open.
pub(super) struct Opened {
    listener: ipc::Listener,
}

impl Daemon {
    /// Opens the RPC at its own endpoint, with a new token, and says where in `rpc.json`. Nothing opens if the token
    /// can't be made this user's only.
    pub(super) fn open_rpc(&self) -> Result<Opened> {
        self.open_rpc_at(ipc::Listener::bind_rpc()?, &paths::rpc_socket(), &paths::rpc_token(), &paths::rpc_info())
    }

    pub(super) fn open_rpc_at(
        &self,
        listener: ipc::Listener,
        endpoint: &std::path::Path,
        token_path: &std::path::Path,
        info_path: &std::path::Path,
    ) -> Result<Opened> {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).map_err(|e| anyhow::anyhow!("system randomness: {e}"))?;
        let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        // Made this user's before the token goes in.
        let _ = std::fs::remove_file(token_path);
        std::fs::write(token_path, "").with_context(|| format!("creating {}", token_path.display()))?;
        private::protect(token_path).with_context(|| format!("making {} private", token_path.display()))?;
        std::fs::write(token_path, &token).with_context(|| format!("writing {}", token_path.display()))?;
        self.rpc.lock().unwrap().token = Some(SecretString::new(token));
        let info = RnRpcInfo {
            endpoint: endpoint.display().to_string(),
            rpc_version: RN_PROTOCOL_VERSION.into(),
            pid: std::process::id(),
        };
        std::fs::write(info_path, serde_json::to_string_pretty(&info)?)?;
        Ok(Opened { listener })
    }

    pub(super) async fn serve_rpc(self: Arc<Self>, opened: Opened) {
        let mut listener = opened.listener;
        loop {
            match listener.accept().await {
                Ok(stream) => {
                    tokio::spawn(self.clone().rpc_connection(stream));
                }
                Err(e) => {
                    log(&format!("TelosWeaver RPC: accepting: {e}"));
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
            }
        }
    }

    async fn rpc_connection(self: Arc<Self>, stream: ipc::Accepted) {
        static CONNECTIONS: AtomicU64 = AtomicU64::new(0);
        let id = format!("rnc-{}", CONNECTIONS.fetch_add(1, Ordering::Relaxed) + 1);
        let (mut rd, mut wr) = tokio::io::split(stream);
        let (tx, mut rx) = mpsc::channel::<String>(QUEUE);
        let writer = tokio::spawn(async move {
            while let Some(line) = rx.recv().await {
                if wr.write_all(line.as_bytes()).await.is_err() {
                    break;
                }
            }
            let _ = wr.flush().await;
        });
        let mut conn =
            Connection { id, authed: false, tx: tx.clone(), close: Arc::new(Notify::new()), subs: Vec::new() };
        let mut buf: Vec<u8> = Vec::new();
        let mut chunk = vec![0u8; 64 * 1024];
        'conn: loop {
            while let Some(end) = buf.iter().position(|b| *b == b'\n') {
                let line: Vec<u8> = buf.drain(..=end).collect();
                if line.len() - 1 > MAX_FRAME_BYTES {
                    conn.too_large().await;
                    break 'conn;
                }
                self.rpc_line(&mut conn, &line[..line.len() - 1]).await;
            }
            if buf.len() > MAX_FRAME_BYTES {
                conn.too_large().await;
                break;
            }
            let close = conn.close.clone();
            tokio::select! {
                read = rd.read(&mut chunk) => match read {
                    Ok(0) | Err(_) => break,
                    Ok(n) => buf.extend_from_slice(&chunk[..n]),
                },
                _ = close.notified() => break,
            }
        }
        for s in conn.subs.drain(..) {
            s.abort();
        }
        drop(conn);
        drop(tx);
        let _ = tokio::time::timeout(Duration::from_secs(1), writer).await;
    }

    async fn rpc_line(self: &Arc<Self>, conn: &mut Connection, line: &[u8]) {
        let Ok(v) =
            std::str::from_utf8(line).map_err(|_| ()).and_then(|t| serde_json::from_str::<Value>(t).map_err(|_| ()))
        else {
            return conn.error("", Code::InvalidParams, "a frame is one JSON object in UTF-8").await;
        };
        let request_id = v.get("requestId").and_then(Value::as_str).unwrap_or("").to_string();
        if v.get("protocolVersion").and_then(Value::as_str) != Some(RN_PROTOCOL_VERSION) {
            let why = format!("this engine speaks RPC {RN_PROTOCOL_VERSION}");
            return conn.error(&request_id, Code::UnsupportedProtocolVersion, &why).await;
        }
        let Some(method) = v.get("method").and_then(|m| serde_json::from_value::<RnMethod>(m.clone()).ok()) else {
            return conn.error(&request_id, Code::UnknownMethod, "unknown method").await;
        };
        let params = v.get("params").cloned().unwrap_or_else(|| json!({}));
        if !conn.authed && method != RnMethod::Hello {
            return conn.error(&request_id, Code::Unauthenticated, "start with rn.hello").await;
        }
        let result = match method {
            RnMethod::Hello => self.rpc_hello(conn, params),
            RnMethod::HealthGet => Ok(json!(self.rpc_health())),
            RnMethod::ProvidersList => Ok(json!(rpc_providers())),
            RnMethod::SessionsDiscover => match parse::<SessionQuery>(params) {
                Ok(q) => self.discover(&q).await.map(|s| json!(s)),
                Err(e) => Err(e),
            },
            RnMethod::SessionsResolve => match parse::<EndpointRef>(params) {
                Ok(e) => self.resolve_endpoint(&e).await.map(|s| json!(s)),
                Err(e) => Err(e),
            },
            RnMethod::SessionsCapabilities => match parse::<EndpointRef>(params) {
                Ok(e) => self.capabilities(&e).await.map(|c| json!(c)),
                Err(e) => Err(e),
            },
            RnMethod::MessagesSend => parse(params).and_then(|env| self.send_envelope(env).map(|r| json!(r))),
            RnMethod::MessagesStatus => {
                parse::<MessageRef>(params).and_then(|m| self.receipt(&m.message_id).map(|r| json!(r)))
            }
            RnMethod::MessagesSubscribe => match parse::<SubscribeParams>(params) {
                Ok(p) => return self.rpc_subscribe(conn, &request_id, p).await,
                Err(e) => Err(e),
            },
            RnMethod::ProvidersAgyProbe => {
                Err(RnRefusal::error(Code::Refused, "providers.agy.probe is P1: not supported in Phase 0"))
            }
        };
        match result {
            Ok(v) => conn.result(&request_id, v).await,
            Err(e) => match e.downcast::<RnRefusal>() {
                Ok(r) => conn.error(&request_id, r.0.code, &r.0.message).await,
                Err(e) => {
                    log(&format!("TelosWeaver RPC: {method:?}: {e:#}"));
                    conn.error(&request_id, Code::Internal, "the engine couldn't do that; see its log").await
                }
            },
        }
    }

    fn rpc_hello(&self, conn: &mut Connection, params: Value) -> Result<Value> {
        let p: RnHelloParams = parse(params)?;
        let ok = self.rpc.lock().unwrap().token.as_ref().is_some_and(|t| same(t.expose(), p.token.expose()));
        if !ok {
            return Err(RnRefusal::error(Code::Unauthenticated, "the token isn't this engine's"));
        }
        conn.authed = true;
        Ok(json!(RnHelloResult {
            rpc_version: RN_PROTOCOL_VERSION.into(),
            host_id: self.host_id.clone(),
            connection_id: conn.id.clone(),
            endpoint: self.core_endpoint(),
        }))
    }

    pub(super) fn core_endpoint(&self) -> EndpointRef {
        telos::endpoint(&self.host_id, CORE_KEY).expect("Core's mailbox key is an endpoint")
    }

    fn rpc_health(&self) -> RnHealth {
        RnHealth {
            status: RnHealthStatus::Ok,
            rpc_version: RN_PROTOCOL_VERSION.into(),
            upstream_commit: UPSTREAM_COMMIT.into(),
            fork_commit: env!("TELOS_RN_FORK_COMMIT").into(),
            host_id: self.host_id.clone(),
        }
    }

    /// The sessions on this host with an endpoint: live ones, remembered ones (offline), and Claude Code sessions its
    /// registry lists that telos-rn doesn't know (unattached). Not Core's own mailbox.
    pub(super) async fn discover(&self, q: &SessionQuery) -> Result<Vec<SessionEndpoint>> {
        if q.host_id.as_ref().is_some_and(|h| *h != self.host_id) {
            return Ok(Vec::new());
        }
        let scan = tokio::task::spawn_blocking(claude::scan).await?;
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        {
            let g = self.inner.lock().unwrap();
            for (key, l) in &g.live {
                let Some(endpoint) = telos::endpoint(&self.host_id, key).filter(|_| key != CORE_KEY) else { continue };
                seen.insert(key.clone());
                let presence = match l.status.as_ref().map(|(s, _)| s.as_str()) {
                    Some("busy") => PresenceState::Busy,
                    Some("idle") => PresenceState::Idle,
                    _ => PresenceState::Unknown,
                };
                let at = l.status.as_ref().map_or(l.since, |(_, at)| *at);
                out.push(SessionEndpoint {
                    endpoint,
                    address: Some(self.addr(&l.name)),
                    workspace: Some(l.reg.cwd.clone()),
                    presence,
                    last_activity_at: Some(telos::rfc3339(at)),
                    attached: l.wired,
                });
            }
            for r in g.store.sessions()? {
                if seen.contains(&r.key) || r.key == CORE_KEY {
                    continue;
                }
                let Some(endpoint) = telos::endpoint(&self.host_id, &r.key) else { continue };
                seen.insert(r.key.clone());
                out.push(SessionEndpoint {
                    endpoint,
                    address: Some(self.addr(&r.name)),
                    workspace: Some(r.cwd.clone()),
                    presence: PresenceState::Offline,
                    last_activity_at: Some(telos::rfc3339(r.last_seen)),
                    attached: r.wired,
                });
            }
        }
        for c in scan {
            let Some(id) = c.session_id.as_deref() else { continue };
            let key = format!("{}:{id}", crate::agent::CLAUDE_CODE);
            let Some(endpoint) = telos::endpoint(&self.host_id, &key).filter(|_| !seen.contains(&key)) else {
                continue;
            };
            out.push(SessionEndpoint {
                endpoint,
                address: None,
                workspace: c.cwd.clone(),
                presence: match c.status.as_deref() {
                    Some("busy") => PresenceState::Busy,
                    Some("idle") => PresenceState::Idle,
                    _ => PresenceState::Unknown,
                },
                last_activity_at: c.status_updated_at.map(telos::rfc3339),
                attached: false,
            });
        }
        out.retain(|s| {
            q.provider.as_ref().is_none_or(|p| *p == s.endpoint.provider)
                && q.workspace.as_ref().is_none_or(|w| s.workspace.as_deref().is_some_and(|x| same_dir(w, x)))
        });
        out.sort_by(|a, b| (&a.endpoint.provider, &a.address).cmp(&(&b.endpoint.provider, &b.address)));
        Ok(out)
    }

    /// The session an endpoint names, whose fingerprint must be its fields'.
    async fn resolve_endpoint(&self, e: &EndpointRef) -> Result<SessionEndpoint> {
        if e.fingerprint != telos::fingerprint(&e.host_id, &e.provider, &e.native_session_id) {
            return Err(RnRefusal::error(
                Code::IdentityMismatch,
                "the fingerprint isn't its host, provider and session id's",
            ));
        }
        if e.host_id != self.host_id {
            let why = format!("the session is on host {}; this engine is host {}", e.host_id, self.host_id);
            return Err(RnRefusal::error(Code::IdentityMismatch, why));
        }
        self.discover(&SessionQuery { provider: Some(e.provider.clone()), host_id: None, workspace: None })
            .await?
            .into_iter()
            .find(|s| s.endpoint == *e)
            .ok_or_else(|| {
                RnRefusal::error(Code::NotFound, format!("no {} session {} here", e.provider, e.native_session_id))
            })
    }

    /// The tiers a session is reached by, as verified (PH0-RN-05): none unattached; its inbox, and with hooks the next
    /// turn too. `push` isn't reported: delivery straight into a running session isn't verified yet (P1).
    async fn capabilities(&self, e: &EndpointRef) -> Result<SessionCapabilities> {
        let s = self.resolve_endpoint(e).await?;
        let hooks =
            self.inner.lock().unwrap().live.iter().any(|(k, l)| {
                l.last_hook.is_some() && l.wired && telos::endpoint(&self.host_id, k).as_ref() == Some(e)
            });
        let tiers = match (s.attached, hooks) {
            (false, _) => Vec::new(),
            (true, true) => vec![DeliveryTier::Turn, DeliveryTier::Pull],
            (true, false) => vec![DeliveryTier::Pull],
        };
        Ok(SessionCapabilities { endpoint: s.endpoint, tiers, can_reply: s.attached, can_resume: false })
    }

    async fn rpc_subscribe(self: &Arc<Self>, conn: &mut Connection, request_id: &str, p: SubscribeParams) {
        static SUBSCRIPTIONS: AtomicU64 = AtomicU64::new(0);
        let subscription_id = format!("rns-{}", SUBSCRIPTIONS.fetch_add(1, Ordering::Relaxed) + 1);
        let (replay, mut rx) = self.rpc.lock().unwrap().subscribe(p.cursor.as_deref());
        // The answer goes out before any of its events (REQ-017's order, here too).
        conn.result(request_id, json!(SubscribeResult { subscription_id: subscription_id.clone() })).await;
        let (tx, close) = (conn.tx.clone(), conn.close.clone());
        conn.subs.push(tokio::spawn(async move {
            let frame = |event: RnEvent| {
                line(&RnNotification {
                    protocol_version: RN_PROTOCOL_VERSION.into(),
                    method: RnNotificationMethod::Event,
                    params: json!(RnEventParams { subscription_id: subscription_id.clone(), event }),
                })
            };
            for e in replay {
                if tx.send(frame(e)).await.is_err() {
                    return;
                }
            }
            loop {
                match rx.recv().await {
                    Ok(e) => {
                        // A connection that doesn't keep up is closed: it resumes with its cursor.
                        if tx.try_send(frame(e)).is_err() {
                            close.notify_one();
                            return;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        close.notify_one();
                        return;
                    }
                    Err(broadcast::error::RecvError::Closed) => return,
                }
            }
        }));
    }

    /// Makes the events: receipts of what Core sent, messages to its mailbox (delivered once a subscriber has them),
    /// and sessions coming and going.
    pub(super) async fn rpc_events(self: Arc<Self>) {
        let core = self.core_endpoint();
        let core_address = self.addr(CORE_NAME);
        loop {
            let arrived = self.arrived.notified();
            tokio::pin!(arrived);
            arrived.as_mut().enable();
            if let Err(e) = self.rpc_tick(&core, &core_address) {
                log(&format!("TelosWeaver RPC events: {e:#}"));
            }
            let _ = tokio::time::timeout(POLL, arrived).await;
        }
    }

    pub(super) fn rpc_tick(&self, core: &EndpointRef, core_address: &str) -> Result<()> {
        let mut g = self.inner.lock().unwrap();
        let mut hub = self.rpc.lock().unwrap();
        // Receipts of Core's envelopes that changed.
        for r in g.store.receipts_by_sender(&core.fingerprint, hub.watermark)? {
            let state = (r.message_id.clone(), telos::status_str(r.status).to_string(), r.reason.clone());
            if r.updated_at == hub.watermark && hub.at_watermark.contains(&state) {
                continue;
            }
            if r.updated_at > hub.watermark {
                hub.watermark = r.updated_at;
                hub.at_watermark.clear();
            }
            hub.at_watermark.insert(state);
            hub.publish(RnEventKind::Receipt, json!(contract(&r)));
        }
        // Sessions that came, changed or went.
        let mut now: HashMap<String, SessionEndpoint> = HashMap::new();
        for (key, l) in &g.live {
            let Some(endpoint) = telos::endpoint(&self.host_id, key).filter(|_| key != CORE_KEY) else { continue };
            let presence = match l.status.as_ref().map(|(s, _)| s.as_str()) {
                Some("busy") => PresenceState::Busy,
                Some("idle") => PresenceState::Idle,
                _ => PresenceState::Unknown,
            };
            let at = l.status.as_ref().map_or(l.since, |(_, at)| *at);
            now.insert(
                key.clone(),
                SessionEndpoint {
                    endpoint,
                    address: Some(self.addr(&l.name)),
                    workspace: Some(l.reg.cwd.clone()),
                    presence,
                    last_activity_at: Some(telos::rfc3339(at)),
                    attached: l.wired,
                },
            );
        }
        let before = std::mem::take(&mut hub.presence);
        for (key, s) in &now {
            let changed = before.get(key).is_none_or(|b| (b.presence, b.attached) != (s.presence, s.attached));
            if changed {
                hub.publish(RnEventKind::Presence, json!(s));
            }
        }
        for (key, mut s) in before {
            if !now.contains_key(&key) {
                s.presence = PresenceState::Offline;
                hub.publish(RnEventKind::Presence, json!(s));
            }
        }
        hub.presence = now;
        // Messages to Core's mailbox, while someone listens: delivered once handed to a subscriber.
        if hub.tx.receiver_count() > 0 {
            for m in g.pending(CORE_KEY, core_address)? {
                let sender = g
                    .live
                    .iter()
                    .find(|(_, l)| self.addr(&l.name) == m.from)
                    .and_then(|(k, _)| telos::endpoint(&self.host_id, k));
                let inbound = InboundMessage {
                    message_id: m.id.clone(),
                    sender,
                    sender_address: m.from.clone(),
                    reply_to: m.reply_to.clone(),
                    body: m.body.clone(),
                    received_at: telos::rfc3339(m.sent_at),
                };
                if hub.publish(RnEventKind::Message, json!(inbound)) > 0 {
                    g.store.mark_read(std::slice::from_ref(&m.id), util::now_ms())?;
                }
            }
        }
        Ok(())
    }
}

/// One RPC connection.
struct Connection {
    id: String,
    authed: bool,
    tx: mpsc::Sender<String>,
    /// Closes the connection (a subscriber that didn't keep up).
    close: Arc<Notify>,
    subs: Vec<tokio::task::JoinHandle<()>>,
}

impl Connection {
    async fn result(&self, request_id: &str, result: Value) {
        let frame = json!({"protocolVersion": RN_PROTOCOL_VERSION, "requestId": request_id, "result": result});
        let _ = self.tx.send(line(&frame)).await;
    }

    async fn error(&self, request_id: &str, code: Code, message: &str) {
        let error = RnError { code, message: message.into(), details: None };
        let frame = json!({"protocolVersion": RN_PROTOCOL_VERSION, "requestId": request_id, "error": error});
        let _ = self.tx.send(line(&frame)).await;
    }

    async fn too_large(&self) {
        let why = format!("a frame is at most {MAX_FRAME_BYTES} bytes");
        self.error("", Code::FrameTooLarge, &why).await;
    }
}

fn line<T: Serialize>(v: &T) -> String {
    let mut s = serde_json::to_string(v).expect("RPC frames serialize");
    s.push('\n');
    s
}

fn parse<T: serde::de::DeserializeOwned>(params: Value) -> Result<T> {
    serde_json::from_value(params).map_err(|e| RnRefusal::error(Code::InvalidParams, format!("params: {e}")))
}

/// The same token, compared in time that doesn't depend on where they differ.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Two spellings of one folder (on Windows, case and slashes aside).
fn same_dir(a: &str, b: &str) -> bool {
    let norm = |s: &str| {
        let s = s.trim_end_matches(['/', '\\']);
        if cfg!(windows) { s.replace('\\', "/").to_lowercase() } else { s.to_string() }
    };
    norm(a) == norm(b)
}

/// What can be reached here, said as verified (PH0-RN-05): no version is claimed that wasn't checked.
fn rpc_providers() -> Vec<ProviderConnector> {
    let claude = paths::claude_sessions().is_dir();
    let codex = crate::agent::codex_bin().is_some();
    vec![
        ProviderConnector {
            provider: "claude".into(),
            available: claude,
            version: None,
            reason: Some(if claude {
                "sessions listed from Claude Code's registry; messages go only to sessions attached through telos-rn"
                    .into()
            } else {
                "no Claude Code session registry here".into()
            }),
        },
        ProviderConnector {
            provider: "codex".into(),
            available: codex,
            version: None,
            reason: (!codex)
                .then(|| "TELOS_RN_CODEX_BIN isn't set: Codex sessions get messages through hooks only".into()),
        },
        ProviderConnector {
            provider: "agy".into(),
            available: false,
            version: None,
            reason: Some("Antigravity sessions are P1".into()),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{git::GitInfo, proto::SessionReg, proto::Tier};
    use std::path::PathBuf;
    use tokio::io::{AsyncBufReadExt, BufReader};

    /// A daemon on an in-memory store with its RPC open at a pipe (or socket) of its own, and its events running.
    struct Rig {
        d: Arc<Daemon>,
        endpoint: PathBuf,
        token: String,
        dir: PathBuf,
    }

    impl Drop for Rig {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    async fn rig() -> Rig {
        let d = Arc::new(super::super::test_daemon(crate::config::Config::default()));
        let dir = std::env::temp_dir().join(format!("telos-rn-rpc-test-{}", ulid::Ulid::generate()));
        std::fs::create_dir_all(&dir).unwrap();
        #[cfg(windows)]
        let endpoint = PathBuf::from(format!(r"\\.\pipe\telos-rn-rpc-test-{}", ulid::Ulid::generate()));
        #[cfg(unix)]
        let endpoint = dir.join("rpc.sock");
        let listener = ipc::Listener::bind_at(endpoint.clone(), false).unwrap();
        let opened = d.open_rpc_at(listener, &endpoint, &dir.join("rpc-token"), &dir.join("rpc.json")).unwrap();
        tokio::spawn(d.clone().serve_rpc(opened));
        tokio::spawn(d.clone().rpc_events());
        let token = std::fs::read_to_string(dir.join("rpc-token")).unwrap();
        Rig { d, endpoint, token, dir }
    }

    struct Client {
        lines: tokio::io::Lines<BufReader<tokio::io::ReadHalf<Stream>>>,
        wr: tokio::io::WriteHalf<Stream>,
    }

    #[cfg(windows)]
    type Stream = tokio::net::windows::named_pipe::NamedPipeClient;
    #[cfg(unix)]
    type Stream = tokio::net::UnixStream;

    async fn connect(r: &Rig) -> Client {
        #[cfg(windows)]
        let s = tokio::net::windows::named_pipe::ClientOptions::new().open(&r.endpoint).unwrap();
        #[cfg(unix)]
        let s = tokio::net::UnixStream::connect(&r.endpoint).await.unwrap();
        let (rd, wr) = tokio::io::split(s);
        Client { lines: BufReader::new(rd).lines(), wr }
    }

    impl Client {
        async fn raw(&mut self, line: &str) {
            self.wr.write_all(line.as_bytes()).await.unwrap();
        }

        async fn send(&mut self, id: &str, method: &str, params: Value) {
            let f =
                json!({"protocolVersion": RN_PROTOCOL_VERSION, "requestId": id, "method": method, "params": params});
            self.raw(&format!("{f}\n")).await;
        }

        async fn next(&mut self) -> Option<Value> {
            let line =
                tokio::time::timeout(Duration::from_secs(5), self.lines.next_line()).await.expect("a frame in time");
            line.unwrap().map(|l| serde_json::from_str(&l).unwrap())
        }

        /// The next frame that isn't a notification.
        async fn reply(&mut self) -> Value {
            loop {
                let v = self.next().await.expect("a reply");
                if v.get("requestId").is_some() {
                    return v;
                }
            }
        }

        async fn call(&mut self, method: &str, params: Value) -> Value {
            self.send("c", method, params).await;
            self.reply().await
        }

        async fn hello(&mut self, token: &str) -> Value {
            self.call("rn.hello", json!({"clientName": "test", "clientVersion": "0", "token": token})).await
        }

        /// Events until one of `kind` for which `pick` holds.
        async fn event(&mut self, kind: &str, pick: impl Fn(&Value) -> bool) -> RnEvent {
            loop {
                let v = self.next().await.expect("an event");
                if v["method"] == "messages.event" && v["params"]["event"]["kind"] == kind {
                    let e: RnEvent = serde_json::from_value(v["params"]["event"].clone()).unwrap();
                    if pick(&e.payload) {
                        return e;
                    }
                }
            }
        }
    }

    fn code(v: &Value) -> &str {
        v["error"]["code"].as_str().unwrap_or_else(|| panic!("not an error: {v}"))
    }

    fn reg(key: &str, agent: &str, name: Option<&str>) -> SessionReg {
        SessionReg {
            key: key.into(),
            agent: agent.into(),
            cwd: "/work/api".into(),
            agent_session_id: key.split_once(':').map(|(_, id)| id.to_string()),
            pid: None,
            tier: Tier::Pull,
            name: name.map(String::from),
        }
    }

    fn envelope(id: &str, core: &EndpointRef, to: &EndpointRef) -> Value {
        json!({
            "messageId": id, "correlationId": "c1", "idempotencyKey": format!("k-{id}"),
            "sender": core, "recipient": to, "tier": "pull",
            "expiresAt": telos::rfc3339(util::now_ms() + 3_600_000), "bodyType": "text", "body": format!("hello {id}"),
            "artifacts": [],
        })
    }

    #[tokio::test]
    async fn a_session_its_own_hooks_report_stays_attached_once_offline_and_one_from_the_registry_never_is() {
        let d = Arc::new(super::super::test_daemon(crate::config::Config::default()));
        let hook = crate::proto::HookCall {
            agent: "codex".into(),
            event: "prompt".into(),
            session_id: "t1".into(),
            cwd: Some("/work/api".into()),
            pid: None,
            mail: true,
            transcript: None,
            doing: None,
            keep: false,
            name: None,
            dirs: Vec::new(),
        };
        d.hook(hook).await.unwrap();
        // As Claude Code's registry adds one (`claude_seen`): no MCP connection, no hooks of its own.
        d.register(None, reg("claude-code:c9", crate::agent::CLAUDE_CODE, None), GitInfo::default()).unwrap();
        let attached = |found: &[SessionEndpoint], id: &str| {
            found.iter().find(|s| s.endpoint.native_session_id == id).map(|s| (s.attached, s.presence))
        };
        let found = d.discover(&SessionQuery { provider: None, host_id: None, workspace: None }).await.unwrap();
        assert_eq!(attached(&found, "t1").map(|a| a.0), Some(true));
        assert_eq!(attached(&found, "c9").map(|a| a.0), Some(false));
        // Both gone: remembered, offline, each as it was.
        d.inner.lock().unwrap().live.clear();
        let found = d.discover(&SessionQuery { provider: None, host_id: None, workspace: None }).await.unwrap();
        assert_eq!(attached(&found, "t1"), Some((true, PresenceState::Offline)));
        assert_eq!(attached(&found, "c9"), Some((false, PresenceState::Offline)));
    }

    #[tokio::test]
    async fn the_rpc_takes_nothing_before_hello_and_only_this_engines_token() {
        let r = rig().await;
        // Where it is, for Core, and a token only it knows.
        let info: RnRpcInfo = serde_json::from_str(&std::fs::read_to_string(r.dir.join("rpc.json")).unwrap()).unwrap();
        assert_eq!(
            (info.endpoint, info.rpc_version.as_str(), info.pid),
            (r.endpoint.display().to_string(), "0.1", std::process::id())
        );
        assert!(r.token.len() == 64 && r.token.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        let mut c = connect(&r).await;
        assert_eq!(code(&c.call("health.get", json!({})).await), "unauthenticated");
        assert_eq!(code(&c.hello(&"0".repeat(64)).await), "unauthenticated");
        c.raw("not json\n").await;
        let v = c.reply().await;
        assert_eq!((code(&v), v["requestId"].as_str()), ("invalid_params", Some("")));
        c.raw(&format!(
            "{}\n",
            json!({"protocolVersion": "9.9", "requestId": "v", "method": "health.get", "params": {}})
        ))
        .await;
        let v = c.reply().await;
        assert_eq!((code(&v), v["requestId"].as_str()), ("unsupported_protocol_version", Some("v")));
        assert_eq!(code(&c.call("messages.unsend", json!({})).await), "unknown_method");
        // The connection carries on after each.
        let hello = c.hello(&r.token).await;
        let hello: RnHelloResult = serde_json::from_value(hello["result"].clone()).unwrap();
        assert_eq!((hello.rpc_version.as_str(), hello.endpoint.provider.as_str()), ("0.1", "telosweaver"));
        assert_eq!(hello.endpoint, r.d.core_endpoint());
        let health: RnHealth = serde_json::from_value(c.call("health.get", json!({})).await["result"].clone()).unwrap();
        assert_eq!((health.status, health.upstream_commit.as_str()), (RnHealthStatus::Ok, UPSTREAM_COMMIT));
        assert!(!health.fork_commit.is_empty());
        let providers: Vec<ProviderConnector> =
            serde_json::from_value(c.call("providers.list", json!({})).await["result"].clone()).unwrap();
        assert_eq!(providers.iter().map(|p| p.provider.as_str()).collect::<Vec<_>>(), ["claude", "codex", "agy"]);
        assert!(providers.iter().all(|p| p.version.is_none()));
        assert_eq!(code(&c.call("providers.agy.probe", json!({})).await), "refused");
        assert_eq!(code(&c.call("messages.status", json!({"messageId": "nope"})).await), "not_found");
        assert_eq!(code(&c.call("messages.status", json!({"id": 1})).await), "invalid_params");
        // A second connection says hello again.
        let mut other = connect(&r).await;
        assert_eq!(code(&other.call("health.get", json!({})).await), "unauthenticated");
    }

    #[tokio::test]
    async fn a_frame_over_the_limit_is_answered_then_the_connection_closes() {
        let r = rig().await;
        let mut c = connect(&r).await;
        c.raw(&"a".repeat(MAX_FRAME_BYTES + 1)).await;
        let v = c.next().await.unwrap();
        assert_eq!((code(&v), v["requestId"].as_str()), ("frame_too_large", Some("")));
        assert!(c.next().await.is_none());
    }

    #[tokio::test]
    async fn core_reaches_attached_sessions_only_and_hears_their_receipts_and_replies() {
        let r = rig().await;
        let d = &r.d;
        // A mailbox that came in through telos-rn, and a Claude Code session known only from its registry.
        let review = d.register(Some(1), reg("cli:review", "cli", Some("review")), GitInfo::default()).unwrap();
        d.register(None, reg("claude-code:c1", crate::agent::CLAUDE_CODE, None), GitInfo::default()).unwrap();
        let mut c = connect(&r).await;
        let core = serde_json::from_value::<RnHelloResult>(c.hello(&r.token).await["result"].clone()).unwrap().endpoint;
        let sub = c.call("messages.subscribe", json!({})).await;
        assert!(sub["result"]["subscriptionId"].as_str().is_some_and(|s| !s.is_empty()));

        let found: Vec<SessionEndpoint> =
            serde_json::from_value(c.call("sessions.discover", json!({})).await["result"].clone()).unwrap();
        let by = |provider: &str| found.iter().find(|s| s.endpoint.provider == provider).unwrap().clone();
        let (mailbox, claude) = (by("cli"), by("claude"));
        assert!(mailbox.attached && !claude.attached);
        assert_eq!(mailbox.address.as_deref(), Some(review.address.as_str()));
        // Not Core's own mailbox.
        assert!(found.iter().all(|s| s.endpoint.provider != "telosweaver"));
        let only: Vec<SessionEndpoint> = serde_json::from_value(
            c.call("sessions.discover", json!({"provider": "claude", "workspace": "/work/api/"})).await["result"]
                .clone(),
        )
        .unwrap();
        assert_eq!(only.len(), 1);
        assert_eq!(c.call("sessions.discover", json!({"hostId": "elsewhere"})).await["result"], json!([]));
        let resolved = c.call("sessions.resolve", json!(mailbox.endpoint)).await;
        assert_eq!(resolved["result"]["attached"], true);
        let forged = EndpointRef { fingerprint: "0".into(), ..mailbox.endpoint.clone() };
        assert_eq!(code(&c.call("sessions.resolve", json!(forged)).await), "identity_mismatch");
        let caps = |v: Value| serde_json::from_value::<SessionCapabilities>(v["result"].clone()).unwrap();
        assert_eq!(caps(c.call("sessions.capabilities", json!(mailbox.endpoint)).await).tiers, [DeliveryTier::Pull]);
        assert!(caps(c.call("sessions.capabilities", json!(claude.endpoint)).await).tiers.is_empty());

        // Not to the unattached one; to the mailbox, with its receipt as an event, then delivered once it reads.
        let refused = c.call("messages.send", envelope("m-claude", &core, &claude.endpoint)).await;
        assert_eq!(code(&refused), "refused");
        assert!(d.receipt("m-claude").is_err());
        let sent = c.call("messages.send", envelope("m-1", &core, &mailbox.endpoint)).await;
        assert_eq!(sent["result"]["status"], "queued");
        c.event("receipt", |p| p["messageId"] == "m-1" && p["status"] == "queued").await;
        d.inbox("cli:review".into(), 0, false).await.unwrap();
        c.event("receipt", |p| p["messageId"] == "m-1" && p["status"] == "delivered").await;

        // It replies to TelosWeaver: a message event, the reply delivered once Core has it.
        let res =
            d.send("cli:review".into(), CORE_NAME, "ack", Some("m-1".into()), true, false, None, Vec::new()).await;
        let reply_id = res.unwrap().ids.remove(0);
        let e = c.event("message", |p| p["messageId"] == reply_id.as_str()).await;
        let inbound: InboundMessage = serde_json::from_value(e.payload).unwrap();
        assert_eq!((inbound.body.as_str(), inbound.reply_to.as_deref()), ("ack", Some("m-1")));
        assert_eq!(inbound.sender.as_ref(), Some(&mailbox.endpoint));
        assert_eq!(inbound.sender_address, review.address);
        let reply = d.inner.lock().unwrap().store.receipt(&reply_id).unwrap().unwrap();
        assert_eq!(reply.status, telos::Status::Delivered);
        // Sessions come and go as presence.
        {
            let mut g = d.inner.lock().unwrap();
            g.live.remove("cli:review");
        }
        c.event("presence", |p| p["endpoint"]["nativeSessionId"] == "review" && p["presence"] == "offline").await;
    }

    #[tokio::test]
    async fn a_cursor_resumes_without_repeats_and_one_it_cant_reach_gets_resync_first() {
        let r = rig().await;
        let d = &r.d;
        d.register(Some(1), reg("cli:review", "cli", Some("review")), GitInfo::default()).unwrap();
        let mut c = connect(&r).await;
        let core = serde_json::from_value::<RnHelloResult>(c.hello(&r.token).await["result"].clone()).unwrap().endpoint;
        let to = d.endpoint_of("review").unwrap();
        c.call("messages.subscribe", json!({})).await;
        c.call("messages.send", envelope("m-1", &core, &to)).await;
        let first = c.event("receipt", |p| p["messageId"] == "m-1").await;
        drop(c);
        // Meanwhile: another receipt.
        let mut c = connect(&r).await;
        c.hello(&r.token).await;
        c.call("messages.send", envelope("m-2", &core, &to)).await;
        tokio::time::sleep(POLL * 3).await;
        // From the cursor on: the answer first, then what came after it, and not what came before.
        c.send("s", "messages.subscribe", json!({"cursor": first.cursor})).await;
        let answer = c.next().await.unwrap();
        assert_eq!(answer["requestId"], "s");
        let next = c.event("receipt", |_| true).await;
        assert_eq!(next.payload["messageId"], "m-2");
        assert!(next.sequence > first.sequence);
        // A cursor from another start of the engine: resync first, then the stream goes on.
        let mut old = connect(&r).await;
        old.hello(&r.token).await;
        old.call("messages.subscribe", json!({"cursor": "1:3"})).await;
        let resync = old.next().await.unwrap();
        assert_eq!(resync["params"]["event"]["kind"], "resync_required");
        // One older than the events kept: resync too.
        {
            let mut hub = d.rpc.lock().unwrap();
            for _ in 0..RING {
                hub.publish(RnEventKind::Presence, json!({}));
            }
            let (replay, _) = hub.subscribe(Some(&first.cursor));
            assert_eq!(replay.iter().map(|e| e.kind).collect::<Vec<_>>(), [RnEventKind::ResyncRequired]);
            let latest = hub.ring.back().unwrap().cursor.clone();
            assert!(hub.subscribe(Some(&latest)).0.is_empty());
            assert_eq!(hub.ring.len(), RING);
        }
    }

    #[test]
    fn same_tokens_and_folders() {
        assert!(same("abc", "abc") && !same("abc", "abd") && !same("abc", "ab"));
        assert!(same_dir("/work/api/", "/work/api"));
        assert_eq!(same_dir(r"C:\Work\API", "c:/work/api"), cfg!(windows));
    }
}

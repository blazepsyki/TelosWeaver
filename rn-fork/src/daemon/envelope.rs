//! TelosWeaver envelopes and receipts (Spec v1.5 §12.2, PH0-RN-05): `messages.send` and `messages.status` before the
//! versioned local RPC (PH0-RN-06) carries them.
//!
//! An envelope names its recipient as an endpoint, which must be a session on this host whose fingerprint matches.
//! Its idempotency key is its sender's: the same key with the same envelope is the same message (its receipt comes
//! back), with another envelope it is refused. It must expire within `telos::MAX_TTL_MS`. Nothing here is a Task's
//! completion (DELEG-09).

use super::{
    Daemon,
    mail::{MAX_BODY, offline_note},
};
use crate::{
    proto::{Message, RnRefusal},
    store::{Enveloped, Receipt},
    telos::{self, Status},
    util,
};
use anyhow::Result;
use serde_json::Value;
use std::sync::Arc;
use telos_schema::{
    DeliveryTier,
    rn::{DeliveryReceipt, EndpointRef, MessageBodyType, MessageEnvelope, RnErrorCode as Code},
};

/// The longest id, key or name an envelope may carry.
const MAX_ID: usize = 128;

impl Daemon {
    /// The endpoint of the session at `address` here (`person/name`, or its name), live or remembered: what an
    /// envelope names it by. Until PH0-RN-06's `sessions.resolve`.
    pub(super) fn endpoint_of(&self, address: &str) -> Result<EndpointRef> {
        let name = address.strip_prefix(&format!("{}/", self.person)).unwrap_or(address);
        let g = self.inner.lock().unwrap();
        let key = match g.live.values().find(|l| l.name == name) {
            Some(l) => Some(l.reg.key.clone()),
            None => g.store.session_by_name(name)?.map(|r| r.key),
        };
        let Some(key) = key else { return Err(RnRefusal::error(Code::NotFound, format!("no session {address} here"))) };
        telos::endpoint(&self.host_id, &key).ok_or_else(|| {
            RnRefusal::error(Code::NotFound, format!("{address} is known only by its process: it has no endpoint"))
        })
    }

    /// A message's receipt (`messages.status`).
    pub(super) fn receipt(&self, id: &str) -> Result<DeliveryReceipt> {
        let g = self.inner.lock().unwrap();
        let r =
            g.store.receipt(id)?.ok_or_else(|| RnRefusal::error(Code::NotFound, format!("no message {id} here")))?;
        Ok(contract(&r))
    }

    /// Sends a TelosWeaver envelope (`messages.send`) and returns its receipt.
    pub(super) fn send_envelope(self: &Arc<Self>, env: MessageEnvelope) -> Result<DeliveryReceipt> {
        let now = util::now_ms();
        for (field, v) in [
            ("messageId", &env.message_id),
            ("idempotencyKey", &env.idempotency_key),
            ("correlationId", &env.correlation_id),
        ] {
            check_id(field, v)?;
        }
        for (field, v) in [("taskId", &env.task_id), ("runId", &env.run_id)] {
            if let Some(v) = v {
                check_id(field, v)?;
            }
        }
        check_endpoint("sender", &env.sender)?;
        check_endpoint("recipient", &env.recipient)?;
        let expires_at = telos::parse_rfc3339(&env.expires_at)
            .ok_or_else(|| RnRefusal::error(Code::InvalidParams, "expiresAt is not an RFC 3339 time"))?;
        if expires_at <= now {
            return Err(RnRefusal::error(Code::Expired, "expiresAt has passed"));
        }
        if expires_at > now.saturating_add(telos::MAX_TTL_MS) {
            return Err(RnRefusal::error(Code::InvalidParams, "expiresAt is more than 7 days away"));
        }
        if !env.artifacts.is_empty() {
            return Err(RnRefusal::error(Code::Refused, "artifact references are not supported yet"));
        }
        let body = match (env.body_type, &env.body) {
            (MessageBodyType::Text, Value::String(s)) => s.trim().to_string(),
            (MessageBodyType::Text, _) => {
                return Err(RnRefusal::error(Code::InvalidParams, "a text body is a string"));
            }
            (MessageBodyType::Structured, v) => serde_json::to_string_pretty(v)?,
        };
        if body.is_empty() || body.len() > MAX_BODY {
            let why = format!("the body is empty or over {} KB", MAX_BODY / 1024);
            return Err(RnRefusal::error(Code::InvalidParams, why));
        }
        // Core and RN are on one host in Phase 0: an endpoint elsewhere is a mix-up, not a route (RN-POC-11).
        if env.recipient.host_id != self.host_id {
            let why =
                format!("the recipient is on host {}; this engine is host {}", env.recipient.host_id, self.host_id);
            return Err(RnRefusal::error(Code::IdentityMismatch, why));
        }
        let digest = digest(&env)?;

        let mut g = self.inner.lock().unwrap();
        self.prune(&mut g);
        if let Some((id, was)) = g.store.envelope_by_key(&env.sender.fingerprint, &env.idempotency_key)? {
            if was != digest {
                let why = "this idempotency key was used for another message";
                return Err(RnRefusal::error(Code::IdempotencyConflict, why));
            }
            let r = g.store.receipt(&id)?.ok_or_else(|| anyhow::anyhow!("message {id} has no receipt"))?;
            return Ok(contract(&r));
        }
        if g.store.receipt(&env.message_id)?.is_some() || g.store.has(&env.message_id)? {
            return Err(RnRefusal::error(Code::InvalidParams, "messageId is another message's"));
        }
        // The session by its id, never by an address: live, or remembered under its name.
        let found = telos::keys(&env.recipient).into_iter().find_map(|k| {
            if let Some(l) = g.live.get(&k) {
                return Some((k, l.name.clone(), true));
            }
            g.store.session_by_key(&k).ok().flatten().map(|r| (k, r.name, false))
        });
        let Some((key, name, live)) = found else {
            let e = &env.recipient;
            let why = format!("no {} session {} on this host", e.provider, e.native_session_id);
            return Err(RnRefusal::error(Code::NotFound, why));
        };
        let from = telos::keys(&env.sender)
            .iter()
            .find_map(|k| g.live.get(k))
            .map_or_else(|| self.addr("telosweaver"), |l| self.addr(&l.name));
        let msg = Message {
            id: env.message_id.clone(),
            from,
            from_agent: Some(env.sender.provider.clone()),
            from_repo: None,
            to: self.addr(&name),
            via: None,
            reply_to: None,
            body,
            wake: env.tier != DeliveryTier::Pull,
            notify: false,
            sent_at: now,
            approval: None,
            files: Vec::new(),
        };
        let kept = Enveloped {
            sender_fp: &env.sender.fingerprint,
            key: &env.idempotency_key,
            digest: &digest,
            envelope: &serde_json::to_string(&env)?,
        };
        g.store.insert_envelope(&msg, &key, expires_at, &kept, now)?;
        if live {
            self.deliver(&mut g, &key, vec![msg.clone()], msg.wake);
        } else {
            let note = offline_note(g.store.session_by_key(&key)?.as_ref(), now);
            g.store.set_status(std::slice::from_ref(&msg.id), Status::Queued, Some(&note), now)?;
        }
        let r = g.store.receipt(&msg.id)?.ok_or_else(|| anyhow::anyhow!("message {} has no receipt", msg.id))?;
        drop(g);
        self.arrived.notify_waiters();
        Ok(contract(&r))
    }
}

pub(super) fn contract(r: &Receipt) -> DeliveryReceipt {
    DeliveryReceipt {
        message_id: r.message_id.clone(),
        status: r.status,
        updated_at: telos::rfc3339(r.updated_at),
        reason: r.reason.clone(),
    }
}

fn check_id(field: &str, v: &str) -> Result<()> {
    let ok = |c: char| c.is_ascii_alphanumeric() || "-_.:".contains(c);
    if v.is_empty() || v.len() > MAX_ID || !v.chars().all(ok) {
        let why = format!("{field} is 1 to {MAX_ID} letters, digits or -_.:");
        return Err(RnRefusal::error(Code::InvalidParams, why));
    }
    Ok(())
}

/// An endpoint's fields are well formed. Only the recipient's fingerprint is checked against them: this engine made
/// it (`sessions.discover`); a sender's is its own name for itself, which scopes its idempotency keys.
fn check_endpoint(which: &str, e: &EndpointRef) -> Result<()> {
    for (field, v) in [
        ("hostId", &e.host_id),
        ("provider", &e.provider),
        ("nativeSessionId", &e.native_session_id),
        ("fingerprint", &e.fingerprint),
    ] {
        if v.is_empty() || v.len() > MAX_ID || v.chars().any(char::is_control) {
            let why = format!("{which}.{field} is 1 to {MAX_ID} printable characters");
            return Err(RnRefusal::error(Code::InvalidParams, why));
        }
    }
    if which == "recipient" && e.fingerprint != telos::fingerprint(&e.host_id, &e.provider, &e.native_session_id) {
        let why = "the recipient's fingerprint isn't its host, provider and session id's";
        return Err(RnRefusal::error(Code::IdentityMismatch, why));
    }
    Ok(())
}

/// What an envelope says, as one hash: the same envelope (whatever the order of its fields) has the same digest.
fn digest(env: &MessageEnvelope) -> Result<String> {
    let canonical = sorted(serde_json::to_value(env)?);
    Ok(blake3::hash(serde_json::to_string(&canonical)?.as_bytes()).to_hex().to_string())
}

fn sorted(v: Value) -> Value {
    match v {
        Value::Object(m) => {
            let mut kv: Vec<(String, Value)> = m.into_iter().collect();
            kv.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Object(kv.into_iter().map(|(k, v)| (k, sorted(v))).collect())
        }
        Value::Array(a) => Value::Array(a.into_iter().map(sorted).collect()),
        v => v,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{git::GitInfo, proto::SessionReg, proto::Tier};
    use serde_json::json;

    fn daemon() -> Arc<Daemon> {
        Arc::new(super::super::test_daemon(crate::config::Config::default()))
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

    /// A mailbox session here (`listen --as <name>`), and its endpoint.
    fn mailbox(d: &Arc<Daemon>, name: &str) -> (String, EndpointRef) {
        let key = format!("cli:{name}");
        let address = d.register(Some(1), reg(&key, "cli", Some(name)), GitInfo::default()).unwrap().address;
        (key, d.endpoint_of(&address).unwrap())
    }

    fn core() -> EndpointRef {
        EndpointRef {
            host_id: "core-host".into(),
            provider: "telosweaver".into(),
            native_session_id: "master".into(),
            fingerprint: "core-master".into(),
        }
    }

    fn envelope(id: &str, to: &EndpointRef, body: &str) -> MessageEnvelope {
        MessageEnvelope {
            message_id: id.into(),
            task_id: Some("task-1".into()),
            run_id: None,
            correlation_id: "corr-1".into(),
            idempotency_key: format!("key-{id}"),
            sender: core(),
            recipient: to.clone(),
            tier: DeliveryTier::Pull,
            expires_at: telos::rfc3339(util::now_ms() + 3_600_000),
            body_type: MessageBodyType::Text,
            body: json!(body),
            artifacts: Vec::new(),
        }
    }

    /// The contract's error code a refusal carries.
    fn code(e: anyhow::Error) -> Code {
        e.downcast_ref::<RnRefusal>().unwrap_or_else(|| panic!("not a refusal: {e:#}")).0.code
    }

    #[tokio::test]
    async fn an_envelope_reaches_its_session_by_endpoint_and_is_delivered_once_read() {
        let d = daemon();
        let (key, to) = mailbox(&d, "review");
        assert_eq!((to.provider.as_str(), to.native_session_id.as_str()), ("cli", "review"));
        assert_eq!(to.host_id, d.host_id);
        let r = d.send_envelope(envelope("m-1", &to, "please review")).unwrap();
        assert_eq!((r.message_id.as_str(), r.status), ("m-1", Status::Queued));
        assert_eq!(d.receipt("m-1").unwrap(), r);
        // Read through its inbox: now delivered (and not before).
        let i = d.inbox(key, 0, false).await.unwrap();
        assert_eq!(i.messages.iter().map(|m| m.body.as_str()).collect::<Vec<_>>(), ["please review"]);
        assert!(i.text.contains("id=\"m-1\""));
        let r = d.receipt("m-1").unwrap();
        assert_eq!((r.status, r.reason.as_deref()), (Status::Delivered, Some("shown to the recipient")));
        assert!(telos::parse_rfc3339(&r.updated_at).is_some());
        assert_eq!(code(d.receipt("m-2").unwrap_err()), Code::NotFound);
    }

    #[tokio::test]
    async fn the_same_envelope_again_is_one_message_and_another_under_its_key_is_refused() {
        let d = daemon();
        let (key, to) = mailbox(&d, "review");
        let once = envelope("m-1", &to, "once");
        let first = d.send_envelope(once.clone()).unwrap();
        // A retry (fields in any order) gets the same receipt; nothing is sent twice.
        let again: MessageEnvelope = serde_json::from_value(sorted_desc(serde_json::to_value(&once).unwrap())).unwrap();
        assert_eq!(d.send_envelope(again).unwrap(), first);
        assert_eq!(d.inbox(key, 0, false).await.unwrap().messages.len(), 1);
        // The same key with other words, or for another message: refused.
        let twice = MessageEnvelope { body: json!("twice"), ..once.clone() };
        assert_eq!(code(d.send_envelope(twice.clone()).unwrap_err()), Code::IdempotencyConflict);
        let other = MessageEnvelope { message_id: "m-9".into(), ..once.clone() };
        assert_eq!(code(d.send_envelope(other).unwrap_err()), Code::IdempotencyConflict);
        // Another key, but the id of a message already here: refused.
        let reused = MessageEnvelope { idempotency_key: "key-other".into(), ..once.clone() };
        assert_eq!(code(d.send_envelope(reused).unwrap_err()), Code::InvalidParams);
        // Another sender's key space is its own.
        let theirs = MessageEnvelope {
            message_id: "m-2".into(),
            sender: EndpointRef { fingerprint: "core-other".into(), ..core() },
            ..twice
        };
        assert_eq!(d.send_envelope(theirs).unwrap().status, Status::Queued);
    }

    /// The same JSON with its object fields in reverse order.
    fn sorted_desc(v: Value) -> Value {
        match v {
            Value::Object(m) => {
                let mut kv: Vec<(String, Value)> = m.into_iter().collect();
                kv.sort_by(|a, b| b.0.cmp(&a.0));
                Value::Object(kv.into_iter().map(|(k, v)| (k, sorted_desc(v))).collect())
            }
            v => v,
        }
    }

    #[tokio::test]
    async fn an_envelope_for_an_endpoint_that_isnt_a_session_here_is_refused_and_nothing_is_kept() {
        let d = daemon();
        let (key, to) = mailbox(&d, "review");
        let cases = [
            (EndpointRef { fingerprint: "forged".into(), ..to.clone() }, Code::IdentityMismatch),
            (telos::endpoint("another-host", "cli:review").unwrap(), Code::IdentityMismatch),
            (telos::endpoint(&d.host_id, "cli:nobody").unwrap(), Code::NotFound),
            (telos::endpoint(&d.host_id, "codex:review").unwrap(), Code::NotFound),
        ];
        for (i, (recipient, want)) in cases.into_iter().enumerate() {
            let e = envelope(&format!("bad-{i}"), &recipient, "hi");
            assert_eq!(code(d.send_envelope(e).unwrap_err()), want, "case {i}");
        }
        let now = util::now_ms();
        let bad = [
            (MessageEnvelope { expires_at: telos::rfc3339(now - 1000), ..envelope("e-1", &to, "hi") }, Code::Expired),
            (
                MessageEnvelope {
                    expires_at: telos::rfc3339(now + telos::MAX_TTL_MS + 60_000),
                    ..envelope("e-2", &to, "hi")
                },
                Code::InvalidParams,
            ),
            (MessageEnvelope { expires_at: "tomorrow".into(), ..envelope("e-3", &to, "hi") }, Code::InvalidParams),
            (MessageEnvelope { message_id: "has space".into(), ..envelope("e-4", &to, "hi") }, Code::InvalidParams),
            (envelope("e-5", &to, "  "), Code::InvalidParams),
            (MessageEnvelope { body: json!({"not": "text"}), ..envelope("e-6", &to, "hi") }, Code::InvalidParams),
            (
                MessageEnvelope {
                    artifacts: vec![telos_schema::ArtifactVersionRef {
                        artifact_id: "a".into(),
                        version_id: "1".into(),
                    }],
                    ..envelope("e-7", &to, "hi")
                },
                Code::Refused,
            ),
        ];
        for (e, want) in bad {
            let id = e.message_id.clone();
            assert_eq!(code(d.send_envelope(e).unwrap_err()), want, "{id}");
        }
        assert!(d.inbox(key, 0, true).await.unwrap().messages.is_empty());
        // A structured body is sent as its JSON.
        let structured = MessageEnvelope {
            body_type: MessageBodyType::Structured,
            body: json!({"k": 1}),
            ..envelope("s-1", &to, "")
        };
        assert_eq!(d.send_envelope(structured).unwrap().status, Status::Queued);
        // A session known only by its process has no endpoint to send to.
        let address = d.register(Some(2), reg("proc:44", "zed", None), GitInfo::default()).unwrap().address;
        assert_eq!(code(d.endpoint_of(&address).unwrap_err()), Code::NotFound);
    }

    #[tokio::test]
    async fn mail_for_a_session_is_held_when_another_takes_its_address_until_the_person_approves() {
        let d = daemon();
        let (_, _) = mailbox(&d, "review");
        let old = d.register(Some(2), reg("proc:11", "zed", None), GitInfo::default()).unwrap();
        // It goes; mail sent to its address afterwards is for it, the session that had the address.
        {
            let mut g = d.inner.lock().unwrap();
            let name = g.live.remove("proc:11").unwrap().name;
            g.store.touch_session(&name, util::now_ms()).unwrap();
        }
        let sent = d.send("cli:review".into(), &old.address, "for the old one", None, true, false, None, Vec::new());
        let id = sent.await.unwrap().ids.remove(0);
        // Another session takes the address: it gets nothing by itself.
        let new = d.register(Some(3), reg("proc:12", "zed", None), GitInfo::default()).unwrap();
        assert_eq!((new.address.as_str(), new.unread), (old.address.as_str(), 0));
        assert!(d.inbox("proc:12".into(), 0, true).await.unwrap().messages.is_empty());
        let r = d.receipt(&id).unwrap();
        assert_eq!(r.status, Status::Held);
        assert!(r.reason.unwrap().starts_with("identity_mismatch:"));
        assert_eq!(d.inner.lock().unwrap().store.held().unwrap().len(), 1);
        // The person approves it for whoever has the address now.
        assert_eq!(d.settle(std::slice::from_ref(&id), true).unwrap(), 1);
        let got = d.inbox("proc:12".into(), 0, false).await.unwrap();
        assert_eq!(got.messages.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), [id.as_str()]);
        assert_eq!(d.receipt(&id).unwrap().status, Status::Delivered);
    }

    #[tokio::test]
    async fn an_envelope_for_a_session_by_endpoint_is_held_from_whoever_has_its_address_later() {
        let d = daemon();
        let (_, _) = mailbox(&d, "review");
        d.register(Some(2), reg("proc:11", "zed", None), GitInfo::default()).unwrap();
        let (key, to) = mailbox(&d, "triage");
        // The envelope is for the session, by its id: it waits for that session, under the name it has.
        {
            let mut g = d.inner.lock().unwrap();
            let name = g.live.remove(&key).unwrap().name;
            g.store.touch_session(&name, util::now_ms()).unwrap();
        }
        let r = d.send_envelope(envelope("m-1", &to, "for triage")).unwrap();
        assert_eq!(r.status, Status::Queued);
        assert!(r.reason.unwrap().starts_with("queued: last seen"));
        // It comes back: it gets it.
        mailbox(&d, "triage");
        let got = d.inbox(key, 0, false).await.unwrap();
        assert_eq!(got.messages.len(), 1);
        assert_eq!(d.receipt("m-1").unwrap().status, Status::Delivered);
    }

    #[tokio::test]
    async fn mail_handed_to_an_agents_own_inbox_is_delivered_once_its_transcript_shows_it() {
        let d = daemon();
        let (_, to) = mailbox(&d, "review");
        let key = "claude-code:abc";
        d.register(Some(2), reg(key, crate::agent::CLAUDE_CODE, None), GitInfo::default()).unwrap();
        let claude = telos::endpoint(&d.host_id, key).unwrap();
        assert_eq!(claude.provider, "claude");
        let transcript = std::env::temp_dir().join(format!("telos-rn-transcript-{}.jsonl", ulid::Ulid::generate()));
        std::fs::write(&transcript, "{\"type\":\"user\"}\n").unwrap();
        d.inner.lock().unwrap().live.get_mut(key).unwrap().transcript = Some(transcript.clone());
        for id in ["m-seen", "m-unseen"] {
            d.send_envelope(MessageEnvelope { sender: to.clone(), ..envelope(id, &claude, "hi") }).unwrap();
        }
        // Its MCP server put both into Claude Code's inbox.
        d.handed_off(key, &["m-seen".into(), "m-unseen".into()]).unwrap();
        for id in ["m-seen", "m-unseen"] {
            let r = d.receipt(id).unwrap();
            assert_eq!(r.status, Status::Queued);
            assert!(r.reason.unwrap().contains("handed off to the Claude Code session's inbox"));
        }
        // The transcript shows one of them (as the agent was shown it, inside its frame).
        let line = "{\"type\":\"user\",\"message\":{\"content\":\"<reason-message from=\\\"lars/review\\\" id=\\\"m-seen\\\">hi\"}}\n";
        std::fs::OpenOptions::new().append(true).open(&transcript).unwrap().write_all(line.as_bytes()).unwrap();
        let mut seen = None;
        for _ in 0..40 {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            let r = d.receipt("m-seen").unwrap();
            if r.status == Status::Delivered {
                seen = Some(r);
                break;
            }
        }
        let seen = seen.expect("delivered once the transcript showed it");
        assert_eq!(seen.reason.as_deref(), Some("seen in the recipient session's transcript"));
        assert_eq!(d.receipt("m-unseen").unwrap().status, Status::Queued);
        let _ = std::fs::remove_file(transcript);
    }

    use std::io::Write as _;
}

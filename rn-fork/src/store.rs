//! SQLite: messages, and the sessions this machine has seen: live ones, ones gone for less than a day, and the
//! person's own mailboxes (`terminal`, `listen --as` names). Files that came with messages are saved in the inbox folder, one
//! folder per message, and go with their message.
//!
//! TelosWeaver (Spec v1.5 §12.2, PH0-RN-05): every message stored here has a receipt (`receipts`), and one sent as a
//! TelosWeaver envelope also its idempotency key (`envelopes`). A message is shown only while its receipt is open; it
//! expires `telos::MAX_TTL_MS` after it was stored here at the latest, and is deleted with its receipt `telos::RETAIN_MS` after
//! the receipt became final. Before a schema migration the database is copied into `backups/`.

use crate::{
    files,
    proto::{Attachment, DayCount, Message, Tier},
    telos::{self, Status},
};
use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

pub struct Store {
    db: Connection,
    inbox: PathBuf,
}

/// What the `files` column keeps of a message's files: their folder in the inbox folder, named by an id made here
/// (never the sender's message id, which a teammate could pick to name another's folder, in another case, or a device
/// name), their size, and their names as saved (and their text while the message is held).
#[derive(Serialize, Deserialize)]
struct Kept {
    dir: String,
    bytes: usize,
    files: Vec<Attachment>,
}

/// What [`Store::count`] counts: a message a session here sent, once the receiving machine took it, or one that reached
/// a mailbox here (counted by whoever stores it, or once approved if it was held).
#[derive(Clone, Copy)]
pub enum Counted {
    Sent,
    Received,
}

/// The days whose message counts are shown: today, the six before it, and tomorrow (SQLite date modifiers).
const COUNTS_FROM: &str = "-6 days";
const COUNTS_TO: &str = "+1 day";
/// Days of counts kept: the last ones started, in the order they started rather than by date, so a clock that jumps
/// doesn't push out the real ones.
const COUNTS_KEPT: i64 = 8;

/// A held message that can't be approved now, and why (it stays held).
pub type Unsaved = (String, String);

#[derive(Debug, Clone)]
pub struct Remembered {
    pub name: String,
    pub key: String,
    pub agent: String,
    pub cwd: String,
    pub repo: Option<String>,
    pub branch: Option<String>,
    pub doing: Option<String>,
    pub tier: Tier,
    pub last_seen: i64,
    /// Whether it could ever be reached beyond its own hooks (see `Live::attached`).
    pub attached: bool,
    /// One of the person's own mailboxes (`terminal`, a `listen --as` name): kept for good, never forgotten.
    pub mailbox: bool,
}

impl Remembered {
    /// Whether the name stays this session's while it is offline: it is an agent's conversation, by the id its agent
    /// gave it (`<agent>:<id>`), so the same conversation can come back to it, or one of the person's own mailboxes
    /// (`cli:<name>`); and it could be reached while it ran. A session known only by its process (`proc:<pid>`) has
    /// nothing that could come back, and one only its hooks ever knew (a one-shot run) nothing anyone could reach.
    pub fn kept(&self) -> bool {
        self.attached && !self.key.starts_with("proc:")
    }
}

const SCHEMA: &str = "
PRAGMA journal_mode = WAL;
CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY,
    sender TEXT NOT NULL,
    sender_agent TEXT,
    sender_repo TEXT,
    recipient TEXT NOT NULL,
    via TEXT,
    reply_to TEXT,
    body TEXT NOT NULL,
    wake INTEGER NOT NULL,
    sent_at INTEGER NOT NULL,
    read_at INTEGER,
    held INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS messages_unread ON messages (recipient, read_at);
CREATE TABLE IF NOT EXISTS outbox (
    id TEXT PRIMARY KEY,
    target TEXT NOT NULL,
    msg TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS kv (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS sessions (
    name TEXT PRIMARY KEY,
    key TEXT NOT NULL,
    agent TEXT NOT NULL,
    cwd TEXT NOT NULL,
    repo TEXT,
    branch TEXT,
    doing TEXT,
    tier TEXT NOT NULL,
    last_seen INTEGER NOT NULL,
    attached INTEGER NOT NULL DEFAULT 1,
    mailbox INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS sessions_key ON sessions (key);
CREATE TABLE IF NOT EXISTS counts (
    day TEXT PRIMARY KEY,
    sent INTEGER NOT NULL DEFAULT 0,
    received INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS receipts (
    message_id TEXT PRIMARY KEY,
    status TEXT NOT NULL,
    reason TEXT,
    updated_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    recipient_key TEXT,
    handed_off INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS receipts_open ON receipts (status, expires_at);
CREATE TABLE IF NOT EXISTS envelopes (
    message_id TEXT PRIMARY KEY,
    sender_fp TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    digest TEXT NOT NULL,
    envelope TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE (sender_fp, idempotency_key)
);
";

/// The schema this build writes (`PRAGMA user_version`). 1: receipts and envelopes (TelosWeaver, PH0-RN-05).
const SCHEMA_VERSION: i64 = 1;
/// A message is shown while no final receipt says otherwise.
const OPEN: &str = "NOT EXISTS (SELECT 1 FROM receipts WHERE message_id = messages.id
     AND status IN ('delivered', 'refused', 'expired', 'failed'))";
const FINAL: &str = "('delivered', 'refused', 'expired', 'failed')";

/// What is known of a message's delivery (Spec v1.5 §12.2).
#[derive(Debug, Clone, PartialEq)]
pub struct Receipt {
    pub message_id: String,
    pub status: Status,
    pub reason: Option<String>,
    pub updated_at: i64,
    pub expires_at: i64,
    /// The session the message is for, by key, once known: another session that takes its address doesn't get it.
    pub recipient_key: Option<String>,
    /// It was handed to the agent's own inbox or queue, which doesn't say when the agent reads it.
    pub handed_off: bool,
}

/// A message sent as a TelosWeaver envelope, as kept for its idempotency key.
pub struct Enveloped<'a> {
    pub sender_fp: &'a str,
    pub key: &'a str,
    pub digest: &'a str,
    pub envelope: &'a str,
}

const MESSAGE_COLS: &str =
    "id, sender, sender_agent, sender_repo, recipient, via, reply_to, body, wake, sent_at, approval, files";
const SESSION_COLS: &str = "name, key, agent, cwd, repo, branch, doing, tier, last_seen, attached, mailbox";

impl Store {
    /// The database at `path`, and the folder `inbox` that messages' files are saved in.
    pub fn open(path: &Path, inbox: PathBuf) -> Result<Self> {
        let db = Connection::open(path)?;
        let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > SCHEMA_VERSION {
            anyhow::bail!(
                "{} is from a newer telos-rn (schema {version}; this one writes {SCHEMA_VERSION})",
                path.display()
            );
        }
        let existing: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'messages')",
            [],
            |r| r.get(0),
        )?;
        // A copy from before the migration, which stops if it can't be made.
        if existing && version < SCHEMA_VERSION {
            backup(&db, path, version)?;
        }
        db.execute_batch(SCHEMA)?;
        // Deleted rows are overwritten, not just let go (see `scrub`).
        db.execute_batch("PRAGMA secure_delete = ON;")?;
        // Only this user reads it, like machine.key: it holds the team link. (SQLite gives the log files the mode the
        // database has, but older installs made them with the default.)
        #[cfg(unix)]
        for suffix in ["", "-wal", "-shm"] {
            let mut p = path.as_os_str().to_owned();
            p.push(suffix);
            let p = PathBuf::from(p);
            if p.exists() {
                crate::util::chmod(&p, 0o600)?;
            }
        }
        // Added in 0.10.7.
        if !db.prepare("SELECT 1 FROM pragma_table_info('messages') WHERE name = 'approval'")?.exists([])? {
            db.execute("ALTER TABLE messages ADD COLUMN approval TEXT", [])?;
        }
        // Added with files: their names, and their text while the message is held (once saved, it is in the inbox
        // folder instead).
        if !db.prepare("SELECT 1 FROM pragma_table_info('messages') WHERE name = 'files'")?.exists([])? {
            db.execute("ALTER TABLE messages ADD COLUMN files TEXT", [])?;
        }
        // Added with names kept for their sessions: whether a session could be reached beyond its own hooks. Sessions
        // remembered from before count as reachable, so their names stay theirs.
        if !db.prepare("SELECT 1 FROM pragma_table_info('sessions') WHERE name = 'attached'")?.exists([])? {
            db.execute("ALTER TABLE sessions ADD COLUMN attached INTEGER NOT NULL DEFAULT 1", [])?;
        }
        // Added when the person's own mailboxes were told from other `cli` sessions. Before, every `cli` session was
        // kept for good; of those remembered, the Reason CLI's one-off ask sessions (`ask-` and six hex digits, as it
        // names them) are sessions like any other from now on.
        if !db.prepare("SELECT 1 FROM pragma_table_info('sessions') WHERE name = 'mailbox'")?.exists([])? {
            let tx = db.unchecked_transaction()?;
            tx.execute("ALTER TABLE sessions ADD COLUMN mailbox INTEGER NOT NULL DEFAULT 0", [])?;
            tx.execute(
                "UPDATE sessions SET mailbox = 1 WHERE agent = 'cli'
                 AND NOT (length(name) = 10 AND name GLOB 'ask-[0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f]')",
                [],
            )?;
            tx.commit()?;
        }
        // Schema 1 (TelosWeaver): a receipt for each message from before. Whether one marked read was shown to its
        // agent or only handed to its inbox wasn't kept, so it isn't said to be delivered.
        if version < 1 {
            db.execute(
                "INSERT OR IGNORE INTO receipts (message_id, status, reason, updated_at, expires_at, handed_off)
                 SELECT id, CASE WHEN read_at IS NULL AND held != 0 THEN 'held' ELSE 'queued' END,
                 CASE WHEN read_at IS NULL THEN NULL
                      ELSE 'stored before receipts were kept: whether its recipient read it is not known' END,
                 COALESCE(read_at, sent_at), ?2 + ?1, read_at IS NOT NULL FROM messages",
                params![telos::MAX_TTL_MS, crate::util::now_ms()],
            )?;
        }
        db.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION};"))?;
        let store = Store { db, inbox };
        store.reopened(crate::util::now_ms())?;
        Ok(store)
    }

    /// Mail handed off before this engine (re)started: what became of it is lost with the engine that handed it (and
    /// whatever watched its transcript).
    fn reopened(&self, now: i64) -> Result<()> {
        const WHY: &str = "handed off before the engine restarted: whether its recipient read it is not confirmed";
        self.db.execute(
            "UPDATE receipts SET reason = ?1, updated_at = ?2
             WHERE status = 'queued' AND handed_off != 0 AND reason LIKE 'handed off to %'",
            params![WHY, now],
        )?;
        Ok(())
    }

    /// A folder named in the `files` column: only one of the ids made here (a ULID), so always just inside the inbox.
    fn dir(&self, kept: &Kept) -> Option<PathBuf> {
        ulid::Ulid::from_string(&kept.dir)
            .ok()
            .filter(|u| u.to_string() == kept.dir)
            .map(|_| self.inbox.join(&kept.dir))
    }

    fn kept(&self, id: &str) -> Result<Option<Kept>> {
        let col: Option<Option<String>> =
            self.db.query_row("SELECT files FROM messages WHERE id = ?1", [id], |r| r.get(0)).optional()?;
        Ok(col.flatten().and_then(|c| serde_json::from_str(&c).ok()))
    }

    /// Stores a message; false if one with this id is already here (a retried delivery). Unless it is held, its files
    /// are saved in a new folder of their own first, and `m` then lists them by path.
    pub fn insert(&self, m: &mut Message, held: bool) -> Result<bool> {
        self.insert_for(m, held, None, None)
    }

    /// [`Store::insert`], for the session `recipient_key` when known, expiring at `expires_at` (by default
    /// `telos::MAX_TTL_MS` after now: the sender's clock isn't this one). Its receipt starts `accepted`, or `held`.
    pub fn insert_for(
        &self,
        m: &mut Message,
        held: bool,
        recipient_key: Option<&str>,
        expires_at: Option<i64>,
    ) -> Result<bool> {
        if self.has(&m.id)? {
            return Ok(false);
        }
        let expires_at = expires_at.unwrap_or(crate::util::now_ms().saturating_add(telos::MAX_TTL_MS));
        let mut kept = (!m.files.is_empty()).then(|| Kept {
            dir: ulid::Ulid::generate().to_string(),
            bytes: files::size(&m.files),
            files: m.files.iter().map(|f| Attachment { path: None, ..f.clone() }).collect(),
        });
        let mut saved = None;
        if let Some(k) = kept.as_mut().filter(|_| !held) {
            let dir = self.inbox.join(&k.dir);
            k.files = files::save(&dir, &k.files)?;
            saved = Some(dir);
        }
        let n = self.db.execute(
            &format!(
                "INSERT OR IGNORE INTO messages ({MESSAGE_COLS}, held) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)"
            ),
            params![
                m.id,
                m.from,
                m.from_agent,
                m.from_repo,
                m.to,
                m.via,
                m.reply_to,
                m.body,
                m.wake,
                m.sent_at,
                m.approval.as_ref().map(|a| serde_json::to_string(a).expect("approvals serialize")),
                kept.as_ref().map(|k| serde_json::to_string(k).expect("files serialize")),
                held
            ],
        );
        let n = n.and_then(|n| {
            if n == 1 {
                let status = if held { Status::Held } else { Status::Accepted };
                self.db.execute(
                    "INSERT OR REPLACE INTO receipts (message_id, status, updated_at, expires_at, recipient_key)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![m.id, telos::status_str(status), crate::util::now_ms(), expires_at, recipient_key],
                )?;
            }
            Ok(n)
        });
        match (n, saved, kept) {
            (Ok(n), Some(dir), Some(k)) => {
                m.files = files::located(&dir, &k.files);
                Ok(n == 1)
            }
            (Ok(n), ..) => Ok(n == 1),
            (Err(e), dir, _) => {
                if let Some(dir) = dir {
                    let _ = std::fs::remove_dir_all(dir);
                }
                Err(e.into())
            }
        }
    }

    pub fn message(&self, id: &str) -> Result<Option<Message>> {
        let sql = format!("SELECT {MESSAGE_COLS}, held FROM messages WHERE id = ?1");
        Ok(self.db.query_row(&sql, [id], |r| self.row(r)).optional()?)
    }

    pub fn has(&self, id: &str) -> Result<bool> {
        Ok(self.db.query_row("SELECT 1 FROM messages WHERE id = ?1", [id], |_| Ok(())).optional()?.is_some())
    }

    /// A mailbox's unread messages, in the order they were sent: those sent the same millisecond in the order they came
    /// (their ids, random within a millisecond, say nothing of it).
    pub fn unread(&self, recipient: &str) -> Result<Vec<Message>> {
        let mut st = self.db.prepare_cached(&format!(
            "SELECT {MESSAGE_COLS}, held FROM messages WHERE recipient = ?1 AND read_at IS NULL AND held = 0 AND {OPEN}
             ORDER BY sent_at, rowid"
        ))?;
        self.expire(crate::util::now_ms())?;
        let rows = st.query_map([recipient], |r| self.row(r))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Whether a mailbox has unread mail that wakes its session.
    pub fn unread_wakes(&self, recipient: &str) -> Result<bool> {
        self.expire(crate::util::now_ms())?;
        Ok(self.db.query_row(
            &format!(
                "SELECT EXISTS(SELECT 1 FROM messages WHERE recipient = ?1 AND read_at IS NULL AND held = 0 AND wake != 0
                 AND {OPEN})"
            ),
            [recipient],
            |r| r.get(0),
        )?)
    }

    pub fn unread_count(&self, recipient: &str) -> Result<u32> {
        self.expire(crate::util::now_ms())?;
        Ok(self.db.query_row(
            &format!("SELECT COUNT(*) FROM messages WHERE recipient = ?1 AND read_at IS NULL AND held = 0 AND {OPEN}"),
            [recipient],
            |r| r.get(0),
        )?)
    }

    pub fn held(&self) -> Result<Vec<Message>> {
        self.expire(crate::util::now_ms())?;
        let mut st = self.db.prepare_cached(&format!(
            "SELECT {MESSAGE_COLS}, held FROM messages WHERE held = 1 AND {OPEN} ORDER BY sent_at, rowid"
        ))?;
        let rows = st.query_map([], |r| self.row(r))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The bytes of teammates' files this machine keeps: saved, or held for approval.
    pub fn files_kept(&self) -> Result<usize> {
        let sql = "SELECT COALESCE(SUM(json_extract(files, '$.bytes')), 0) FROM messages WHERE files IS NOT NULL";
        Ok(self.db.query_row(sql, [], |r| r.get::<_, i64>(0))?.max(0) as usize)
    }

    /// The same, of files from `person`'s sessions.
    pub fn files_kept_from(&self, person: &str) -> Result<usize> {
        let sql = "SELECT COALESCE(SUM(json_extract(files, '$.bytes')), 0) FROM messages
                   WHERE files IS NOT NULL AND substr(sender, 1, length(?1) + 1) = ?1 || '/'";
        Ok(self.db.query_row(sql, [person], |r| r.get::<_, i64>(0))?.max(0) as usize)
    }

    /// Lets files go: a day after their message was read, a week after they came while it is unread (the message stays,
    /// without them), and a message held a week with files goes altogether. Then deletes every folder in the inbox
    /// folder that no kept message names.
    pub fn sweep_files(&self, now: i64) -> Result<()> {
        let mut st = self.db.prepare("SELECT id, files, read_at, held FROM messages WHERE files IS NOT NULL")?;
        let rows: Vec<(String, String, Option<i64>, bool)> =
            st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<rusqlite::Result<_>>()?;
        let mut keep = HashSet::new();
        for (id, col, read_at, held) in rows {
            let kept: Option<Kept> = serde_json::from_str(&col).ok();
            let came =
                kept.as_ref().and_then(|k| ulid::Ulid::from_string(&k.dir).ok()).map(|u| u.timestamp_ms() as i64);
            let fresh = came.is_some_and(|t| t > now - files::KEEP_UNREAD_MS);
            if held {
                if !fresh {
                    self.db.execute("DELETE FROM messages WHERE id = ?1", [&id])?;
                }
            } else if (read_at.is_none() && fresh) || read_at.is_some_and(|t| t > now - files::KEEP_MS) {
                if let Some(k) = &kept {
                    keep.insert(k.dir.clone());
                }
            } else {
                self.db.execute("UPDATE messages SET files = NULL WHERE id = ?1", [&id])?;
            }
        }
        if let Ok(entries) = std::fs::read_dir(&self.inbox) {
            for e in entries.flatten() {
                if !keep.contains(&*e.file_name().to_string_lossy()) {
                    let _ = std::fs::remove_dir_all(e.path());
                }
            }
        }
        Ok(())
    }

    /// Approves (`release`) or drops held messages; returns the ones affected, and those whose files couldn't be saved
    /// (which stay held). An approved message's files are saved now; a dropped one's never are.
    pub fn settle_held(&self, ids: &[String], release: bool) -> Result<(Vec<Message>, Vec<Unsaved>)> {
        let held = self.held()?;
        let picked: Vec<Message> = held.into_iter().filter(|m| ids.is_empty() || ids.contains(&m.id)).collect();
        let (mut done, mut unsaved) = (Vec::new(), Vec::new());
        let now = crate::util::now_ms();
        for mut m in picked {
            let id = &[m.id.clone()];
            if !release {
                self.db.execute("DELETE FROM messages WHERE id = ?1", [&m.id])?;
                self.set_status(id, Status::Refused, Some("dropped by the person"), now)?;
                done.push(m);
                continue;
            }
            let mut col = None;
            let mut saved = None;
            if let Some(mut k) = self.kept(&m.id)? {
                let Some(dir) = self.dir(&k) else {
                    unsaved.push((m.id.clone(), "its files' record is damaged".into()));
                    continue;
                };
                // Held after its files were saved (another session took its recipient's address): they are there.
                if dir.exists() {
                    m.files = files::located(&dir, &k.files);
                    self.db.execute("UPDATE messages SET held = 0 WHERE id = ?1", [&m.id])?;
                    self.set_status(id, Status::Queued, Some("approved by the person"), now)?;
                    let _ = self.count(now, Counted::Received);
                    done.push(m);
                    continue;
                }
                match files::save(&dir, &k.files) {
                    Ok(names) => k.files = names,
                    Err(e) => {
                        unsaved.push((m.id.clone(), format!("{e:#}")));
                        continue;
                    }
                }
                m.files = files::located(&dir, &k.files);
                col = Some(serde_json::to_string(&k).expect("files serialize"));
                saved = Some(dir);
            }
            if let Err(e) =
                self.db.execute("UPDATE messages SET held = 0, files = ?2 WHERE id = ?1", params![m.id, col])
            {
                if let Some(dir) = saved {
                    let _ = std::fs::remove_dir_all(dir);
                }
                return Err(e.into());
            }
            self.set_status(id, Status::Queued, Some("approved by the person"), now)?;
            let _ = self.count(now, Counted::Received);
            done.push(m);
        }
        Ok((done, unsaved))
    }

    pub fn outbox_add(&self, target: &str, m: &Message) -> Result<()> {
        self.db.execute(
            "INSERT OR REPLACE INTO outbox (id, target, msg, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![m.id, target, serde_json::to_string(m)?, m.sent_at],
        )?;
        Ok(())
    }

    /// Undelivered messages for a machine; older than a week are given up on.
    pub fn outbox_for(&self, target: &str, now: i64) -> Result<Vec<Message>> {
        self.db.execute("DELETE FROM outbox WHERE created_at < ?1", [now - 7 * 86_400_000])?;
        let mut st = self.db.prepare_cached("SELECT msg FROM outbox WHERE target = ?1 ORDER BY created_at, rowid")?;
        let rows = st.query_map([target], |r| r.get::<_, String>(0))?;
        Ok(rows.filter_map(|r| r.ok().and_then(|s| serde_json::from_str(&s).ok())).collect())
    }

    /// The receiving machine answered for a message: it leaves the outbox, and counts as sent if that machine took it
    /// (once, however many answers come).
    pub fn outbox_done(&self, id: &str, taken: bool) -> Result<()> {
        if self.db.execute("DELETE FROM outbox WHERE id = ?1", [id])? == 1 && taken {
            let _ = self.count(crate::util::now_ms(), Counted::Sent);
        }
        Ok(())
    }

    /// Adds one to today's count (by UTC day). Only numbers are kept, and only for the last few days.
    pub fn count(&self, now: i64, what: Counted) -> Result<()> {
        let col = match what {
            Counted::Sent => "sent",
            Counted::Received => "received",
        };
        self.db.execute(
            &format!(
                "INSERT INTO counts (day, {col}) VALUES (date(?1 / 1000, 'unixepoch'), 1)
                 ON CONFLICT (day) DO UPDATE SET {col} = {col} + 1"
            ),
            [now],
        )?;
        self.db.execute(
            "DELETE FROM counts WHERE rowid NOT IN (SELECT rowid FROM counts ORDER BY rowid DESC LIMIT ?1)",
            [COUNTS_KEPT],
        )?;
        Ok(())
    }

    /// The days shown that had messages, oldest first.
    pub fn counts(&self, now: i64) -> Result<Vec<DayCount>> {
        let mut st = self.db.prepare_cached(
            "SELECT day, sent, received FROM counts
             WHERE day BETWEEN date(?1 / 1000, 'unixepoch', ?2) AND date(?1 / 1000, 'unixepoch', ?3) ORDER BY day",
        )?;
        // SQLite integers are i64; a count is never negative.
        let count = |r: &rusqlite::Row, i: usize| r.get::<_, i64>(i).map(|n| n.max(0) as u64);
        let rows = st.query_map(params![now, COUNTS_FROM, COUNTS_TO], |r| {
            Ok(DayCount { day: r.get(0)?, sent: count(r, 1)?, received: count(r, 2)? })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn get(&self, key: &str) -> Result<Option<String>> {
        Ok(self.db.query_row("SELECT value FROM kv WHERE key = ?1", [key], |r| r.get(0)).optional()?)
    }

    pub fn set(&self, key: &str, value: &str) -> Result<()> {
        self.db.execute("INSERT OR REPLACE INTO kv (key, value) VALUES (?1, ?2)", params![key, value])?;
        Ok(())
    }

    pub fn prefixed(&self, prefix: &str) -> Result<Vec<(String, String)>> {
        let mut st = self.db.prepare_cached("SELECT key, value FROM kv WHERE key >= ?1 AND key < ?1 || x'ff'")?;
        let rows = st.query_map([prefix], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Writes everything in the log into the database and empties the log, so what was deleted (with `secure_delete`,
    /// overwritten) isn't still in it.
    pub fn scrub(&self) -> Result<()> {
        self.db.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
        Ok(())
    }

    pub fn delete_prefix(&self, prefix: &str) -> Result<()> {
        self.db.execute("DELETE FROM kv WHERE key >= ?1 AND key < ?1 || x'ff'", [prefix])?;
        Ok(())
    }

    /// The recipient was shown these messages (its inbox gave them to it, or its hook printed them): delivered.
    pub fn mark_read(&self, ids: &[String], now: i64) -> Result<()> {
        let mut st = self.db.prepare_cached("UPDATE messages SET read_at = ?2 WHERE id = ?1 AND read_at IS NULL")?;
        for id in ids {
            st.execute(params![id, now])?;
        }
        self.set_status(ids, Status::Delivered, Some("shown to the recipient"), now)
    }

    /// These messages went into the agent's own inbox or queue (`how`): not shown again here, but not known to be read.
    pub fn mark_handed_off(&self, ids: &[String], now: i64, how: &str) -> Result<()> {
        let mut st = self.db.prepare_cached("UPDATE messages SET read_at = ?2 WHERE id = ?1 AND read_at IS NULL")?;
        let mut flag = self.db.prepare_cached(&format!(
            "UPDATE receipts SET handed_off = 1 WHERE message_id = ?1 AND status NOT IN {FINAL}"
        ))?;
        for id in ids {
            st.execute(params![id, now])?;
            flag.execute([id])?;
        }
        self.set_status(ids, Status::Queued, Some(&format!("handed off to {how}; not yet confirmed read")), now)
    }

    /// Handed-off messages the recipient was since seen to have (`why`): delivered.
    pub fn confirm(&self, ids: &[String], now: i64, why: &str) -> Result<()> {
        self.set_status(ids, Status::Delivered, Some(why), now)
    }

    /// Moves receipts that aren't final yet to `status`. A final one stays as it is.
    pub fn set_status(&self, ids: &[String], status: Status, reason: Option<&str>, now: i64) -> Result<()> {
        let mut st = self.db.prepare_cached(&format!(
            "UPDATE receipts SET status = ?2, reason = ?3, updated_at = ?4 WHERE message_id = ?1 AND status NOT IN {FINAL}"
        ))?;
        for id in ids {
            st.execute(params![id, telos::status_str(status), reason, now])?;
        }
        Ok(())
    }

    pub fn receipt(&self, id: &str) -> Result<Option<Receipt>> {
        self.expire(crate::util::now_ms())?;
        let row = self
            .db
            .query_row(
                "SELECT message_id, status, reason, updated_at, expires_at, recipient_key, handed_off
                 FROM receipts WHERE message_id = ?1",
                [id],
                |r| {
                    Ok(Receipt {
                        message_id: r.get(0)?,
                        // Only this build writes it: one it can't read would be a damaged row.
                        status: telos::parse_status(&r.get::<_, String>(1)?).unwrap_or(Status::Failed),
                        reason: r.get(2)?,
                        updated_at: r.get(3)?,
                        expires_at: r.get(4)?,
                        recipient_key: r.get(5)?,
                        handed_off: r.get(6)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    /// The receipt of a message that isn't stored in a mailbox here (refused before it was, or sent to a teammate's
    /// machine), so its sender can look it up.
    pub fn record(&self, id: &str, status: Status, reason: Option<&str>, now: i64, expires_at: i64) -> Result<()> {
        self.db.execute(
            "INSERT OR IGNORE INTO receipts (message_id, status, reason, updated_at, expires_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, telos::status_str(status), reason, now, expires_at],
        )?;
        self.set_status(&[id.to_string()], status, reason, now)
    }

    /// A message for an address whose receipt names no session yet is now that session's (`key`).
    pub fn bind_recipient(&self, id: &str, key: &str) -> Result<()> {
        self.db.execute(
            "UPDATE receipts SET recipient_key = ?2 WHERE message_id = ?1 AND recipient_key IS NULL",
            params![id, key],
        )?;
        Ok(())
    }

    /// The person approved a held message for whoever has its address: the session `key`, or (none) the next to take it.
    pub fn rebind_recipient(&self, id: &str, key: Option<&str>) -> Result<()> {
        self.db.execute("UPDATE receipts SET recipient_key = ?2 WHERE message_id = ?1", params![id, key])?;
        Ok(())
    }

    /// Holds a message for the person to approve or drop (`telos-rn held`): it isn't sent on by itself.
    pub fn hold(&self, id: &str, why: &str, now: i64) -> Result<()> {
        self.db.execute("UPDATE messages SET held = 1 WHERE id = ?1", [id])?;
        self.set_status(&[id.to_string()], Status::Held, Some(why), now)
    }

    /// Receipts past their expiry that aren't final: expired, and their messages are no longer shown.
    pub fn expire(&self, now: i64) -> Result<()> {
        self.db.execute(
            "UPDATE receipts SET status = 'expired', updated_at = ?1,
             reason = CASE WHEN handed_off != 0 THEN 'expired before its reading was confirmed'
                           ELSE 'expired before its recipient read it' END
             WHERE status IN ('accepted', 'queued', 'held') AND expires_at <= ?1",
            [now],
        )?;
        Ok(())
    }

    /// Deletes messages, receipts and idempotency keys whose receipt has been final for `telos::RETAIN_MS` (their
    /// files go at the next `sweep_files`).
    pub fn sweep_receipts(&self, now: i64) -> Result<()> {
        self.expire(now)?;
        let cut = now - telos::RETAIN_MS;
        let done = format!("SELECT message_id FROM receipts WHERE status IN {FINAL} AND updated_at < ?1");
        let tx = self.db.unchecked_transaction()?;
        tx.execute(&format!("DELETE FROM messages WHERE id IN ({done})"), [cut])?;
        tx.execute(&format!("DELETE FROM envelopes WHERE message_id IN ({done})"), [cut])?;
        tx.execute(&format!("DELETE FROM receipts WHERE status IN {FINAL} AND updated_at < ?1"), [cut])?;
        tx.commit()?;
        Ok(())
    }

    /// The message an envelope's sender sent under `key`, and the digest of that envelope.
    pub fn envelope_by_key(&self, sender_fp: &str, key: &str) -> Result<Option<(String, String)>> {
        Ok(self
            .db
            .query_row(
                "SELECT message_id, digest FROM envelopes WHERE sender_fp = ?1 AND idempotency_key = ?2",
                params![sender_fp, key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
    }

    /// Stores a message that came as an envelope, its receipt and its idempotency key, all or none. False if a
    /// message with its id is here already.
    pub fn insert_envelope(
        &self,
        m: &Message,
        recipient_key: &str,
        expires_at: i64,
        e: &Enveloped<'_>,
        now: i64,
    ) -> Result<bool> {
        anyhow::ensure!(m.files.is_empty(), "an envelope carries no files");
        let tx = self.db.unchecked_transaction()?;
        let n = tx.execute(
            &format!(
                "INSERT OR IGNORE INTO messages ({MESSAGE_COLS}, held)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, NULL, NULL, 0)"
            ),
            params![m.id, m.from, m.from_agent, m.from_repo, m.to, m.via, m.reply_to, m.body, m.wake, m.sent_at],
        )?;
        if n == 0 {
            return Ok(false);
        }
        tx.execute(
            "INSERT OR REPLACE INTO receipts (message_id, status, updated_at, expires_at, recipient_key)
             VALUES (?1, 'accepted', ?2, ?3, ?4)",
            params![m.id, now, expires_at, recipient_key],
        )?;
        tx.execute(
            "INSERT INTO envelopes (message_id, sender_fp, idempotency_key, digest, envelope, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![m.id, e.sender_fp, e.key, e.digest, e.envelope, now],
        )?;
        tx.commit()?;
        Ok(true)
    }

    /// A renamed session keeps its unread mail.
    pub fn rename(&self, old_name: &str, new_name: &str, old_addr: &str, new_addr: &str) -> Result<()> {
        self.db.execute(
            "UPDATE messages SET recipient = ?2 WHERE recipient = ?1 AND read_at IS NULL",
            params![old_addr, new_addr],
        )?;
        self.db.execute("DELETE FROM sessions WHERE name = ?1", [new_name])?;
        self.db.execute("UPDATE sessions SET name = ?2 WHERE name = ?1", params![old_name, new_name])?;
        Ok(())
    }

    /// Remembers a session under its name. A session has one name: if it had another before (its own was taken by a
    /// live session meanwhile), that one goes, and its unread mail comes along to the new one.
    pub fn upsert_session(&self, s: &Remembered, person: &str) -> Result<()> {
        let before: Vec<String> = {
            let mut st = self.db.prepare_cached("SELECT name FROM sessions WHERE key = ?1 AND name != ?2")?;
            st.query_map(params![s.key, s.name], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?
        };
        for old in before {
            self.rename(&old, &s.name, &format!("{person}/{old}"), &format!("{person}/{}", s.name))?;
        }
        // Once reachable, a session stays so for as long as it is remembered.
        self.db.execute(
            &format!(
                "INSERT INTO sessions ({SESSION_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                 ON CONFLICT (name) DO UPDATE SET key = ?2, agent = ?3, cwd = ?4, repo = ?5, branch = ?6,
                 doing = ?7, tier = ?8, last_seen = ?9,
                 attached = CASE WHEN key = ?2 THEN max(attached, ?10) ELSE ?10 END,
                 mailbox = CASE WHEN key = ?2 THEN max(mailbox, ?11) ELSE ?11 END"
            ),
            params![
                s.name,
                s.key,
                s.agent,
                s.cwd,
                s.repo,
                s.branch,
                s.doing,
                s.tier.as_str(),
                s.last_seen,
                s.attached,
                s.mailbox
            ],
        )?;
        Ok(())
    }

    pub fn touch_session(&self, name: &str, now: i64) -> Result<()> {
        self.db.execute("UPDATE sessions SET last_seen = ?2 WHERE name = ?1", params![name, now])?;
        Ok(())
    }

    /// Forgets the sessions not seen since `before`, other than `live` ones and the person's own mailboxes (`mailbox`), and
    /// deletes the mail they never read, so a session that takes the name later doesn't get it (see `forget_session`).
    pub fn forget_sessions(&self, before: i64, live: &HashSet<&str>, person: &str) -> Result<()> {
        let mut st = self
            .db
            .prepare_cached(&format!("SELECT {SESSION_COLS} FROM sessions WHERE last_seen < ?1 AND mailbox = 0"))?;
        let gone: Vec<Remembered> = st.query_map([before], remembered)?.collect::<rusqlite::Result<_>>()?;
        for r in gone.iter().filter(|r| !live.contains(r.name.as_str())) {
            self.forget_session(r, person)?;
        }
        Ok(())
    }

    /// Forgets one session, with the mail it never read and that mail's files. Mail held for the person goes too when
    /// the name was kept for the session: it was that conversation's, never the next one's. Otherwise it stays the
    /// person's to settle, for whoever has the name.
    pub fn forget_session(&self, r: &Remembered, person: &str) -> Result<()> {
        let unread = if r.kept() {
            "FROM messages WHERE recipient = ?1 AND read_at IS NULL"
        } else {
            "FROM messages WHERE recipient = ?1 AND read_at IS NULL AND held = 0"
        };
        let name = r.name.as_str();
        let to = format!("{person}/{name}");
        let mut cols = self.db.prepare_cached(&format!("SELECT files {unread} AND files IS NOT NULL"))?;
        let cols: Vec<String> = cols.query_map([&to], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        // Their receipts stay, for their senders: it never got them.
        self.db.execute(
            &format!(
                "UPDATE receipts SET status = 'failed', reason = 'its recipient session was forgotten before reading it',
                 updated_at = ?2 WHERE status NOT IN {FINAL} AND message_id IN (SELECT id {unread})"
            ),
            params![&to, crate::util::now_ms()],
        )?;
        self.db.execute(&format!("DELETE {unread}"), [&to])?;
        for kept in cols.iter().filter_map(|c| serde_json::from_str::<Kept>(c).ok()) {
            if let Some(dir) = self.dir(&kept) {
                let _ = std::fs::remove_dir_all(dir);
            }
        }
        self.db.execute("DELETE FROM sessions WHERE name = ?1", [name])?;
        Ok(())
    }

    pub fn session_by_key(&self, key: &str) -> Result<Option<Remembered>> {
        let sql = format!("SELECT {SESSION_COLS} FROM sessions WHERE key = ?1 ORDER BY last_seen DESC LIMIT 1");
        Ok(self.db.query_row(&sql, [key], remembered).optional()?)
    }

    pub fn session_by_name(&self, name: &str) -> Result<Option<Remembered>> {
        let sql = format!("SELECT {SESSION_COLS} FROM sessions WHERE name = ?1");
        Ok(self.db.query_row(&sql, [name], remembered).optional()?)
    }

    pub fn sessions(&self) -> Result<Vec<Remembered>> {
        let mut st = self.db.prepare_cached(&format!("SELECT {SESSION_COLS} FROM sessions ORDER BY name"))?;
        let rows = st.query_map([], remembered)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

impl Store {
    fn row(&self, r: &Row) -> rusqlite::Result<Message> {
        let kept: Option<Kept> = r.get::<_, Option<String>>(11)?.and_then(|f| serde_json::from_str(&f).ok());
        // A held message's files aren't saved yet: their text is here. Once released, they are in their folder.
        let files = match kept {
            Some(k) if r.get::<_, bool>(12)? => k.files,
            Some(k) => self.dir(&k).map(|dir| files::located(&dir, &k.files)).unwrap_or_default(),
            None => Vec::new(),
        };
        message(r).map(|m| Message { files, ..m })
    }
}

fn message(r: &Row) -> rusqlite::Result<Message> {
    Ok(Message {
        id: r.get(0)?,
        from: r.get(1)?,
        from_agent: r.get(2)?,
        from_repo: r.get(3)?,
        to: r.get(4)?,
        via: r.get(5)?,
        reply_to: r.get(6)?,
        body: r.get(7)?,
        wake: r.get(8)?,
        notify: false,
        sent_at: r.get(9)?,
        approval: r.get::<_, Option<String>>(10)?.and_then(|a| serde_json::from_str(&a).ok()),
        files: Vec::new(),
    })
}

/// Copies the database at `path` (schema `version`) into `backups/` beside it, and keeps the last
/// `telos::BACKUPS_KEPT` copies. Nothing for a database in memory.
fn backup(db: &Connection, path: &Path, version: i64) -> Result<()> {
    let Some(dir) = path.parent().filter(|_| path.is_file()) else { return Ok(()) };
    let dir = dir.join("backups");
    std::fs::create_dir_all(&dir)?;
    let stem = path.file_stem().map_or("network".into(), |s| s.to_string_lossy().into_owned());
    let to = dir.join(format!("{stem}-v{version}-{:013}.db", crate::util::now_ms()));
    db.execute("VACUUM INTO ?1", [to.to_string_lossy()])
        .map_err(|e| anyhow::anyhow!("backing up {} before migrating it: {e}", path.display()))?;
    #[cfg(unix)]
    crate::util::chmod(&to, 0o600)?;
    // By when they were made (the name ends with it), oldest first.
    let prefix = format!("{stem}-v");
    let mut copies: Vec<(String, PathBuf)> = std::fs::read_dir(&dir)?
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let when = name.strip_prefix(&prefix)?.strip_suffix(".db")?.rsplit_once('-')?.1.to_string();
            Some((when, e.path()))
        })
        .collect();
    copies.sort();
    for (_, old) in copies.iter().take(copies.len().saturating_sub(telos::BACKUPS_KEPT)) {
        std::fs::remove_file(old)?;
    }
    Ok(())
}

fn remembered(r: &Row) -> rusqlite::Result<Remembered> {
    Ok(Remembered {
        name: r.get(0)?,
        key: r.get(1)?,
        agent: r.get(2)?,
        cwd: r.get(3)?,
        repo: r.get(4)?,
        branch: r.get(5)?,
        doing: r.get(6)?,
        tier: Tier::parse(&r.get::<_, String>(7)?),
        last_seen: r.get(8)?,
        attached: r.get(9)?,
        mailbox: r.get(10)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util;
    use std::collections::HashMap;

    #[test]
    #[cfg(unix)]
    fn the_database_is_this_users_and_keeps_nothing_it_deleted() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("telos-rn-store-{}", ulid::Ulid::generate()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("network.db");
        // A database made before, with the default mode.
        drop(Store::open(&db, temp_inbox()).unwrap());
        util::chmod(&db, 0o644).unwrap();
        let store = Store::open(&db, temp_inbox()).unwrap();
        let secret = "5ec7e75ec7e75ec7e75ec7e75ec7e75e";
        store.set("team", &format!(r#"{{"link":{{"body":{{"secret":"{secret}"}}}}}}"#)).unwrap();
        for f in ["network.db", "network.db-wal", "network.db-shm"] {
            let mode = std::fs::metadata(dir.join(f)).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "{f}");
        }
        store.delete_prefix("team").unwrap();
        store.scrub().unwrap();
        for f in ["network.db", "network.db-wal"] {
            let bytes = std::fs::read(dir.join(f)).unwrap_or_default();
            assert!(!bytes.windows(secret.len()).any(|w| w == secret.as_bytes()), "{f} still holds it");
        }
        drop(store);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn messages_are_counted_by_day_for_a_week_and_kept_across_restarts() {
        let dir = std::env::temp_dir().join(format!("telos-rn-counts-{}", ulid::Ulid::generate()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("network.db");
        let day = 86_400_000;
        let oct20 = 1_792_454_400_000; // 2026-10-20T00:00:00Z
        let store = Store::open(&db, temp_inbox()).unwrap();
        assert_eq!(store.counts(oct20).unwrap(), []);
        for (at, what) in [
            (oct20 - 7 * day, Counted::Sent),
            (oct20 - 6 * day, Counted::Received),
            (oct20 - 1, Counted::Sent),
            (oct20, Counted::Sent),
            (oct20 + day - 1, Counted::Sent),
            (oct20 + 5, Counted::Received),
        ] {
            store.count(at, what).unwrap();
        }
        drop(store);
        let store = Store::open(&db, temp_inbox()).unwrap();
        let count = |day: &str, sent, received| DayCount { day: day.into(), sent, received };
        assert_eq!(
            store.counts(oct20 + 1).unwrap(),
            [count("2026-10-14", 0, 1), count("2026-10-19", 1, 0), count("2026-10-20", 2, 1)]
        );
        // The next day, the oldest goes.
        store.count(oct20 + day, Counted::Received).unwrap();
        assert_eq!(
            store.counts(oct20 + day).unwrap(),
            [count("2026-10-19", 1, 0), count("2026-10-20", 2, 1), count("2026-10-21", 0, 1)]
        );
        drop(store);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn counts_are_shown_to_tomorrow_and_a_clock_that_jumps_loses_no_real_days() {
        let store = Store::open(Path::new(":memory:"), temp_inbox()).unwrap();
        let day = 86_400_000;
        let oct20 = 1_792_454_400_000; // 2026-10-20T00:00:00Z
        let count = |day: &str, sent| DayCount { day: day.into(), sent, received: 0 };
        for at in [oct20 - 6 * day, oct20, oct20 + day, oct20 + 2 * day] {
            store.count(at, Counted::Sent).unwrap();
        }
        let week = [count("2026-10-14", 1), count("2026-10-20", 1), count("2026-10-21", 1)];
        assert_eq!(store.counts(oct20).unwrap(), week, "not the day after tomorrow");
        // The clock is a year ahead for a while, then right again.
        store.count(oct20 + 365 * day, Counted::Sent).unwrap();
        assert_eq!(store.counts(oct20).unwrap(), week);
        assert_eq!(store.counts(oct20 + 365 * day).unwrap(), [count("2027-10-20", 1)]);
        // Only the last days begun are kept.
        for i in 3..20 {
            store.count(oct20 + i * day, Counted::Sent).unwrap();
        }
        let kept: i64 = store.db.query_row("SELECT COUNT(*) FROM counts", [], |r| r.get(0)).unwrap();
        assert_eq!(kept, COUNTS_KEPT);
        assert_eq!(store.counts(oct20 + 19 * day).unwrap().len(), 7);
    }

    #[test]
    fn a_message_counts_as_sent_once_taken_and_as_received_once_in_a_mailbox() {
        let inbox = temp_inbox();
        let store = Store::open(Path::new(":memory:"), inbox.clone()).unwrap();
        let totals = |store: &Store| {
            let days = store.counts(util::now_ms()).unwrap();
            (days.iter().map(|d| d.sent).sum::<u64>(), days.iter().map(|d| d.received).sum::<u64>())
        };
        // Sent: once the other machine takes it (answered twice, after a lost answer), not when it refuses it or it
        // expires in the outbox.
        for id in ["01TAKEN", "01REFUSED", "01EXPIRED"] {
            store.outbox_add("peer", &with_files(id, "sven/api")).unwrap();
        }
        store.outbox_done("01TAKEN", true).unwrap();
        store.outbox_done("01TAKEN", true).unwrap();
        store.outbox_done("01REFUSED", false).unwrap();
        assert!(store.outbox_for("peer", util::now_ms() + 8 * 86_400_000).unwrap().is_empty());
        assert_eq!(totals(&store), (1, 0));
        // Received: held, it counts once approved. Dropped, then sent again after a lost answer, it counts once.
        store.insert(&mut with_files("01HELD", "lars/api"), true).unwrap();
        store.settle_held(&["01HELD".into()], false).unwrap();
        store.insert(&mut with_files("01HELD", "lars/api"), true).unwrap();
        assert_eq!(totals(&store), (1, 0));
        store.settle_held(&["01HELD".into()], true).unwrap();
        assert!(!store.insert(&mut with_files("01HELD", "lars/api"), true).unwrap());
        assert_eq!(totals(&store), (1, 1));
        // Storing one that isn't held counts nothing: the caller counts it if it is a message.
        store.insert(&mut with_files("01NEW", "lars/api"), false).unwrap();
        assert_eq!(totals(&store), (1, 1));
        std::fs::remove_dir_all(&inbox).unwrap();
    }

    #[test]
    fn a_session_gone_a_day_is_forgotten_with_the_mail_it_never_read() {
        // (Not one that is live, nor the person's own mailbox.)
        let store = Store::open(Path::new(":memory:"), temp_inbox()).unwrap();
        let day = 86_400_000;
        let session = |name: &str, agent: &str, last_seen: i64| Remembered {
            mailbox: agent == "cli",
            name: name.into(),
            key: name.into(),
            agent: agent.into(),
            cwd: "~".into(),
            repo: None,
            branch: None,
            doing: None,
            tier: Tier::Push,
            last_seen,
            attached: true,
        };
        for s in [
            session("gone", "claude-code", 0),
            session("recent", "claude-code", 2 * day),
            session("still-up", "claude-code", 0),
            session("terminal", "cli", 0),
            Remembered { key: "proc:7".into(), ..session("no-id", "zed", 0) },
        ] {
            store.upsert_session(&s, "lars").unwrap();
        }
        let to = |id: &str, name: &str| Message {
            id: id.into(),
            from: "sven/claude".into(),
            from_agent: None,
            from_repo: None,
            to: format!("lars/{name}"),
            via: None,
            reply_to: None,
            body: "hi".into(),
            wake: false,
            notify: false,
            sent_at: 0,
            approval: None,
            files: Vec::new(),
        };
        store.insert(&mut to("1", "gone"), false).unwrap();
        store.insert(&mut to("2", "recent"), false).unwrap();
        store.insert(&mut to("3", "terminal"), false).unwrap();
        store.insert(&mut to("4", "gone"), true).unwrap();
        store.insert(&mut to("5", "no-id"), true).unwrap();
        store.forget_sessions(day + 1, &HashSet::from(["still-up"]), "lars").unwrap();
        let names: Vec<String> = store.sessions().unwrap().into_iter().map(|s| s.name).collect();
        assert_eq!(names, ["recent", "still-up", "terminal"]);
        assert_eq!(store.unread_count("lars/gone").unwrap(), 0);
        assert_eq!(store.unread_count("lars/recent").unwrap(), 1);
        assert_eq!(store.unread_count("lars/terminal").unwrap(), 1);
        // Mail held for a conversation goes with it; for a session without an id it stays the person's to settle.
        assert_eq!(store.held().unwrap().iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["5"]);
    }

    #[test]
    fn a_session_has_one_name_and_one_an_older_engine_left_under_two_is_folded_into_its_latest() {
        let store = Store::open(Path::new(":memory:"), temp_inbox()).unwrap();
        let session = |name: &str, last_seen: i64| Remembered {
            name: name.into(),
            key: "claude-code:a".into(),
            agent: "claude-code".into(),
            cwd: "~".into(),
            repo: None,
            branch: None,
            doing: None,
            tier: Tier::Push,
            last_seen,
            attached: true,
            mailbox: false,
        };
        // As older engines could: the same session under two names, mail waiting under each.
        for (name, seen) in [("api-claude", 1), ("api-claude-2", 2)] {
            let s = session(name, seen);
            store
                .db
                .execute(
                    &format!(
                        "INSERT INTO sessions ({SESSION_COLS}) VALUES (?1, ?2, ?3, ?4, NULL, NULL, NULL, ?5, ?6, 1, 0)"
                    ),
                    params![s.name, s.key, s.agent, s.cwd, s.tier.as_str(), s.last_seen],
                )
                .unwrap();
        }
        for (id, name) in [("1", "api-claude"), ("2", "api-claude-2")] {
            store.insert(&mut message(id, &format!("lars/{name}")), false).unwrap();
        }
        assert_eq!(store.session_by_key("claude-code:a").unwrap().unwrap().name, "api-claude-2");
        store.upsert_session(&session("api-claude-2", 3), "lars").unwrap();
        let names: Vec<String> = store.sessions().unwrap().into_iter().map(|s| s.name).collect();
        assert_eq!(names, ["api-claude-2"]);
        assert_eq!(store.unread_count("lars/api-claude-2").unwrap(), 2);
        // Under a new name, the old one goes, and its mail comes along.
        store.upsert_session(&session("auth", 4), "lars").unwrap();
        let names: Vec<String> = store.sessions().unwrap().into_iter().map(|s| s.name).collect();
        assert_eq!(names, ["auth"]);
        assert_eq!(store.unread_count("lars/auth").unwrap(), 2);
    }

    fn message(id: &str, to: &str) -> Message {
        Message {
            id: id.into(),
            from: "sven/claude".into(),
            from_agent: None,
            from_repo: None,
            to: to.into(),
            via: None,
            reply_to: None,
            body: "hi".into(),
            wake: false,
            notify: false,
            sent_at: 0,
            approval: None,
            files: Vec::new(),
        }
    }

    #[test]
    fn a_store_from_before_keeps_the_persons_mailboxes_but_not_the_clis_ask_sessions() {
        let dir = std::env::temp_dir().join(format!("telos-rn-mailbox-{}", ulid::Ulid::generate()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("network.db");
        {
            // The sessions table as 0.12.7 left it.
            let old = Connection::open(&db).unwrap();
            old.execute_batch(
                "CREATE TABLE sessions (name TEXT PRIMARY KEY, key TEXT NOT NULL, agent TEXT NOT NULL, cwd TEXT NOT NULL,
                 repo TEXT, branch TEXT, doing TEXT, tier TEXT NOT NULL, last_seen INTEGER NOT NULL,
                 attached INTEGER NOT NULL DEFAULT 1);
                 INSERT INTO sessions (name, key, agent, cwd, tier, last_seen) VALUES
                 ('terminal', 'cli:terminal', 'cli', '~', 'pull', 0),
                 ('review', 'cli:review', 'cli', '~', 'pull', 0),
                 ('ask-1a2b3c', 'cli:ask-1a2b3c', 'cli', '~', 'pull', 0),
                 ('ask-lunch', 'cli:ask-lunch', 'cli', '~', 'pull', 0),
                 ('api-claude', 'claude-code:a', 'claude-code', '~', 'push', 0);",
            )
            .unwrap();
        }
        let mailboxes = |store: &Store| -> Vec<String> {
            store.sessions().unwrap().into_iter().filter(|s| s.mailbox).map(|s| s.name).collect()
        };
        let store = Store::open(&db, temp_inbox()).unwrap();
        assert_eq!(mailboxes(&store), ["ask-lunch", "review", "terminal"]);
        drop(store);
        // Opened again, nothing changes.
        let store = Store::open(&db, temp_inbox()).unwrap();
        assert_eq!(mailboxes(&store), ["ask-lunch", "review", "terminal"]);
        drop(store);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn messages_sent_the_same_millisecond_come_out_in_the_order_they_came_in() {
        let store = Store::open(Path::new(":memory:"), temp_inbox()).unwrap();
        // Ids within one millisecond are random: here the later message has the smaller one.
        let msg = |id: &str, body: &str| Message { body: body.into(), sent_at: 1_000, ..message(id, "lars/api") };
        for (id, body) in [("01M5Z", "do X"), ("01M5A", "never mind X")] {
            store.insert(&mut msg(id, body), false).unwrap();
            store.insert(&mut msg(&format!("{id}H"), body), true).unwrap();
            store.outbox_add("peer", &msg(id, body)).unwrap();
        }
        let bodies = |ms: Vec<Message>| ms.into_iter().map(|m| m.body).collect::<Vec<_>>();
        assert_eq!(bodies(store.unread("lars/api").unwrap()), ["do X", "never mind X"]);
        assert_eq!(bodies(store.held().unwrap()), ["do X", "never mind X"]);
        assert_eq!(bodies(store.outbox_for("peer", 1_000).unwrap()), ["do X", "never mind X"]);
    }

    fn temp_inbox() -> PathBuf {
        std::env::temp_dir().join(format!("telos-rn-inbox-{}", ulid::Ulid::generate()))
    }

    fn file(name: &str, content: &str) -> Attachment {
        Attachment { name: name.into(), content: content.into(), path: None }
    }

    fn with_files(id: &str, to: &str) -> Message {
        Message {
            id: id.into(),
            from: "sven/claude".into(),
            from_agent: None,
            from_repo: None,
            to: to.into(),
            via: None,
            reply_to: None,
            body: "see the files".into(),
            wake: true,
            notify: false,
            sent_at: 0,
            approval: None,
            files: vec![file("notes.md", "# notes\n"), file("run.log", "ok\n")],
        }
    }

    /// The folder a stored message's files are in (from the path it lists).
    fn folder(m: &Message) -> PathBuf {
        PathBuf::from(m.files[0].path.as_ref().expect("saved")).parent().unwrap().to_path_buf()
    }

    /// The files in a folder: name, text, and whether anyone may execute it.
    fn saved(dir: &Path) -> Vec<(String, String, bool)> {
        let mut out: Vec<(String, String, bool)> = std::fs::read_dir(dir)
            .map(|d| {
                d.flatten()
                    .map(|e| {
                        #[cfg(unix)]
                        let exec =
                            std::os::unix::fs::PermissionsExt::mode(&e.metadata().unwrap().permissions()) & 0o111 != 0;
                        #[cfg(not(unix))]
                        let exec = false;
                        (e.file_name().to_string_lossy().into_owned(), std::fs::read_to_string(e.path()).unwrap(), exec)
                    })
                    .collect()
            })
            .unwrap_or_default();
        out.sort();
        out
    }

    fn folders(inbox: &Path) -> HashSet<String> {
        std::fs::read_dir(inbox)
            .map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
            .unwrap_or_default()
    }

    fn stored_text(store: &Store, id: &str) -> Option<String> {
        store.db.query_row("SELECT files FROM messages WHERE id = ?1", [id], |r| r.get(0)).unwrap()
    }

    #[test]
    fn files_are_saved_in_a_folder_named_here_and_kept_out_of_the_database() {
        let inbox = temp_inbox();
        let store = Store::open(Path::new(":memory:"), inbox.clone()).unwrap();
        let mut m = with_files("01A", "lars/api");
        assert!(store.insert(&mut m, false).unwrap());
        let dir = folder(&m);
        let want =
            vec![("notes.md".to_string(), "# notes\n".to_string(), false), ("run.log".into(), "ok\n".into(), false)];
        assert_eq!(saved(&dir), want);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(dir.join("notes.md")).unwrap().permissions().mode() & 0o777, 0o600);
            assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
        }
        // The message lists them by path, here and when read back; the database keeps only their names.
        assert!(
            m.files
                .iter()
                .all(|f| f.content.is_empty() && f.path.as_deref().unwrap().starts_with(&*dir.display().to_string()))
        );
        assert_eq!(store.unread("lars/api").unwrap()[0].files, m.files);
        assert!(!stored_text(&store, "01A").unwrap().contains("# notes"));
        // A retried delivery changes nothing.
        std::fs::write(dir.join("run.log"), "kept").unwrap();
        assert!(!store.insert(&mut with_files("01A", "lars/api"), false).unwrap());
        assert_eq!(std::fs::read_to_string(dir.join("run.log")).unwrap(), "kept");
        std::fs::remove_dir_all(&inbox).unwrap();
    }

    #[test]
    fn a_teammate_cant_name_another_teammates_folder() {
        let inbox = temp_inbox();
        let store = Store::open(Path::new(":memory:"), inbox.clone()).unwrap();
        let mut alice = with_files("01ALICEMSG", "lars/api");
        store.insert(&mut alice, false).unwrap();
        // The same id in another case (one name on macOS and Windows), and a device name: folders of their own.
        let mut bob = with_files("01alicemsg", "lars/api");
        bob.files = vec![file("notes.md", "bob's")];
        store.insert(&mut bob, false).unwrap();
        store.insert(&mut with_files("CON", "lars/api"), false).unwrap();
        assert_eq!(std::fs::read_to_string(folder(&alice).join("notes.md")).unwrap(), "# notes\n");
        assert_eq!(std::fs::read_to_string(folder(&bob).join("notes.md")).unwrap(), "bob's");
        let names = folders(&inbox);
        assert_eq!(names.len(), 3);
        assert!(names.iter().all(|n| ulid::Ulid::from_string(n).is_ok_and(|u| u.to_string() == *n)), "{names:?}");
        std::fs::remove_dir_all(&inbox).unwrap();
    }

    #[test]
    fn a_held_messages_files_are_saved_only_once_approved() {
        let inbox = temp_inbox();
        let store = Store::open(Path::new(":memory:"), inbox.clone()).unwrap();
        store.insert(&mut with_files("01H", "lars/api"), true).unwrap();
        store.insert(&mut with_files("01D", "lars/api"), true).unwrap();
        assert!(folders(&inbox).is_empty());
        // Held, the text is kept to save later (and shown to the person by name, with no path).
        let held = store.held().unwrap();
        assert_eq!(held[0].files[0].content, "# notes\n");
        assert_eq!(held[0].files[0].path, None);
        // Dropped: nothing is ever written.
        assert_eq!(store.settle_held(&["01D".into()], false).unwrap().0.len(), 1);
        assert!(folders(&inbox).is_empty() && store.message("01D").unwrap().is_none());
        // Approved: saved now, and listed by path.
        let (released, unsaved) = store.settle_held(&["01H".into()], true).unwrap();
        assert!(unsaved.is_empty());
        assert_eq!(saved(&folder(&released[0])).len(), 2);
        assert_eq!(store.unread("lars/api").unwrap()[0].files, released[0].files);
        assert!(!stored_text(&store, "01H").unwrap().contains("# notes"));
        std::fs::remove_dir_all(&inbox).unwrap();
    }

    #[test]
    fn a_held_message_whose_files_cant_be_saved_stays_held_and_the_rest_are_approved() {
        let inbox = temp_inbox();
        let store = Store::open(Path::new(":memory:"), inbox.clone()).unwrap();
        // A name no file system takes (it never passes `check`, but a damaged record could hold one).
        let mut bad = with_files("01BAD", "lars/api");
        bad.files = vec![file("no/such/folder.md", "x")];
        store.insert(&mut bad, true).unwrap();
        store.insert(&mut with_files("01GOOD", "lars/api"), true).unwrap();
        let (released, unsaved) = store.settle_held(&[], true).unwrap();
        assert_eq!(released.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["01GOOD"]);
        assert_eq!(unsaved.len(), 1);
        assert_eq!(unsaved[0].0, "01BAD");
        assert_eq!(store.held().unwrap().iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["01BAD"]);
        std::fs::remove_dir_all(&inbox).unwrap();
    }

    #[test]
    fn files_go_with_their_message_and_none_stay_long() {
        let inbox = temp_inbox();
        let store = Store::open(Path::new(":memory:"), inbox.clone()).unwrap();
        let day = 86_400_000;
        let session = |name: &str| Remembered {
            name: name.into(),
            key: name.into(),
            agent: "claude-code".into(),
            cwd: "~".into(),
            repo: None,
            branch: None,
            doing: None,
            tier: Tier::Push,
            last_seen: 0,
            attached: true,
            mailbox: false,
        };
        store.upsert_session(&session("gone"), "lars").unwrap();
        store.upsert_session(&session("api"), "lars").unwrap();
        let mut gone = with_files("01GONE", "lars/gone");
        store.insert(&mut gone, false).unwrap();
        let mut stored = HashMap::new();
        for id in ["01UNREAD", "01RECENT", "01OLD"] {
            let mut m = with_files(id, "lars/api");
            store.insert(&mut m, false).unwrap();
            stored.insert(id, folder(&m).file_name().unwrap().to_string_lossy().into_owned());
        }
        store.insert(&mut with_files("01HELD", "lars/api"), true).unwrap();
        std::fs::create_dir_all(inbox.join("01ORPHAN")).unwrap();
        // What they take counts against the quota: saved or held.
        let one = files::size(&with_files("x", "y").files);
        assert_eq!(store.files_kept().unwrap(), 5 * one);
        // Counted per sending person too (`sven/claude` sent them all; `sv` is someone else).
        assert_eq!((store.files_kept_from("sven").unwrap(), store.files_kept_from("sv").unwrap()), (5 * one, 0));
        // The unread mail of a session forgotten after a day goes, and its files with it.
        store.forget_sessions(i64::MAX, &HashSet::from(["api"]), "lars").unwrap();
        assert!(!folder(&gone).exists());
        // Read files stay a day; unread ones until read; a folder no message names goes.
        let now = util::now_ms();
        store.mark_read(&["01RECENT".into()], now - day / 2).unwrap();
        store.mark_read(&["01OLD".into()], now - day - 1).unwrap();
        store.sweep_files(now).unwrap();
        assert_eq!(folders(&inbox), HashSet::from([stored["01UNREAD"].clone(), stored["01RECENT"].clone()]));
        assert_eq!(store.files_kept().unwrap(), 3 * one);
        // A week on: unread files go (the message stays, without them), and so does a message held with files.
        store.sweep_files(now + files::KEEP_UNREAD_MS + 1).unwrap();
        assert!(folders(&inbox).is_empty());
        assert!(store.unread("lars/api").unwrap().iter().all(|m| m.files.is_empty()));
        assert_eq!(store.unread_count("lars/api").unwrap(), 1);
        assert!(store.held().unwrap().is_empty());
        assert_eq!(store.files_kept().unwrap(), 0);
        std::fs::remove_dir_all(&inbox).unwrap();
    }
}

/// Receipts, expiry, retention, restart and backups (TelosWeaver, Spec v1.5 §12.2, PH0-RN-05).
#[cfg(test)]
mod receipt_tests {
    use super::*;
    use crate::util;

    fn store() -> Store {
        Store::open(Path::new(":memory:"), temp_dir("inbox")).unwrap()
    }

    fn temp_dir(what: &str) -> PathBuf {
        std::env::temp_dir().join(format!("telos-rn-{what}-{}", ulid::Ulid::generate()))
    }

    fn msg(id: &str) -> Message {
        Message {
            id: id.into(),
            from: "sven/claude".into(),
            from_agent: None,
            from_repo: None,
            to: "lars/api".into(),
            via: None,
            reply_to: None,
            body: format!("body of {id}"),
            wake: true,
            notify: false,
            sent_at: util::now_ms(),
            approval: None,
            files: Vec::new(),
        }
    }

    fn status(store: &Store, id: &str) -> (Status, Option<String>) {
        let r = store.receipt(id).unwrap().unwrap();
        (r.status, r.reason)
    }

    fn unread(store: &Store) -> Vec<String> {
        store.unread("lars/api").unwrap().into_iter().map(|m| m.id).collect()
    }

    #[test]
    fn a_message_is_delivered_only_once_its_recipient_was_shown_it() {
        let store = store();
        let now = util::now_ms();
        for id in ["SHOWN", "HANDED"] {
            assert!(store.insert_for(&mut msg(id), false, Some("claude-code:a"), None).unwrap());
            assert_eq!(status(&store, id), (Status::Accepted, None));
        }
        // Into the agent's own inbox: not shown here again, and not said to be read.
        store.mark_handed_off(&["HANDED".into()], now, "the Claude Code session's inbox").unwrap();
        assert_eq!(unread(&store), ["SHOWN"]);
        let r = store.receipt("HANDED").unwrap().unwrap();
        assert_eq!((r.status, r.handed_off), (Status::Queued, true));
        assert!(r.reason.unwrap().contains("not yet confirmed read"));
        // Its hook printed it, or its inbox gave it: delivered.
        store.mark_read(&["SHOWN".into()], now).unwrap();
        assert_eq!(status(&store, "SHOWN"), (Status::Delivered, Some("shown to the recipient".into())));
        // Its transcript shows the handed-off one: delivered too.
        store.confirm(&["HANDED".into()], now, "seen in the recipient session's transcript").unwrap();
        assert_eq!(status(&store, "HANDED").0, Status::Delivered);
        // A final receipt never changes.
        store.set_status(&["SHOWN".into()], Status::Queued, Some("again"), now).unwrap();
        store.mark_handed_off(&["SHOWN".into()], now, "anywhere").unwrap();
        assert_eq!(status(&store, "SHOWN"), (Status::Delivered, Some("shown to the recipient".into())));
        // The same message again is no second one (a retried delivery).
        assert!(!store.insert_for(&mut msg("SHOWN"), false, None, None).unwrap());
        assert_eq!(store.receipt("SHOWN").unwrap().unwrap().recipient_key.as_deref(), Some("claude-code:a"));
    }

    #[test]
    fn mail_past_its_expiry_is_expired_and_no_longer_shown() {
        let store = store();
        let now = util::now_ms();
        store.insert_for(&mut msg("LATE"), false, None, Some(now - 1)).unwrap();
        store.insert_for(&mut msg("HANDED"), false, None, Some(now + 60_000)).unwrap();
        store.insert_for(&mut msg("HELD"), true, None, Some(now - 1)).unwrap();
        store.insert_for(&mut msg("FRESH"), false, None, None).unwrap();
        store.mark_handed_off(&["HANDED".into()], now, "the Codex session's queue").unwrap();
        assert_eq!(unread(&store), ["FRESH"]);
        assert_eq!(status(&store, "LATE"), (Status::Expired, Some("expired before its recipient read it".into())));
        assert_eq!(status(&store, "HELD").0, Status::Expired);
        // Not offered for approval once expired.
        assert!(store.held().unwrap().is_empty());
        // One handed off whose reading was never seen expires as such.
        store.expire(now + 60_000).unwrap();
        let expired = (Status::Expired, Some("expired before its reading was confirmed".into()));
        assert_eq!(status(&store, "HANDED"), expired);
        // An expired one isn't delivered after all.
        store.mark_read(&["LATE".into()], now).unwrap();
        assert_eq!(status(&store, "LATE").0, Status::Expired);
        // Without an expiry it lasts a week from when it was stored, whatever the sender's clock says.
        let r = store.receipt("FRESH").unwrap().unwrap();
        assert!((r.expires_at - now - telos::MAX_TTL_MS).abs() < 60_000);
    }

    #[test]
    fn a_final_receipt_goes_with_its_message_and_key_a_week_on() {
        let store = store();
        let now = util::now_ms();
        let e = Enveloped { sender_fp: "core", key: "k1", digest: "d1", envelope: "{}" };
        store.insert_envelope(&msg("ENV"), "cli:review", now + 60_000, &e, now).unwrap();
        store.insert_for(&mut msg("OPEN"), false, None, None).unwrap();
        store.mark_read(&["ENV".into()], now).unwrap();
        store.sweep_receipts(now + telos::RETAIN_MS - 60_000).unwrap();
        assert!(store.has("ENV").unwrap() && store.envelope_by_key("core", "k1").unwrap().is_some());
        // A week after it became final: message, receipt and idempotency key all go. An open one stays (until it
        // expires, then a week on from that).
        store.sweep_receipts(now + telos::RETAIN_MS + 60_000).unwrap();
        assert!(!store.has("ENV").unwrap());
        assert!(store.receipt("ENV").unwrap().is_none());
        assert!(store.envelope_by_key("core", "k1").unwrap().is_none());
        assert!(store.has("OPEN").unwrap());
    }

    #[test]
    fn an_envelope_is_stored_whole_or_not_at_all() {
        let store = store();
        let now = util::now_ms();
        let e = Enveloped { sender_fp: "core", key: "k1", digest: "d1", envelope: "{}" };
        assert!(store.insert_envelope(&msg("ENV1"), "cli:review", now + 60_000, &e, now).unwrap());
        // Its key is taken: nothing of the second is kept.
        assert!(store.insert_envelope(&msg("ENV2"), "cli:review", now + 60_000, &e, now).is_err());
        assert!(!store.has("ENV2").unwrap() && store.receipt("ENV2").unwrap().is_none());
        assert_eq!(store.envelope_by_key("core", "k1").unwrap(), Some(("ENV1".into(), "d1".into())));
        // Its id is taken: not stored again.
        let e2 = Enveloped { key: "k2", ..e };
        assert!(!store.insert_envelope(&msg("ENV1"), "cli:review", now + 60_000, &e2, now).unwrap());
        assert!(store.envelope_by_key("core", "k2").unwrap().is_none());
    }

    #[test]
    fn after_a_restart_mail_handed_off_is_unconfirmed_and_the_rest_waits_as_it_was() {
        let dir = temp_dir("db");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("network.db");
        let now = util::now_ms();
        {
            let store = Store::open(&path, dir.join("inbox")).unwrap();
            for id in ["HANDED", "WAITING", "READ"] {
                store.insert_for(&mut msg(id), false, Some("claude-code:a"), None).unwrap();
            }
            store.mark_handed_off(&["HANDED".into()], now, "the Claude Code session's inbox").unwrap();
            store.mark_read(&["READ".into()], now).unwrap();
        }
        let store = Store::open(&path, dir.join("inbox")).unwrap();
        let r = store.receipt("HANDED").unwrap().unwrap();
        assert_eq!(r.status, Status::Queued);
        let why = r.reason.unwrap();
        assert!(why.contains("before the engine restarted: whether its recipient read it is not confirmed"), "{why}");
        // Never said to be delivered, and not shown twice either.
        assert_eq!(unread(&store), ["WAITING"]);
        assert_eq!(status(&store, "WAITING").0, Status::Accepted);
        assert_eq!(status(&store, "READ").0, Status::Delivered);
        // A fresh database is no migration: nothing was backed up.
        assert!(!dir.join("backups").exists());
        drop(store);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_migration_backs_the_database_up_first_and_keeps_the_last_three_copies() {
        let dir = temp_dir("db");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("network.db");
        // A database as the engine wrote it before receipts: one message read, one unread, one held.
        let old = |n: usize| {
            let db = Connection::open(&path).unwrap();
            db.execute_batch("DROP TABLE IF EXISTS receipts; DROP TABLE IF EXISTS envelopes; PRAGMA user_version = 0;")
                .unwrap();
            db.execute("DELETE FROM messages", []).unwrap();
            for (id, read, held) in [("R", Some(5), 0), ("U", None, 0), ("H", None, 1)] {
                db.execute(
                    &format!(
                        "INSERT INTO messages ({MESSAGE_COLS}, held, read_at) VALUES
                         (?1, 'sven/claude', NULL, NULL, 'lars/api', NULL, NULL, 'hi {n}', 1, 1, NULL, NULL, ?2, ?3)"
                    ),
                    params![id, held, read],
                )
                .unwrap();
            }
        };
        drop(Store::open(&path, dir.join("inbox")).unwrap());
        for n in 0..4 {
            old(n);
            std::thread::sleep(std::time::Duration::from_millis(2));
            let store = Store::open(&path, dir.join("inbox")).unwrap();
            // Read before receipts: whether it was shown wasn't kept, so it isn't said to be delivered.
            let r = store.receipt("R").unwrap().unwrap();
            assert_eq!((r.status, r.handed_off), (Status::Queued, true));
            assert!(r.reason.unwrap().contains("whether its recipient read it is not known"));
            assert_eq!(status(&store, "U"), (Status::Queued, None));
            assert_eq!(status(&store, "H"), (Status::Held, None));
            assert_eq!(unread(&store), ["U"]);
        }
        let mut copies: Vec<String> = std::fs::read_dir(dir.join("backups"))
            .unwrap()
            .flatten()
            .map(|e| e.file_name().into_string().unwrap())
            .collect();
        copies.sort();
        assert_eq!(copies.len(), telos::BACKUPS_KEPT);
        assert!(copies.iter().all(|c| c.starts_with("network-v0-") && c.ends_with(".db")));
        // A copy is the database as it was: the last one's message, no receipts yet.
        let copy = Connection::open(dir.join("backups").join(&copies[2])).unwrap();
        let body: String = copy.query_row("SELECT body FROM messages WHERE id = 'U'", [], |r| r.get(0)).unwrap();
        assert_eq!(body, "hi 3");
        assert_eq!(copy.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
        drop(copy);
        // A database from a newer engine isn't opened (nor migrated down).
        Connection::open(&path).unwrap().execute_batch("PRAGMA user_version = 99;").unwrap();
        let e = Store::open(&path, dir.join("inbox")).err().unwrap().to_string();
        assert!(e.contains("from a newer telos-rn"), "{e}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn mail_a_session_never_read_fails_when_it_is_forgotten_and_settled_mail_says_how() {
        let store = store();
        let r = Remembered {
            name: "api".into(),
            key: "codex:1".into(),
            agent: "codex".into(),
            cwd: "/work/api".into(),
            repo: None,
            branch: None,
            doing: None,
            tier: Tier::Pull,
            last_seen: 0,
            attached: true,
            mailbox: false,
        };
        store.upsert_session(&r, "lars").unwrap();
        store.insert_for(&mut msg("GONE"), false, Some("codex:1"), None).unwrap();
        store.forget_sessions(1, &HashSet::new(), "lars").unwrap();
        assert!(!store.has("GONE").unwrap());
        let failed = (Status::Failed, Some("its recipient session was forgotten before reading it".into()));
        assert_eq!(status(&store, "GONE"), failed);
        // Held for the person: dropped is refused, approved waits for its session again.
        store.insert_for(&mut msg("DROP"), true, None, None).unwrap();
        store.insert_for(&mut msg("KEEP"), true, None, None).unwrap();
        store.settle_held(&["DROP".into()], false).unwrap();
        store.settle_held(&["KEEP".into()], true).unwrap();
        assert_eq!(status(&store, "DROP"), (Status::Refused, Some("dropped by the person".into())));
        assert_eq!(status(&store, "KEEP"), (Status::Queued, Some("approved by the person".into())));
        assert_eq!(unread(&store), ["KEEP"]);
    }
}

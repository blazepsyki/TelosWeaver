//! [`Store`]의 SQLite 구현 (Spec §14.1, H0 D1).
//!
//! - 기록은 계약 타입의 JSON을 그대로 저장하고, 조회·조건 검사에 쓰는 값만 열로 둡니다.
//! - 연결은 하나이며 `locking_mode=EXCLUSIVE`로 파일을 독점합니다. 같은 프로젝트 DB를 다른 프로세스가 열면
//!   [`StoreError::AlreadyOpen`]입니다 (OPS-01 단일 소유권).
//! - `journal_mode=WAL`, `synchronous=FULL`: Commit이 돌아오면 그 기록은 프로세스·OS 장애 뒤에도 남습니다.
//! - Schema 버전은 `PRAGMA user_version`이며 `telos_schema::db_open_decision`의 판정대로 엽니다.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use rusqlite::{
    Connection, ErrorCode, OptionalExtension, Params, Transaction, TransactionBehavior, params,
};
use serde::{Serialize, de::DeserializeOwned};
use telos_schema::{
    ApprovalDecision, ArtifactReference, ArtifactVersion, ArtifactVersionRef, BudgetLedger,
    CORE_DB_SCHEMA_VERSION, ContextSnapshot, DbOpenDecision, IdempotencyCheck, IdempotencyRecord,
    IntentPhase, JournalEntry, JournalKind, JournalScope, JsonObject, Plan, ProviderEvent,
    ResourceLock, RunIntent, RunRecord, RunSession, SessionRecord, TaskRecord, UsageRecord,
    ValidationJob, WorkspaceRecord, check_idempotency, db_open_decision,
};

use super::{AppliedBatch, StateWrite, Store, StoreError, Versioned, WriteBatch};

const DB_FILE_NAME: &str = "core.db";

/// 프로젝트 상태 디렉터리(`<사용자 상태 디렉터리>/telos-weaver/<projectKey>/`) 안의 Core DB 경로입니다.
pub fn core_db_path(project_state_dir: &Path) -> PathBuf {
    project_state_dir.join(DB_FILE_NAME)
}

/// (올라갈 버전, 그 버전으로 올리는 SQL). 버전 순서대로 둡니다.
type Migration = (u32, &'static str);

const MIGRATIONS: &[Migration] = &[(1, SCHEMA_V1)];

const SCHEMA_V1: &str = "
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);
CREATE TABLE plans (plan_id TEXT PRIMARY KEY, version INTEGER NOT NULL, body TEXT NOT NULL);
CREATE TABLE tasks (task_id TEXT PRIMARY KEY, plan_id TEXT NOT NULL, version INTEGER NOT NULL, body TEXT NOT NULL);
CREATE INDEX tasks_by_plan ON tasks (plan_id);
CREATE TABLE runs (run_id TEXT PRIMARY KEY, task_id TEXT NOT NULL, version INTEGER NOT NULL, body TEXT NOT NULL);
CREATE INDEX runs_by_task ON runs (task_id);
CREATE TABLE run_intents (intent_id TEXT PRIMARY KEY, run_id TEXT NOT NULL UNIQUE, finished INTEGER NOT NULL, body TEXT NOT NULL);
CREATE INDEX run_intents_unfinished ON run_intents (finished);
CREATE TABLE sessions (session_id TEXT PRIMARY KEY, owner_task_id TEXT NOT NULL, body TEXT NOT NULL);
CREATE INDEX sessions_by_owner ON sessions (owner_task_id);
CREATE TABLE run_sessions (run_id TEXT NOT NULL, session_id TEXT NOT NULL, body TEXT NOT NULL, PRIMARY KEY (run_id, session_id));
CREATE TABLE context_snapshots (run_id TEXT PRIMARY KEY, body TEXT NOT NULL);
CREATE TABLE workspaces (workspace_id TEXT PRIMARY KEY, owner_task_id TEXT NOT NULL, version INTEGER NOT NULL, body TEXT NOT NULL);
CREATE INDEX workspaces_by_owner ON workspaces (owner_task_id);
CREATE TABLE resource_locks (lock_id TEXT PRIMARY KEY, resource_key TEXT NOT NULL, owner_epoch INTEGER NOT NULL, body TEXT NOT NULL);
CREATE INDEX resource_locks_by_resource ON resource_locks (resource_key);
CREATE TABLE artifact_versions (artifact_id TEXT NOT NULL, version_id TEXT NOT NULL, task_id TEXT NOT NULL, body TEXT NOT NULL, PRIMARY KEY (artifact_id, version_id));
CREATE INDEX artifact_versions_by_task ON artifact_versions (task_id);
CREATE TABLE artifact_references (artifact_id TEXT NOT NULL, version_id TEXT NOT NULL, holder_key TEXT NOT NULL, body TEXT NOT NULL, PRIMARY KEY (artifact_id, version_id, holder_key));
CREATE TABLE validations (validation_id TEXT PRIMARY KEY, task_id TEXT NOT NULL, version INTEGER NOT NULL, body TEXT NOT NULL);
CREATE INDEX validations_by_task ON validations (task_id);
CREATE TABLE approvals (approval_id TEXT PRIMARY KEY, plan_id TEXT NOT NULL, version INTEGER NOT NULL, body TEXT NOT NULL);
CREATE INDEX approvals_by_plan ON approvals (plan_id);
CREATE TABLE budgets (plan_id TEXT PRIMARY KEY, version INTEGER NOT NULL, body TEXT NOT NULL);
CREATE TABLE idempotency (scope TEXT NOT NULL, key TEXT NOT NULL, payload_digest TEXT NOT NULL, body TEXT NOT NULL, PRIMARY KEY (scope, key));
CREATE TABLE usage_records (usage_id TEXT PRIMARY KEY, plan_id TEXT NOT NULL, body TEXT NOT NULL);
CREATE INDEX usage_records_by_plan ON usage_records (plan_id);
CREATE TABLE journal (sequence INTEGER PRIMARY KEY, plan_id TEXT, body TEXT NOT NULL);
CREATE INDEX journal_by_plan ON journal (plan_id, sequence);
CREATE TABLE run_events (run_id TEXT NOT NULL, sequence INTEGER NOT NULL, body TEXT NOT NULL, PRIMARY KEY (run_id, sequence));
INSERT INTO meta (key, value) VALUES ('last_sequence', '0'), ('journal_floor', '1');
";

type Clock = Arc<dyn Fn() -> String + Send + Sync>;

#[derive(Clone, Default)]
pub struct SqliteOptions {
    /// DB 크기 상한(Page 수, 기본 Page는 4096 Byte). 넘으면 쓰기가 [`StoreError::Io`]로 실패합니다.
    pub max_pages: Option<u32>,
    /// Journal 항목 등에 적는 시각. 없으면 현재 UTC 시각(RFC 3339)입니다.
    pub clock: Option<Clock>,
}

pub struct SqliteStore {
    conn: Mutex<Connection>,
    clock: Clock,
}

impl std::fmt::Debug for SqliteStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteStore").finish_non_exhaustive()
    }
}

impl SqliteStore {
    /// DB를 열거나 새로 만듭니다. 이전 Schema면 같은 디렉터리에 백업을 만든 뒤 Migration하고,
    /// 더 새 Schema면 열지 않습니다 (§14.1).
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        Self::open_with(path, SqliteOptions::default())
    }

    pub fn open_with(path: &Path, options: SqliteOptions) -> Result<Self, StoreError> {
        Self::open_at(path, options, CORE_DB_SCHEMA_VERSION, MIGRATIONS)
    }

    fn open_at(
        path: &Path,
        options: SqliteOptions,
        target: u32,
        migrations: &[Migration],
    ) -> Result<Self, StoreError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| StoreError::Io {
                message: format!("create {}: {e}", parent.display()),
            })?;
        }
        let clock = options.clock.unwrap_or_else(|| Arc::new(timestamp));
        let busy = |e: rusqlite::Error| match e.sqlite_error_code() {
            Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked) => StoreError::AlreadyOpen {
                path: path.display().to_string(),
            },
            _ => io(e),
        };

        let mut conn = Connection::open(path).map_err(busy)?;
        conn.busy_timeout(Duration::from_millis(250))
            .map_err(busy)?;
        conn.pragma_update(None, "locking_mode", "EXCLUSIVE")
            .map_err(busy)?;
        let found: u32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(busy)?;
        let tables: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table'",
                [],
                |row| row.get(0),
            )
            .map_err(busy)?;

        // 판정은 계약 함수(`db_open_decision`)와 같은 규칙입니다. 시험에서 다른 목표 버전을 쓸 수 있게 비교만 풀어 씁니다.
        let decision = if target == CORE_DB_SCHEMA_VERSION {
            db_open_decision(found)
        } else {
            match found.cmp(&target) {
                std::cmp::Ordering::Equal => DbOpenDecision::Current,
                std::cmp::Ordering::Less => DbOpenDecision::MigrateWithBackup { from: found },
                std::cmp::Ordering::Greater => DbOpenDecision::TooNew { found },
            }
        };
        match decision {
            DbOpenDecision::TooNew { found } => return Err(StoreError::SchemaTooNew { found }),
            DbOpenDecision::Current => {}
            DbOpenDecision::MigrateWithBackup { from } => {
                if from == 0 && tables > 0 {
                    return Err(StoreError::Corrupt {
                        message: "database has tables but no schema version".into(),
                    });
                }
                if from > 0 {
                    backup(&conn, path, from, &clock())?;
                }
                migrate(&mut conn, from, target, migrations, &clock())?;
            }
        }

        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(busy)?;
        conn.pragma_update(None, "synchronous", "FULL")
            .map_err(busy)?;
        if let Some(pages) = options.max_pages {
            conn.pragma_update(None, "max_page_count", pages)
                .map_err(busy)?;
        }
        // 쓰기 한 번으로 독점 잠금을 잡습니다. 다른 프로세스가 쓰고 있으면 여기서 실패합니다.
        conn.execute(
            "INSERT INTO meta (key, value) VALUES ('opened_at', ?1)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            [clock()],
        )
        .map_err(busy)?;

        Ok(Self {
            conn: Mutex::new(conn),
            clock,
        })
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
        // Transaction은 Drop에서 Rollback되므로 다른 스레드의 Panic 뒤에도 연결을 계속 쓸 수 있습니다.
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn one<T: DeserializeOwned>(
        &self,
        sql: &str,
        params: impl Params,
    ) -> Result<Option<T>, StoreError> {
        let body: Option<String> = self
            .conn()
            .query_row(sql, params, |row| row.get(0))
            .optional()
            .map_err(io)?;
        body.as_deref().map(decode).transpose()
    }

    fn many<T: DeserializeOwned>(
        &self,
        sql: &str,
        params: impl Params,
    ) -> Result<Vec<T>, StoreError> {
        let conn = self.conn();
        let mut statement = conn.prepare(sql).map_err(io)?;
        let rows = statement
            .query_map(params, |row| row.get::<_, String>(0))
            .map_err(io)?;
        let mut out = Vec::new();
        for body in rows {
            out.push(decode(&body.map_err(io)?)?);
        }
        Ok(out)
    }

    fn versioned<T: DeserializeOwned>(
        &self,
        sql: &str,
        params: impl Params,
    ) -> Result<Vec<Versioned<T>>, StoreError> {
        let conn = self.conn();
        let mut statement = conn.prepare(sql).map_err(io)?;
        let rows = statement
            .query_map(params, |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(io)?;
        let mut out = Vec::new();
        for row in rows {
            let (body, version) = row.map_err(io)?;
            out.push(Versioned {
                value: decode(&body)?,
                version: version as u64,
            });
        }
        Ok(out)
    }
}

fn backup(conn: &Connection, path: &Path, from: u32, now: &str) -> Result<(), StoreError> {
    let stamp: String = now.chars().filter(char::is_ascii_digit).collect();
    let mut name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(format!(".v{from}.{stamp}.bak"));
    let target = path.with_file_name(name);
    let failed = |message: String| StoreError::Io {
        message: format!("backup before migration failed: {message}"),
    };
    if target.exists() {
        return Err(failed(format!("{} already exists", target.display())));
    }
    let target_text = target
        .to_str()
        .ok_or_else(|| failed("backup path is not valid UTF-8".into()))?;
    conn.execute("VACUUM INTO ?1", [target_text])
        .map_err(|e| failed(e.to_string()))?;
    Ok(())
}

fn migrate(
    conn: &mut Connection,
    from: u32,
    target: u32,
    migrations: &[Migration],
    now: &str,
) -> Result<(), StoreError> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(io)?;
    let mut reached = from;
    for (version, sql) in migrations {
        if *version <= from || *version > target {
            continue;
        }
        if *version != reached + 1 {
            break;
        }
        tx.execute_batch(sql).map_err(io)?;
        tx.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            params![version, now],
        )
        .map_err(io)?;
        reached = *version;
    }
    if reached != target {
        return Err(StoreError::Corrupt {
            message: format!("no migration path from schema {from} to {target}"),
        });
    }
    tx.pragma_update(None, "user_version", target).map_err(io)?;
    tx.commit().map_err(io)
}

fn io(error: rusqlite::Error) -> StoreError {
    StoreError::Io {
        message: error.to_string(),
    }
}

fn encode<T: Serialize>(value: &T) -> Result<String, StoreError> {
    serde_json::to_string(value).map_err(|e| StoreError::Invalid {
        reason: format!("record is not serializable: {e}"),
    })
}

fn decode<T: DeserializeOwned>(body: &str) -> Result<T, StoreError> {
    serde_json::from_str(body).map_err(|e| StoreError::Corrupt {
        message: format!("{}: {e}", std::any::type_name::<T>()),
    })
}

fn payload<T: Serialize>(key: &str, value: &T) -> Result<JsonObject, StoreError> {
    let value = serde_json::to_value(value).map_err(|e| StoreError::Invalid {
        reason: format!("record is not serializable: {e}"),
    })?;
    let mut object = JsonObject::new();
    object.insert(key.to_string(), value);
    Ok(object)
}

fn conflict(kind: &str, id: &str) -> StoreError {
    StoreError::VersionConflict {
        entity: format!("{kind}:{id}"),
    }
}

/// `recorded → dispatched → started → settled`의 순서. `abandoned`는 `recorded`에서만 갑니다.
fn intent_may_advance(from: IntentPhase, to: IntentPhase) -> bool {
    use IntentPhase::*;
    let rank = |phase| match phase {
        Recorded => 0,
        Dispatched => 1,
        Started => 2,
        Settled => 3,
        Abandoned => 4,
    };
    match (from, to) {
        (Settled | Abandoned, _) => false,
        (Recorded, Abandoned) => true,
        (_, Abandoned) => false,
        _ => rank(to) > rank(from),
    }
}

fn intent_finished(phase: IntentPhase) -> bool {
    matches!(phase, IntentPhase::Settled | IntentPhase::Abandoned)
}

/// 한 Transaction 안에서 쓰기를 차례로 적용합니다. 오류가 나면 호출자가 Transaction을 버려 전체가 취소됩니다.
struct Batch<'a> {
    tx: &'a Transaction<'a>,
    now: &'a str,
    last: u64,
    entries: u64,
}

impl Batch<'_> {
    fn journal(
        &mut self,
        scope: JournalScope,
        kind: JournalKind,
        entity_version: u64,
        payload: JsonObject,
    ) -> Result<(), StoreError> {
        let entry = JournalEntry {
            sequence: self.last + 1,
            recorded_at: self.now.to_string(),
            scope,
            kind,
            entity_version,
            payload,
        };
        self.tx
            .execute(
                "INSERT INTO journal (sequence, plan_id, body) VALUES (?1, ?2, ?3)",
                params![entry.sequence as i64, entry.scope.plan_id, encode(&entry)?],
            )
            .map_err(io)?;
        self.last += 1;
        self.entries += 1;
        Ok(())
    }

    fn body(&self, sql: &str, params: impl Params) -> Result<Option<String>, StoreError> {
        self.tx
            .query_row(sql, params, |row| row.get(0))
            .optional()
            .map_err(io)
    }

    fn plan_of_task(&self, task_id: &str) -> Result<Option<String>, StoreError> {
        self.body("SELECT plan_id FROM tasks WHERE task_id = ?1", [task_id])
    }

    fn scope(&self, task_id: &str, run_id: Option<&str>) -> Result<JournalScope, StoreError> {
        Ok(JournalScope {
            plan_id: self.plan_of_task(task_id)?,
            task_id: Some(task_id.to_string()),
            run_id: run_id.map(str::to_string),
        })
    }

    /// 조건부 갱신의 버전 검사입니다. 새 버전을 돌려줍니다.
    fn next_version(
        &self,
        table: &str,
        id_column: &str,
        kind: &str,
        id: &str,
        expected: Option<u64>,
    ) -> Result<u64, StoreError> {
        let current: Option<i64> = self
            .tx
            .query_row(
                &format!("SELECT version FROM {table} WHERE {id_column} = ?1"),
                [id],
                |row| row.get(0),
            )
            .optional()
            .map_err(io)?;
        match (current, expected) {
            (None, None) => Ok(1),
            (Some(current), Some(expected)) if current as u64 == expected => Ok(expected + 1),
            _ => Err(conflict(kind, id)),
        }
    }

    /// 불변 기록의 저장입니다. 없으면 `insert`를 실행하고 참, 같은 내용이 있으면 거짓, 다른 내용이면 충돌입니다.
    fn insert_immutable<T: Serialize + DeserializeOwned + PartialEq>(
        &self,
        existing: Option<String>,
        value: &T,
        kind: &str,
        id: &str,
    ) -> Result<bool, StoreError> {
        match existing {
            None => Ok(true),
            Some(body) if decode::<T>(&body)? == *value => Ok(false),
            Some(_) => Err(conflict(kind, id)),
        }
    }

    fn write(&mut self, write: StateWrite) -> Result<(), StoreError> {
        match write {
            StateWrite::Plan {
                plan,
                expected_version,
            } => {
                let version =
                    self.next_version("plans", "plan_id", "plan", &plan.id, expected_version)?;
                self.tx
                    .execute(
                        "INSERT INTO plans (plan_id, version, body) VALUES (?1, ?2, ?3)
                         ON CONFLICT (plan_id) DO UPDATE SET version = excluded.version, body = excluded.body",
                        params![plan.id, version as i64, encode(&plan)?],
                    )
                    .map_err(io)?;
                let scope = JournalScope {
                    plan_id: Some(plan.id.clone()),
                    ..JournalScope::default()
                };
                self.journal(
                    scope,
                    JournalKind::PlanChanged,
                    version,
                    payload("plan", &plan)?,
                )
            }
            StateWrite::Task {
                mut record,
                expected_version,
            } => {
                let id = record.spec.id.clone();
                let version =
                    self.next_version("tasks", "task_id", "task", &id, expected_version)?;
                record.state_version = version;
                self.tx
                    .execute(
                        "INSERT INTO tasks (task_id, plan_id, version, body) VALUES (?1, ?2, ?3, ?4)
                         ON CONFLICT (task_id) DO UPDATE SET version = excluded.version, body = excluded.body",
                        params![id, record.spec.plan_id, version as i64, encode(&record)?],
                    )
                    .map_err(io)?;
                let scope = JournalScope {
                    plan_id: Some(record.spec.plan_id.clone()),
                    task_id: Some(id),
                    run_id: None,
                };
                self.journal(
                    scope,
                    JournalKind::TaskChanged,
                    version,
                    payload("task", &record)?,
                )
            }
            StateWrite::Run {
                mut record,
                expected_version,
            } => {
                let version =
                    self.next_version("runs", "run_id", "run", &record.run_id, expected_version)?;
                record.state_version = version;
                self.tx
                    .execute(
                        "INSERT INTO runs (run_id, task_id, version, body) VALUES (?1, ?2, ?3, ?4)
                         ON CONFLICT (run_id) DO UPDATE SET version = excluded.version, body = excluded.body",
                        params![record.run_id, record.task_id, version as i64, encode(&record)?],
                    )
                    .map_err(io)?;
                let scope = self.scope(&record.task_id, Some(&record.run_id))?;
                self.journal(
                    scope,
                    JournalKind::RunChanged,
                    version,
                    payload("run", &record)?,
                )
            }
            StateWrite::Intent { intent } => self.write_intent(intent),
            StateWrite::Session { record } => {
                let existing = self.body(
                    "SELECT body FROM sessions WHERE session_id = ?1",
                    [&record.session_id],
                )?;
                if !self.insert_immutable(existing, &record, "session", &record.session_id)? {
                    return Ok(());
                }
                self.tx
                    .execute(
                        "INSERT INTO sessions (session_id, owner_task_id, body) VALUES (?1, ?2, ?3)",
                        params![record.session_id, record.owner_task_id, encode(&record)?],
                    )
                    .map_err(io)?;
                let scope = self.scope(&record.owner_task_id, Some(&record.created_by_run_id))?;
                self.journal(
                    scope,
                    JournalKind::SessionChanged,
                    0,
                    payload("session", &record)?,
                )
            }
            StateWrite::RunSession { link } => {
                self.tx
                    .execute(
                        "INSERT INTO run_sessions (run_id, session_id, body) VALUES (?1, ?2, ?3)
                         ON CONFLICT (run_id, session_id) DO UPDATE SET body = excluded.body",
                        params![link.run_id, link.session_id, encode(&link)?],
                    )
                    .map_err(io)?;
                let task_id: Option<String> =
                    self.body("SELECT task_id FROM runs WHERE run_id = ?1", [&link.run_id])?;
                let scope = match task_id {
                    Some(task_id) => self.scope(&task_id, Some(&link.run_id))?,
                    None => JournalScope {
                        run_id: Some(link.run_id.clone()),
                        ..JournalScope::default()
                    },
                };
                self.journal(
                    scope,
                    JournalKind::SessionChanged,
                    0,
                    payload("runSession", &link)?,
                )
            }
            StateWrite::ContextSnapshot { snapshot } => {
                let existing = self.body(
                    "SELECT body FROM context_snapshots WHERE run_id = ?1",
                    [&snapshot.run_id],
                )?;
                if self.insert_immutable(
                    existing,
                    &snapshot,
                    "context-snapshot",
                    &snapshot.run_id,
                )? {
                    self.tx
                        .execute(
                            "INSERT INTO context_snapshots (run_id, body) VALUES (?1, ?2)",
                            params![snapshot.run_id, encode(&snapshot)?],
                        )
                        .map_err(io)?;
                }
                Ok(())
            }
            StateWrite::Workspace {
                mut record,
                expected_version,
            } => {
                let version = self.next_version(
                    "workspaces",
                    "workspace_id",
                    "workspace",
                    &record.workspace_id,
                    expected_version,
                )?;
                record.state_version = version;
                self.tx
                    .execute(
                        "INSERT INTO workspaces (workspace_id, owner_task_id, version, body) VALUES (?1, ?2, ?3, ?4)
                         ON CONFLICT (workspace_id) DO UPDATE SET version = excluded.version, body = excluded.body",
                        params![
                            record.workspace_id,
                            record.owner_task_id,
                            version as i64,
                            encode(&record)?
                        ],
                    )
                    .map_err(io)?;
                let scope = self.scope(&record.owner_task_id, None)?;
                self.journal(
                    scope,
                    JournalKind::WorkspaceChanged,
                    version,
                    payload("workspace", &record)?,
                )
            }
            StateWrite::AcquireLock { lock } => self.acquire_lock(lock),
            StateWrite::ReleaseLock {
                lock_id,
                owner_epoch,
            } => {
                let body = self
                    .body(
                        "SELECT body FROM resource_locks WHERE lock_id = ?1 AND owner_epoch = ?2",
                        params![lock_id, owner_epoch as i64],
                    )?
                    .ok_or_else(|| conflict("lock", &lock_id))?;
                let lock: ResourceLock = decode(&body)?;
                self.tx
                    .execute("DELETE FROM resource_locks WHERE lock_id = ?1", [&lock_id])
                    .map_err(io)?;
                self.journal_lock(&lock, false)
            }
            StateWrite::ArtifactVersion { version } => {
                let existing = self.body(
                    "SELECT body FROM artifact_versions WHERE artifact_id = ?1 AND version_id = ?2",
                    params![version.artifact_id, version.version_id],
                )?;
                let id = format!("{}/{}", version.artifact_id, version.version_id);
                if !self.insert_immutable(existing, &version, "artifact-version", &id)? {
                    return Ok(());
                }
                self.tx
                    .execute(
                        "INSERT INTO artifact_versions (artifact_id, version_id, task_id, body) VALUES (?1, ?2, ?3, ?4)",
                        params![
                            version.artifact_id,
                            version.version_id,
                            version.task_id,
                            encode(&version)?
                        ],
                    )
                    .map_err(io)?;
                let scope = self.scope(&version.task_id, Some(&version.run_id))?;
                self.journal(
                    scope,
                    JournalKind::ArtifactRegistered,
                    0,
                    payload("artifactVersion", &version)?,
                )
            }
            StateWrite::ArtifactReference { reference } => {
                let target = &reference.target;
                let exists = self
                    .body(
                        "SELECT artifact_id FROM artifact_versions WHERE artifact_id = ?1 AND version_id = ?2",
                        params![target.artifact_id, target.version_id],
                    )?
                    .is_some();
                if !exists {
                    return Err(StoreError::Invalid {
                        reason: format!(
                            "reference to unknown artifact version {}/{}",
                            target.artifact_id, target.version_id
                        ),
                    });
                }
                self.tx
                    .execute(
                        "INSERT OR IGNORE INTO artifact_references (artifact_id, version_id, holder_key, body) VALUES (?1, ?2, ?3, ?4)",
                        params![
                            target.artifact_id,
                            target.version_id,
                            encode(&reference.holder)?,
                            encode(&reference)?
                        ],
                    )
                    .map_err(io)?;
                Ok(())
            }
            StateWrite::Validation {
                mut job,
                expected_version,
            } => {
                let version = self.next_version(
                    "validations",
                    "validation_id",
                    "validation",
                    &job.validation_id,
                    expected_version,
                )?;
                job.state_version = version;
                self.tx
                    .execute(
                        "INSERT INTO validations (validation_id, task_id, version, body) VALUES (?1, ?2, ?3, ?4)
                         ON CONFLICT (validation_id) DO UPDATE SET version = excluded.version, body = excluded.body",
                        params![job.validation_id, job.task_id, version as i64, encode(&job)?],
                    )
                    .map_err(io)?;
                let scope = self.scope(&job.task_id, None)?;
                self.journal(
                    scope,
                    JournalKind::ValidationRecorded,
                    version,
                    payload("validation", &job)?,
                )
            }
            StateWrite::Approval {
                mut decision,
                expected_version,
            } => {
                let version = self.next_version(
                    "approvals",
                    "approval_id",
                    "approval",
                    &decision.approval_id,
                    expected_version,
                )?;
                decision.state_version = version;
                self.tx
                    .execute(
                        "INSERT INTO approvals (approval_id, plan_id, version, body) VALUES (?1, ?2, ?3, ?4)
                         ON CONFLICT (approval_id) DO UPDATE SET version = excluded.version, body = excluded.body",
                        params![
                            decision.approval_id,
                            decision.plan_id,
                            version as i64,
                            encode(&decision)?
                        ],
                    )
                    .map_err(io)?;
                let scope = JournalScope {
                    plan_id: Some(decision.plan_id.clone()),
                    task_id: decision.task_id.clone(),
                    run_id: decision.run_id.clone(),
                };
                self.journal(
                    scope,
                    JournalKind::ApprovalRecorded,
                    version,
                    payload("approval", &decision)?,
                )
            }
            StateWrite::Budget {
                ledger,
                expected_version,
            } => {
                let version = self.next_version(
                    "budgets",
                    "plan_id",
                    "budget",
                    &ledger.plan_id,
                    expected_version,
                )?;
                self.tx
                    .execute(
                        "INSERT INTO budgets (plan_id, version, body) VALUES (?1, ?2, ?3)
                         ON CONFLICT (plan_id) DO UPDATE SET version = excluded.version, body = excluded.body",
                        params![ledger.plan_id, version as i64, encode(&ledger)?],
                    )
                    .map_err(io)?;
                let scope = JournalScope {
                    plan_id: Some(ledger.plan_id.clone()),
                    ..JournalScope::default()
                };
                self.journal(
                    scope,
                    JournalKind::BudgetChanged,
                    version,
                    payload("budget", &ledger)?,
                )
            }
            StateWrite::Idempotency { record } => {
                let existing = self.body(
                    "SELECT payload_digest FROM idempotency WHERE scope = ?1 AND key = ?2",
                    params![record.scope, record.key],
                )?;
                match check_idempotency(existing.as_deref(), &record.payload_digest) {
                    IdempotencyCheck::New => {}
                    IdempotencyCheck::Replay => {
                        return Err(StoreError::IdempotencyReplay {
                            scope: record.scope,
                            key: record.key,
                        });
                    }
                    IdempotencyCheck::Conflict => {
                        return Err(StoreError::IdempotencyConflict {
                            scope: record.scope,
                            key: record.key,
                        });
                    }
                }
                self.tx
                    .execute(
                        "INSERT INTO idempotency (scope, key, payload_digest, body) VALUES (?1, ?2, ?3, ?4)",
                        params![
                            record.scope,
                            record.key,
                            record.payload_digest,
                            encode(&record)?
                        ],
                    )
                    .map_err(io)?;
                Ok(())
            }
            StateWrite::Usage { record } => {
                let existing = self.body(
                    "SELECT body FROM usage_records WHERE usage_id = ?1",
                    [&record.usage_id],
                )?;
                if !self.insert_immutable(existing, &record, "usage", &record.usage_id)? {
                    return Ok(());
                }
                self.tx
                    .execute(
                        "INSERT INTO usage_records (usage_id, plan_id, body) VALUES (?1, ?2, ?3)",
                        params![record.usage_id, record.plan_id, encode(&record)?],
                    )
                    .map_err(io)?;
                let scope = JournalScope {
                    plan_id: Some(record.plan_id.clone()),
                    task_id: Some(record.task_id.clone()),
                    run_id: Some(record.run_id.clone()),
                };
                self.journal(
                    scope,
                    JournalKind::UsageRecorded,
                    0,
                    payload("usage", &record)?,
                )
            }
            StateWrite::Journal {
                scope,
                kind,
                payload,
            } => {
                if !matches!(kind, JournalKind::DecisionApplied | JournalKind::Control) {
                    return Err(StoreError::Invalid {
                        reason: format!(
                            "journal-only write accepts DecisionApplied and Control, got {kind:?}"
                        ),
                    });
                }
                self.journal(scope, kind, 0, payload)
            }
        }
    }

    fn write_intent(&mut self, intent: RunIntent) -> Result<(), StoreError> {
        let existing = self.body(
            "SELECT body FROM run_intents WHERE intent_id = ?1",
            [&intent.intent_id],
        )?;
        match existing {
            None => {
                if intent.phase != IntentPhase::Recorded {
                    return Err(StoreError::Invalid {
                        reason: format!("new intent {} must start as recorded", intent.intent_id),
                    });
                }
                let taken = self
                    .body(
                        "SELECT intent_id FROM run_intents WHERE run_id = ?1",
                        [&intent.run_id],
                    )?
                    .is_some();
                if taken {
                    return Err(conflict("intent", &intent.intent_id));
                }
                self.tx
                    .execute(
                        "INSERT INTO run_intents (intent_id, run_id, finished, body) VALUES (?1, ?2, 0, ?3)",
                        params![intent.intent_id, intent.run_id, encode(&intent)?],
                    )
                    .map_err(io)?;
            }
            Some(body) => {
                let stored: RunIntent = decode(&body)?;
                let same_request = stored.run_id == intent.run_id
                    && stored.task_id == intent.task_id
                    && stored.idempotency_key == intent.idempotency_key
                    && stored.payload_digest == intent.payload_digest;
                if !same_request || !intent_may_advance(stored.phase, intent.phase) {
                    return Err(conflict("intent", &intent.intent_id));
                }
                self.tx
                    .execute(
                        "UPDATE run_intents SET finished = ?2, body = ?3 WHERE intent_id = ?1",
                        params![
                            intent.intent_id,
                            intent_finished(intent.phase),
                            encode(&intent)?
                        ],
                    )
                    .map_err(io)?;
            }
        }
        let scope = self.scope(&intent.task_id, Some(&intent.run_id))?;
        self.journal(
            scope,
            JournalKind::IntentChanged,
            0,
            payload("intent", &intent)?,
        )
    }

    fn acquire_lock(&mut self, lock: ResourceLock) -> Result<(), StoreError> {
        let resource_key = encode(&lock.resource)?;
        let conflict_error = || StoreError::LockConflict {
            resource: resource_key.clone(),
        };
        if self
            .body(
                "SELECT lock_id FROM resource_locks WHERE lock_id = ?1",
                [&lock.lock_id],
            )?
            .is_some()
        {
            return Err(conflict_error());
        }
        let held: Vec<String> = {
            let mut statement = self
                .tx
                .prepare("SELECT body FROM resource_locks WHERE resource_key = ?1")
                .map_err(io)?;
            let rows = statement
                .query_map([&resource_key], |row| row.get::<_, String>(0))
                .map_err(io)?;
            rows.collect::<Result<_, _>>().map_err(io)?
        };
        for body in held {
            let other: ResourceLock = decode(&body)?;
            if lock.mode.conflicts_with(other.mode) {
                return Err(conflict_error());
            }
        }
        self.tx
            .execute(
                "INSERT INTO resource_locks (lock_id, resource_key, owner_epoch, body) VALUES (?1, ?2, ?3, ?4)",
                params![
                    lock.lock_id,
                    resource_key,
                    lock.owner_epoch as i64,
                    encode(&lock)?
                ],
            )
            .map_err(io)?;
        self.journal_lock(&lock, true)
    }

    fn journal_lock(&mut self, lock: &ResourceLock, held: bool) -> Result<(), StoreError> {
        let scope = self.scope(&lock.holder_task_id, Some(&lock.holder_run_id))?;
        let mut body = payload("lock", lock)?;
        body.insert("held".into(), held.into());
        self.journal(scope, JournalKind::LockChanged, lock.owner_epoch, body)
    }
}

impl Store for SqliteStore {
    fn apply(&self, batch: WriteBatch) -> Result<AppliedBatch, StoreError> {
        let now = (self.clock)();
        let mut conn = self.conn();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(io)?;
        let last: i64 = tx
            .query_row(
                "SELECT CAST(value AS INTEGER) FROM meta WHERE key = 'last_sequence'",
                [],
                |row| row.get(0),
            )
            .map_err(io)?;
        let mut state = Batch {
            tx: &tx,
            now: &now,
            last: last as u64,
            entries: 0,
        };
        for write in batch.writes {
            state.write(write)?;
        }
        let applied = AppliedBatch {
            entries: state.entries,
            last_sequence: state.last,
        };
        if applied.entries > 0 {
            tx.execute(
                "UPDATE meta SET value = ?1 WHERE key = 'last_sequence'",
                [applied.last_sequence.to_string()],
            )
            .map_err(io)?;
        }
        tx.commit().map_err(io)?;
        Ok(applied)
    }

    fn plan(&self, plan_id: &str) -> Result<Option<Versioned<Plan>>, StoreError> {
        Ok(self
            .versioned(
                "SELECT body, version FROM plans WHERE plan_id = ?1",
                [plan_id],
            )?
            .pop())
    }

    fn plans(&self) -> Result<Vec<Versioned<Plan>>, StoreError> {
        self.versioned("SELECT body, version FROM plans ORDER BY rowid", [])
    }

    fn task(&self, task_id: &str) -> Result<Option<TaskRecord>, StoreError> {
        self.one("SELECT body FROM tasks WHERE task_id = ?1", [task_id])
    }

    fn plan_tasks(&self, plan_id: &str) -> Result<Vec<TaskRecord>, StoreError> {
        self.many(
            "SELECT body FROM tasks WHERE plan_id = ?1 ORDER BY rowid",
            [plan_id],
        )
    }

    fn task_runs(&self, task_id: &str) -> Result<Vec<RunRecord>, StoreError> {
        self.many(
            "SELECT body FROM runs WHERE task_id = ?1 ORDER BY rowid",
            [task_id],
        )
    }

    fn run(&self, run_id: &str) -> Result<Option<RunRecord>, StoreError> {
        self.one("SELECT body FROM runs WHERE run_id = ?1", [run_id])
    }

    fn run_intent(&self, run_id: &str) -> Result<Option<RunIntent>, StoreError> {
        self.one("SELECT body FROM run_intents WHERE run_id = ?1", [run_id])
    }

    fn session(&self, session_id: &str) -> Result<Option<SessionRecord>, StoreError> {
        self.one(
            "SELECT body FROM sessions WHERE session_id = ?1",
            [session_id],
        )
    }

    fn task_sessions(&self, owner_task_id: &str) -> Result<Vec<SessionRecord>, StoreError> {
        self.many(
            "SELECT body FROM sessions WHERE owner_task_id = ?1 ORDER BY rowid",
            [owner_task_id],
        )
    }

    fn run_sessions(&self, run_id: &str) -> Result<Vec<RunSession>, StoreError> {
        self.many(
            "SELECT body FROM run_sessions WHERE run_id = ?1 ORDER BY rowid",
            [run_id],
        )
    }

    fn context_snapshot(&self, run_id: &str) -> Result<Option<ContextSnapshot>, StoreError> {
        self.one(
            "SELECT body FROM context_snapshots WHERE run_id = ?1",
            [run_id],
        )
    }

    fn workspace(&self, workspace_id: &str) -> Result<Option<WorkspaceRecord>, StoreError> {
        self.one(
            "SELECT body FROM workspaces WHERE workspace_id = ?1",
            [workspace_id],
        )
    }

    fn task_workspaces(&self, owner_task_id: &str) -> Result<Vec<WorkspaceRecord>, StoreError> {
        self.many(
            "SELECT body FROM workspaces WHERE owner_task_id = ?1 ORDER BY rowid",
            [owner_task_id],
        )
    }

    fn artifact_version(
        &self,
        target: &ArtifactVersionRef,
    ) -> Result<Option<ArtifactVersion>, StoreError> {
        self.one(
            "SELECT body FROM artifact_versions WHERE artifact_id = ?1 AND version_id = ?2",
            params![target.artifact_id, target.version_id],
        )
    }

    fn task_artifacts(&self, task_id: &str) -> Result<Vec<ArtifactVersion>, StoreError> {
        self.many(
            "SELECT body FROM artifact_versions WHERE task_id = ?1 ORDER BY rowid",
            [task_id],
        )
    }

    fn artifact_references(
        &self,
        target: &ArtifactVersionRef,
    ) -> Result<Vec<ArtifactReference>, StoreError> {
        self.many(
            "SELECT body FROM artifact_references WHERE artifact_id = ?1 AND version_id = ?2 ORDER BY rowid",
            params![target.artifact_id, target.version_id],
        )
    }

    fn validation(&self, validation_id: &str) -> Result<Option<ValidationJob>, StoreError> {
        self.one(
            "SELECT body FROM validations WHERE validation_id = ?1",
            [validation_id],
        )
    }

    fn task_validations(&self, task_id: &str) -> Result<Vec<ValidationJob>, StoreError> {
        self.many(
            "SELECT body FROM validations WHERE task_id = ?1 ORDER BY rowid",
            [task_id],
        )
    }

    fn approval(&self, approval_id: &str) -> Result<Option<ApprovalDecision>, StoreError> {
        self.one(
            "SELECT body FROM approvals WHERE approval_id = ?1",
            [approval_id],
        )
    }

    fn plan_approvals(&self, plan_id: &str) -> Result<Vec<ApprovalDecision>, StoreError> {
        self.many(
            "SELECT body FROM approvals WHERE plan_id = ?1 ORDER BY rowid",
            [plan_id],
        )
    }

    fn budget(&self, plan_id: &str) -> Result<Option<Versioned<BudgetLedger>>, StoreError> {
        Ok(self
            .versioned(
                "SELECT body, version FROM budgets WHERE plan_id = ?1",
                [plan_id],
            )?
            .pop())
    }

    fn idempotency(&self, scope: &str, key: &str) -> Result<Option<IdempotencyRecord>, StoreError> {
        self.one(
            "SELECT body FROM idempotency WHERE scope = ?1 AND key = ?2",
            [scope, key],
        )
    }

    fn plan_usage(&self, plan_id: &str) -> Result<Vec<UsageRecord>, StoreError> {
        self.many(
            "SELECT body FROM usage_records WHERE plan_id = ?1 ORDER BY rowid",
            [plan_id],
        )
    }

    fn unfinished_intents(&self) -> Result<Vec<RunIntent>, StoreError> {
        self.many(
            "SELECT body FROM run_intents WHERE finished = 0 ORDER BY rowid",
            [],
        )
    }

    fn locks(&self) -> Result<Vec<ResourceLock>, StoreError> {
        self.many("SELECT body FROM resource_locks ORDER BY rowid", [])
    }

    fn journal_after(
        &self,
        plan_id: Option<&str>,
        after_sequence: u64,
        limit: usize,
    ) -> Result<Option<Vec<JournalEntry>>, StoreError> {
        let floor: i64 = self
            .conn()
            .query_row(
                "SELECT CAST(value AS INTEGER) FROM meta WHERE key = 'journal_floor'",
                [],
                |row| row.get(0),
            )
            .map_err(io)?;
        // 커서 다음 항목(`after_sequence + 1`)이 이미 지워졌으면 빠진 변경이 있을 수 있습니다.
        if after_sequence.saturating_add(1) < floor as u64 {
            return Ok(None);
        }
        let after = i64::try_from(after_sequence).unwrap_or(i64::MAX);
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let entries = match plan_id {
            Some(plan_id) => self.many(
                "SELECT body FROM journal WHERE plan_id = ?1 AND sequence > ?2 ORDER BY sequence LIMIT ?3",
                params![plan_id, after, limit],
            )?,
            None => self.many(
                "SELECT body FROM journal WHERE sequence > ?1 ORDER BY sequence LIMIT ?2",
                params![after, limit],
            )?,
        };
        Ok(Some(entries))
    }

    fn last_sequence(&self) -> Result<u64, StoreError> {
        let last: i64 = self
            .conn()
            .query_row(
                "SELECT CAST(value AS INTEGER) FROM meta WHERE key = 'last_sequence'",
                [],
                |row| row.get(0),
            )
            .map_err(io)?;
        Ok(last as u64)
    }

    fn prune_journal(&self, keep_from: u64) -> Result<u64, StoreError> {
        let mut conn = self.conn();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(io)?;
        let (last, floor): (i64, i64) = tx
            .query_row(
                "SELECT (SELECT CAST(value AS INTEGER) FROM meta WHERE key = 'last_sequence'),
                        (SELECT CAST(value AS INTEGER) FROM meta WHERE key = 'journal_floor')",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(io)?;
        // 아직 만들지 않은 번호까지 지운 것으로 적지 않습니다.
        let keep_from = (keep_from.min(last as u64 + 1)) as i64;
        if keep_from <= floor {
            return Ok(0);
        }
        let removed = tx
            .execute("DELETE FROM journal WHERE sequence < ?1", [keep_from])
            .map_err(io)?;
        tx.execute(
            "UPDATE meta SET value = ?1 WHERE key = 'journal_floor'",
            [keep_from.to_string()],
        )
        .map_err(io)?;
        tx.commit().map_err(io)?;
        Ok(removed as u64)
    }

    fn append_run_events(&self, events: &[ProviderEvent]) -> Result<(), StoreError> {
        if events.is_empty() {
            return Ok(());
        }
        let mut conn = self.conn();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(io)?;
        for event in events {
            tx.execute(
                "INSERT OR IGNORE INTO run_events (run_id, sequence, body) VALUES (?1, ?2, ?3)",
                params![event.run_id, event.sequence as i64, encode(event)?],
            )
            .map_err(io)?;
        }
        tx.commit().map_err(io)
    }

    fn run_events(
        &self,
        run_id: &str,
        from_sequence: u64,
        limit: usize,
    ) -> Result<Vec<ProviderEvent>, StoreError> {
        self.many(
            "SELECT body FROM run_events WHERE run_id = ?1 AND sequence >= ?2 ORDER BY sequence LIMIT ?3",
            params![
                run_id,
                i64::try_from(from_sequence).unwrap_or(i64::MAX),
                i64::try_from(limit).unwrap_or(i64::MAX)
            ],
        )
    }
}

/// RFC 3339 UTC 초 단위 시각.
fn timestamp() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    let (days, rem) = ((seconds / 86_400) as i64, seconds % 86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        rem % 3_600 / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join("telos-persist-unit")
            .join(format!("{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir.join(DB_FILE_NAME)
    }

    const V2: &str = "ALTER TABLE plans ADD COLUMN note TEXT; INSERT INTO meta (key, value) VALUES ('v2', 'yes');";

    #[test]
    fn intent_phases_only_move_forward() {
        use IntentPhase::*;
        assert!(intent_may_advance(Recorded, Dispatched));
        assert!(intent_may_advance(Recorded, Abandoned));
        assert!(intent_may_advance(Dispatched, Settled));
        assert!(intent_may_advance(Started, Settled));
        assert!(!intent_may_advance(Dispatched, Abandoned));
        assert!(!intent_may_advance(Started, Dispatched));
        assert!(!intent_may_advance(Started, Started));
        assert!(!intent_may_advance(Settled, Started));
        assert!(!intent_may_advance(Abandoned, Dispatched));
    }

    // §14.1: 이전 Schema는 백업을 만든 뒤 Migration합니다.
    #[test]
    fn older_schema_is_backed_up_then_migrated() {
        let path = temp_db("migrate");
        drop(SqliteStore::open(&path).unwrap());

        let migrations: &[Migration] = &[(1, SCHEMA_V1), (2, V2)];
        let store = SqliteStore::open_at(&path, SqliteOptions::default(), 2, migrations).unwrap();
        let conn = store.conn();
        let version: u32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, 2);
        let applied: Vec<u32> = conn
            .prepare("SELECT version FROM schema_migrations ORDER BY version")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(applied, [1, 2]);
        drop(conn);
        drop(store);

        let backups: Vec<_> = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with("core.db.v1.") && name.ends_with(".bak"))
            .collect();
        assert_eq!(backups.len(), 1, "{backups:?}");
        // 백업은 Migration 전 Schema 그대로입니다.
        let backup = Connection::open(path.with_file_name(&backups[0])).unwrap();
        let backup_version: u32 = backup
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(backup_version, 1);
    }

    // §14.1: 백업을 만들지 못하면 Migration하지 않고 열지 않습니다.
    #[test]
    fn failed_backup_blocks_migration() {
        let path = temp_db("backup-fail");
        drop(SqliteStore::open(&path).unwrap());
        let fixed: Clock = Arc::new(|| "2026-11-16T00:00:00Z".to_string());
        // 같은 이름의 백업이 이미 있으면 덮어쓰지 않고 실패합니다.
        fs::write(path.with_file_name("core.db.v1.20261116000000.bak"), b"x").unwrap();
        let options = SqliteOptions {
            clock: Some(fixed),
            ..SqliteOptions::default()
        };
        let migrations: &[Migration] = &[(1, SCHEMA_V1), (2, V2)];
        let error = SqliteStore::open_at(&path, options, 2, migrations).unwrap_err();
        assert!(matches!(error, StoreError::Io { .. }), "{error:?}");

        let conn = Connection::open(&path).unwrap();
        let version: u32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, 1);
    }

    #[test]
    fn missing_migration_step_is_reported() {
        let path = temp_db("gap");
        drop(SqliteStore::open(&path).unwrap());
        let migrations: &[Migration] = &[(1, SCHEMA_V1), (3, V2)];
        let error =
            SqliteStore::open_at(&path, SqliteOptions::default(), 3, migrations).unwrap_err();
        assert!(matches!(error, StoreError::Corrupt { .. }), "{error:?}");
    }

    #[test]
    fn tables_without_schema_version_are_not_adopted() {
        let path = temp_db("foreign");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TABLE other (x)").unwrap();
        drop(conn);
        let error = SqliteStore::open(&path).unwrap_err();
        assert!(matches!(error, StoreError::Corrupt { .. }), "{error:?}");
    }

    #[test]
    fn timestamp_is_rfc3339_utc() {
        let now = timestamp();
        assert_eq!(now.len(), 20);
        assert!(now.ends_with('Z') && now.as_bytes()[10] == b'T');
    }
}

//! PH1-PERSIST 시험용 기록 생성 도우미입니다.
#![allow(dead_code)]

use std::path::PathBuf;

use telos_core::persistence::{SqliteStore, StateWrite, WriteBatch};
use telos_schema::*;

pub const AT: &str = "2026-11-16T00:00:00Z";

/// 시험마다 새로 만드는 임시 디렉터리입니다. Drop에서 지웁니다.
pub struct TempDir {
    pub root: PathBuf,
}

impl TempDir {
    pub fn new(name: &str) -> Self {
        let root = std::env::temp_dir()
            .join("telos-persist")
            .join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    pub fn db(&self) -> PathBuf {
        telos_core::persistence::core_db_path(&self.root)
    }

    pub fn open(&self) -> SqliteStore {
        SqliteStore::open(&self.db()).unwrap()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

pub fn batch(writes: Vec<StateWrite>) -> WriteBatch {
    WriteBatch::new(writes)
}

pub fn plan(id: &str) -> Plan {
    Plan {
        id: id.into(),
        project_id: "project-1".into(),
        revision: 1,
        goal: "goal".into(),
        constraints: vec![],
        status: PlanStatus::Active,
        controller_epoch: 1,
        master_task_id: format!("{id}-master"),
        created_at: AT.into(),
        updated_at: AT.into(),
    }
}

pub fn task(id: &str, plan_id: &str) -> TaskRecord {
    let mut spec = fixtures::task_spec();
    spec.id = id.into();
    spec.plan_id = plan_id.into();
    TaskRecord {
        spec,
        status: TaskStatus::Created,
        state_version: 0,
        wait: None,
        cancel_propagation: CancelPropagation::Descendants,
        delegation_depth: 0,
        created_at: AT.into(),
        updated_at: AT.into(),
    }
}

pub fn run(id: &str, task_id: &str) -> RunRecord {
    RunRecord {
        run_id: id.into(),
        task_id: task_id.into(),
        task_revision: 1,
        attempt: 1,
        status: RunStatus::Starting,
        state_version: 0,
        provider: "codex".into(),
        host_id: "windows-local".into(),
        session_mode: SessionMode::Fresh,
        outcome: None,
        created_at: AT.into(),
        finished_at: None,
    }
}

pub fn intent(id: &str, run_id: &str, task_id: &str, phase: IntentPhase) -> RunIntent {
    RunIntent {
        intent_id: id.into(),
        run_id: run_id.into(),
        task_id: task_id.into(),
        task_revision: 1,
        attempt: 1,
        idempotency_key: format!("key-{id}"),
        payload_digest: "sha256:payload".into(),
        phase,
        host_id: "windows-local".into(),
        provider: "codex".into(),
        native_session_id: None,
        process: None,
        recorded_at: AT.into(),
        updated_at: AT.into(),
    }
}

pub fn session(id: &str, owner_task_id: &str, run_id: &str) -> SessionRecord {
    SessionRecord {
        session_id: id.into(),
        provider: "codex".into(),
        host_id: "windows-local".into(),
        native_session_id: format!("native-{id}"),
        owner_task_id: owner_task_id.into(),
        parent_session_id: None,
        fork_basis: None,
        created_by_run_id: run_id.into(),
        created_at: AT.into(),
    }
}

pub fn snapshot(run_id: &str, task_id: &str) -> ContextSnapshot {
    ContextSnapshot {
        snapshot_id: format!("ctx-{run_id}"),
        run_id: run_id.into(),
        task_id: task_id.into(),
        strategy: SessionMode::Fresh,
        fallback: None,
        instructions: vec![],
        inputs: vec![],
        selected: vec![],
        exclusions: vec![],
        environment: EnvironmentFacts {
            host_id: "windows-local".into(),
            workspace_id: "ws-1".into(),
            cwd: None,
            executable_path: "codex.exe".into(),
            cli_version: "0.164.0".into(),
            tools: vec![],
        },
        git: None,
        contract: ContractDigests {
            plan_revision: 1,
            task_revision: 1,
            policy_revision: "policy-1".into(),
            config_digest: "sha256:c".into(),
            permission_digest: "sha256:p".into(),
            budget_digest: "sha256:b".into(),
            validation_digest: "sha256:v".into(),
        },
        issues: vec![],
        assumptions: vec![],
        capsule: None,
        created_at: AT.into(),
    }
}

pub fn workspace(id: &str, task_id: &str) -> WorkspaceRecord {
    WorkspaceRecord {
        workspace_id: id.into(),
        mode: WorkspaceMode::Directory,
        host_id: "windows-local".into(),
        root_path: Some("C:/work".into()),
        branch: None,
        baseline: None,
        owner_task_id: task_id.into(),
        created_by_run_id: "run-1".into(),
        read_only: false,
        state: WorkspaceState::Active,
        state_version: 0,
        created_at: AT.into(),
    }
}

pub fn lock(id: &str, workspace_id: &str, mode: LockMode, epoch: u64) -> ResourceLock {
    ResourceLock {
        lock_id: id.into(),
        resource: LockResource::Workspace {
            workspace_id: workspace_id.into(),
        },
        mode,
        holder_task_id: "task-1".into(),
        holder_run_id: format!("run-{id}"),
        owner_epoch: epoch,
        process: None,
        acquired_at: AT.into(),
        expires_at: None,
    }
}

pub fn artifact(artifact_id: &str, version_id: &str, task_id: &str) -> ArtifactVersion {
    ArtifactVersion {
        artifact_id: artifact_id.into(),
        version_id: version_id.into(),
        task_id: task_id.into(),
        task_revision: 1,
        run_id: "run-1".into(),
        output_name: "answer".into(),
        kind: ArtifactKind::Text,
        content_type: "text/plain".into(),
        storage_uri: "artifact://sha256/aa".into(),
        checksum: "sha256:aa".into(),
        size_bytes: 5,
        source_revision: None,
        created_at: AT.into(),
    }
}

pub fn artifact_ref(artifact_id: &str, version_id: &str) -> ArtifactVersionRef {
    ArtifactVersionRef {
        artifact_id: artifact_id.into(),
        version_id: version_id.into(),
    }
}

pub fn validation(id: &str, task_id: &str) -> ValidationJob {
    ValidationJob {
        validation_id: id.into(),
        task_id: task_id.into(),
        task_revision: 1,
        contract: ValidationContract {
            id: "check-1".into(),
            revision: 1,
            kind: ValidationKind::File,
            target_output_names: vec!["answer".into()],
            parameters: JsonObject::new(),
            required: true,
        },
        targets: vec![],
        status: ValidationJobStatus::Queued,
        state_version: 0,
        verdict: None,
        evidence: vec![],
        message: None,
        created_at: AT.into(),
        finished_at: None,
    }
}

pub fn approval(id: &str, plan_id: &str) -> ApprovalDecision {
    ApprovalDecision {
        approval_id: id.into(),
        kind: ApprovalKind::ResultReview,
        plan_id: plan_id.into(),
        task_id: Some("task-1".into()),
        run_id: None,
        contract_revision: Some(1),
        targets: vec![],
        action: None,
        decided_by: None,
        answer: None,
        user_only: true,
        policy_revision: "policy-1".into(),
        expires_at: None,
        state: ApprovalState::Pending,
        state_version: 0,
        created_at: AT.into(),
        decided_at: None,
    }
}

pub fn budget(plan_id: &str, consumed: u32) -> BudgetLedger {
    BudgetLedger {
        plan_id: plan_id.into(),
        tasks_reserved: 0,
        tasks_consumed: consumed,
        tokens_measurable: true,
        tokens_consumed: 0,
    }
}

pub fn idempotency(scope: &str, key: &str, digest: &str) -> IdempotencyRecord {
    let mut result = JsonObject::new();
    result.insert("runId".into(), "run-1".into());
    IdempotencyRecord {
        scope: scope.into(),
        key: key.into(),
        payload_digest: digest.into(),
        result,
        created_at: AT.into(),
    }
}

pub fn usage(id: &str, plan_id: &str, output_tokens: u64) -> UsageRecord {
    UsageRecord {
        usage_id: id.into(),
        plan_id: plan_id.into(),
        task_id: "task-1".into(),
        run_id: "run-1".into(),
        native_turn_id: None,
        provider: "codex".into(),
        role: UsageRole::Worker,
        usage: UsagePayload {
            scope: UsageScope::Delta,
            source: UsageSource::ProviderReported,
            input_tokens: None,
            output_tokens: Some(output_tokens),
            cache_read_tokens: None,
            cache_write_tokens: None,
            cache_included_in_input: None,
            cost_usd: None,
        },
        included_in_parent: false,
        recorded_at: AT.into(),
    }
}

//! Workspace·Git·자원 잠금 (Spec §10, §12, PH1-WORKSPACE). H0 공개 인터페이스이며 구현은 PH1-WORKSPACE가 합니다.
//!
//! 쓰기 Task는 상태 디렉터리 아래 Worktree에서 실행합니다(H0 D9). 원본 Branch 반영은 승인된 Revision의
//! 로컬 Merge Commit뿐이며 push하지 않습니다. 잠금은 [`crate::persistence::Store`]의 `AcquireLock`으로 기록합니다.

use async_trait::async_trait;
use telos_schema::{
    DiffSummary, LockMode, LockResource, ResourceLock, SourceMergeRequest, SourceMergeResult,
    TaskRecord, WorkspaceRecord,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceError {
    LockConflict {
        resource: String,
    },
    /// 원본 상태가 기준과 다름(변경·Branch 이동 등). 사용자 판단이 필요합니다.
    SourceChanged {
        reason: String,
    },
    /// 승인 Revision과 현재 Revision 불일치, Merge 충돌 등
    MergeRejected {
        reason: String,
    },
    Git {
        message: String,
    },
    Storage {
        message: String,
    },
}

#[async_trait]
pub trait WorkspaceManager: Send + Sync {
    /// Task의 Workspace를 준비합니다. 읽기 Task는 원본, 쓰기 Task는 새 Worktree입니다.
    async fn prepare(
        &self,
        task: &TaskRecord,
        run_id: &str,
    ) -> Result<WorkspaceRecord, WorkspaceError>;
    /// 기준 Revision 대비 변경입니다. 주입 경로(`INJECTED_WORKSPACE_PATHS`)는 제외합니다.
    async fn diff(&self, workspace_id: &str) -> Result<DiffSummary, WorkspaceError>;
    /// 현재 작업 내용을 Commit으로 고정해 Revision을 돌려줍니다. 검증·승인은 이 Revision을 대상으로 합니다.
    async fn freeze(&self, workspace_id: &str, message: &str) -> Result<String, WorkspaceError>;
    /// 승인 확인(`ApprovalDecision::permits`)은 호출자가 하고, 여기서는 Revision 일치를 다시 확인합니다.
    async fn merge(&self, request: SourceMergeRequest)
    -> Result<SourceMergeResult, WorkspaceError>;
    /// Worktree·Branch 정리. 참조 중인 Artifact의 고정 Commit은 보존합니다.
    async fn release(&self, workspace_id: &str) -> Result<(), WorkspaceError>;

    async fn acquire(
        &self,
        resource: LockResource,
        mode: LockMode,
        holder_task_id: &str,
    ) -> Result<ResourceLock, WorkspaceError>;
    async fn release_lock(&self, lock: &ResourceLock) -> Result<(), WorkspaceError>;
}

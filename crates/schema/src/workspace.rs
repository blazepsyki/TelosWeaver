//! Spec §10 Workspace·Git·산출물 보관, §14.3 Lock 대조 (H0 Workspace)
//!
//! Git Worktree는 보안 Sandbox가 아닙니다. 경로 정규화·허용 범위·Symlink 우회 검사는 PH1-WORKSPACE가 집행합니다.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ArtifactVersionRef, ProcessEvidence, WorkspaceMode};

/// Diff·Artifact 수집과 원본 Branch 반영에서 빼는 주입 경로입니다(Task MCP 주입용 `.agents/`, H0 D5).
pub const INJECTED_WORKSPACE_PATHS: &[&str] = &[".agents/"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub enum WorkspaceState {
    Active,
    /// 사용이 끝났지만 미완 Run·장애 조사·참조 때문에 남겨 둡니다. 자동 삭제하지 않습니다 (§10).
    Retained,
    Cleaned,
}

/// 시작 시점의 Git 상태입니다. staged/unstaged/untracked 변경을 자동 초기화하지 않습니다 (GIT-03).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GitBaseline {
    pub git_root: String,
    pub base_commit: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    pub staged: Vec<String>,
    pub unstaged: Vec<String>,
    pub untracked: Vec<String>,
    pub recorded_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRecord {
    pub workspace_id: String,
    pub mode: WorkspaceMode,
    pub host_id: String,
    /// 정규화한 절대 경로. `none` 모드는 비어 있습니다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_path: Option<String>,
    /// `git-worktree`의 Task 전용 Branch
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline: Option<GitBaseline>,
    pub owner_task_id: String,
    pub created_by_run_id: String,
    pub read_only: bool,
    pub state: WorkspaceState,
    pub state_version: u64,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum LockResource {
    Workspace {
        workspace_id: String,
    },
    /// 같은 Session의 쓰기 Turn은 한 번에 하나입니다 (§8.1).
    Session {
        session_id: String,
    },
    /// 장치·포트·공유 출력 위치 등 (§9.1)
    Named {
        resource_id: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum LockMode {
    Writer,
    Reader,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResourceLock {
    pub lock_id: String,
    pub resource: LockResource,
    pub mode: LockMode,
    pub holder_task_id: String,
    pub holder_run_id: String,
    /// 소유권 세대. 넘겨받을 때 올리고 이전 소유자의 갱신을 거부합니다 (§14.3).
    pub owner_epoch: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process: Option<ProcessEvidence>,
    pub acquired_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

impl LockMode {
    /// Writer는 다른 모든 Lock과 겹치지 않습니다. Reader끼리는 겹칠 수 있습니다 (GIT-02, WS-02).
    pub fn conflicts_with(self, other: Self) -> bool {
        self == Self::Writer || other == Self::Writer
    }
}

impl ResourceLock {
    /// 기존 Lock을 다른 Run이 넘겨받을 수 있는지입니다. 기한이 지났어도 소유자가 살아 있으면 넘겨받지 않습니다.
    /// 소유자 생존을 확인하지 못하면(`None`) 넘겨받지 않고 대조로 보냅니다 (§14.3).
    pub fn can_take_over(&self, expired: bool, holder_alive: Option<bool>) -> bool {
        expired && holder_alive == Some(false)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "change",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum FileChange {
    Added { path: String },
    Modified { path: String },
    Deleted { path: String },
    Renamed { from: String, path: String },
}

/// 대상 Revision의 변경 요약입니다. 전체 Diff는 Artifact로 둡니다 (GIT-04, §10).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiffSummary {
    pub workspace_id: String,
    pub base_revision: String,
    pub target_revision: String,
    pub files: Vec<FileChange>,
    pub diff: ArtifactVersionRef,
}

/// 승인된 Revision을 로컬 원본 Branch에 Merge Commit으로 반영하는 요청입니다. push는 하지 않습니다 (H0 D9, GIT-05·08).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SourceMergeRequest {
    pub workspace_id: String,
    pub source_branch: String,
    /// 검증·승인한 Revision. 반영 직전 Worktree의 현재 Revision과 다르면 거부합니다.
    pub approved_revision: String,
    pub approval_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SourceMergeResult {
    pub merge_commit: String,
    pub source_branch: String,
    pub merged_revision: String,
}

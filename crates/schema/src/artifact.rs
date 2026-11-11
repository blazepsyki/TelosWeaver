//! Spec §5.3 Artifact Version과 결과

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ArtifactKind, ArtifactVersionRef};

/// Artifact Version 하나의 기본 크기 상한입니다(64 MiB, 설정으로 변경). 넘으면 등록을 거부합니다 (H0 D2).
pub const DEFAULT_ARTIFACT_MAX_BYTES: u64 = 64 * 1024 * 1024;

/// 불변입니다. 변경 가능한 작업 디렉터리 경로를 검증된 버전으로 간주하지 않습니다.
///
/// `checksum`은 `sha256:<hex>`, Core 저장소의 내용은 `storage_uri` = `artifact://sha256/<hex>`입니다 (H0 D2).
/// 같은 내용의 Version은 저장 내용을 공유하지만 Version 기록은 따로 둡니다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactVersion {
    pub artifact_id: String,
    pub version_id: String,
    pub task_id: String,
    pub task_revision: u32,
    pub run_id: String,
    pub output_name: String,
    pub kind: ArtifactKind,
    pub content_type: String,
    pub storage_uri: String,
    pub checksum: String,
    pub size_bytes: u64,
    /// Git 작업이면 내용을 고정한 Commit, 파일 출력이면 원래 Workspace 상대 경로 등 출처입니다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_revision: Option<String>,
    pub created_at: String,
}

/// Artifact Version을 쓰는 곳입니다. 참조가 남아 있는 Version의 저장 내용은 정리하지 않습니다 (ART-03, §10).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "holder",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum ArtifactHolder {
    /// Run 시작 때 고정한 입력
    RunInput {
        run_id: String,
    },
    TaskResult {
        task_id: String,
        task_revision: u32,
        run_id: String,
    },
    Validation {
        validation_id: String,
    },
    Approval {
        approval_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactReference {
    pub target: ArtifactVersionRef,
    pub holder: ArtifactHolder,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionOutcome {
    Succeeded,
    Failed,
    Cancelled,
    Unknown,
}

/// `execution_outcome`이 `Succeeded`여도 Task 완료를 뜻하지 않습니다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TaskResult {
    pub task_id: String,
    pub task_revision: u32,
    pub run_id: String,
    pub execution_outcome: ExecutionOutcome,
    pub artifacts: Vec<ArtifactVersionRef>,
    pub summary: String,
    pub unresolved: Vec<String>,
    pub evidence_ids: Vec<String>,
}

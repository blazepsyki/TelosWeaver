//! Spec §5.3 Artifact Version과 결과

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ArtifactKind, ArtifactVersionRef};

/// 불변입니다. 변경 가능한 작업 디렉터리 경로를 검증된 버전으로 간주하지 않습니다.
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_revision: Option<String>,
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

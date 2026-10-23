//! Spec §5.2 Task·입출력 계약, §9.4 Task와 Run 상태

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::JsonObject;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum TaskType {
    General,
    Research,
    Analysis,
    Document,
    Coding,
    Automation,
    Review,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ArtifactKind {
    Text,
    File,
    Document,
    Dataset,
    Code,
    Diff,
    Structured,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactVersionRef {
    pub artifact_id: String,
    pub version_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "source",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum InputBinding {
    Artifact {
        name: String,
        r#ref: ArtifactVersionRef,
    },
    TaskOutput {
        name: String,
        producer_task_id: String,
        producer_task_revision: u32,
        output_name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OutputContract {
    pub id: String,
    /// Task Revision 안에서 유일합니다.
    pub name: String,
    pub kind: ArtifactKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relative_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_id: Option<String>,
    pub required: bool,
    pub min_count: u32,
    pub max_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ValidationKind {
    Schema,
    File,
    Checksum,
    Command,
    Assertion,
    AiReview,
    Human,
    Visual,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ValidationContract {
    pub id: String,
    pub revision: u32,
    pub kind: ValidationKind,
    pub target_output_names: Vec<String>,
    pub parameters: JsonObject,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentSelection {
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub role: String,
}

/// P0 실행 대상은 None·Directory·GitWorktree입니다 (§10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum WorkspaceMode {
    None,
    Directory,
    GitWorktree,
    Sandbox,
    Remote,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRequest {
    pub mode: WorkspaceMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    pub host_id: String,
    pub read_only: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TaskSpec {
    pub id: String,
    pub revision: u32,
    pub project_id: String,
    pub plan_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_task_id: Option<String>,
    pub r#type: TaskType,
    pub objective: String,
    pub constraints: Vec<String>,
    pub dependencies: Vec<String>,
    pub agent: AgentSelection,
    pub workspace: WorkspaceRequest,
    pub inputs: Vec<InputBinding>,
    pub outputs: Vec<OutputContract>,
    pub validation: Vec<ValidationContract>,
    pub permission_profile_id: String,
    pub budget_profile_id: String,
    pub completion_policy_id: String,
}

/// 기본 경로: Created → Ready → Running → Executed → Validating → Verified → Completed.
/// 전이 규칙은 §9.4 표를 따르며 Core Scheduler가 소유합니다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub enum TaskStatus {
    Created,
    Blocked,
    Ready,
    Running,
    WaitingChild,
    WaitingMaster,
    WaitingUser,
    Executed,
    Validating,
    Verified,
    Completed,
    Failed,
    Skipped,
    Cancelling,
    Cancelled,
    Interrupted,
    NeedsReconciliation,
}

/// Run 성공은 Task 완료가 아닙니다 (§9.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub enum RunStatus {
    Starting,
    Running,
    Waiting,
    Succeeded,
    Failed,
    Cancelling,
    Cancelled,
    Interrupted,
    Unknown,
}

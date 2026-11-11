//! Spec §9.2 Task MCP 위임 계약, §9.3 동기 대기 (H0 Task MCP)
//!
//! Worker가 Core에 하위 작업을 맡기고 결과를 보고하는 여섯 도구의 입력·출력입니다.
//! 호출자는 MCP 인자가 아니라 Run 단위 연결 자격으로 확인하며, 허용 범위([`TaskMcpScope`])는 Core만 보관합니다.
//! 연결 설정은 [`TaskMcpBinding`]으로 Adapter에 넘기고 Provider별 주입 방식은 Spec §9.2 표를 따릅니다.

use std::collections::HashMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    AgentSelection, ArtifactVersionRef, CancelPropagation, Capability, InputBinding,
    OutputContract, TaskResult, TaskStatus, TaskType, ValidationContract, ValidationVerdict,
    WorkspaceRequest,
};

/// MCP 서버가 노출하는 도구 이름입니다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskTool {
    DelegateTask,
    GetTaskResult,
    ListTasks,
    CancelTask,
    GetAgentStatus,
    ReportTaskResult,
}

/// Run 시작 때 Adapter에 넘기는 연결 정보입니다. 자격 원문은 넣지 않고 Core 상태 디렉터리의 파일 경로만 넘깁니다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TaskMcpBinding {
    /// Provider 설정에 쓰는 MCP 서버 이름입니다. 고정 이름을 써서 Provider의 도구 Schema 캐시를 재사용합니다.
    pub server_name: String,
    /// stdio MCP 서버로 띄울 실행 파일(`telos-weaver`)과 인자
    pub command: String,
    pub args: Vec<String>,
    /// Run 범위 연결 자격 파일. 현재 사용자만 읽을 수 있게 만들고 Run 종료 때 폐기합니다.
    pub credential_file: String,
}

/// Core가 연결 자격에 묶어 보관하는 허용 범위입니다. Provider에 넘기지 않습니다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TaskMcpScope {
    pub project_id: String,
    pub plan_id: String,
    /// 호출자 Task와 Run. 다른 Task·Run을 대신해 보고하지 못합니다.
    pub task_id: String,
    pub run_id: String,
    pub allowed_tools: Vec<TaskTool>,
    /// 이 Run이 만들 수 있는 자식 수의 남은 한도
    pub remaining_children: u32,
    pub expires_at: String,
}

// --- delegate_task ---

/// 권한·예산 Profile은 입력으로 받지 않습니다. Core가 부모의 남은 범위 안에서 정합니다 (§9.3, §13.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DelegateTaskInput {
    pub idempotency_key: String,
    pub r#type: TaskType,
    pub objective: String,
    #[serde(default)]
    pub constraints: Vec<String>,
    pub agent: AgentSelection,
    pub workspace: WorkspaceRequest,
    #[serde(default)]
    pub inputs: Vec<InputBinding>,
    #[serde(default)]
    pub outputs: Vec<OutputContract>,
    #[serde(default)]
    pub validation: Vec<ValidationContract>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline: Option<String>,
    /// 참이면 호출자 Task를 WaitingChild로 두고 결과를 기다립니다. 등록 자체는 항상 비동기입니다 (§9.3).
    #[serde(default)]
    pub wait_for_result: bool,
    #[serde(default)]
    pub cancel_propagation: CancelPropagation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DelegateTaskOutput {
    pub task_id: String,
    pub task_revision: u32,
    pub status: TaskStatus,
}

// --- get_task_result ---

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetTaskResultInput {
    pub task_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ValidationSummary {
    pub validation_id: String,
    pub contract_revision: u32,
    pub verdict: ValidationVerdict,
    pub target: ArtifactVersionRef,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetTaskResultOutput {
    pub task_id: String,
    pub task_revision: u32,
    pub status: TaskStatus,
    /// 고정된 결과입니다. 완료 전에는 비어 있을 수 있습니다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<TaskResult>,
    pub validations: Vec<ValidationSummary>,
    pub unresolved: Vec<String>,
}

// --- list_tasks ---

/// 호출자에게 허용된 Project·Plan 안에서만 조회합니다.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListTasksInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_task_id: Option<String>,
    #[serde(default)]
    pub statuses: Vec<TaskStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TaskListItem {
    pub task_id: String,
    pub task_revision: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_task_id: Option<String>,
    pub r#type: TaskType,
    pub objective: String,
    pub provider: String,
    pub status: TaskStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListTasksOutput {
    pub tasks: Vec<TaskListItem>,
}

// --- cancel_task ---

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CancelTaskInput {
    pub idempotency_key: String,
    pub task_id: String,
    pub reason: String,
}

/// 접수(`accepted`)와 실제 정지는 다릅니다. 정지 결과는 `get_task_result`로 확인합니다 (§7.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CancelTaskOutput {
    pub request_id: String,
    pub accepted: bool,
}

// --- get_agent_status ---

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetAgentStatusInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentStatus {
    pub provider: String,
    pub host_id: String,
    pub cli_version: String,
    pub capabilities: HashMap<String, Capability>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetAgentStatusOutput {
    pub agents: Vec<AgentStatus>,
}

// --- report_task_result ---

/// Workspace 안의 파일 또는 본문으로 출력을 보고합니다. Core가 경로 범위·소유권·출력 계약을 검사해 불변 Artifact로 등록합니다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "source",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ReportedOutput {
    WorkspaceFile {
        output_name: String,
        relative_path: String,
    },
    Inline {
        output_name: String,
        content_type: String,
        text: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReportTaskResultInput {
    pub idempotency_key: String,
    pub summary: String,
    pub outputs: Vec<ReportedOutput>,
    #[serde(default)]
    pub unresolved: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReportTaskResultOutput {
    pub artifacts: Vec<ArtifactVersionRef>,
}

// --- 오류 ---

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskToolErrorCode {
    /// 자격이 없거나 만료됨
    Unauthenticated,
    /// 이 Run의 허용 도구·범위 밖
    OutOfScope,
    NotFound,
    InvalidInput,
    /// 깊이·자식 수·전체 Task·예산 상한 초과 (§13.2)
    LimitExceeded,
    /// 같은 멱등 키에 다른 내용 (§4.2)
    IdempotencyConflict,
    /// 요청한 Provider·모드를 지원하지 않음 (`capability` 공통 키와 함께 보고)
    Unsupported,
    /// 지금 실행할 수 없어 즉시 설명 가능한 거부를 돌려줌. 무한 대기하지 않습니다 (§9.3).
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TaskToolError {
    pub code: TaskToolErrorCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability: Option<String>,
}

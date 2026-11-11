//! Task MCP (Spec §7.3, PH1-MCP). H0 공개 인터페이스이며 구현은 PH1-MCP가 합니다.
//!
//! Core는 Run마다 범위([`TaskMcpScope`])에 묶인 자격을 발급하고 연결 정보([`TaskMcpBinding`])만 Adapter에 넘깁니다(H0 D4).
//! `telos-weaver` stdio 하위 명령이 자격으로 Core API에 `TaskMcp` 범위로 연결해 `tasks.tool`을 부릅니다.
//! 도구 처리 결과는 Scheduler를 거쳐 상태를 바꿉니다.

use async_trait::async_trait;
use telos_schema::{TaskMcpBinding, TaskMcpScope, TaskTool, TaskToolError};

#[async_trait]
pub trait TaskMcpService: Send + Sync {
    /// 자격 파일을 만들고 연결 정보를 돌려줍니다. 자격 원문은 Provider 설정·로그에 남기지 않습니다.
    async fn issue_binding(&self, scope: TaskMcpScope) -> Result<TaskMcpBinding, TaskToolError>;
    /// Run 종료·취소 때 자격을 폐기합니다. 폐기 뒤 호출은 거부합니다.
    async fn revoke(&self, run_id: &str) -> Result<(), TaskToolError>;
    /// 연결의 `run_id`로 범위를 찾아 도구를 실행합니다. 입력·출력은 각 도구 Schema(`DelegateTaskInput` 등)의 JSON입니다.
    async fn handle(
        &self,
        run_id: &str,
        tool: TaskTool,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, TaskToolError>;
}

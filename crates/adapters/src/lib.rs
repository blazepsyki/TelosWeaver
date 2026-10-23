//! Provider Adapter (Spec §7).
//!
//! 메서드가 존재한다는 것이 기능 지원을 뜻하지 않습니다. 미지원 제어는
//! [`AdapterError::UnsupportedCapability`]를 반환합니다.

use std::{collections::HashMap, fmt};

use async_trait::async_trait;
use futures::stream::BoxStream;
use telos_schema::{
    ApprovalAnswer, Capability, ProviderEvent, RunContext, RunInspection, RunStarted, TaskSpec,
};

mod agy;
mod claude;
mod codex;
pub mod runtime;
mod unimplemented;

pub use agy::AgyAdapter;
pub use claude::ClaudeCodeAdapter;
pub use codex::CodexAdapter;

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AdapterError {
    UnsupportedCapability { capability: String, reason: String },
    // 시작 실패·인증·설정·Protocol Parse 등은 §7.3 분류에 따라 추가합니다.
}

impl fmt::Display for AdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedCapability { capability, reason } => {
                write!(f, "unsupported capability '{capability}': {reason}")
            }
        }
    }
}

impl std::error::Error for AdapterError {}

/// Core는 여러 Provider를 `Arc<dyn AgentAdapter>`로 다룹니다.
#[async_trait]
pub trait AgentAdapter: Send + Sync {
    fn id(&self) -> &str;

    async fn detect(&self, host_id: &str) -> Result<HashMap<String, Capability>, AdapterError>;

    /// Run ID는 Core가 발급합니다. 동일 실행 키의 재요청으로 프로세스를 중복 생성하지 않습니다.
    async fn start_run(
        &self,
        task: &TaskSpec,
        context: &RunContext,
    ) -> Result<RunStarted, AdapterError>;

    /// 구독 전에 발생한 이벤트도 `after_sequence` 이후부터 회수할 수 있어야 합니다 (§7.2).
    fn events(
        &self,
        run_id: &str,
        after_sequence: u64,
    ) -> BoxStream<'static, Result<ProviderEvent, AdapterError>>;

    async fn send_input(
        &self,
        run_id: &str,
        input_id: &str,
        text: &str,
    ) -> Result<(), AdapterError>;

    async fn answer_approval(
        &self,
        run_id: &str,
        request_id: &str,
        decision: ApprovalAnswer,
    ) -> Result<(), AdapterError>;

    /// 반환값은 취소 요청 접수 여부이며 실제 종료가 아닙니다.
    async fn cancel_run(&self, run_id: &str) -> Result<bool, AdapterError>;

    async fn inspect_run(&self, run_id: &str) -> Result<RunInspection, AdapterError>;
}

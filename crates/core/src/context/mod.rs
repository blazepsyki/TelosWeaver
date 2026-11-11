//! 문맥 구성 (Spec §8, PH1-CONTEXT). H0 공개 인터페이스이며 구현은 PH1-CONTEXT가 합니다.
//!
//! Run마다 입력·지침·환경을 하나의 [`ContextSnapshot`]으로 고정합니다. 막는 문제가 있으면
//! `ContextSnapshot::blocks_execution`이 참이고 Scheduler는 실행을 보류합니다. 요청 전략을 쓸 수 없으면
//! `telos_schema::session_fallback`으로 Capsule로 대체해 `fallback`에 기록합니다.

use async_trait::async_trait;
use telos_schema::{ArtifactVersionRef, CapsuleContent, ContextSnapshot, RunContext, TaskRecord};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextError {
    /// 입력 Artifact·Session을 찾을 수 없거나 읽을 수 없음
    Unresolvable {
        reason: String,
    },
    Storage {
        message: String,
    },
}

#[async_trait]
pub trait ContextBuilder: Send + Sync {
    /// Run 시작 전 Snapshot을 만듭니다. 입력 Version은 이 시점에 고정하고 `RunInput` 참조를 추가합니다.
    async fn build(
        &self,
        task: &TaskRecord,
        run: &RunContext,
    ) -> Result<ContextSnapshot, ContextError>;
    /// Capsule을 `structured` Artifact로 저장하고 참조를 돌려줍니다 (§8.1).
    async fn write_capsule(
        &self,
        task: &TaskRecord,
        run_id: &str,
        content: CapsuleContent,
    ) -> Result<ArtifactVersionRef, ContextError>;
    /// Provider 입력으로 바꾼 문맥 본문입니다. 신뢰 수준이 낮은 출처는 지침이 아닌 자료로 구분해 넣습니다 (§8.3).
    fn render_prompt(&self, snapshot: &ContextSnapshot) -> String;
}

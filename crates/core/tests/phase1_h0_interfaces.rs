//! H0 Core 모듈 인터페이스: 각 Trait을 `Arc<dyn …>`로 주입할 수 있는지 확인합니다 (Spec §19.6.2).
//! 모듈 사이 의존은 이 Trait 객체로만 연결하므로 object safety가 깨지면 계약 변경입니다.

use std::sync::Arc;

use telos_core::artifact::ArtifactStore;
use telos_core::config::ConfigService;
use telos_core::context::ContextBuilder;
use telos_core::mcp::TaskMcpService;
use telos_core::persistence::Store;
use telos_core::scheduler::{RunExecutor, Scheduler};
use telos_core::validation::{ApprovalService, Validator};
use telos_core::workspace::WorkspaceManager;

#[allow(dead_code)]
struct CoreServices {
    store: Arc<dyn Store>,
    scheduler: Arc<dyn Scheduler>,
    executor: Arc<dyn RunExecutor>,
    context: Arc<dyn ContextBuilder>,
    workspace: Arc<dyn WorkspaceManager>,
    artifacts: Arc<dyn ArtifactStore>,
    validator: Arc<dyn Validator>,
    approvals: Arc<dyn ApprovalService>,
    task_mcp: Arc<dyn TaskMcpService>,
    config: Arc<dyn ConfigService>,
}

fn assert_send_sync<T: Send + Sync + ?Sized>() {}

#[test]
fn service_traits_are_object_safe_and_thread_safe() {
    assert_send_sync::<CoreServices>();
    assert_send_sync::<dyn Store>();
    assert_send_sync::<dyn Scheduler>();
    assert_send_sync::<dyn RunExecutor>();
}

#[test]
fn write_batch_is_plain_data() {
    use telos_core::persistence::{StateWrite, WriteBatch};
    use telos_schema::IdempotencyRecord;
    let batch = WriteBatch { writes: vec![] };
    assert_eq!(batch.clone(), batch);
    let _ = |record: IdempotencyRecord| StateWrite::Idempotency { record };
}

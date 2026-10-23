use std::sync::Arc;

use futures::StreamExt;
use telos_adapters::{AdapterError, AgentAdapter, AgyAdapter, ClaudeCodeAdapter, CodexAdapter};
use telos_schema::RunInspection;

fn adapters() -> Vec<(&'static str, Arc<dyn AgentAdapter>)> {
    vec![
        ("codex", Arc::new(CodexAdapter)),
        ("claude", Arc::new(ClaudeCodeAdapter)),
        ("agy", Arc::new(AgyAdapter)),
    ]
}

#[tokio::test]
async fn stubs_claim_no_capabilities_and_reject_control() {
    for (id, adapter) in adapters() {
        assert_eq!(adapter.id(), id);
        assert!(adapter.detect("local").await.unwrap().is_empty(), "{id}");
        assert!(
            matches!(
                adapter.cancel_run("run-1").await,
                Err(AdapterError::UnsupportedCapability { .. })
            ),
            "{id}"
        );
        assert!(matches!(
            adapter.events("run-1", 0).next().await,
            Some(Err(AdapterError::UnsupportedCapability { .. }))
        ));
        assert_eq!(
            adapter.inspect_run("run-1").await.unwrap(),
            RunInspection::Unknown
        );
    }
}

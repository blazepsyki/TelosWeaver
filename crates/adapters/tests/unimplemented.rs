//! 미지원 시험 대역 `UnsupportedAdapter`의 계약 시험 (Spec §19.5.2).
//!
//! 실제 Provider Adapter의 동작은 검사하지 않습니다. Provider 구현이 진행되어도 이 시험은 바뀌지 않으며,
//! Provider별 시험은 각 담당의 `tests/<provider>_*.rs`에 둡니다.

use std::sync::Arc;

use futures::StreamExt;
use telos_adapters::{AdapterError, AgentAdapter, UnsupportedAdapter};
use telos_schema::{ApprovalAnswer, FailureKind, RunInspection, fixtures};

fn adapter() -> Arc<dyn AgentAdapter> {
    Arc::new(UnsupportedAdapter::new("test-double"))
}

fn is_unsupported<T: std::fmt::Debug>(result: &Result<T, AdapterError>) -> bool {
    matches!(result, Err(AdapterError::UnsupportedCapability { .. }))
}

#[tokio::test]
async fn claims_no_capabilities() {
    let adapter = adapter();
    assert_eq!(adapter.id(), "test-double");
    assert!(adapter.detect(fixtures::HOST_ID).await.unwrap().is_empty());
}

#[tokio::test]
async fn rejects_every_control_as_unsupported() {
    let adapter = adapter();
    let task = fixtures::task_spec();
    let context = fixtures::run_context(&task);

    assert!(is_unsupported(&adapter.start_run(&task, &context).await));
    assert!(is_unsupported(
        &adapter.send_input("run-1", "in-1", "hi").await
    ));
    assert!(is_unsupported(
        &adapter
            .answer_approval("run-1", "req-1", ApprovalAnswer::Deny)
            .await
    ));
    assert!(is_unsupported(&adapter.cancel_run("run-1").await));

    let mut events = adapter.events("run-1", 0);
    assert!(is_unsupported(&events.next().await.unwrap()));
    assert!(events.next().await.is_none());
}

#[tokio::test]
async fn reports_unknown_instead_of_guessing_run_state() {
    assert_eq!(
        adapter().inspect_run("run-1").await.unwrap(),
        RunInspection::Unknown
    );
}

#[test]
fn unsupported_maps_to_failure_kind() {
    let error = AdapterError::UnsupportedCapability {
        capability: "cancel_run".into(),
        reason: "test".into(),
    };
    assert_eq!(
        error.failure_kind(),
        Some(FailureKind::UnsupportedCapability)
    );
    assert_eq!(
        AdapterError::UnknownRun {
            run_id: "run-1".into()
        }
        .failure_kind(),
        None
    );
}

// --- CLI 탐지 Port (REQ-010). 실제 탐색은 PH0-HOST의 `discovery_*` 시험이 검사합니다. ---

struct NotFound;

#[async_trait::async_trait]
impl telos_adapters::CliDiscoverer for NotFound {
    async fn discover(
        &self,
        provider_id: &str,
        _rule: &telos_adapters::ProviderCliRule,
        request: &telos_adapters::DiscoveryRequest,
    ) -> telos_schema::CliDiscovery {
        telos_schema::CliDiscovery {
            provider_id: provider_id.into(),
            host_id: request.host_id.clone(),
            checked_at: request.checked_at.clone(),
            outcome: telos_schema::CliDiscoveryOutcome::Unavailable {
                failure: telos_schema::CliDiscoveryFailure::NotFound,
                source: None,
                reason: "no candidate".into(),
            },
        }
    }
}

#[tokio::test]
async fn cli_discoverer_is_object_safe_and_reports_failure_as_outcome() {
    use telos_adapters::{CliBinaryRule, CliDiscoverer, DiscoveryRequest, ProviderCliRule};

    let discoverer: Arc<dyn CliDiscoverer> = Arc::new(NotFound);
    let rule = ProviderCliRule {
        extension_id: "google.google-antigravity".into(),
        binary: CliBinaryRule::Managed {
            home_relative_path: ".gemini/bin/agy".into(),
        },
        binary_name: "agy".into(),
        version_args: vec!["--version".into()],
    };
    let request = DiscoveryRequest {
        host_id: "wsl-ubuntu-2604".into(),
        target_platform: "linux-x64".into(),
        explicit_path: None,
        extension_roots: vec!["/home/u/.vscode-server/extensions".into()],
        home_dir: "/home/u".into(),
        path_env: None,
        checked_at: "2026-10-24T08:00:00Z".into(),
    };
    let result = discoverer.discover("agy", &rule, &request).await;
    assert_eq!(result.host_id, "wsl-ubuntu-2604");
    assert!(result.found().is_none());
}

//! G0 개정 1 REQ-002: 공용 런타임 인터페이스의 사용 형태 시험 (PH0-INTEGRATION).
//!
//! 실제 프로세스 동작은 검사하지 않습니다. 같은 Handle을 공유하여 대기와 종료를 동시에 호출하고,
//! 대기 Future를 버린 뒤 다시 대기할 수 있는 형태로 인터페이스가 컴파일되는지 확인합니다.
//! 실제 취소 안전성·경합 시험은 PH0-HOST의 `runtime_*` 시험이 담당합니다.

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use futures::{FutureExt, stream::BoxStream};
use telos_adapters::runtime::{
    ChildrenReaped, ExitStatus, OutputEvent, ProcessHandle, ProcessIdentity, RuntimeError,
    TerminationCause, TerminationReport,
};
use tokio::sync::watch;

/// 종료 요청 시 즉시 끝나는 최소 대역입니다.
struct FakeHandle {
    identity: ProcessIdentity,
    exit: watch::Sender<Option<ExitStatus>>,
}

impl FakeHandle {
    fn new() -> Self {
        Self {
            identity: ProcessIdentity {
                pid: 1,
                started_at: "2026-10-24T00:00:00Z".into(),
            },
            exit: watch::channel(None).0,
        }
    }
}

#[async_trait]
impl ProcessHandle for FakeHandle {
    fn identity(&self) -> &ProcessIdentity {
        &self.identity
    }

    fn take_output(&self) -> Result<BoxStream<'static, OutputEvent>, RuntimeError> {
        Err(RuntimeError::OutputAlreadyTaken)
    }

    async fn write_stdin(&self, _data: &[u8]) -> Result<(), RuntimeError> {
        Err(RuntimeError::StdinUnavailable)
    }

    async fn close_stdin(&self) -> Result<(), RuntimeError> {
        Ok(())
    }

    async fn wait(&self) -> Result<ExitStatus, RuntimeError> {
        let mut rx = self.exit.subscribe();
        let status = rx
            .wait_for(Option::is_some)
            .await
            .map_err(|e| RuntimeError::Io {
                message: e.to_string(),
            })?;
        Ok(status.expect("checked by wait_for"))
    }

    async fn terminate(&self, _grace: Duration) -> Result<TerminationReport, RuntimeError> {
        let status = ExitStatus {
            code: None,
            cause: TerminationCause::TerminatedOnRequest,
        };
        // 이미 끝났다면 처음 기록한 종료 상태를 유지합니다(멱등).
        self.exit.send_if_modified(|current| {
            if current.is_none() {
                *current = Some(status);
                true
            } else {
                false
            }
        });
        Ok(TerminationReport {
            status: self.exit.borrow().expect("set above"),
            children: ChildrenReaped::All,
        })
    }
}

#[tokio::test]
async fn shared_handle_allows_concurrent_wait_and_terminate() {
    let handle: Arc<dyn ProcessHandle> = Arc::new(FakeHandle::new());

    let (waited, report) = tokio::join!(handle.wait(), handle.terminate(Duration::ZERO));
    assert_eq!(waited.unwrap(), report.unwrap().status);
}

#[tokio::test]
async fn dropped_wait_can_be_retried_and_terminate_is_repeatable() {
    let handle: Arc<dyn ProcessHandle> = Arc::new(FakeHandle::new());

    // 아직 끝나지 않은 대기 Future를 한 번 폴링한 뒤 버립니다.
    assert!(handle.wait().now_or_never().is_none());

    let first = handle.terminate(Duration::ZERO).await.unwrap();
    let second = handle.terminate(Duration::ZERO).await.unwrap();
    assert_eq!(first, second);
    assert_eq!(handle.wait().await.unwrap(), first.status);
}

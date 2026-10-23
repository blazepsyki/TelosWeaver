//! 미지원 시험 대역과 구현 전 Provider Stub.
//! Spec §2.1-7에 따라 어떤 Capability도 주장하지 않고 모든 제어를 명시적으로 거부합니다.

use std::collections::HashMap;

use async_trait::async_trait;
use futures::{StreamExt, stream::BoxStream};
use telos_schema::{
    ApprovalAnswer, Capability, ProviderEvent, RunContext, RunInspection, RunStarted, TaskSpec,
};

use crate::{AdapterError, AgentAdapter};

const NOT_IMPLEMENTED: &str = "adapter not implemented yet";

/// 모든 제어를 거부하는 `AgentAdapter` 시험 대역입니다.
/// Core·MASTER·HOST 시험에서 "Provider가 기능을 지원하지 않는 경우"를 재현할 때 사용합니다.
#[derive(Debug, Clone)]
pub struct UnsupportedAdapter {
    id: String,
}

impl UnsupportedAdapter {
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }

    fn unsupported(capability: &str) -> AdapterError {
        AdapterError::UnsupportedCapability {
            capability: capability.to_owned(),
            reason: NOT_IMPLEMENTED.to_owned(),
        }
    }
}

#[async_trait]
impl AgentAdapter for UnsupportedAdapter {
    fn id(&self) -> &str {
        &self.id
    }

    async fn detect(&self, _host_id: &str) -> Result<HashMap<String, Capability>, AdapterError> {
        Ok(HashMap::new())
    }

    async fn start_run(
        &self,
        _task: &TaskSpec,
        _context: &RunContext,
    ) -> Result<RunStarted, AdapterError> {
        Err(Self::unsupported("start_run"))
    }

    fn events(
        &self,
        _run_id: &str,
        _after_sequence: u64,
    ) -> BoxStream<'static, Result<ProviderEvent, AdapterError>> {
        futures::stream::once(async { Err(Self::unsupported("events")) }).boxed()
    }

    async fn send_input(
        &self,
        _run_id: &str,
        _input_id: &str,
        _text: &str,
    ) -> Result<(), AdapterError> {
        Err(Self::unsupported("send_input"))
    }

    async fn answer_approval(
        &self,
        _run_id: &str,
        _request_id: &str,
        _decision: ApprovalAnswer,
    ) -> Result<(), AdapterError> {
        Err(Self::unsupported("answer_approval"))
    }

    async fn cancel_run(&self, _run_id: &str) -> Result<bool, AdapterError> {
        Err(Self::unsupported("cancel_run"))
    }

    async fn inspect_run(&self, _run_id: &str) -> Result<RunInspection, AdapterError> {
        Ok(RunInspection::Unknown)
    }
}

/// 구현 전 Provider 타입을 [`UnsupportedAdapter`]에 위임합니다. 각 Provider가 실제 구현으로 교체할 때 제거합니다.
macro_rules! unimplemented_adapter {
    ($ty:ty, $id:literal) => {
        #[async_trait::async_trait]
        impl $crate::AgentAdapter for $ty {
            fn id(&self) -> &str {
                $id
            }

            async fn detect(
                &self,
                host_id: &str,
            ) -> Result<
                std::collections::HashMap<String, telos_schema::Capability>,
                $crate::AdapterError,
            > {
                $crate::UnsupportedAdapter::new($id).detect(host_id).await
            }

            async fn start_run(
                &self,
                task: &telos_schema::TaskSpec,
                context: &telos_schema::RunContext,
            ) -> Result<telos_schema::RunStarted, $crate::AdapterError> {
                $crate::UnsupportedAdapter::new($id)
                    .start_run(task, context)
                    .await
            }

            fn events(
                &self,
                run_id: &str,
                after_sequence: u64,
            ) -> futures::stream::BoxStream<
                'static,
                Result<telos_schema::ProviderEvent, $crate::AdapterError>,
            > {
                $crate::UnsupportedAdapter::new($id).events(run_id, after_sequence)
            }

            async fn send_input(
                &self,
                run_id: &str,
                input_id: &str,
                text: &str,
            ) -> Result<(), $crate::AdapterError> {
                $crate::UnsupportedAdapter::new($id)
                    .send_input(run_id, input_id, text)
                    .await
            }

            async fn answer_approval(
                &self,
                run_id: &str,
                request_id: &str,
                decision: telos_schema::ApprovalAnswer,
            ) -> Result<(), $crate::AdapterError> {
                $crate::UnsupportedAdapter::new($id)
                    .answer_approval(run_id, request_id, decision)
                    .await
            }

            async fn cancel_run(&self, run_id: &str) -> Result<bool, $crate::AdapterError> {
                $crate::UnsupportedAdapter::new($id)
                    .cancel_run(run_id)
                    .await
            }

            async fn inspect_run(
                &self,
                run_id: &str,
            ) -> Result<telos_schema::RunInspection, $crate::AdapterError> {
                $crate::UnsupportedAdapter::new($id)
                    .inspect_run(run_id)
                    .await
            }
        }
    };
}

pub(crate) use unimplemented_adapter;

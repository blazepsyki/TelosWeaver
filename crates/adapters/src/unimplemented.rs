//! 구현 전 Adapter의 공통 동작입니다.
//! Spec §2.1-7에 따라 어떤 Capability도 주장하지 않고 모든 제어를 명시적으로 거부합니다.

use crate::AdapterError;

const NOT_IMPLEMENTED: &str = "adapter not implemented yet";

pub(crate) fn unsupported(capability: &str) -> AdapterError {
    AdapterError::UnsupportedCapability {
        capability: capability.to_owned(),
        reason: NOT_IMPLEMENTED.to_owned(),
    }
}

/// `AgentAdapter`를 미구현 상태로 구현합니다. 실제 구현으로 교체할 때 제거합니다.
macro_rules! unimplemented_adapter {
    ($ty:ty, $id:literal) => {
        #[async_trait::async_trait]
        impl $crate::AgentAdapter for $ty {
            fn id(&self) -> &str {
                $id
            }

            async fn detect(
                &self,
                _host_id: &str,
            ) -> Result<
                std::collections::HashMap<String, telos_schema::Capability>,
                $crate::AdapterError,
            > {
                Ok(std::collections::HashMap::new())
            }

            async fn start_run(
                &self,
                _task: &telos_schema::TaskSpec,
                _context: &telos_schema::RunContext,
            ) -> Result<telos_schema::RunStarted, $crate::AdapterError> {
                Err($crate::unimplemented::unsupported("start_run"))
            }

            fn events(
                &self,
                _run_id: &str,
                _after_sequence: u64,
            ) -> futures::stream::BoxStream<
                'static,
                Result<telos_schema::ProviderEvent, $crate::AdapterError>,
            > {
                use futures::StreamExt;
                futures::stream::once(async { Err($crate::unimplemented::unsupported("events")) })
                    .boxed()
            }

            async fn send_input(
                &self,
                _run_id: &str,
                _input_id: &str,
                _text: &str,
            ) -> Result<(), $crate::AdapterError> {
                Err($crate::unimplemented::unsupported("send_input"))
            }

            async fn answer_approval(
                &self,
                _run_id: &str,
                _request_id: &str,
                _decision: telos_schema::ApprovalAnswer,
            ) -> Result<(), $crate::AdapterError> {
                Err($crate::unimplemented::unsupported("answer_approval"))
            }

            async fn cancel_run(&self, _run_id: &str) -> Result<bool, $crate::AdapterError> {
                Err($crate::unimplemented::unsupported("cancel_run"))
            }

            async fn inspect_run(
                &self,
                _run_id: &str,
            ) -> Result<telos_schema::RunInspection, $crate::AdapterError> {
                Ok(telos_schema::RunInspection::Unknown)
            }
        }
    };
}

pub(crate) use unimplemented_adapter;

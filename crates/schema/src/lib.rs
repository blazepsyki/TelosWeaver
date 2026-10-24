//! TelosWeaver 공통 계약 타입 (Spec §3.4).
//!
//! Wire 형식은 JSON이며 구조체 필드는 camelCase, 열거 값은 Spec의 문자열 원문을 사용합니다.
//! VS Code Extension이 쓰는 타입은 이 crate에서 생성한 JSON Schema(`schemas/`)를 원본으로 삼습니다.
//! G0 계약 범위와 정의 위치는 `docs/phase-0/WORK_ALLOCATION.md` §5를 따릅니다.

pub mod api;
mod artifact;
mod decision;
mod events;
mod failure;
#[cfg(feature = "fixtures")]
pub mod fixtures;
mod limits;
mod master;
mod messaging;
mod provider;
pub mod rn;
mod secret;
mod task;
mod validation;

pub use artifact::*;
pub use decision::*;
pub use events::*;
pub use failure::*;
pub use limits::*;
pub use master::*;
pub use messaging::*;
pub use provider::*;
pub use secret::*;
pub use task::*;
pub use validation::*;

/// 구조가 정해지지 않은 JSON 객체 (`Record<string, unknown>`에 해당).
pub type JsonObject = serde_json::Map<String, serde_json::Value>;

/// `schemas/<name>.schema.json`으로 내보내는 최상위 계약 타입 목록입니다.
/// 이름 앞부분(`Api`, `Rn`)으로 계약 영역을 구분합니다.
pub fn json_schemas() -> Vec<(&'static str, schemars::Schema)> {
    use schemars::schema_for;
    vec![
        // 도메인·Adapter 실행 (§4~§7)
        ("MasterDecision", schema_for!(MasterDecision)),
        ("TaskSpec", schema_for!(TaskSpec)),
        ("TaskResult", schema_for!(TaskResult)),
        ("ArtifactVersion", schema_for!(ArtifactVersion)),
        ("Capability", schema_for!(Capability)),
        ("RunContext", schema_for!(RunContext)),
        ("RunStarted", schema_for!(RunStarted)),
        ("ProviderEvent", schema_for!(ProviderEvent)),
        ("ProviderEventPayload", schema_for!(ProviderEventPayload)),
        // 마스터 결정 검증 (§4.2)
        ("DecisionPayload", schema_for!(DecisionPayload)),
        ("DecisionContext", schema_for!(DecisionContext)),
        ("DecisionOutcome", schema_for!(DecisionOutcome)),
        // Local Core API (§3.3, §17)
        ("ApiRequest", schema_for!(api::ApiRequest)),
        ("ApiResponse", schema_for!(api::ApiResponse)),
        ("ApiNotification", schema_for!(api::ApiNotification)),
        ("ApiHelloParams", schema_for!(api::HelloParams)),
        ("ApiHelloResult", schema_for!(api::HelloResult)),
        ("ApiHealthResult", schema_for!(api::HealthResult)),
        ("ApiRunsStartParams", schema_for!(api::RunsStartParams)),
        ("ApiRunsStartResult", schema_for!(api::RunsStartResult)),
        ("ApiRunRef", schema_for!(api::RunRef)),
        ("ApiRunsInspectResult", schema_for!(api::RunsInspectResult)),
        ("ApiRunsCancelParams", schema_for!(api::RunsCancelParams)),
        ("ApiRunsCancelResult", schema_for!(api::RunsCancelResult)),
        (
            "ApiRunsSubscribeParams",
            schema_for!(api::RunsSubscribeParams),
        ),
        (
            "ApiRunsSubscribeResult",
            schema_for!(api::RunsSubscribeResult),
        ),
        ("ApiSubscriptionRef", schema_for!(api::SubscriptionRef)),
        ("ApiRunEventParams", schema_for!(api::RunEventParams)),
        (
            "ApiSubscriptionClosedParams",
            schema_for!(api::SubscriptionClosedParams),
        ),
        // RN RPC (§12)
        ("RnRequest", schema_for!(rn::RnRequest)),
        ("RnResponse", schema_for!(rn::RnResponse)),
        ("RnHealth", schema_for!(rn::RnHealth)),
        ("RnProviderConnector", schema_for!(rn::ProviderConnector)),
        ("RnSessionQuery", schema_for!(rn::SessionQuery)),
        ("RnSessionEndpoint", schema_for!(rn::SessionEndpoint)),
        (
            "RnSessionCapabilities",
            schema_for!(rn::SessionCapabilities),
        ),
        ("RnEndpointRef", schema_for!(rn::EndpointRef)),
        ("RnMessageEnvelope", schema_for!(rn::MessageEnvelope)),
        ("RnMessageRef", schema_for!(rn::MessageRef)),
        ("RnDeliveryReceipt", schema_for!(rn::DeliveryReceipt)),
        ("RnSubscribeParams", schema_for!(rn::SubscribeParams)),
        ("RnEvent", schema_for!(rn::RnEvent)),
    ]
}

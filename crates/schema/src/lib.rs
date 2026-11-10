//! TelosWeaver 공통 계약 타입 (Spec §3.4).
//!
//! Wire 형식은 JSON이며 구조체 필드는 camelCase, 열거 값은 Spec의 문자열 원문을 사용합니다.
//! VS Code Extension이 쓰는 타입은 이 crate에서 생성한 JSON Schema(`schemas/`)를 원본으로 삼습니다.
//! G0 계약 범위와 정의 위치는 `docs/phase-0/WORK_ALLOCATION.md` §5를 따릅니다.

pub mod api;
mod artifact;
pub mod capability_keys;
mod context;
mod decision;
mod discovery;
mod events;
mod failure;
#[cfg(feature = "fixtures")]
pub mod fixtures;
mod journal;
mod limits;
mod master;
mod mcp;
mod messaging;
mod policy;
mod provider;
pub mod rn;
mod secret;
mod state;
mod task;
mod validation;
mod workspace;

pub use artifact::*;
pub use context::*;
pub use decision::*;
pub use discovery::*;
pub use events::*;
pub use failure::*;
pub use journal::*;
pub use limits::*;
pub use master::*;
pub use mcp::*;
pub use messaging::*;
pub use policy::*;
pub use provider::*;
pub use secret::*;
pub use state::*;
pub use task::*;
pub use validation::*;
pub use workspace::*;

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
        ("CommonCapabilityKey", capability_keys::schema()),
        ("RunContext", schema_for!(RunContext)),
        ("RunStarted", schema_for!(RunStarted)),
        ("ProviderEvent", schema_for!(ProviderEvent)),
        ("ProviderEventPayload", schema_for!(ProviderEventPayload)),
        ("CliDiscovery", schema_for!(CliDiscovery)),
        // Phase 1 H0: Task·Run·Session 모델 (§5.1, §8.1, §9.4)
        ("Plan", schema_for!(Plan)),
        ("TaskRecord", schema_for!(TaskRecord)),
        ("RunRecord", schema_for!(RunRecord)),
        ("SessionRecord", schema_for!(SessionRecord)),
        ("RunSession", schema_for!(RunSession)),
        // Phase 1 H0: 영속 기록 (§14)
        ("RunIntent", schema_for!(RunIntent)),
        ("JournalEntry", schema_for!(JournalEntry)),
        ("IdempotencyRecord", schema_for!(IdempotencyRecord)),
        // Phase 1 H0: Task MCP (§9.2)
        ("TaskMcpBinding", schema_for!(TaskMcpBinding)),
        ("McpDelegateTaskInput", schema_for!(DelegateTaskInput)),
        ("McpDelegateTaskOutput", schema_for!(DelegateTaskOutput)),
        ("McpGetTaskResultInput", schema_for!(GetTaskResultInput)),
        ("McpGetTaskResultOutput", schema_for!(GetTaskResultOutput)),
        ("McpListTasksInput", schema_for!(ListTasksInput)),
        ("McpListTasksOutput", schema_for!(ListTasksOutput)),
        ("McpCancelTaskInput", schema_for!(CancelTaskInput)),
        ("McpCancelTaskOutput", schema_for!(CancelTaskOutput)),
        ("McpGetAgentStatusInput", schema_for!(GetAgentStatusInput)),
        ("McpGetAgentStatusOutput", schema_for!(GetAgentStatusOutput)),
        (
            "McpReportTaskResultInput",
            schema_for!(ReportTaskResultInput),
        ),
        (
            "McpReportTaskResultOutput",
            schema_for!(ReportTaskResultOutput),
        ),
        ("McpTaskToolError", schema_for!(TaskToolError)),
        // Phase 1 H0: 산출물·검증 (§5.3, §11)
        ("ArtifactReference", schema_for!(ArtifactReference)),
        ("ValidationJob", schema_for!(ValidationJob)),
        (
            "CommandValidationParameters",
            schema_for!(CommandValidationParameters),
        ),
        ("ApprovalDecision", schema_for!(ApprovalDecision)),
        // Phase 1 H0: Workspace (§10)
        ("WorkspaceRecord", schema_for!(WorkspaceRecord)),
        ("ResourceLock", schema_for!(ResourceLock)),
        ("DiffSummary", schema_for!(DiffSummary)),
        ("SourceMergeRequest", schema_for!(SourceMergeRequest)),
        ("SourceMergeResult", schema_for!(SourceMergeResult)),
        // Phase 1 H0: 문맥 (§6, §8)
        ("ContextSnapshot", schema_for!(ContextSnapshot)),
        ("CapsuleContent", schema_for!(CapsuleContent)),
        ("UsageRecord", schema_for!(UsageRecord)),
        // Phase 1 H0: 마스터 결정·정책 (§4.3, §13)
        ("PermissionSet", schema_for!(PermissionSet)),
        ("BudgetProfile", schema_for!(BudgetProfile)),
        ("BudgetLedger", schema_for!(BudgetLedger)),
        (
            "ProviderApprovalRequest",
            schema_for!(ProviderApprovalRequest),
        ),
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
        // Phase 1 H0: Local Core API (§17)
        ("ApiPlansCreateParams", schema_for!(api::PlansCreateParams)),
        ("ApiPlansCreateResult", schema_for!(api::PlansCreateResult)),
        ("ApiPlanRef", schema_for!(api::PlanRef)),
        (
            "ApiPlansInspectResult",
            schema_for!(api::PlansInspectResult),
        ),
        ("ApiPlansCancelParams", schema_for!(api::PlansCancelParams)),
        (
            "ApiPlansMessageParams",
            schema_for!(api::PlansMessageParams),
        ),
        (
            "ApiPlansMessageResult",
            schema_for!(api::PlansMessageResult),
        ),
        ("ApiAcceptedResult", schema_for!(api::AcceptedResult)),
        ("ApiTaskRef", schema_for!(api::TaskRef)),
        (
            "ApiTasksInspectResult",
            schema_for!(api::TasksInspectResult),
        ),
        ("ApiTasksCancelParams", schema_for!(api::TasksCancelParams)),
        ("ApiTasksToolParams", schema_for!(api::TasksToolParams)),
        (
            "ApiArtifactsReadParams",
            schema_for!(api::ArtifactsReadParams),
        ),
        (
            "ApiArtifactsReadResult",
            schema_for!(api::ArtifactsReadResult),
        ),
        ("ApiIntervention", schema_for!(api::Intervention)),
        (
            "ApiInterventionsListResult",
            schema_for!(api::InterventionsListResult),
        ),
        (
            "ApiInterventionsAnswerParams",
            schema_for!(api::InterventionsAnswerParams),
        ),
        (
            "ApiJournalSubscribeParams",
            schema_for!(api::JournalSubscribeParams),
        ),
        (
            "ApiJournalSubscribeResult",
            schema_for!(api::JournalSubscribeResult),
        ),
        (
            "ApiJournalEntryParams",
            schema_for!(api::JournalEntryParams),
        ),
        ("ApiRnStatusResult", schema_for!(api::RnStatusResult)),
        ("ApiRnSendParams", schema_for!(api::RnSendParams)),
        ("ApiRnSendResult", schema_for!(api::RnSendResult)),
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
        ("RnRpcInfo", schema_for!(rn::RnRpcInfo)),
        ("RnHelloParams", schema_for!(rn::RnHelloParams)),
        ("RnHelloResult", schema_for!(rn::RnHelloResult)),
        ("RnNotification", schema_for!(rn::RnNotification)),
        ("RnEventParams", schema_for!(rn::RnEventParams)),
        ("RnSubscribeResult", schema_for!(rn::SubscribeResult)),
        ("RnInboundMessage", schema_for!(rn::InboundMessage)),
    ]
}

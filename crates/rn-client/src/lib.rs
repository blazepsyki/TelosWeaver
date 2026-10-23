//! Reason Network Fork Local RPC 클라이언트 (Spec §12.1).
//!
//! 메서드별 Payload Schema는 RN Fork의 `rpc_schema_version`과 함께 Phase 0에서 고정합니다.

use serde::{Deserialize, Serialize};

pub type JsonObject = serde_json::Map<String, serde_json::Value>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RpcMethod {
    #[serde(rename = "health.get")]
    HealthGet,
    #[serde(rename = "providers.list")]
    ProvidersList,
    #[serde(rename = "sessions.discover")]
    SessionsDiscover,
    #[serde(rename = "sessions.resolve")]
    SessionsResolve,
    #[serde(rename = "sessions.capabilities")]
    SessionsCapabilities,
    #[serde(rename = "messages.send")]
    MessagesSend,
    #[serde(rename = "messages.status")]
    MessagesStatus,
    #[serde(rename = "messages.subscribe")]
    MessagesSubscribe,
    /// P1
    #[serde(rename = "providers.agy.probe")]
    ProvidersAgyProbe,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcRequest {
    pub protocol_version: String,
    pub request_id: String,
    pub method: RpcMethod,
    pub params: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcError {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// 응답은 요청과 같은 `request_id`를 가집니다.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcResponse {
    pub protocol_version: String,
    pub request_id: String,
    #[serde(flatten)]
    pub outcome: RpcOutcome,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RpcOutcome {
    Result(serde_json::Value),
    Error(RpcError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn method_names_match_spec() {
        assert_eq!(
            serde_json::to_value(RpcMethod::HealthGet).unwrap(),
            json!("health.get")
        );
        assert_eq!(
            serde_json::to_value(RpcMethod::ProvidersAgyProbe).unwrap(),
            json!("providers.agy.probe")
        );
    }

    #[test]
    fn error_response_round_trips() {
        let wire = json!({
            "protocolVersion": "0",
            "requestId": "r1",
            "error": { "code": "unsupported", "message": "no" },
        });
        let response: RpcResponse = serde_json::from_value(wire.clone()).unwrap();
        assert!(matches!(response.outcome, RpcOutcome::Error(_)));
        assert_eq!(serde_json::to_value(&response).unwrap(), wire);
    }
}

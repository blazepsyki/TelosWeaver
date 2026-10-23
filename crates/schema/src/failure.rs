//! Spec §7.3 실행 실패 분류

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 알 수 없는 상태를 성공으로 처리하지 않습니다. 분류할 수 없으면 `Unknown`입니다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    StartFailed,
    Authentication,
    Configuration,
    ProtocolParse,
    ToolDenied,
    Timeout,
    ResourceLimit,
    Provider,
    UnsupportedCapability,
    Unknown,
}

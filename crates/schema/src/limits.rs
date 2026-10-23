//! Spec §13.2 예산과 제어 상한

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ControlLimits {
    pub provider_concurrency: u32,
    pub max_delegation_depth: u32,
    pub max_plan_tasks: u32,
    pub execution_retry_limit: u32,
    pub rework_limit: u32,
}

/// 검증된 권장 수치가 아니라 PoC 시작값입니다. 설정으로 바꿀 수 있습니다.
impl Default for ControlLimits {
    fn default() -> Self {
        Self {
            provider_concurrency: 1,
            max_delegation_depth: 3,
            max_plan_tasks: 32,
            execution_retry_limit: 1,
            rework_limit: 2,
        }
    }
}

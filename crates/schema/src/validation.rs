//! Spec §11.1 검증 Profile

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ValidationVerdict {
    Passed,
    Failed,
    Error,
    Inconclusive,
    Skipped,
}

impl ValidationVerdict {
    /// 필수 검사는 `Passed`만 통과로 인정합니다. 실행 오류와 판정 불가를 성공으로 표시하지 않습니다.
    pub fn passes_required_check(self) -> bool {
        self == Self::Passed
    }
}

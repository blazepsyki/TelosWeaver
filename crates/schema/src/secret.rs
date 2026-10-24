//! 비밀값 래퍼 (G0 개정 1 REQ-005, SAFE-04).

use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Wire에서는 일반 문자열이지만 `Debug`로 원문을 출력하지 않습니다.
/// 원문은 전송·검증이 필요한 곳에서 [`SecretString::expose`]로만 꺼냅니다.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct SecretString(String);

impl SecretString {
    pub const REDACTED: &'static str = "<redacted>";

    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(Self::REDACTED)
    }
}

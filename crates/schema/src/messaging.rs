//! Spec §12.2 Message와 DeliveryReceipt

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum DeliveryTier {
    Push,
    Turn,
    Pull,
}

/// 전달 상태입니다. 어떤 값도 Task 완료를 뜻하지 않습니다 (DELEG-09).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum DeliveryReceiptStatus {
    Accepted,
    Queued,
    Delivered,
    Held,
    Refused,
    Expired,
    Failed,
}

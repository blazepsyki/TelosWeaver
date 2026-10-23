//! TelosWeaver 공통 계약 타입 (Spec §3.4).
//!
//! Wire 형식은 JSON이며 구조체 필드는 camelCase, 열거 값은 Spec의 문자열 원문을 사용합니다.
//! VS Code Extension이 쓰는 타입은 이 crate에서 생성한 JSON Schema(`schemas/`)를 원본으로 삼습니다.

mod artifact;
mod limits;
mod master;
mod messaging;
mod provider;
mod task;
mod validation;

pub use artifact::*;
pub use limits::*;
pub use master::*;
pub use messaging::*;
pub use provider::*;
pub use task::*;
pub use validation::*;

/// 구조가 정해지지 않은 JSON 객체 (`Record<string, unknown>`에 해당).
pub type JsonObject = serde_json::Map<String, serde_json::Value>;

/// `schemas/<name>.schema.json`으로 내보내는 최상위 계약 타입 목록입니다.
pub fn json_schemas() -> Vec<(&'static str, schemars::Schema)> {
    use schemars::schema_for;
    vec![
        ("MasterDecision", schema_for!(MasterDecision)),
        ("TaskSpec", schema_for!(TaskSpec)),
        ("TaskResult", schema_for!(TaskResult)),
        ("ArtifactVersion", schema_for!(ArtifactVersion)),
        ("Capability", schema_for!(Capability)),
        ("RunContext", schema_for!(RunContext)),
        ("ProviderEvent", schema_for!(ProviderEvent)),
    ]
}

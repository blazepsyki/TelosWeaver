//! Managed 연동 기준: Headless 단발 또는 NDJSON 입출력, Turn별 입력 직렬화 (Spec §7.1, §7.4, 미검증)

use crate::unimplemented::unimplemented_adapter;

#[derive(Debug, Default)]
pub struct AgyAdapter;

unimplemented_adapter!(AgyAdapter, "agy");

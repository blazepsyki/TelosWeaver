//! Managed 연동 기준: `codex app-server`, Thread/Turn 이벤트. 포크: `thread/fork` (Spec §7.1, 미검증)

use crate::unimplemented::unimplemented_adapter;

#[derive(Debug, Default)]
pub struct CodexAdapter;

unimplemented_adapter!(CodexAdapter, "codex");

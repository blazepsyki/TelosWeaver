//! Managed 연동 기준: `claude -p` + Stream JSON. 포크: `--resume`/`--continue` + `--fork-session` (Spec §7.1, 미검증)

use crate::unimplemented::unimplemented_adapter;

#[derive(Debug, Default)]
pub struct ClaudeCodeAdapter;

unimplemented_adapter!(ClaudeCodeAdapter, "claude");

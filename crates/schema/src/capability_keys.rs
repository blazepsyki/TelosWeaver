//! Common Provider capability names (Spec §7.1/§7.2, REQ-024).
//! The catalogue does not close String maps to Provider-specific or future keys.

pub const MANAGED_RUN: &str = "managed_run";
pub const EVENT_STREAM: &str = "event_stream";
pub const CANCEL: &str = "cancel";
pub const NATIVE_RESUME: &str = "native_resume";
pub const NATIVE_FORK: &str = "native_fork";
pub const SEND_INPUT: &str = "send_input";
pub const ANSWER_APPROVAL: &str = "answer_approval";
pub const VSCODE_LISTING: &str = "vscode_listing";

/// The single catalogue used for both Rust consumers and the generated key schema.
pub const ALL: &[&str] = &[
    MANAGED_RUN,
    EVENT_STREAM,
    CANCEL,
    NATIVE_RESUME,
    NATIVE_FORK,
    SEND_INPUT,
    ANSWER_APPROVAL,
    VSCODE_LISTING,
];

pub(crate) fn schema() -> schemars::Schema {
    schemars::json_schema!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "CommonCapabilityKey",
        "type": "string",
        "enum": ALL,
    })
}

use serde_json::json;

use rust_multi_cli_harness::{HarnessMessage, MessageLevel, MessageSource};

#[test]
fn harness_message_can_build_system_events() {
    let message = HarnessMessage::system(
        "step-1",
        "corr-1",
        MessageLevel::Info,
        "ok",
        json!({"kind": "assistant", "text": "ok"}),
    );

    assert_eq!(message.tool_id, "system");
    assert_eq!(message.source, MessageSource::System);
    assert_eq!(message.level, MessageLevel::Info);
    assert!(!message.truncated);
}

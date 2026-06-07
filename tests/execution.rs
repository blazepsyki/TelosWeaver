use std::sync::Arc;
use std::time::Duration;

use rust_multi_cli_harness::{execute_tool, parse_with_rhai, AppState, Config, ToolDefinition};
use tokio::sync::RwLock;

fn test_tool(script: &str, command: &str, args: Vec<&str>) -> ToolDefinition {
    ToolDefinition {
        id: "test-tool".to_owned(),
        name: "Test Tool".to_owned(),
        command: command.to_owned(),
        args: args.into_iter().map(str::to_owned).collect(),
        parser: rust_multi_cli_harness::ToolParser {
            entrypoint: "parse".to_owned(),
            script: script.to_owned(),
        },
        working_dir: None,
    }
}

#[test]
fn parse_with_rhai_builds_harness_message() {
    let tool = test_tool(
        r#"
            #{
                "tool_id": tool_id,
                "source": if stderr_text.len() > 0 { "stderr" } else { "stdout" },
                "level": if exit_code != 0 { "error" } else { "info" },
                "raw_output": stdout_text,
                "normalized_payload": #{
                    "exit_code": exit_code,
                    "stdout_text": stdout_text,
                    "stderr_text": stderr_text
                },
                "truncated": false
            }
        "#,
        "pwsh",
        vec!["-NoProfile", "-Command", "Write-Output 'hello'"],
    );

    let message = parse_with_rhai(&tool, Some(0), "hello", "", "step-1", "corr-1")
        .expect("Rhai script should parse");

    assert_eq!(message.tool_id, "test-tool");
    assert_eq!(message.step_id, "step-1");
    assert!(!message.truncated);
}

#[tokio::test]
async fn execute_tool_truncates_long_output() {
    let tool = test_tool(
        r#"
            #{
                "tool_id": tool_id,
                "source": "stdout",
                "level": "info",
                "raw_output": stdout_text,
                "normalized_payload": #{
                    "exit_code": exit_code,
                    "stdout_text": stdout_text,
                    "stderr_text": stderr_text
                },
                "truncated": false
            }
        "#,
        "pwsh",
        vec![
            "-NoProfile",
            "-Command",
            "$i=1; while ($i -le 101) { Write-Output $i; $i++ }",
        ],
    );

    let config = Arc::new(Config {
        version: 1,
        supervisor: None,
        tools: vec![tool],
    });
    let state = Arc::new(RwLock::new(AppState::new()));

    let message = execute_tool(
        config,
        state,
        None,
        "test-tool",
        "step-1",
        "corr-1",
        Duration::from_secs(60),
    )
    .await
    .expect("tool should execute successfully");

    assert!(message.truncated);
    assert!(message.raw_output.contains("[Skipped"));
    assert_eq!(message.step_id, "step-1");
}

#[tokio::test]
async fn execute_tool_truncates_very_long_single_line() {
    let tool = test_tool(
        r#"
            #{
                "tool_id": tool_id,
                "source": "stdout",
                "level": "info",
                "raw_output": stdout_text,
                "normalized_payload": #{
                    "exit_code": exit_code,
                    "stdout_text": stdout_text,
                    "stderr_text": stderr_text
                },
                "truncated": false
            }
        "#,
        "pwsh",
        vec!["-NoProfile", "-Command", "Write-Output ('x' * 5000)"],
    );

    let config = Arc::new(Config {
        version: 1,
        supervisor: None,
        tools: vec![tool],
    });
    let state = Arc::new(RwLock::new(AppState::new()));

    let message = execute_tool(
        config,
        state,
        None,
        "test-tool",
        "step-1",
        "corr-1",
        Duration::from_secs(60),
    )
    .await
    .expect("tool should execute successfully");

    assert!(message.truncated);
    assert!(message.raw_output.contains("[Truncated"));
    assert!(message.raw_output.len() < 5000);
}

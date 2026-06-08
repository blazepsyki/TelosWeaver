use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use rust_multi_cli_harness::{
    parse_supervisor_decision, run_orchestration_loop, write_harness_context_file, AppState,
    Config, SupervisorDefinition, ToolDefinition, ToolParser,
};
use serde_json::json;
use tokio::sync::RwLock;
use uuid::Uuid;

fn temp_workspace() -> PathBuf {
    let path = std::env::temp_dir().join(format!("harness-test-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&path).expect("temp workspace should be creatable");
    path
}

fn worker_tool(work_dir: &PathBuf) -> ToolDefinition {
    worker_tool_with_output(work_dir, "worker completed")
}

fn worker_tool_with_output(work_dir: &PathBuf, output: &str) -> ToolDefinition {
    worker_tool_with_id_and_output(work_dir, "test-tool", output)
}

fn worker_tool_with_id_and_output(work_dir: &PathBuf, id: &str, output: &str) -> ToolDefinition {
    ToolDefinition {
        id: id.to_owned(),
        name: "Test Tool".to_owned(),
        command: "pwsh".to_owned(),
        args: vec![
            "-NoProfile".to_owned(),
            "-Command".to_owned(),
            format!(
                "$null = $args; Write-Output '{}'",
                output.replace('\'', "''")
            ),
        ],
        parser: ToolParser {
            entrypoint: "parse".to_owned(),
            script: r#"
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
            "#
            .to_owned(),
        },
        working_dir: Some(work_dir.display().to_string()),
    }
}

fn actions_supervisor_tool(work_dir: &PathBuf) -> SupervisorDefinition {
    SupervisorDefinition {
        command: "pwsh".to_owned(),
        args: vec![
            "-NoProfile".to_owned(),
            "-Command".to_owned(),
            "$content = Get-Content $env:HARNESS_CONTEXT_FILE -Raw; if ($content -match 'task-one' -and $content -match 'task-two') { Write-Output '{\"is_done\":true,\"summary\":\"done\",\"actions\":[]}' } else { Write-Output '{\"is_done\":false,\"summary\":\"split\",\"actions\":[{\"task_id\":\"task-one\",\"tool_id\":\"tool-one\",\"args\":[]},{\"task_id\":\"task-two\",\"tool_id\":\"tool-two\",\"args\":[]}]}' }".to_owned(),
        ],
        working_dir: Some(work_dir.display().to_string()),
        parser: ToolParser {
            entrypoint: "decide".to_owned(),
            script: r#"
                #{
                    "summary": stdout_text
                }
            "#
            .to_owned(),
        },
    }
}

fn stdout_fallback_supervisor_tool(work_dir: &PathBuf) -> SupervisorDefinition {
    SupervisorDefinition {
        command: "unused".to_owned(),
        args: vec![],
        working_dir: Some(work_dir.display().to_string()),
        parser: ToolParser {
            entrypoint: "decide".to_owned(),
            script: r#"
                #{
                    "summary": stdout_text
                }
            "#
            .to_owned(),
        },
    }
}

fn builtin_supervisor_tool(work_dir: &PathBuf) -> SupervisorDefinition {
    SupervisorDefinition {
        command: "__harness_done_supervisor".to_owned(),
        args: vec![],
        working_dir: Some(work_dir.display().to_string()),
        parser: ToolParser {
            entrypoint: "decide".to_owned(),
            script: r#"
                let finished = stdout_text.contains("done");
                #{
                    "next_tool": if finished { () } else { "test-tool" },
                    "tool_args": [],
                    "is_done": finished,
                    "summary": if finished { "Supervisor marked the task complete" } else { "Continue with the next worker" }
                }
            "#
            .to_owned(),
        },
    }
}

fn supervisor_tool(work_dir: &PathBuf) -> SupervisorDefinition {
    SupervisorDefinition {
        command: "pwsh".to_owned(),
        args: vec![
            "-NoProfile".to_owned(),
            "-Command".to_owned(),
            "$content = Get-Content $env:HARNESS_CONTEXT_FILE -Raw; if ($content -match 'worker completed') { Write-Output 'done' } else { Write-Output 'continue' }".to_owned(),
        ],
        working_dir: Some(work_dir.display().to_string()),
        parser: ToolParser {
            entrypoint: "decide".to_owned(),
            script: r#"
                let finished = stdout_text.contains("done");
                #{
                    "next_tool": if finished { () } else { "test-tool" },
                    "tool_args": if finished { [] } else { ["--review"] },
                    "is_done": finished,
                    "summary": if finished { "Supervisor marked the task complete" } else { "Continue with the next worker" }
                }
            "#
            .to_owned(),
        },
    }
}

#[tokio::test]
async fn writes_context_file_with_state_and_tools() {
    let work_dir = temp_workspace();
    let config = Config {
        version: 1,
        supervisor: None,
        tools: vec![worker_tool(&work_dir)],
    };
    let mut state = AppState::new();
    state.begin_tool_run("test-tool", "step-1", "corr-1");

    let path = work_dir.join(".harness_context.json");
    let written = write_harness_context_file(&path, &state, &config)
        .await
        .expect("context file should write");

    let content = tokio::fs::read_to_string(written)
        .await
        .expect("context file should be readable");

    assert!(content.contains("test-tool"));
    assert!(content.contains("worker_tools"));
}

#[tokio::test]
async fn orchestration_loop_runs_supervisor_then_worker_until_done() {
    let work_dir = temp_workspace();
    let config = Arc::new(Config {
        version: 1,
        supervisor: Some(supervisor_tool(&work_dir)),
        tools: vec![worker_tool(&work_dir)],
    });
    let state = Arc::new(RwLock::new(AppState::new()));

    run_orchestration_loop(
        config,
        state.clone(),
        None,
        None,
        Duration::from_secs(30),
        Some(Duration::from_secs(30)),
        3,
    )
    .await
    .expect("orchestration loop should complete");

    let final_state = state.read().await;
    assert!(final_state.context_log.len() >= 3);
    assert!(final_state.current_tool.is_none());
}

#[tokio::test]
async fn builtin_supervisor_stops_after_worker_response_without_done_marker() {
    let work_dir = temp_workspace();
    let config = Arc::new(Config {
        version: 1,
        supervisor: Some(builtin_supervisor_tool(&work_dir)),
        tools: vec![worker_tool_with_output(&work_dir, "worker answered")],
    });
    let state = Arc::new(RwLock::new(AppState::new()));

    run_orchestration_loop(
        config,
        state.clone(),
        None,
        Some("initial request".to_owned()),
        Duration::from_secs(30),
        Some(Duration::from_secs(30)),
        3,
    )
    .await
    .expect("builtin supervisor should complete after a worker response");

    let final_state = state.read().await;
    let worker_messages: Vec<_> = final_state
        .context_log
        .iter()
        .filter(|message| message.tool_id == "test-tool")
        .collect();

    assert_eq!(worker_messages.len(), 1);
    assert!(worker_messages[0].raw_output.contains("worker answered"));
    assert!(final_state.current_tool.is_none());
}

#[tokio::test]
async fn orchestration_loop_runs_multiple_worker_actions_from_supervisor() {
    let work_dir = temp_workspace();
    let config = Arc::new(Config {
        version: 1,
        supervisor: Some(actions_supervisor_tool(&work_dir)),
        tools: vec![
            worker_tool_with_id_and_output(&work_dir, "tool-one", "worker one"),
            worker_tool_with_id_and_output(&work_dir, "tool-two", "worker two"),
        ],
    });
    let state = Arc::new(RwLock::new(AppState::new()));

    run_orchestration_loop(
        config,
        state.clone(),
        None,
        Some("split this request".to_owned()),
        Duration::from_secs(30),
        Some(Duration::from_secs(30)),
        3,
    )
    .await
    .expect("orchestration loop should run multiple worker actions");

    let final_state = state.read().await;
    assert!(final_state
        .context_log
        .iter()
        .any(|message| message.tool_id == "tool-one" && message.raw_output.contains("worker one")));
    assert!(final_state
        .context_log
        .iter()
        .any(|message| message.tool_id == "tool-two" && message.raw_output.contains("worker two")));
    assert!(final_state.active_tools.is_empty());
    assert!(final_state.current_tool.is_none());
}

#[test]
fn supervisor_decision_parser_prefers_agent_message_over_context_dump() {
    let work_dir = temp_workspace();
    let supervisor = stdout_fallback_supervisor_tool(&work_dir);
    let context_dump = json!({
        "type": "item.completed",
        "item": {
            "type": "command_execution",
            "aggregated_output": json!({
                "worker_tools": [
                    {
                        "id": "antigravity-cli",
                        "command": "agy",
                        "args": ["-p"]
                    }
                ]
            }).to_string()
        }
    });
    let decision = json!({
        "is_done": false,
        "summary": "route work",
        "actions": [
            {
                "task_id": "project-structure",
                "tool_id": "codex-cli",
                "args": [],
                "prompt": "inspect project"
            }
        ]
    });
    let agent_message = json!({
        "type": "item.completed",
        "item": {
            "type": "agent_message",
            "text": decision.to_string()
        }
    });
    let stdout_text = format!("{context_dump}\n{agent_message}");

    let parsed = parse_supervisor_decision(
        &supervisor,
        Some(0),
        &stdout_text,
        "",
        &work_dir.join(".harness_context.json"),
    )
    .expect("supervisor decision should parse from agent message");

    assert_eq!(parsed.actions.len(), 1);
    assert_eq!(parsed.actions[0].tool_id, "codex-cli");
    assert_ne!(parsed.actions[0].tool_id, "-p");
}

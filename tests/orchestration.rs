use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use rust_multi_cli_harness::{
    run_orchestration_loop, write_harness_context_file, AppState, Config, SupervisorDefinition,
    ToolDefinition, ToolParser,
};
use tokio::sync::RwLock;
use uuid::Uuid;

fn temp_workspace() -> PathBuf {
    let path = std::env::temp_dir().join(format!("harness-test-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&path).expect("temp workspace should be creatable");
    path
}

fn worker_tool(work_dir: &PathBuf) -> ToolDefinition {
    ToolDefinition {
        id: "test-tool".to_owned(),
        name: "Test Tool".to_owned(),
        command: "pwsh".to_owned(),
        args: vec![
            "-NoProfile".to_owned(),
            "-Command".to_owned(),
            "$null = $args; Write-Output 'worker completed'".to_owned(),
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
        Duration::from_secs(30),
        3,
    )
    .await
    .expect("orchestration loop should complete");

    let final_state = state.read().await;
    assert!(final_state.context_log.len() >= 3);
    assert!(final_state.current_tool.is_none());
}

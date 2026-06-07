use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use chrono::{Local, Utc};
use rust_multi_cli_harness::{
    config::Config,
    message::{HarnessMessage, MessageLevel},
    orchestration::run_orchestration_loop,
    state::{AppState, SharedAppState, SystemStatus},
    tui::run_tui,
    web::start_web_server,
};
use serde_json::json;
use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::{broadcast, oneshot, RwLock};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config_path = Path::new("tools.toml");

    let config = match Config::from_file(config_path) {
        Ok(c) => Arc::new(c),
        Err(err) => {
            eprintln!("Failed to load {}: {}", config_path.display(), err);
            std::process::exit(1);
        }
    };

    let state: SharedAppState = Arc::new(RwLock::new(AppState::new()));
    let (tx, _) = broadcast::channel(1024);
    let log_path = run_log_path();

    let logger_rx = tx.subscribe();
    let logger_path = log_path.clone();
    tokio::spawn(async move {
        if let Err(err) = run_file_logger(logger_path, logger_rx).await {
            eprintln!("Failed to write harness run log: {err}");
        }
    });

    let startup_message = HarnessMessage::system(
        "startup",
        "main",
        MessageLevel::Info,
        format!("Writing harness run log to {}", log_path.display()),
        json!({ "log_path": log_path.display().to_string() }),
    );
    {
        let mut state = state.write().await;
        state.push_context(startup_message.clone());
    }
    let _ = tx.send(startup_message);

    // 1. Web 서버 백그라운드 구동
    let web_state = state.clone();
    let web_tx = tx.clone();
    tokio::spawn(async move {
        start_web_server(web_state, web_tx).await;
    });

    let (prompt_tx, prompt_rx) = oneshot::channel();
    let tui_state = state.clone();
    let tui_rx = tx.subscribe();
    let mut tui_task =
        tokio::spawn(async move { run_tui(tui_state, tui_rx, Some(prompt_tx)).await });

    let initial_prompt = tokio::select! {
        prompt = prompt_rx => match prompt {
            Ok(prompt) => prompt,
            Err(_) => {
                wait_for_tui(tui_task).await?;
                return Ok(());
            }
        },
        result = &mut tui_task => {
            handle_tui_result(result)?;
            return Ok(());
        }
    };

    let prompt_message = HarnessMessage::system(
        "startup",
        "main",
        MessageLevel::Info,
        "Initial prompt received",
        json!({ "prompt": initial_prompt.clone() }),
    );
    {
        let mut state = state.write().await;
        state.push_context(prompt_message.clone());
    }
    let _ = tx.send(prompt_message);

    // 2. 오케스트레이션 코어 로직 백그라운드 구동
    let bg_config = config.clone();
    let bg_state = state.clone();
    let bg_tx = tx.clone();
    let bg_initial_prompt = initial_prompt.clone();
    tokio::spawn(async move {
        if let Err(e) = run_orchestration_loop(
            bg_config,
            bg_state.clone(),
            Some(bg_tx.clone()),
            Some(bg_initial_prompt),
            Duration::from_secs(60),  // Supervisor timeout
            Duration::from_secs(120), // Worker timeout
            50,                       // max iterations
        )
        .await
        {
            let error_text = format!("Orchestration failed: {e}");
            let message = HarnessMessage::system(
                "orchestration",
                "main",
                MessageLevel::Error,
                error_text.clone(),
                json!({ "error": error_text }),
            );

            {
                let mut state = bg_state.write().await;
                state.push_context(message.clone());
                state.status = SystemStatus::Halted;
            }

            let _ = bg_tx.send(message);
            eprintln!("{error_text}");
        }
    });

    // 3. TUI (블로킹/메인 루프) 실행
    wait_for_tui(tui_task).await?;

    Ok(())
}

async fn wait_for_tui(
    tui_task: tokio::task::JoinHandle<std::io::Result<()>>,
) -> Result<(), Box<dyn std::error::Error>> {
    handle_tui_result(tui_task.await)
}

fn handle_tui_result(
    result: Result<std::io::Result<()>, tokio::task::JoinError>,
) -> Result<(), Box<dyn std::error::Error>> {
    match result {
        Ok(inner) => inner.map_err(|err| Box::new(err) as Box<dyn std::error::Error>),
        Err(err) => Err(Box::new(err)),
    }
}

fn run_log_path() -> PathBuf {
    let timestamp = Local::now().format("%Y%m%d-%H%M%S").to_string();
    Path::new("logs").join(format!("harness-{timestamp}.jsonl"))
}

async fn run_file_logger(
    path: PathBuf,
    mut rx: broadcast::Receiver<HarnessMessage>,
) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .await?;

    let run_start = json!({
        "type": "run_start",
        "timestamp": Utc::now(),
        "log_path": path.display().to_string()
    });
    write_json_line(&mut file, &run_start).await?;

    loop {
        match rx.recv().await {
            Ok(message) => {
                let entry = json!({
                    "type": "harness_message",
                    "data": message
                });
                write_json_line(&mut file, &entry).await?;
            }
            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                let entry = json!({
                    "type": "logger_lagged",
                    "timestamp": Utc::now(),
                    "skipped_messages": skipped
                });
                write_json_line(&mut file, &entry).await?;
            }
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }

    Ok(())
}

async fn write_json_line(
    file: &mut tokio::fs::File,
    value: &serde_json::Value,
) -> std::io::Result<()> {
    let mut line = serde_json::to_vec(value)?;
    line.push(b'\n');
    file.write_all(&line).await?;
    file.flush().await
}

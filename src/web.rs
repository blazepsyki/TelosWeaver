use std::env;

use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::get,
    Json, Router,
};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::fs;
use tokio::sync::broadcast;

use crate::message::{HarnessMessage, MessageLevel, MessageSource};
use crate::orchestration::CONTEXT_FILE_NAME;
use crate::state::{AppState, SharedAppState, SystemStatus};

#[derive(Clone)]
pub struct AppStateWrapper {
    pub state: SharedAppState,
    pub log_tx: broadcast::Sender<HarnessMessage>,
}

#[derive(Debug, Deserialize)]
pub struct WebControlCommand {
    pub action: String, // "proceed", "modify", "abort"
    pub feedback: Option<String>,
}

#[derive(Debug, Serialize)]
struct WebLogMessage {
    id: String,
    tool_id: String,
    source: MessageSource,
    level: MessageLevel,
    step_id: String,
    correlation_id: String,
    timestamp: String,
    text: String,
    truncated: bool,
}

pub async fn start_web_server(state: SharedAppState, log_tx: broadcast::Sender<HarnessMessage>) {
    let port = env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let addr = format!("0.0.0.0:{}", port);

    let app_state = AppStateWrapper { state, log_tx };

    let app = Router::new()
        .route("/", get(index_handler))
        .route("/api/state", get(get_state))
        .route("/ws", get(ws_handler))
        .fallback(|| async { (StatusCode::NOT_FOUND, "Not Found") })
        .with_state(app_state.clone());

    let message = HarnessMessage::system(
        "web",
        "web-server",
        MessageLevel::Info,
        format!("Starting web dashboard on http://{}", addr),
        json!({ "url": format!("http://{}", addr) }),
    );
    {
        let mut state = app_state.state.write().await;
        state.push_context(message.clone());
    }
    let _ = app_state.log_tx.send(message);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn index_handler() -> Html<&'static str> {
    Html(include_str!("index.html"))
}

async fn get_state(State(app): State<AppStateWrapper>) -> Json<AppState> {
    let state = app.state.read().await;
    Json(state.clone())
}

async fn ws_handler(ws: WebSocketUpgrade, State(app): State<AppStateWrapper>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, app))
}

async fn handle_socket(socket: WebSocket, app: AppStateWrapper) {
    let mut rx = app.log_tx.subscribe();
    let state_arc = app.state.clone();

    // Spawn a task to send logs to the client
    let (mut sender, mut receiver) = socket.split();

    // Initial state dump on connect
    {
        let st = state_arc.read().await;
        if let Ok(json) = serde_json::to_string(&*st) {
            let _ = sender
                .send(Message::Text(
                    format!("{{\"type\": \"state_init\", \"data\": {}}}", json).into(),
                ))
                .await;
        }
        let history = st
            .context_log
            .iter()
            .filter_map(web_log_message)
            .collect::<Vec<_>>();
        let payload = json!({
            "type": "log_history",
            "data": history,
        });
        let _ = sender.send(Message::Text(payload.to_string().into())).await;
    }

    let mut send_task = tokio::spawn(async move {
        // Send state updates on loop (basic polling for status, or broadcast everything)
        // For simplicity, we just relay HarnessMessages when they arrive. State updates could be manual ticks.
        while let Ok(msg) = rx.recv().await {
            let Some(log) = web_log_message(&msg) else {
                continue;
            };
            let payload = json!({
                "type": "log",
                "data": log,
            });
            if sender
                .send(Message::Text(payload.to_string().into()))
                .await
                .is_err()
            {
                break;
            }
        }
    });

    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(Message::Text(text))) = receiver.next().await {
            if let Ok(cmd) = serde_json::from_str::<WebControlCommand>(&text) {
                let mut st = state_arc.write().await;
                if st.status == SystemStatus::PausedForIntervention {
                    match cmd.action.as_str() {
                        "proceed" => {
                            st.intervention_required = None;
                            st.status = SystemStatus::Running;
                        }
                        "modify" => {
                            if let Some(feedback) = cmd.feedback {
                                let path = std::path::Path::new(CONTEXT_FILE_NAME);
                                if let Ok(content) = fs::read_to_string(path).await {
                                    if let Ok(mut json_val) =
                                        serde_json::from_str::<serde_json::Value>(&content)
                                    {
                                        json_val["human_intervention_feedback"] =
                                            serde_json::Value::String(feedback);
                                        let _ = fs::write(
                                            path,
                                            serde_json::to_string_pretty(&json_val).unwrap(),
                                        )
                                        .await;
                                    }
                                }
                                st.intervention_required = None;
                                st.status = SystemStatus::Running;
                            }
                        }
                        "abort" => {
                            st.status = SystemStatus::Halted;
                        }
                        _ => {}
                    }

                    // Alert TUI that input was eaten (resolving collision control implicitly by checking state in TUI)
                }
            }
        }
    });

    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    };
}

fn web_log_message(message: &HarnessMessage) -> Option<WebLogMessage> {
    let text = display_output(message)?;
    if text.trim().is_empty() {
        return None;
    }

    Some(WebLogMessage {
        id: message.id.to_string(),
        tool_id: message.tool_id.clone(),
        source: message.source.clone(),
        level: message.level.clone(),
        step_id: message.step_id.clone(),
        correlation_id: message.correlation_id.clone(),
        timestamp: message.timestamp.to_rfc3339(),
        text,
        truncated: message.truncated,
    })
}

fn display_output(message: &HarnessMessage) -> Option<String> {
    if message.source == MessageSource::System {
        return Some(message.raw_output.clone());
    }

    if let Some(text) = extract_response_from_text(&message.raw_output) {
        return Some(text);
    }

    if let Some(text) = extract_response_from_value(&message.normalized_payload) {
        return Some(text);
    }

    if looks_like_json(&message.raw_output) {
        return None;
    }

    Some(message.raw_output.clone())
}

fn looks_like_json(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with('{') || trimmed.starts_with('[')
}

fn extract_response_from_text(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return extract_response_from_value(&value);
    }

    let parts: Vec<String> = trimmed
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
        .filter_map(|value| extract_response_from_value(&value))
        .filter(|part| !part.trim().is_empty())
        .collect();

    (!parts.is_empty()).then(|| parts.join("\n"))
}

fn extract_response_from_value(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => non_empty(text),
        Value::Array(items) => {
            let parts: Vec<String> = items
                .iter()
                .filter_map(extract_response_from_value)
                .filter(|part| !part.trim().is_empty())
                .collect();
            (!parts.is_empty()).then(|| parts.join("\n"))
        }
        Value::Object(map) => {
            for key in [
                "response",
                "answer",
                "message",
                "text",
                "content",
                "output",
                "last_agent_message",
                "final_answer",
            ] {
                if let Some(text) = map.get(key).and_then(extract_response_from_value) {
                    return Some(text);
                }
            }

            for key in ["stdout_text", "stderr_text", "line"] {
                if let Some(text) = map.get(key).and_then(Value::as_str) {
                    if let Some(response) = extract_response_from_text(text) {
                        return Some(response);
                    }
                }
            }

            for key in ["msg", "data", "delta", "result", "event", "item", "items"] {
                if let Some(text) = map.get(key).and_then(extract_response_from_value) {
                    return Some(text);
                }
            }

            None
        }
        _ => None,
    }
}

fn non_empty(text: &str) -> Option<String> {
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

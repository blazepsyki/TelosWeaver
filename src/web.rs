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
use serde::Deserialize;
use tokio::fs;
use tokio::sync::broadcast;

use crate::message::HarnessMessage;
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

pub async fn start_web_server(state: SharedAppState, log_tx: broadcast::Sender<HarnessMessage>) {
    let port = env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let addr = format!("0.0.0.0:{}", port);

    let app_state = AppStateWrapper { state, log_tx };

    let app = Router::new()
        .route("/", get(index_handler))
        .route("/api/state", get(get_state))
        .route("/ws", get(ws_handler))
        .fallback(|| async { (StatusCode::NOT_FOUND, "Not Found") })
        .with_state(app_state);

    println!("Starting web dashboard on http://{}", addr);

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
    }

    let mut send_task = tokio::spawn(async move {
        // Send state updates on loop (basic polling for status, or broadcast everything)
        // For simplicity, we just relay HarnessMessages when they arrive. State updates could be manual ticks.
        while let Ok(msg) = rx.recv().await {
            if let Ok(json) = serde_json::to_string(&msg) {
                let payload = format!("{{\"type\": \"log\", \"data\": {}}}", json);
                if sender.send(Message::Text(payload.into())).await.is_err() {
                    break;
                }
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

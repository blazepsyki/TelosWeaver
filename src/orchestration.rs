use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use rhai::{Dynamic, Engine, Map, Scope, INT};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map as JsonMap, Value};
use tokio::fs;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::broadcast;
use tokio::time::timeout;
use uuid::Uuid;

use crate::config::{Config, SupervisorDefinition, ToolDefinition};
use crate::execution::{execute_tool, ExecutionError};
use crate::message::{HarnessMessage, MessageLevel, MessageSource};
use crate::state::{AppState, SharedAppState};

pub const CONTEXT_FILE_NAME: &str = ".harness_context.json";
const BUILTIN_DONE_SUPERVISOR: &str = "__harness_done_supervisor";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SupervisorDecision {
    #[serde(default)]
    pub next_tool: Option<String>,
    #[serde(default)]
    pub tool_args: Vec<String>,
    #[serde(default)]
    pub is_done: bool,
    #[serde(default)]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessContextSnapshot {
    pub generated_at: String,
    pub state: AppState,
    pub worker_tools: Vec<ToolDefinition>,
}

#[derive(Debug)]
pub enum OrchestrationError {
    MissingSupervisor,
    Io(std::io::Error),
    Spawn(std::io::Error),
    Timeout { command: String, duration: Duration },
    Tool(ExecutionError),
    Decision(String),
    Rhai(String),
    MissingPipe(&'static str),
    Join(String),
}

impl fmt::Display for OrchestrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingSupervisor => write!(f, "supervisor configuration is missing"),
            Self::Io(err) => write!(f, "io error: {err}"),
            Self::Spawn(err) => write!(f, "failed to spawn supervisor: {err}"),
            Self::Timeout { command, duration } => {
                write!(f, "command '{command}' timed out after {:?}", duration)
            }
            Self::Tool(err) => write!(f, "worker execution error: {err}"),
            Self::Decision(err) => write!(f, "invalid supervisor decision: {err}"),
            Self::Rhai(err) => write!(f, "rhai error: {err}"),
            Self::MissingPipe(stream) => write!(f, "missing {stream} pipe"),
            Self::Join(err) => write!(f, "task join error: {err}"),
        }
    }
}

impl std::error::Error for OrchestrationError {}

impl From<std::io::Error> for OrchestrationError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<ExecutionError> for OrchestrationError {
    fn from(value: ExecutionError) -> Self {
        Self::Tool(value)
    }
}

pub async fn write_harness_context_file(
    path: impl AsRef<Path>,
    state: &AppState,
    config: &Config,
) -> Result<PathBuf, OrchestrationError> {
    let snapshot = HarnessContextSnapshot {
        generated_at: Utc::now().to_rfc3339(),
        state: state.clone(),
        worker_tools: config.tools.clone(),
    };

    let serialized = serde_json::to_vec_pretty(&snapshot)
        .map_err(|err| OrchestrationError::Decision(err.to_string()))?;
    let path = path.as_ref().to_path_buf();
    fs::write(&path, serialized).await?;
    Ok(path)
}

pub async fn run_orchestration_loop(
    config: Arc<Config>,
    state: SharedAppState,
    broadcaster: Option<broadcast::Sender<HarnessMessage>>,
    initial_prompt: Option<String>,
    supervisor_timeout: Duration,
    worker_timeout: Duration,
    max_iterations: usize,
) -> Result<(), OrchestrationError> {
    let supervisor = config
        .supervisor()
        .ok_or(OrchestrationError::MissingSupervisor)?;
    let root_dir = supervisor
        .working_dir
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let context_path = root_dir.join(CONTEXT_FILE_NAME);
    let correlation_id = Uuid::new_v4().to_string();

    for iteration in 0..max_iterations {
        let snapshot = {
            let state_guard = state.read().await;
            state_guard.clone()
        };

        write_harness_context_file(&context_path, &snapshot, config.as_ref()).await?;
        broadcast_system_event(
            &state,
            broadcaster.as_ref(),
            format!("Supervisor cycle {iteration} starting"),
            &correlation_id,
        )
        .await;

        let supervisor_decision = run_supervisor_cli(
            supervisor,
            &context_path,
            supervisor_timeout,
            broadcaster.as_ref(),
            &state,
            &correlation_id,
        )
        .await?;

        if supervisor_decision.is_done {
            broadcast_system_event(
                &state,
                broadcaster.as_ref(),
                "Supervisor marked orchestration complete".to_owned(),
                &correlation_id,
            )
            .await;
            return Ok(());
        }

        let next_tool = supervisor_decision.next_tool.ok_or_else(|| {
            OrchestrationError::Decision("next_tool is required when is_done is false".to_owned())
        })?;

        let mut worker_config = (*config).clone();
        let worker = worker_config
            .tools
            .iter_mut()
            .find(|tool| tool.id == next_tool)
            .ok_or_else(|| {
                OrchestrationError::Decision(format!("worker '{next_tool}' not found"))
            })?;

        let next_tool_args = if let Some(prompt) = initial_prompt
            .as_ref()
            .filter(|prompt| !prompt.trim().is_empty())
        {
            vec![prompt.clone()]
        } else {
            supervisor_decision.tool_args.clone()
        };

        worker.args = {
            let mut args = worker.args.clone();
            args.extend(next_tool_args);
            args
        };

        broadcast_system_event(
            &state,
            broadcaster.as_ref(),
            format!("Running worker CLI: {next_tool}"),
            &correlation_id,
        )
        .await;

        execute_tool(
            Arc::new(worker_config),
            state.clone(),
            broadcaster.clone(),
            &next_tool,
            format!("orchestration-{iteration}"),
            correlation_id.clone(),
            worker_timeout,
        )
        .await?;
    }

    Err(OrchestrationError::Decision(format!(
        "orchestration did not finish within {max_iterations} iterations"
    )))
}

async fn run_supervisor_cli(
    supervisor: &SupervisorDefinition,
    context_path: &Path,
    timeout_duration: Duration,
    broadcaster: Option<&broadcast::Sender<HarnessMessage>>,
    state: &SharedAppState,
    correlation_id: &str,
) -> Result<SupervisorDecision, OrchestrationError> {
    broadcast_system_event(
        state,
        broadcaster,
        "Running supervisor CLI".to_owned(),
        correlation_id,
    )
    .await;

    {
        let mut state_guard = state.write().await;
        state_guard.begin_tool_run("supervisor", "supervisor-cycle", correlation_id.to_owned());
    }

    if supervisor.command == BUILTIN_DONE_SUPERVISOR {
        return run_builtin_done_supervisor(
            supervisor,
            context_path,
            broadcaster,
            state,
            correlation_id,
        )
        .await;
    }

    let mut command = Command::new(&supervisor.command);
    command.stdin(Stdio::null()); // 입력 대기를 방지하여 프로세스 행(Hang) 해결
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    command.env("HARNESS_CONTEXT_FILE", context_path);
    if !supervisor.args.is_empty() {
        command.args(&supervisor.args);
    }
    if let Some(working_dir) = &supervisor.working_dir {
        command.current_dir(PathBuf::from(working_dir));
    }

    let mut child = command.spawn().map_err(OrchestrationError::Spawn)?;
    let stdout = child
        .stdout
        .take()
        .ok_or(OrchestrationError::MissingPipe("stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or(OrchestrationError::MissingPipe("stderr"))?;

    let stdout_task = tokio::spawn(read_text_stream(
        stdout,
        "supervisor".to_owned(),
        broadcaster.cloned(),
        correlation_id.to_owned(),
    ));
    let stderr_task = tokio::spawn(read_text_stream(
        stderr,
        "supervisor".to_owned(),
        broadcaster.cloned(),
        correlation_id.to_owned(),
    ));

    let exit_status = match timeout(timeout_duration, child.wait()).await {
        Ok(result) => result.map_err(OrchestrationError::Io)?,
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            {
                let mut state_guard = state.write().await;
                state_guard.record_step_error("supervisor-cycle", "supervisor timed out");
            }
            return Err(OrchestrationError::Timeout {
                command: supervisor.command.clone(),
                duration: timeout_duration,
            });
        }
    };

    let stdout_text = stdout_task
        .await
        .map_err(|err| OrchestrationError::Join(err.to_string()))??;
    let stderr_text = stderr_task
        .await
        .map_err(|err| OrchestrationError::Join(err.to_string()))??;

    let decision = parse_supervisor_decision(
        supervisor,
        exit_status.code().map(i64::from),
        &stdout_text,
        &stderr_text,
        context_path,
    )?;

    {
        let mut state_guard = state.write().await;
        state_guard.finish_tool_run();
    }

    Ok(decision)
}

async fn run_builtin_done_supervisor(
    supervisor: &SupervisorDefinition,
    context_path: &Path,
    broadcaster: Option<&broadcast::Sender<HarnessMessage>>,
    state: &SharedAppState,
    correlation_id: &str,
) -> Result<SupervisorDecision, OrchestrationError> {
    let context = fs::read_to_string(context_path).await?;
    let snapshot: HarnessContextSnapshot = serde_json::from_str(&context)
        .map_err(|err| OrchestrationError::Decision(err.to_string()))?;
    let stdout_text = if snapshot
        .state
        .context_log
        .iter()
        .any(|message| raw_output_marks_done(&message.raw_output))
    {
        "done"
    } else {
        "continue"
    };

    let message = HarnessMessage::new(
        "supervisor",
        MessageSource::System,
        MessageLevel::Info,
        "supervisor-cycle",
        correlation_id.to_owned(),
        stdout_text,
        json!({"stream": "builtin", "line": stdout_text}),
        false,
    );

    if let Some(sender) = broadcaster {
        let _ = sender.send(message);
    }

    let decision = parse_supervisor_decision(supervisor, Some(0), stdout_text, "", context_path)?;

    {
        let mut state_guard = state.write().await;
        state_guard.finish_tool_run();
    }

    Ok(decision)
}

fn raw_output_marks_done(raw_output: &str) -> bool {
    raw_output
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .any(|token| token.eq_ignore_ascii_case("done"))
}

pub fn parse_supervisor_decision(
    supervisor: &SupervisorDefinition,
    exit_code: Option<i64>,
    stdout_text: &str,
    stderr_text: &str,
    context_path: &Path,
) -> Result<SupervisorDecision, OrchestrationError> {
    let engine = Engine::new();
    let mut scope = Scope::new();
    scope.push("exit_code", exit_code.unwrap_or(-1));
    scope.push("stdout_text", stdout_text.to_owned());
    scope.push("stderr_text", stderr_text.to_owned());
    scope.push("context_file_path", context_path.display().to_string());

    let ast = engine
        .compile(&supervisor.parser.script)
        .map_err(|err| OrchestrationError::Rhai(err.to_string()))?;
    let result: Map = engine
        .eval_ast_with_scope(&mut scope, &ast)
        .map_err(|err| OrchestrationError::Rhai(err.to_string()))?;

    let payload = rhai_map_to_json(result);
    let mut decision: SupervisorDecision = serde_json::from_value(payload)
        .map_err(|err| OrchestrationError::Decision(err.to_string()))?;
    if decision.next_tool.as_deref() == Some("") {
        decision.next_tool = None;
    }
    Ok(decision)
}

async fn read_text_stream<R>(
    reader: R,
    stream_name: String,
    broadcaster: Option<broadcast::Sender<HarnessMessage>>,
    correlation_id: String,
) -> Result<String, std::io::Error>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut lines = BufReader::new(reader).lines();
    let mut collected = String::new();
    let mut first_line = true;

    while let Some(line) = lines.next_line().await? {
        if !first_line {
            collected.push('\n');
        }
        first_line = false;
        collected.push_str(&line);

        let message = HarnessMessage::new(
            stream_name.clone(),
            MessageSource::System,
            MessageLevel::Info,
            "supervisor-cycle",
            correlation_id.clone(),
            line.clone(),
            json!({"stream": stream_name, "line": line}),
            false,
        );

        if let Some(sender) = &broadcaster {
            let _ = sender.send(message);
        }
    }

    Ok(collected)
}

async fn broadcast_system_event(
    state: &SharedAppState,
    broadcaster: Option<&broadcast::Sender<HarnessMessage>>,
    text: String,
    correlation_id: &str,
) {
    let message = HarnessMessage::new(
        "system",
        MessageSource::System,
        MessageLevel::Info,
        "orchestration",
        correlation_id.to_owned(),
        text.clone(),
        json!({"text": text}),
        false,
    );

    {
        let mut state_guard = state.write().await;
        state_guard.push_context(message.clone());
    }

    if let Some(sender) = broadcaster {
        let _ = sender.send(message);
    }
}

fn rhai_map_to_json(map: Map) -> Value {
    let mut json_map = JsonMap::new();
    for (key, value) in map {
        json_map.insert(key.into(), dynamic_to_json(&value));
    }
    Value::Object(json_map)
}

fn dynamic_to_json(value: &Dynamic) -> Value {
    if value.is::<()>() {
        Value::Null
    } else if let Some(integer) = value.clone().try_cast::<INT>() {
        json!(integer)
    } else if let Some(float) = value.clone().try_cast::<rhai::FLOAT>() {
        json!(float)
    } else if let Some(boolean) = value.clone().try_cast::<bool>() {
        json!(boolean)
    } else if let Some(string) = value.clone().try_cast::<String>() {
        json!(string)
    } else if let Some(array) = value.clone().try_cast::<rhai::Array>() {
        Value::Array(
            array
                .into_iter()
                .map(|item| dynamic_to_json(&item))
                .collect(),
        )
    } else if let Some(map) = value.clone().try_cast::<Map>() {
        let mut json_map = JsonMap::new();
        for (key, item) in map {
            json_map.insert(key.into(), dynamic_to_json(&item));
        }
        Value::Object(json_map)
    } else {
        Value::String(value.to_string())
    }
}

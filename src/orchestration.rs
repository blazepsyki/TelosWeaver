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
use tokio::task::JoinSet;
use uuid::Uuid;

use crate::config::{Config, SupervisorDefinition, ToolDefinition};
use crate::execution::{execute_tool, ExecutionError};
use crate::message::{HarnessMessage, MessageLevel, MessageSource};
use crate::state::{AppState, SharedAppState};

pub const CONTEXT_FILE_NAME: &str = ".harness_context.json";
const BUILTIN_DONE_SUPERVISOR: &str = "__harness_done_supervisor";
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SupervisorDecision {
    #[serde(default)]
    pub next_tool: Option<String>,
    #[serde(default)]
    pub tool_args: Vec<String>,
    #[serde(default)]
    pub actions: Vec<WorkerInvocation>,
    #[serde(default)]
    pub is_done: bool,
    #[serde(default)]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct WorkerInvocation {
    #[serde(default)]
    pub task_id: String,
    #[serde(default)]
    pub tool_id: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessContextSnapshot {
    pub generated_at: String,
    pub state: AppState,
    pub worker_tools: Vec<ToolDefinition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub human_intervention_feedback: Option<String>,
}

#[derive(Debug)]
pub enum OrchestrationError {
    MissingSupervisor,
    Halted,
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
            Self::Halted => write!(f, "orchestration halted by user"),
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
        human_intervention_feedback: state.human_intervention_feedback.clone(),
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
    worker_timeout: Option<Duration>,
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
    let mut pending_initial_prompt = initial_prompt.filter(|prompt| !prompt.trim().is_empty());

    for iteration in 0..max_iterations {
        loop {
            let status = {
                let state_guard = state.read().await;
                state_guard.status.clone()
            };

            if status == crate::state::SystemStatus::Halted {
                broadcast_system_event(
                    &state,
                    broadcaster.as_ref(),
                    "Orchestration halted by user".to_owned(),
                    &correlation_id,
                )
                .await;
                return Ok(());
            }

            if status != crate::state::SystemStatus::PausedForIntervention {
                break;
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }

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

        let supervisor_decision = match run_supervisor_cli(
            supervisor,
            &context_path,
            supervisor_timeout,
            broadcaster.as_ref(),
            &state,
            &correlation_id,
        )
        .await
        {
            Ok(decision) => decision,
            Err(OrchestrationError::Halted) => {
                broadcast_system_event(
                    &state,
                    broadcaster.as_ref(),
                    "Orchestration halted by user".to_owned(),
                    &correlation_id,
                )
                .await;
                return Ok(());
            }
            Err(err) => return Err(err),
        };

        {
            let mut state_guard = state.write().await;
            state_guard.human_intervention_feedback = None;
        }

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

        let used_legacy_action = supervisor_decision.actions.is_empty();
        let mut actions = decision_worker_actions(supervisor_decision, iteration, config.as_ref())?;
        if used_legacy_action {
            if let Some(prompt) = pending_initial_prompt.take() {
                if let Some(action) = actions.first_mut() {
                    action.args = vec![prompt];
                    action.prompt = None;
                }
            }
        } else {
            let _ = pending_initial_prompt.take();
        }

        run_worker_actions(
            config.clone(),
            state.clone(),
            broadcaster.clone(),
            actions,
            correlation_id.clone(),
            worker_timeout,
        )
        .await?;
    }

    Err(OrchestrationError::Decision(format!(
        "orchestration did not finish within {max_iterations} iterations"
    )))
}

fn decision_worker_actions(
    decision: SupervisorDecision,
    iteration: usize,
    config: &Config,
) -> Result<Vec<WorkerInvocation>, OrchestrationError> {
    let mut actions = if decision.actions.is_empty() {
        let next_tool = decision.next_tool.ok_or_else(|| {
            OrchestrationError::Decision(
                "actions or next_tool is required when is_done is false".to_owned(),
            )
        })?;
        vec![WorkerInvocation {
            task_id: format!("orchestration-{iteration}-worker-0"),
            tool_id: next_tool,
            args: decision.tool_args,
            prompt: None,
        }]
    } else {
        decision.actions
    };

    if actions.is_empty() {
        return Err(OrchestrationError::Decision(
            "supervisor returned no worker actions".to_owned(),
        ));
    }

    for (index, action) in actions.iter_mut().enumerate() {
        if action.tool_id.trim().is_empty() {
            return Err(OrchestrationError::Decision(format!(
                "worker action {index} is missing tool_id"
            )));
        }

        if !config.tools.iter().any(|tool| tool.id == action.tool_id) {
            let known_tools = config
                .tools
                .iter()
                .map(|tool| tool.id.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(OrchestrationError::Decision(format!(
                "worker action {index} references unknown tool_id '{}' (known tools: {known_tools})",
                action.tool_id
            )));
        }

        if action.task_id.trim().is_empty() {
            action.task_id = format!("orchestration-{iteration}-worker-{index}");
        }
    }

    Ok(actions)
}

async fn run_worker_actions(
    config: Arc<Config>,
    state: SharedAppState,
    broadcaster: Option<broadcast::Sender<HarnessMessage>>,
    actions: Vec<WorkerInvocation>,
    correlation_id: String,
    worker_timeout: Option<Duration>,
) -> Result<(), OrchestrationError> {
    let mut tasks = JoinSet::new();

    for action in actions {
        let config = config.clone();
        let state = state.clone();
        let broadcaster = broadcaster.clone();
        let correlation_id = correlation_id.clone();

        tasks.spawn(async move {
            run_worker_action(
                config,
                state,
                broadcaster,
                action,
                correlation_id,
                worker_timeout,
            )
            .await
        });
    }

    loop {
        if tasks.is_empty() {
            break;
        }

        tokio::select! {
            result = tasks.join_next() => {
                if let Some(res) = result {
                    res.map_err(|err| OrchestrationError::Join(err.to_string()))??;
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(200)) => {
                let status = {
                    let state_guard = state.read().await;
                    state_guard.status.clone()
                };
                if status == crate::state::SystemStatus::Halted {
                    tasks.abort_all();
                    return Ok(());
                }
            }
        }
    }

    Ok(())
}

async fn run_worker_action(
    config: Arc<Config>,
    state: SharedAppState,
    broadcaster: Option<broadcast::Sender<HarnessMessage>>,
    action: WorkerInvocation,
    correlation_id: String,
    worker_timeout: Option<Duration>,
) -> Result<(), OrchestrationError> {
    let mut worker_config = (*config).clone();
    let worker = worker_config
        .tools
        .iter_mut()
        .find(|tool| tool.id == action.tool_id)
        .ok_or_else(|| {
            OrchestrationError::Decision(format!("worker '{}' not found", action.tool_id))
        })?;

    worker.args = {
        let mut args = worker.args.clone();
        args.extend(action.args.clone());
        if let Some(prompt) = action.prompt.filter(|prompt| !prompt.trim().is_empty()) {
            args.push(prompt);
        }
        args
    };

    broadcast_system_event(
        &state,
        broadcaster.as_ref(),
        format!(
            "Running worker CLI: {} ({})",
            action.tool_id, action.task_id
        ),
        &correlation_id,
    )
    .await;

    let exec_result = execute_tool(
        Arc::new(worker_config),
        state.clone(),
        broadcaster.clone(),
        &action.tool_id,
        action.task_id.clone(),
        correlation_id.clone(),
        worker_timeout,
    )
    .await;

    if let Err(err) = exec_result {
        let error_msg = format!("Worker system error: {err}");
        let message = HarnessMessage::system(
            &action.task_id,
            &correlation_id,
            MessageLevel::Error,
            error_msg.clone(),
            json!({ "error": error_msg }),
        );
        {
            let mut state_guard = state.write().await;
            state_guard.finish_tool_run_for_step(&action.task_id);
            state_guard.push_context(message.clone());
            state_guard.record_step_error(&action.task_id, error_msg);
        }
        if let Some(sender) = broadcaster {
            let _ = sender.send(message);
        }
    }

    Ok(())
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
    command.kill_on_drop(true);
    suppress_child_console(&mut command);
    command.env("HARNESS_CONTEXT_FILE", context_path);
    if !supervisor.args.is_empty() {
        command.args(&supervisor.args);
    }
    if let Some(working_dir) = &supervisor.working_dir {
        command.current_dir(PathBuf::from(working_dir));
    }

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => {
            let mut state_guard = state.write().await;
            state_guard.finish_tool_run_for_step("supervisor-cycle");
            return Err(OrchestrationError::Spawn(err));
        }
    };
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

    let timeout_sleep = tokio::time::sleep(timeout_duration);
    tokio::pin!(timeout_sleep);
    let exit_status = loop {
        tokio::select! {
            result = child.wait() => {
                break result.map_err(OrchestrationError::Io)?;
            }
            _ = &mut timeout_sleep => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                {
                    let mut state_guard = state.write().await;
                    state_guard.finish_tool_run_for_step("supervisor-cycle");
                    state_guard.record_step_error("supervisor-cycle", "supervisor timed out");
                }
                return Err(OrchestrationError::Timeout {
                    command: supervisor.command.clone(),
                    duration: timeout_duration,
                });
            }
            _ = tokio::time::sleep(Duration::from_millis(200)) => {
                let status = {
                    let state_guard = state.read().await;
                    state_guard.status.clone()
                };
                if status == crate::state::SystemStatus::Halted {
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                    let mut state_guard = state.write().await;
                    state_guard.finish_tool_run_for_step("supervisor-cycle");
                    return Err(OrchestrationError::Halted);
                }
            }
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

fn suppress_child_console(command: &mut Command) {
    #[cfg(windows)]
    {
        command.creation_flags(CREATE_NO_WINDOW);
    }

    #[cfg(not(windows))]
    {
        let _ = command;
    }
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
    let has_done_marker = snapshot
        .state
        .context_log
        .iter()
        .filter(|message| message.correlation_id == correlation_id)
        .any(|message| raw_output_marks_done(&message.raw_output));
    let has_worker_response = snapshot
        .state
        .context_log
        .iter()
        .filter(|message| message.correlation_id == correlation_id)
        .any(|message| message_is_worker_response(message, &snapshot.worker_tools));
    let stdout_text = if has_done_marker || has_worker_response {
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

fn message_is_worker_response(message: &HarnessMessage, worker_tools: &[ToolDefinition]) -> bool {
    !message.raw_output.trim().is_empty()
        && worker_tools.iter().any(|tool| tool.id == message.tool_id)
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
    if !decision_has_work_or_done(&decision) {
        if let Some(extracted) = parse_decision_from_text(stdout_text) {
            decision = extracted;
        }
    }
    if decision.next_tool.as_deref() == Some("") {
        decision.next_tool = None;
    }
    Ok(decision)
}

fn decision_has_work_or_done(decision: &SupervisorDecision) -> bool {
    decision.is_done || decision.next_tool.is_some() || !decision.actions.is_empty()
}

fn parse_decision_from_text(text: &str) -> Option<SupervisorDecision> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    let json_lines = trimmed
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
        .collect::<Vec<_>>();

    if let Some(decision) = json_lines
        .iter()
        .filter_map(extract_agent_message_decision)
        .last()
    {
        return Some(decision);
    }

    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        if let Some(decision) = direct_decision_from_value(&value) {
            return Some(decision);
        }
    }

    if let Some(decision) = json_lines
        .iter()
        .filter_map(direct_decision_from_value)
        .last()
    {
        return Some(decision);
    }

    if let Some(decision) = json_lines
        .iter()
        .filter_map(|value| find_decision_in_value(value, 0))
        .last()
    {
        return Some(decision);
    }

    extract_json_object_candidates(trimmed)
        .into_iter()
        .filter_map(|candidate| serde_json::from_str::<Value>(&candidate).ok())
        .filter_map(|value| direct_decision_from_value(&value))
        .last()
}

fn extract_agent_message_decision(value: &Value) -> Option<SupervisorDecision> {
    let item = value.get("item")?;
    let item_type = item.get("type").and_then(Value::as_str)?;
    if item_type != "agent_message" {
        return None;
    }

    let text = item.get("text").and_then(Value::as_str)?;
    parse_decision_text_payload(text)
}

fn parse_decision_text_payload(text: &str) -> Option<SupervisorDecision> {
    let trimmed = text.trim();

    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        if let Some(decision) = direct_decision_from_value(&value) {
            return Some(decision);
        }
    }

    extract_json_object_candidates(trimmed)
        .into_iter()
        .filter_map(|candidate| serde_json::from_str::<Value>(&candidate).ok())
        .filter_map(|value| direct_decision_from_value(&value))
        .last()
}

fn direct_decision_from_value(value: &Value) -> Option<SupervisorDecision> {
    let object = value.as_object()?;
    if !object.contains_key("is_done")
        && !object.contains_key("actions")
        && !object.contains_key("next_tool")
    {
        return None;
    }

    serde_json::from_value::<SupervisorDecision>(value.clone())
        .ok()
        .filter(decision_has_work_or_done)
}

fn find_decision_in_value(value: &Value, depth: usize) -> Option<SupervisorDecision> {
    if depth > 8 {
        return None;
    }

    if let Some(decision) = direct_decision_from_value(value) {
        return Some(decision);
    }

    match value {
        Value::Array(items) => items
            .iter()
            .find_map(|item| find_decision_in_value(item, depth + 1)),
        Value::Object(map) => {
            if map
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|value| value == "command_execution")
            {
                return None;
            }

            for key in ["text", "message", "content", "response", "output"] {
                if let Some(item) = map.get(key) {
                    if let Some(decision) = find_decision_in_value(item, depth + 1) {
                        return Some(decision);
                    }
                }
            }

            None
        }
        Value::String(text) => {
            if let Ok(value) = serde_json::from_str::<Value>(text.trim()) {
                return find_decision_in_value(&value, depth + 1);
            }

            extract_json_object_candidates(text)
                .into_iter()
                .filter_map(|candidate| serde_json::from_str::<Value>(&candidate).ok())
                .find_map(|value| find_decision_in_value(&value, depth + 1))
        }
        _ => None,
    }
}

fn extract_json_object_candidates(text: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let mut start = None;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    for (index, character) in text.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }

        if character == '\\' && in_string {
            escaped = true;
            continue;
        }

        if character == '"' {
            in_string = !in_string;
            continue;
        }

        if in_string {
            continue;
        }

        match character {
            '{' => {
                if depth == 0 {
                    start = Some(index);
                }
                depth += 1;
            }
            '}' => {
                if depth == 0 {
                    continue;
                }
                depth -= 1;
                if depth == 0 {
                    if let Some(start_index) = start.take() {
                        candidates.push(text[start_index..=index].to_owned());
                    }
                }
            }
            _ => {}
        }
    }

    candidates
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
    let mut reader = BufReader::new(reader);
    let mut collected = String::new();
    let mut first_line = true;
    let mut line_bytes = Vec::new();

    loop {
        line_bytes.clear();
        let bytes_read = reader.read_until(b'\n', &mut line_bytes).await?;
        if bytes_read == 0 {
            break;
        }
        while matches!(line_bytes.last(), Some(b'\n' | b'\r')) {
            line_bytes.pop();
        }
        let line = String::from_utf8_lossy(&line_bytes).into_owned();

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

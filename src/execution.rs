#[cfg(windows)]
use std::env;
use std::ffi::OsString;
use std::fmt;
#[cfg(windows)]
use std::fs;
#[cfg(windows)]
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use rhai::{Array, Dynamic, Engine, Map, Scope, INT};
use serde_json::{json, Map as JsonMap, Value};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::broadcast;
use tokio::time::timeout;

use crate::config::{Config, ToolDefinition};
use crate::message::{HarnessMessage, MessageLevel, MessageSource};
use crate::state::SharedAppState;

const MAX_STREAM_LINE_CHARS: usize = 2_000;
const MAX_CAPTURED_LINE_CHARS: usize = 4_000;
const MAX_CAPTURED_LINES: usize = 100;
const KEEP_CAPTURED_LINES: usize = 30;
const MAX_CAPTURED_CHARS: usize = 120_000;

#[derive(Debug)]
pub enum ExecutionError {
    ToolNotFound(String),
    Spawn {
        tool_id: String,
        command: String,
        resolved_program: String,
        args: Vec<String>,
        working_dir: Option<String>,
        source: std::io::Error,
    },
    Io(std::io::Error),
    Rhai(String),
    Timeout {
        tool_id: String,
        duration: Duration,
    },
    MissingPipe(&'static str),
    Join(String),
}

impl fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ToolNotFound(tool_id) => write!(f, "tool not found: {tool_id}"),
            Self::Spawn {
                tool_id,
                command,
                resolved_program,
                args,
                working_dir,
                source,
            } => write!(
                f,
                "failed to spawn tool '{tool_id}' (command: '{command}', resolved: '{resolved_program}', args: {:?}, working_dir: {:?}): {source}",
                args, working_dir
            ),
            Self::Io(err) => write!(f, "io error: {err}"),
            Self::Rhai(err) => write!(f, "rhai error: {err}"),
            Self::Timeout { tool_id, duration } => {
                write!(f, "tool '{tool_id}' timed out after {:?}", duration)
            }
            Self::MissingPipe(stream) => write!(f, "missing {stream} pipe"),
            Self::Join(err) => write!(f, "task join error: {err}"),
        }
    }
}

impl std::error::Error for ExecutionError {}

impl From<std::io::Error> for ExecutionError {
    fn from(value: std::io::Error) -> Self {
        ExecutionError::Io(value)
    }
}

pub struct ExecutionEngine {
    config: Arc<Config>,
    state: SharedAppState,
    broadcaster: Option<broadcast::Sender<HarnessMessage>>,
    timeout: Duration,
}

impl ExecutionEngine {
    pub fn new(
        config: Arc<Config>,
        state: SharedAppState,
        broadcaster: Option<broadcast::Sender<HarnessMessage>>,
        timeout: Duration,
    ) -> Self {
        Self {
            config,
            state,
            broadcaster,
            timeout,
        }
    }

    pub async fn execute_tool(
        &self,
        tool_id: &str,
        step_id: impl Into<String>,
        correlation_id: impl Into<String>,
    ) -> Result<HarnessMessage, ExecutionError> {
        let step_id = step_id.into();
        let correlation_id = correlation_id.into();
        let tool = self
            .config
            .tool(tool_id)
            .cloned()
            .ok_or_else(|| ExecutionError::ToolNotFound(tool_id.to_owned()))?;

        {
            let mut state = self.state.write().await;
            state.begin_tool_run(tool.id.clone(), step_id.clone(), correlation_id.clone());
        }

        let invocation = resolve_invocation(&tool.command, &tool.args);
        let resolved_program = invocation.program.to_string_lossy().into_owned();
        let resolved_args = invocation
            .args
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        let mut command = build_process_command(invocation);
        if let Some(working_dir) = &tool.working_dir {
            command.current_dir(PathBuf::from(working_dir));
        }

        let mut child = command.spawn().map_err(|source| ExecutionError::Spawn {
            tool_id: tool.id.clone(),
            command: tool.command.clone(),
            resolved_program,
            args: resolved_args,
            working_dir: tool.working_dir.clone(),
            source,
        })?;
        let stdout = child
            .stdout
            .take()
            .ok_or(ExecutionError::MissingPipe("stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or(ExecutionError::MissingPipe("stderr"))?;

        let stdout_task = tokio::spawn(read_stream(
            stdout,
            tool.id.clone(),
            step_id.clone(),
            correlation_id.clone(),
            MessageSource::Stdout,
            self.broadcaster.clone(),
        ));
        let stderr_task = tokio::spawn(read_stream(
            stderr,
            tool.id.clone(),
            step_id.clone(),
            correlation_id.clone(),
            MessageSource::Stderr,
            self.broadcaster.clone(),
        ));

        let exit_status = match timeout(self.timeout, child.wait()).await {
            Ok(result) => result.map_err(ExecutionError::Io)?,
            Err(_) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                {
                    let mut state = self.state.write().await;
                    state.record_step_error(step_id.clone(), "tool execution timed out");
                }
                return Err(ExecutionError::Timeout {
                    tool_id: tool.id.clone(),
                    duration: self.timeout,
                });
            }
        };

        let stdout_text = stdout_task
            .await
            .map_err(|err| ExecutionError::Join(err.to_string()))??;
        let stderr_text = stderr_task
            .await
            .map_err(|err| ExecutionError::Join(err.to_string()))??;
        let (stdout_text, stdout_truncated) = compact_captured_text(&stdout_text);
        let (stderr_text, stderr_truncated) = compact_captured_text(&stderr_text);
        let exit_code = exit_status.code().map(i64::from);

        let mut message = parse_with_rhai(
            &tool,
            exit_code,
            &stdout_text,
            &stderr_text,
            &step_id,
            &correlation_id,
        )?;
        if stdout_truncated || stderr_truncated {
            message = message.marked_truncated();
        }

        {
            let mut state = self.state.write().await;
            state.push_context(message.clone());
            if exit_status.success() {
                state.finish_tool_run();
            } else {
                state.record_step_error(
                    step_id.clone(),
                    format!("process exited with {:?}", exit_status.code()),
                );
            }
        }

        if let Some(sender) = &self.broadcaster {
            let _ = sender.send(message.clone());
        }

        Ok(message)
    }
}

fn build_process_command(invocation: ToolInvocation) -> Command {
    let mut command = Command::new(invocation.program);
    command.args(invocation.args);
    command.stdin(Stdio::null()); // 입력 대기를 방지하여 프로세스 행(Hang) 해결
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    command
}

struct ToolInvocation {
    program: OsString,
    args: Vec<OsString>,
}

fn resolve_invocation(command: &str, args: &[String]) -> ToolInvocation {
    #[cfg(windows)]
    {
        return resolve_windows_invocation(command, args);
    }

    #[cfg(not(windows))]
    {
        ToolInvocation {
            program: OsString::from(command),
            args: args.iter().map(OsString::from).collect(),
        }
    }
}

#[cfg(windows)]
fn resolve_windows_invocation(command: &str, args: &[String]) -> ToolInvocation {
    let program = resolve_windows_program(command).unwrap_or_else(|| PathBuf::from(command));
    let extension = program
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase());

    if matches!(extension.as_deref(), Some("cmd" | "bat")) {
        let mut shell_args = vec![
            OsString::from("/D"),
            OsString::from("/S"),
            OsString::from("/C"),
            program.into_os_string(),
        ];
        shell_args.extend(args.iter().map(OsString::from));

        return ToolInvocation {
            program: OsString::from("cmd.exe"),
            args: shell_args,
        };
    }

    if extension.as_deref() == Some("ps1") {
        let mut shell_args = vec![
            OsString::from("-NoProfile"),
            OsString::from("-ExecutionPolicy"),
            OsString::from("Bypass"),
            OsString::from("-File"),
            program.into_os_string(),
        ];
        shell_args.extend(args.iter().map(OsString::from));

        return ToolInvocation {
            program: OsString::from("powershell.exe"),
            args: shell_args,
        };
    }

    ToolInvocation {
        program: program.into_os_string(),
        args: args.iter().map(OsString::from).collect(),
    }
}

#[cfg(windows)]
fn resolve_windows_program(command: &str) -> Option<PathBuf> {
    let command_path = Path::new(command);
    let has_path_separator = command.contains('\\') || command.contains('/');
    let has_extension = command_path.extension().is_some();

    if has_path_separator {
        return resolve_windows_candidate(command_path, has_extension);
    }

    env::var_os("PATH")
        .and_then(|path| {
            env::split_paths(&path).find_map(|dir| {
                let candidate = dir.join(command);
                resolve_windows_candidate(&candidate, has_extension)
            })
        })
        .or_else(|| resolve_windows_known_program(command))
}

#[cfg(windows)]
fn resolve_windows_candidate(candidate: &Path, has_extension: bool) -> Option<PathBuf> {
    if has_extension {
        return candidate.exists().then(|| candidate.to_path_buf());
    }

    if candidate.exists() {
        return Some(candidate.to_path_buf());
    }

    windows_path_extensions()
        .into_iter()
        .map(|extension| candidate.with_extension(extension.trim_start_matches('.')))
        .find(|path| path.exists())
}

#[cfg(windows)]
fn windows_path_extensions() -> Vec<String> {
    env::var("PATHEXT")
        .ok()
        .map(|value| {
            value
                .split(';')
                .filter_map(|extension| {
                    let extension = extension.trim();
                    (!extension.is_empty()).then(|| extension.to_owned())
                })
                .collect()
        })
        .filter(|extensions: &Vec<String>| !extensions.is_empty())
        .unwrap_or_else(|| {
            vec![
                ".COM".to_owned(),
                ".EXE".to_owned(),
                ".BAT".to_owned(),
                ".CMD".to_owned(),
                ".PS1".to_owned(),
            ]
        })
}

#[cfg(windows)]
fn resolve_windows_known_program(command: &str) -> Option<PathBuf> {
    match command.to_ascii_lowercase().as_str() {
        "codex" => resolve_vscode_codex(),
        "copilot" => resolve_npm_shim("copilot"),
        "agy" => resolve_npm_shim("agy"),
        _ => None,
    }
}

#[cfg(windows)]
fn resolve_npm_shim(command: &str) -> Option<PathBuf> {
    env::var_os("APPDATA").and_then(|appdata| {
        let candidate = PathBuf::from(appdata)
            .join("npm")
            .join(format!("{command}.cmd"));
        candidate.exists().then_some(candidate)
    })
}

#[cfg(windows)]
fn resolve_vscode_codex() -> Option<PathBuf> {
    let extensions_dir = env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .map(|home| home.join(".vscode").join("extensions"))?;

    fs::read_dir(extensions_dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.starts_with("openai.chatgpt-"))
                .unwrap_or(false)
        })
        .filter_map(|extension| {
            let candidate = extension
                .join("bin")
                .join("windows-x86_64")
                .join("codex.exe");
            candidate.exists().then_some(candidate)
        })
        .max()
}

pub async fn execute_tool(
    config: Arc<Config>,
    state: SharedAppState,
    broadcaster: Option<broadcast::Sender<HarnessMessage>>,
    tool_id: &str,
    step_id: impl Into<String>,
    correlation_id: impl Into<String>,
    timeout_duration: Duration,
) -> Result<HarnessMessage, ExecutionError> {
    ExecutionEngine::new(config, state, broadcaster, timeout_duration)
        .execute_tool(tool_id, step_id, correlation_id)
        .await
}

pub fn parse_with_rhai(
    tool: &ToolDefinition,
    exit_code: Option<i64>,
    stdout_text: &str,
    stderr_text: &str,
    step_id: &str,
    correlation_id: &str,
) -> Result<HarnessMessage, ExecutionError> {
    let engine = Engine::new();
    let mut scope = Scope::new();
    scope.push("exit_code", exit_code.unwrap_or(-1));
    scope.push("stdout_text", stdout_text.to_owned());
    scope.push("stderr_text", stderr_text.to_owned());
    scope.push("tool_id", tool.id.clone());
    scope.push("step_id", step_id.to_owned());
    scope.push("correlation_id", correlation_id.to_owned());

    let ast = engine
        .compile(&tool.parser.script)
        .map_err(|err| ExecutionError::Rhai(err.to_string()))?;
    let result: Map = engine
        .eval_ast_with_scope(&mut scope, &ast)
        .map_err(|err| ExecutionError::Rhai(err.to_string()))?;

    Ok(map_to_message(
        result,
        tool,
        exit_code,
        stdout_text,
        stderr_text,
        step_id,
        correlation_id,
    ))
}

async fn read_stream<R>(
    reader: R,
    tool_id: String,
    step_id: String,
    correlation_id: String,
    source: MessageSource,
    broadcaster: Option<broadcast::Sender<HarnessMessage>>,
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

        let (display_line, line_truncated) = compact_line(&line, MAX_STREAM_LINE_CHARS);

        let line_message = HarnessMessage::new(
            tool_id.clone(),
            source.clone(),
            if matches!(source, MessageSource::Stderr) {
                MessageLevel::Warn
            } else {
                MessageLevel::Info
            },
            step_id.clone(),
            correlation_id.clone(),
            display_line.clone(),
            json!({
                "stream": stream_name(&source),
                "line": display_line,
                "truncated": line_truncated
            }),
            line_truncated,
        );

        if let Some(sender) = &broadcaster {
            let _ = sender.send(line_message);
        } else {
            println!("[{}] {}: {}", tool_id, stream_name(&source), line);
        }
    }

    Ok(collected)
}

fn stream_name(source: &MessageSource) -> &'static str {
    match source {
        MessageSource::Stdout => "stdout",
        MessageSource::Stderr => "stderr",
        MessageSource::System => "system",
    }
}

fn compact_captured_text(input: &str) -> (String, bool) {
    let (line_compacted, line_truncated) = compact_long_lines(input, MAX_CAPTURED_LINE_CHARS);
    let (line_limited, count_truncated) = compress_text(&line_compacted);
    let (char_limited, char_truncated) = compact_line(&line_limited, MAX_CAPTURED_CHARS);

    (
        char_limited,
        line_truncated || count_truncated || char_truncated,
    )
}

fn compact_long_lines(input: &str, max_chars: usize) -> (String, bool) {
    let mut truncated = false;
    let compacted = input
        .lines()
        .map(|line| {
            let (line, was_truncated) = compact_line(line, max_chars);
            truncated |= was_truncated;
            line
        })
        .collect::<Vec<_>>()
        .join("\n");

    (compacted, truncated)
}

fn compact_line(input: &str, max_chars: usize) -> (String, bool) {
    let char_count = input.chars().count();
    if char_count <= max_chars {
        return (input.to_owned(), false);
    }

    let head_len = max_chars / 2;
    let tail_len = max_chars.saturating_sub(head_len);
    let head = input.chars().take(head_len).collect::<String>();
    let tail = input
        .chars()
        .rev()
        .take(tail_len)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    let skipped = char_count.saturating_sub(head_len + tail_len);

    (
        format!("{head}\n... [Truncated {skipped} chars] ...\n{tail}"),
        true,
    )
}

fn compress_text(input: &str) -> (String, bool) {
    let lines: Vec<&str> = input.lines().collect();
    if lines.len() <= MAX_CAPTURED_LINES {
        return (input.to_owned(), false);
    }

    let skipped = lines.len().saturating_sub(KEEP_CAPTURED_LINES * 2);
    let mut compressed: Vec<String> = Vec::with_capacity((KEEP_CAPTURED_LINES * 2) + 1);
    compressed.extend(
        lines
            .iter()
            .take(KEEP_CAPTURED_LINES)
            .map(|line| (*line).to_owned()),
    );
    compressed.push(format!("... [Skipped {skipped} lines] ..."));
    compressed.extend(
        lines
            .iter()
            .rev()
            .take(KEEP_CAPTURED_LINES)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .map(|line| (*line).to_owned()),
    );
    (compressed.join("\n"), true)
}

fn map_to_message(
    map: Map,
    tool: &ToolDefinition,
    exit_code: Option<i64>,
    stdout_text: &str,
    stderr_text: &str,
    step_id: &str,
    correlation_id: &str,
) -> HarnessMessage {
    let tool_id = map
        .get("tool_id")
        .and_then(|value| value.clone().try_cast::<String>())
        .unwrap_or_else(|| tool.id.clone());
    let source = map
        .get("source")
        .and_then(|value| value.clone().try_cast::<String>())
        .map(|value| {
            if value.eq_ignore_ascii_case("stderr") {
                MessageSource::Stderr
            } else {
                MessageSource::Stdout
            }
        })
        .unwrap_or_else(|| {
            if stderr_text.is_empty() {
                MessageSource::Stdout
            } else {
                MessageSource::Stderr
            }
        });
    let level = map
        .get("level")
        .and_then(|value| value.clone().try_cast::<String>())
        .map(|value| match value.to_ascii_lowercase().as_str() {
            "debug" => MessageLevel::Debug,
            "warn" => MessageLevel::Warn,
            "error" => MessageLevel::Error,
            _ => MessageLevel::Info,
        })
        .unwrap_or_else(|| {
            if exit_code.unwrap_or(0) == 0 {
                MessageLevel::Info
            } else {
                MessageLevel::Error
            }
        });
    let raw_output = map
        .get("raw_output")
        .and_then(|value| value.clone().try_cast::<String>())
        .unwrap_or_else(|| {
            if stderr_text.is_empty() {
                stdout_text.to_owned()
            } else {
                format!("stdout:\n{stdout_text}\nstderr:\n{stderr_text}")
            }
        });
    let normalized_payload = map
        .get("normalized_payload")
        .map(dynamic_to_json)
        .unwrap_or_else(|| {
            json!({
                "exit_code": exit_code,
                "stdout_text": stdout_text,
                "stderr_text": stderr_text,
            })
        });
    let truncated = map
        .get("truncated")
        .and_then(|value| value.clone().try_cast::<bool>())
        .unwrap_or(false)
        || stdout_text.contains("[Skipped ")
        || stderr_text.contains("[Skipped ");

    HarnessMessage::new(
        tool_id,
        source,
        level,
        step_id.to_owned(),
        correlation_id.to_owned(),
        raw_output,
        normalized_payload,
        truncated,
    )
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
    } else if let Some(array) = value.clone().try_cast::<Array>() {
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

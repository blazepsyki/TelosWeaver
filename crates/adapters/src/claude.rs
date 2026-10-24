//! Claude Code Managed 실행 (Spec §7.1, §7.5, PH0-CLAUDE).
//!
//! - 실행 파일은 `anthropic.claude-code` 확장이 포함한 CLI입니다 ([`cli_rule`], DEC-06).
//! - Run 하나는 `claude -p --input-format stream-json --output-format stream-json` 프로세스 하나와 Turn 하나입니다.
//!   프롬프트를 stdin의 Stream JSON 메시지로 보내고 바로 닫습니다. 명령줄 길이·인용 문제를 피하기 위해서입니다.
//! - 이어지는 대화는 같은 Run의 입력이 아니라 다음 Run의 `resume`으로 처리합니다. 공통 이벤트 규칙상
//!   전용 프로세스 Run에서 `turn_finished` 뒤에 다른 Turn을 둘 수 없기 때문입니다. 그래서 `send_input`은 미지원입니다.
//! - Print 모드에는 실행 중 승인 응답 경로가 없습니다. 권한은 시작 전에 [`ClaudeLaunch`]로 정하고
//!   `answer_approval`은 미지원입니다 (§7.4와 같은 원칙).
//! - Resume(`--resume`)·Fork(`--resume` + `--fork-session`) 인자는 구성하지만 Capability는 실험 전까지 `unverified`입니다.

use std::{
    collections::HashMap,
    ffi::OsString,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use futures::{StreamExt, stream::BoxStream};
use telos_schema::{
    ApprovalAnswer, Capability, CapabilityState, EvidenceLevel, FailureKind, ProviderEvent,
    RunContext, RunInspection, RunStarted, SessionMode, TaskSpec,
};

use crate::{
    AdapterError, AgentAdapter, CliBinaryRule, ProviderCliRule,
    runtime::{CommandSpec, OutputLimits, ProcessRuntime, RuntimeError, StdinMode},
};

mod protocol;
mod run;

use run::{Clock, RunEntry};

pub const PROVIDER_ID: &str = "claude";
pub const EXTENSION_ID: &str = "anthropic.claude-code";

/// 실환경 시험으로 기본 실행 경로를 확인한 CLI 버전입니다. 다른 버전은 `unverified`로 보고합니다.
const VERIFIED_CLI_VERSIONS: &[&str] = &["2.1.310"];

/// Stream JSON 한 줄의 상한. 도구 결과가 큰 경우를 고려하되 상한을 넘으면 Protocol 오류로 처리합니다.
const OUTPUT_LIMITS: OutputLimits = OutputLimits {
    max_line_bytes: 4 * 1024 * 1024,
    max_buffered_bytes: 16 * 1024 * 1024,
};

/// 실행 Host의 VS Code `targetPlatform`에 맞는 확장 실행 파일 규칙입니다 (§7.5 표).
pub fn cli_rule(target_platform: &str) -> Option<ProviderCliRule> {
    let (os, _arch) = target_platform.split_once('-')?;
    let binary = match os {
        "win32" => "claude.exe",
        "linux" | "alpine" | "darwin" => "claude",
        _ => return None,
    };
    Some(ProviderCliRule {
        extension_id: EXTENSION_ID.into(),
        binary: CliBinaryRule::InExtension {
            relative_path: PathBuf::from("resources/native-binary").join(binary),
        },
        binary_name: binary.into(),
        version_args: vec!["--version".into()],
    })
}

/// Run마다 Core가 정하는 실행 조건입니다. 경로·권한은 승인된 Workspace·Policy에서 나와야 합니다 (§7.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeLaunch {
    /// 검증된 Workspace 경로. 사용자 임의 경로를 그대로 쓰지 않습니다.
    pub cwd: PathBuf,
    pub prompt: String,
    /// `None`이면 CLI 기본 모델입니다.
    pub model: Option<String>,
    /// `default`, `acceptEdits`, `plan` 등. `bypassPermissions`는 거부합니다.
    pub permission_mode: String,
    pub allowed_tools: Vec<String>,
    pub disallowed_tools: Vec<String>,
    /// `Some(vec![])`이면 모든 도구를 끕니다(`--tools ""`).
    pub tools: Option<Vec<String>>,
    /// 예: `user`. `None`이면 CLI 기본값입니다.
    pub setting_sources: Option<String>,
}

/// Task·Run에서 [`ClaudeLaunch`]를 만듭니다. Core가 승인된 Workspace·Policy·문맥 조립으로 구현합니다.
pub trait ClaudeLaunchResolver: Send + Sync {
    fn resolve(&self, task: &TaskSpec, context: &RunContext) -> Result<ClaudeLaunch, AdapterError>;
}

pub struct ClaudeCodeConfig {
    /// §7.5 탐지 결과의 실행 파일. Adapter 수명 동안 바꾸지 않습니다.
    pub executable: PathBuf,
    /// 탐지 결과의 CLI 버전 (예: `2.1.310 (Claude Code)`).
    pub cli_version: String,
    pub host_id: String,
    pub runtime: Arc<dyn ProcessRuntime>,
    pub launcher: Arc<dyn ClaudeLaunchResolver>,
    /// 취소 시 정상 종료를 기다리는 시간. 지나면 강제 종료합니다.
    pub cancel_grace: Duration,
}

pub struct ClaudeCodeAdapter {
    config: Option<ClaudeCodeConfig>,
    runs: Mutex<HashMap<String, Arc<RunEntry>>>,
    keys: Mutex<HashMap<String, String>>,
    clock: Clock,
}

impl Default for ClaudeCodeAdapter {
    /// 실행 파일이 없는 Adapter. 모든 실행을 `unsupported`로 거부합니다.
    fn default() -> Self {
        Self::with_config(None)
    }
}

impl ClaudeCodeAdapter {
    pub fn new(config: ClaudeCodeConfig) -> Self {
        Self::with_config(Some(config))
    }

    fn with_config(config: Option<ClaudeCodeConfig>) -> Self {
        Self {
            config,
            runs: Mutex::new(HashMap::new()),
            keys: Mutex::new(HashMap::new()),
            clock: Arc::new(timestamp),
        }
    }

    fn config(&self) -> Result<&ClaudeCodeConfig, AdapterError> {
        self.config
            .as_ref()
            .ok_or_else(|| unsupported("start_run", "Claude Code CLI was not discovered"))
    }

    fn run(&self, run_id: &str) -> Result<Arc<RunEntry>, AdapterError> {
        self.runs
            .lock()
            .unwrap()
            .get(run_id)
            .cloned()
            .ok_or_else(|| AdapterError::UnknownRun {
                run_id: run_id.to_owned(),
            })
    }

    fn verified(&self) -> bool {
        self.config.as_ref().is_some_and(|c| {
            let version = c.cli_version.split_whitespace().next();
            VERIFIED_CLI_VERSIONS.iter().any(|v| version == Some(v))
        })
    }

    /// 같은 멱등 키·같은 Run이면 처음 결과를, 다른 Run이면 충돌을 돌려줍니다.
    fn replay(&self, context: &RunContext) -> Option<Result<RunStarted, AdapterError>> {
        let keys = self.keys.lock().unwrap();
        let conflict = || AdapterError::IdempotencyConflict {
            idempotency_key: context.idempotency_key.clone(),
        };
        match keys.get(&context.idempotency_key) {
            Some(run_id) if *run_id == context.run_id => {
                Some(self.run(run_id).map(|entry| entry.started.clone()))
            }
            Some(_) => Some(Err(conflict())),
            None if self.runs.lock().unwrap().contains_key(&context.run_id) => {
                Some(Err(conflict()))
            }
            None => None,
        }
    }
}

/// CLI 인자를 만듭니다. 프롬프트는 인자가 아니라 stdin으로 보냅니다.
fn command_args(
    launch: &ClaudeLaunch,
    context: &RunContext,
) -> Result<Vec<OsString>, AdapterError> {
    if launch.permission_mode == "bypassPermissions" {
        return Err(configuration(
            "bypassPermissions is not allowed for managed runs",
        ));
    }
    let mut args: Vec<OsString> = [
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--strict-mcp-config",
        "--permission-mode",
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    args.push(launch.permission_mode.clone().into());
    if let Some(model) = &launch.model {
        args.extend(["--model".into(), model.into()]);
    }
    if let Some(sources) = &launch.setting_sources {
        args.extend(["--setting-sources".into(), sources.into()]);
    }
    if let Some(tools) = &launch.tools {
        args.extend(["--tools".into(), tools.join(",").into()]);
    }
    if !launch.allowed_tools.is_empty() {
        args.extend([
            "--allowedTools".into(),
            launch.allowed_tools.join(",").into(),
        ]);
    }
    if !launch.disallowed_tools.is_empty() {
        args.extend([
            "--disallowedTools".into(),
            launch.disallowed_tools.join(",").into(),
        ]);
    }
    let session = &context.session;
    match session.mode {
        SessionMode::Fresh | SessionMode::Capsule => {}
        SessionMode::Resume | SessionMode::NativeFork => {
            let source = session
                .source_session_id
                .as_deref()
                .ok_or_else(|| configuration("resume requires sourceSessionId"))?;
            args.extend(["--resume".into(), source.into()]);
            if session.mode == SessionMode::NativeFork {
                args.push("--fork-session".into());
            }
        }
    }
    Ok(args)
}

/// Stream JSON 입력의 사용자 메시지 한 줄입니다.
fn prompt_line(prompt: &str) -> Vec<u8> {
    let mut line = serde_json::to_vec(&serde_json::json!({
        "type": "user",
        "message": { "role": "user", "content": prompt },
    }))
    .expect("prompt message serializes");
    line.push(b'\n');
    line
}

fn configuration(message: &str) -> AdapterError {
    AdapterError::Failure {
        kind: FailureKind::Configuration,
        message: message.into(),
    }
}

fn start_failed(error: RuntimeError) -> AdapterError {
    AdapterError::Failure {
        kind: FailureKind::StartFailed,
        message: error.to_string(),
    }
}

fn unsupported(capability: &str, reason: &str) -> AdapterError {
    AdapterError::UnsupportedCapability {
        capability: capability.into(),
        reason: reason.into(),
    }
}

#[async_trait]
impl AgentAdapter for ClaudeCodeAdapter {
    fn id(&self) -> &str {
        PROVIDER_ID
    }

    async fn detect(&self, host_id: &str) -> Result<HashMap<String, Capability>, AdapterError> {
        let Some(config) = &self.config else {
            return Ok(HashMap::new());
        };
        let checked_at = (self.clock)();
        let capability = |state, evidence_level, reason: Option<&str>| Capability {
            state,
            provider_version: config.cli_version.clone(),
            host_id: host_id.to_owned(),
            checked_at: checked_at.clone(),
            evidence_level,
            reason: reason.map(str::to_owned),
        };
        let mut caps = HashMap::new();
        if host_id != config.host_id {
            caps.insert(
                "managed_run".into(),
                capability(
                    CapabilityState::Unsupported,
                    EvidenceLevel::Documented,
                    Some("the CLI was discovered on a different host"),
                ),
            );
            return Ok(caps);
        }
        let (state, evidence, reason) = if self.verified() {
            (
                CapabilityState::Supported,
                EvidenceLevel::EnvironmentTested,
                None,
            )
        } else {
            (
                CapabilityState::Unverified,
                EvidenceLevel::ContractTested,
                Some("this CLI version has not been environment-tested"),
            )
        };
        for name in ["managed_run", "event_stream", "cancel"] {
            caps.insert(name.into(), capability(state, evidence, reason));
        }
        caps.insert(
            "send_input".into(),
            capability(
                CapabilityState::Unsupported,
                EvidenceLevel::ContractTested,
                Some("follow-up turns run as a new resumed run"),
            ),
        );
        caps.insert(
            "answer_approval".into(),
            capability(
                CapabilityState::Unsupported,
                EvidenceLevel::Documented,
                Some("print mode has no live approval path; permissions are set before start"),
            ),
        );
        for name in ["native_resume", "native_fork"] {
            caps.insert(
                name.into(),
                capability(
                    CapabilityState::Unverified,
                    EvidenceLevel::Documented,
                    Some("Phase 0 experiment pending (PROV-05, FORK-POC-01)"),
                ),
            );
        }
        Ok(caps)
    }

    async fn start_run(
        &self,
        task: &TaskSpec,
        context: &RunContext,
    ) -> Result<RunStarted, AdapterError> {
        let config = self.config()?;
        if let Some(result) = self.replay(context) {
            return result;
        }
        if context.host_id != config.host_id {
            return Err(configuration(
                "run host differs from the host where the CLI was discovered",
            ));
        }
        let launch = config.launcher.resolve(task, context)?;
        let args = command_args(&launch, context)?;
        let handle = config
            .runtime
            .spawn(CommandSpec {
                program: config.executable.clone(),
                args,
                cwd: launch.cwd.clone(),
                env: vec![],
                stdin: StdinMode::Piped,
                limits: OUTPUT_LIMITS,
            })
            .await
            .map_err(start_failed)?;
        let wrote = async {
            handle.write_stdin(&prompt_line(&launch.prompt)).await?;
            handle.close_stdin().await
        }
        .await;
        if let Err(error) = wrote {
            let _ = handle.terminate(Duration::ZERO).await;
            return Err(start_failed(error));
        }

        let entry = RunEntry::new(
            context.run_id.clone(),
            context.idempotency_key.clone(),
            handle,
            self.clock.clone(),
        );
        // 같은 키의 동시 요청이 경합하면 먼저 등록한 Run을 돌려주고 나중 프로세스는 끝냅니다.
        if let Some(result) = self.replay(context) {
            entry.cancel(Duration::ZERO);
            tokio::spawn(entry.supervise(Duration::ZERO));
            return result;
        }
        self.keys
            .lock()
            .unwrap()
            .insert(entry.idempotency_key.clone(), entry.run_id.clone());
        self.runs
            .lock()
            .unwrap()
            .insert(entry.run_id.clone(), entry.clone());
        let started = entry.started.clone();
        tokio::spawn(entry.supervise(config.cancel_grace));
        Ok(started)
    }

    fn events(
        &self,
        run_id: &str,
        after_sequence: u64,
    ) -> BoxStream<'static, Result<ProviderEvent, AdapterError>> {
        match self.run(run_id) {
            Ok(entry) => entry.subscribe(after_sequence),
            Err(error) => futures::stream::once(async move { Err(error) }).boxed(),
        }
    }

    async fn send_input(
        &self,
        _run_id: &str,
        _input_id: &str,
        _text: &str,
    ) -> Result<(), AdapterError> {
        Err(unsupported(
            "send_input",
            "follow-up turns run as a new resumed run",
        ))
    }

    async fn answer_approval(
        &self,
        _run_id: &str,
        _request_id: &str,
        _decision: ApprovalAnswer,
    ) -> Result<(), AdapterError> {
        Err(unsupported(
            "answer_approval",
            "print mode has no live approval path",
        ))
    }

    async fn cancel_run(&self, run_id: &str) -> Result<bool, AdapterError> {
        let grace = self.config()?.cancel_grace;
        Ok(self.run(run_id)?.cancel(grace))
    }

    async fn inspect_run(&self, run_id: &str) -> Result<RunInspection, AdapterError> {
        Ok(self.run(run_id)?.inspect())
    }
}

/// RFC 3339 UTC 초 단위 시각.
fn timestamp() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    let (days, rem) = ((seconds / 86_400) as i64, seconds % 86_400);
    // Howard Hinnant의 civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        rem % 3_600 / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_is_rfc3339_utc() {
        let t = timestamp();
        assert_eq!(t.len(), 20);
        assert!(t.ends_with('Z') && t.as_bytes()[10] == b'T');
    }
}

//! Claude Code Managed 실행 (Spec §7.1, §7.5, PH0-CLAUDE·PH1-CLAUDE).
//!
//! - 실행 파일은 `anthropic.claude-code` 확장이 포함한 CLI입니다 ([`cli_rule`], DEC-06).
//! - Run 하나는 `claude -p --input-format stream-json --output-format stream-json` 프로세스 하나와 Turn 하나입니다.
//!   프롬프트를 stdin의 Stream JSON 메시지로 보냅니다. 명령줄 길이·인용 문제를 피하기 위해서입니다.
//! - 이어지는 대화는 같은 Run의 입력이 아니라 다음 Run의 `resume`으로 처리합니다. 실행 중에 보낸 메시지는 뒤에 도구 호출
//!   경계가 남아 있을 때만 같은 Turn에 들어가고, 아니면 CLI가 새 Turn을 시작합니다(2.1.325 실험). 보내는 쪽이 어느 쪽이
//!   될지 정할 수 없고 새 Turn은 Run당 Turn 하나 규칙(§7.3)에 어긋나므로 `send_input`은 미지원입니다.
//! - 권한은 시작 전에 [`ClaudeLaunch`]로 정합니다. [`ClaudeApprovals::Prompt`]로 만든 Adapter는
//!   `--permission-prompt-tool stdio`의 승인 요청을 `approval_required` 이벤트(대상·인자 Digest 포함)로 내고
//!   `answer_approval`의 답을 CLI에 돌려줍니다. 기본값([`ClaudeApprovals::PolicyOnly`])에서는 승인이 필요한 도구를
//!   CLI가 거부하고 `answer_approval`은 `UnsupportedCapability`입니다.
//! - Capability의 `supported`/`unverified`는 실환경 증거의 버전과 사전 점검으로 정합니다([`preflight`], §7.1 판정 기준).
//!   특정 Turn 기준 재개·분기(`source_turn_id`)는 CLI 수단이 없어 실행 전에 거부합니다.
//! - `RunContext.taskMcp`가 있으면 그 Run에만 `--mcp-config`로 Task MCP 서버를 연결합니다([`mcp`], §9.2).
//!   항상 `--strict-mcp-config`로 실행해 사용자·프로젝트의 MCP 설정을 그 Run에 섞지 않습니다.
//! - `capsule`은 새 세션입니다. 이전 세션을 쓰지 않으며 Core가 만든 입력 본문([`ClaudeLaunch::prompt`])을 그대로 보냅니다.
//! - 세션을 VS Code 확장 대화 목록에 표시할지는 [`VsCodeListing`]으로 정하고 `CLAUDE_CODE_ENTRYPOINT`로
//!   지정합니다 (§7.6, DEC-08). 기본은 표시입니다.
//! - 백그라운드 작업(`CLAUDE_CODE_DISABLE_BACKGROUND_TASKS=1`)을 끕니다. 켜져 있으면 CLI가 첫 `result` 뒤에도
//!   끝나지 않고 작업 완료를 받아 같은 프로세스에서 Turn을 이어가므로 Run당 Turn 하나 규칙과 맞지 않습니다.

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
    ApprovalAnswer, Capability, CapabilityEvidence, CapabilityState, EvidenceLevel, FailureKind,
    PreflightReport, ProviderEvent, RunContext, RunInspection, RunStarted, SessionMode, TaskSpec,
    capability_keys, judge, major_minor,
};

use crate::{
    AdapterError, AgentAdapter, CliBinaryRule, ProviderCliRule,
    runtime::{CommandSpec, OutputLimits, ProcessRuntime, RuntimeError, StdinMode},
};

pub mod mcp;
pub mod preflight;
mod protocol;
mod run;

use mcp::Injection;
use run::{Clock, RunEntry, RunOptions};

pub const PROVIDER_ID: &str = "claude";
pub const EXTENSION_ID: &str = "anthropic.claude-code";

const SEND_INPUT_REASON: &str = "a message sent during a turn joins that turn only if a tool-call boundary follows, otherwise the CLI starts a new turn (observed on 2.1.325); a run holds one turn, so follow-up input runs as a new resumed run";
const NATIVE_RESUME_LIMITS: &str = "resume appends turns to the source session; resuming at a specific turn is rejected before start";
const NATIVE_FORK_LIMITS: &str = "forks continue from the latest point of the source session; forking at a specific turn is rejected before start";
const ANSWER_APPROVAL_LIMITS: &str = "applies to runs of an adapter built with ClaudeApprovals::Prompt (--permission-prompt-tool stdio); other runs deny tools that need approval and reject answer_approval";

/// 세션 출처를 정하는 환경 변수. 확장은 `sdk-cli`·`sdk-ts`·`sdk-py` 세션을 기본 목록에서 뺍니다 (§7.6).
const ENTRYPOINT_ENV: &str = "CLAUDE_CODE_ENTRYPOINT";

/// Managed Run에서 백그라운드 작업(Bash `run_in_background`, 자동 백그라운드 전환)을 끕니다.
const DISABLE_BACKGROUND_ENV: &str = "CLAUDE_CODE_DISABLE_BACKGROUND_TASKS";

/// Core가 Claude 세션 안에서 시작되었을 때 상속될 수 있는 부모 세션 변수입니다.
/// Managed Run은 별도 세션이므로 자식 환경에서 제거합니다 (§7.6).
const PARENT_SESSION_ENV: &[&str] = &[
    "CLAUDECODE",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_PID",
];

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

/// 실행 중 승인 요청을 다루는 방식입니다 (이월 P1-2, SB-07).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClaudeApprovals {
    /// 시작 전 정책만 씁니다. 승인이 필요한 도구는 CLI가 거부합니다.
    #[default]
    PolicyOnly,
    /// `--permission-prompt-tool stdio`로 승인 요청을 받아 `approval_required`로 내고 `answer_approval`로 답합니다.
    /// 답이 올 때까지 CLI가 기다리므로 Core가 기한 안에 답하거나 Run을 취소해야 합니다.
    Prompt,
}

/// Managed Run 세션의 VS Code 확장 대화 목록 표시 모드입니다 (§7.6, DEC-08, AGENT-05).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VsCodeListing {
    /// 확장 목록의 조회 대상으로 기록합니다(`claude-vscode`).
    #[default]
    Listed,
    /// 자동화 출처로 기록해 확장 기본 목록에서 뺍니다(`sdk-cli`).
    Hidden,
}

impl VsCodeListing {
    /// 세션에 기록할 `CLAUDE_CODE_ENTRYPOINT` 값입니다.
    pub fn entrypoint(self) -> &'static str {
        match self {
            Self::Listed => "claude-vscode",
            Self::Hidden => "sdk-cli",
        }
    }
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
    listing: VsCodeListing,
    approvals: ClaudeApprovals,
    evidence: Vec<CapabilityEvidence>,
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
            listing: VsCodeListing::default(),
            approvals: ClaudeApprovals::default(),
            evidence: preflight::builtin_evidence(),
            runs: Mutex::new(HashMap::new()),
            keys: Mutex::new(HashMap::new()),
            clock: Arc::new(timestamp),
        }
    }

    /// 운영자 설정의 확장 목록 표시 모드를 적용합니다. 지정하지 않으면 [`VsCodeListing::Listed`]입니다.
    pub fn with_vscode_listing(mut self, listing: VsCodeListing) -> Self {
        self.listing = listing;
        self
    }

    pub fn vscode_listing(&self) -> VsCodeListing {
        self.listing
    }

    /// 실행 중 승인 방식을 정합니다. 지정하지 않으면 [`ClaudeApprovals::PolicyOnly`]입니다.
    pub fn with_approvals(mut self, approvals: ClaudeApprovals) -> Self {
        self.approvals = approvals;
        self
    }

    pub fn approvals(&self) -> ClaudeApprovals {
        self.approvals
    }

    /// Core가 기록한 실환경 증거를 내장 증거([`preflight::builtin_evidence`])에 더합니다 (§7.1).
    pub fn with_evidence(mut self, evidence: impl IntoIterator<Item = CapabilityEvidence>) -> Self {
        self.evidence.extend(evidence);
        self
    }

    /// 모델을 호출하지 않는 사전 점검을 실행합니다 (H0 D14). `detect`도 같은 점검을 실행해 판정에 씁니다.
    pub async fn preflight(&self, host_id: &str) -> Result<PreflightReport, AdapterError> {
        let config = self.config()?;
        Ok(self.run_preflight(config, host_id).await.report())
    }

    async fn run_preflight(
        &self,
        config: &ClaudeCodeConfig,
        host_id: &str,
    ) -> preflight::Preflight {
        preflight::run(preflight::PreflightRequest {
            runtime: config.runtime.as_ref(),
            executable: &config.executable,
            discovered_version: &config.cli_version,
            host_id,
            // 세션을 만들지 않지만 Run과 같은 환경에서 확인합니다. 출처는 목록 밖(`sdk-cli`)으로 둡니다.
            env: command_env(VsCodeListing::Hidden),
            checked_at: (self.clock)(),
        })
        .await
    }

    /// 이 플랫폼에서 `key`의 실환경 시험을 통과한 CLI 버전입니다.
    fn passed_versions(&self, key: &str, platform: &str) -> Vec<&str> {
        let mut versions: Vec<&str> = self
            .evidence
            .iter()
            .filter(|e| {
                e.passed && e.provider == PROVIDER_ID && e.key == key && e.platform == platform
            })
            .map(|e| e.cli_version.as_str())
            .collect();
        versions.sort_unstable();
        versions.dedup();
        versions
    }

    fn config(&self) -> Result<&ClaudeCodeConfig, AdapterError> {
        self.config.as_ref().ok_or_else(|| {
            unsupported(
                capability_keys::MANAGED_RUN,
                "Claude Code CLI was not discovered",
            )
        })
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
    approvals: ClaudeApprovals,
    injection: Option<&Injection>,
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
    if approvals == ClaudeApprovals::Prompt {
        args.extend(["--permission-prompt-tool".into(), "stdio".into()]);
    }
    // 이 Run에만 연결합니다. `--strict-mcp-config`와 함께라 다른 MCP 설정은 쓰이지 않습니다 (§9.2).
    if let Some(injection) = injection {
        args.extend(["--mcp-config".into(), injection.config_path().into()]);
    }
    if let Some(model) = &launch.model {
        args.extend(["--model".into(), model.into()]);
    }
    if let Some(sources) = &launch.setting_sources {
        args.extend(["--setting-sources".into(), sources.into()]);
    }
    if let Some(tools) = &launch.tools {
        args.extend(["--tools".into(), tools.join(",").into()]);
    }
    // 주입한 서버의 도구만 추가로 허용합니다. 나머지는 Core가 정한 목록 그대로입니다.
    let mut allowed = launch.allowed_tools.clone();
    allowed.extend(injection.map(Injection::allowed_tool));
    if !allowed.is_empty() {
        args.extend(["--allowedTools".into(), allowed.join(",").into()]);
    }
    if !launch.disallowed_tools.is_empty() {
        args.extend([
            "--disallowedTools".into(),
            launch.disallowed_tools.join(",").into(),
        ]);
    }
    let session = &context.session;
    match session.mode {
        SessionMode::Fresh => {}
        // 새 세션으로 시작하고 이전 세션(`sourceSessionId`)은 쓰지 않습니다. 본문은 Core가 만든 프롬프트에 들어 있습니다.
        // Capsule 참조가 없는 요청을 `fresh`처럼 조용히 실행하지 않습니다 (§2.1, SB-11).
        SessionMode::Capsule => {
            if session.capsule_version_id.is_none() {
                return Err(configuration("capsule requires capsuleVersionId"));
            }
        }
        SessionMode::Resume | SessionMode::NativeFork => {
            // CLI에 특정 Turn 기준 재개·분기 옵션이 없습니다. 마지막 지점으로 대체하지 않고 거부합니다 (Spec §2.1·§7.1).
            if session.source_turn_id.is_some() {
                let key = if session.mode == SessionMode::Resume {
                    capability_keys::NATIVE_RESUME
                } else {
                    capability_keys::NATIVE_FORK
                };
                return Err(unsupported(
                    key,
                    "resume or fork at a specific turn is not supported by this CLI",
                ));
            }
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

/// 자식 환경: 세션 출처를 명시하고 부모 세션 변수를 제거합니다. 상속된 값에 기대지 않습니다 (§7.6).
/// 백그라운드 작업도 끕니다. 첫 Turn 뒤에 CLI가 스스로 Turn을 이어가지 않게 합니다.
fn command_env(listing: VsCodeListing) -> Vec<(OsString, Option<OsString>)> {
    let mut env = vec![
        (ENTRYPOINT_ENV.into(), Some(listing.entrypoint().into())),
        (DISABLE_BACKGROUND_ENV.into(), Some("1".into())),
    ];
    env.extend(PARENT_SESSION_ENV.iter().map(|key| ((*key).into(), None)));
    env
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

/// `judge`의 결과를 설명하는 근거입니다. 같은 버전의 증거로 `supported`이면 덧붙일 말이 없습니다.
fn judgment_basis(
    passed: &[&str],
    current: &str,
    platform: &str,
    report: &PreflightReport,
) -> Option<String> {
    if passed.contains(&current) {
        return None;
    }
    if current.is_empty() {
        return Some("the CLI version could not be read, so no evidence applies".into());
    }
    if passed.is_empty() {
        return Some(format!(
            "no environment test has passed on {platform} for this capability"
        ));
    }
    let tested = passed.join(", ");
    let same_line =
        major_minor(current).is_some_and(|mm| passed.iter().any(|v| major_minor(v) == Some(mm)));
    if !same_line {
        return Some(format!(
            "environment-tested on {tested}; {current} is a different major.minor and needs the environment tests again"
        ));
    }
    if report.passed() {
        return Some(format!(
            "environment-tested on {tested}; {current} is the same major.minor and passed the preflight checks"
        ));
    }
    let failed: Vec<String> = report
        .results
        .iter()
        .filter(|result| !result.passed)
        .map(|result| match &result.check {
            telos_schema::PreflightCheck::VersionOutput => "--version output".to_owned(),
            telos_schema::PreflightCheck::OptionAccepted { option } => {
                format!("{option} not accepted")
            }
            telos_schema::PreflightCheck::HelpContains { flag } => format!("{flag} not in --help"),
        })
        .collect();
    Some(format!(
        "environment-tested on {tested}; preflight failed for {current}: {}",
        failed.join(", ")
    ))
}

pub(crate) fn configuration(message: &str) -> AdapterError {
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
        let mut caps = HashMap::new();
        if host_id != config.host_id {
            caps.insert(
                capability_keys::MANAGED_RUN.into(),
                Capability {
                    state: CapabilityState::Unsupported,
                    provider_version: config.cli_version.clone(),
                    host_id: host_id.to_owned(),
                    checked_at,
                    evidence_level: EvidenceLevel::Documented,
                    reason: Some("the CLI was discovered on a different host".into()),
                },
            );
            return Ok(caps);
        }
        // §7.1 판정 기준: 같은 플랫폼의 통과 증거, 지금 보고된 버전, 그 기능에 필요한 사전 점검으로 정합니다.
        let preflight = self.run_preflight(config, host_id).await;
        let provider_version = preflight.report().cli_version;
        let platform = preflight::host_platform();
        // 버전을 읽지 못하면 어떤 증거와도 맞지 않아 `unverified`입니다.
        let current = preflight.version.as_deref().unwrap_or("");
        let capability = |state, evidence_level, reason: Option<String>| Capability {
            state,
            provider_version: provider_version.clone(),
            host_id: host_id.to_owned(),
            checked_at: checked_at.clone(),
            evidence_level,
            reason,
        };
        for (key, limits) in [
            (capability_keys::MANAGED_RUN, None),
            (capability_keys::EVENT_STREAM, None),
            (capability_keys::CANCEL, None),
            (capability_keys::NATIVE_RESUME, Some(NATIVE_RESUME_LIMITS)),
            (capability_keys::NATIVE_FORK, Some(NATIVE_FORK_LIMITS)),
            (
                capability_keys::ANSWER_APPROVAL,
                Some(ANSWER_APPROVAL_LIMITS),
            ),
            // 운영자 설정과 무관하게 수단의 지원 여부를 보고합니다 (§7.6).
            (capability_keys::VSCODE_LISTING, None),
        ] {
            let passed = self.passed_versions(key, &platform);
            let report = preflight.report_for(key);
            let state = judge(&passed, current, Some(&report));
            let basis = judgment_basis(&passed, current, &platform, &report);
            let reason = match (basis, limits) {
                (Some(basis), Some(limits)) => Some(format!("{basis}; {limits}")),
                (basis, limits) => basis.or(limits.map(str::to_owned)),
            };
            let level = if state == CapabilityState::Supported {
                EvidenceLevel::EnvironmentTested
            } else {
                EvidenceLevel::ContractTested
            };
            caps.insert(key.into(), capability(state, level, reason));
        }
        // 수단이 계약과 맞지 않는 기능은 버전과 관계없이 `unsupported`입니다 (SB-06).
        caps.insert(
            capability_keys::SEND_INPUT.into(),
            capability(
                CapabilityState::Unsupported,
                EvidenceLevel::ContractTested,
                Some(SEND_INPUT_REASON.into()),
            ),
        );
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
        let live_approval = self.approvals == ClaudeApprovals::Prompt;
        // 주입 파일은 버려질 때 지워집니다. 아래에서 시작에 실패하면 남지 않습니다.
        let injection = context
            .task_mcp
            .as_ref()
            .map(|binding| Injection::prepare(binding, &context.run_id))
            .transpose()?;
        let args = command_args(&launch, context, self.approvals, injection.as_ref())?;
        let handle = config
            .runtime
            .spawn(CommandSpec {
                program: config.executable.clone(),
                args,
                cwd: launch.cwd.clone(),
                env: command_env(self.listing),
                stdin: StdinMode::Piped,
                limits: OUTPUT_LIMITS,
            })
            .await
            .map_err(start_failed)?;
        let wrote = async {
            handle.write_stdin(&prompt_line(&launch.prompt)).await?;
            // 실시간 승인 Run은 응답을 보내야 하므로 열어 두고, 감시 Task가 `result` 뒤에 닫습니다.
            if live_approval {
                Ok(())
            } else {
                handle.close_stdin().await
            }
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
            RunOptions {
                live_approval,
                injection,
            },
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
        Err(unsupported(capability_keys::SEND_INPUT, SEND_INPUT_REASON))
    }

    async fn answer_approval(
        &self,
        run_id: &str,
        request_id: &str,
        decision: ApprovalAnswer,
    ) -> Result<(), AdapterError> {
        self.run(run_id)?
            .answer_approval(request_id, decision)
            .await
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

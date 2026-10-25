//! Google Antigravity (`agy`) Managed 실행 (Spec §7.1, §7.2, §7.3, §7.4, §7.5, §7.6, DEC-08, AGENT-05, PH0-AGY).
//!
//! - 실행 파일은 `google.google-antigravity` 확장이 `~/.gemini/bin/agy[.exe]`에 내려받아 관리하는 CLI입니다 ([`cli_rule`], DEC-06, §7.5).
//! - Run 하나는 `agy [--app_data_dir=antigravity] --input-format stream-json --output-format stream-json`
//!   프로세스 하나와 Turn 하나입니다. 프롬프트를 stdin의 Stream JSON 메시지(`{"event":"user","message":{"role":"user","content":...}}`)로
//!   보내고 즉시 stdin을 닫습니다. `stdin`을 열어 둔 채 프로세스 종료만 기다리지 않으며(§7.4),
//!   명령줄 인용·길이 문제와 `-p` 인자 파싱 모호성을 피합니다.
//! - 후속 Turn은 같은 Run의 추가 입력이 아니라 다음 Run의 `SessionMode::Resume`(`--conversation <id>`)으로 직렬화해 실행합니다 (§7.4, §8.1).
//!   공통 이벤트 계약(`telos_schema::check_run_events`)에서 전용 프로세스 Run은 `turn_finished` 뒤에 새 Turn 이벤트를 허용하지 않기 때문입니다.
//! - Headless Print 모드에는 실행 중 승인 응답(`control_request`/`control_response`) 경로가 없습니다 (§7.4).
//!   권한 확인이 필요한 동작은 소프트 거부되고 `result.denied_actions` 및 `step_update` 오류로 보고됩니다.
//!   따라서 `answer_approval`은 `UnsupportedCapability`를 반환하고, 시작 전 모드·권한 설정을 사용합니다.
//! - Headless Print 모드에서 `/fork`는 거부되고 `--fork` 플래그도 없으므로 `SessionMode::NativeFork`는 `UnsupportedCapability`로 거부합니다 (`FORK-POC-01`).
//! - 데이터 디렉터리와 VS Code 확장 대화 목록 표시(`vscode_listing`, §7.5·§7.6·`DEC-08`·`AGENT-05`)는 `AGY-DATA-01~05` 검증에 따라
//!   기본값 `listed`(`AgyVsCodeListing::Listed` ↔ `AgyDataDirMode::ExtensionShared`, `--app_data_dir=antigravity`)를 쓰며,
//!   `hidden`(`AgyVsCodeListing::Hidden` ↔ `AgyDataDirMode::DefaultCli`)은 플래그를 생략해 `~/.gemini/antigravity-cli`에 분리 기록합니다.
//!   옵션 수용 여부를 확인해 거부되면 조용히 다른 디렉터리로 바꾸지 않고 `unsupported`로 보고합니다 (§7.5).

use std::{
    collections::HashMap,
    ffi::OsString,
    path::{Path, PathBuf},
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
    runtime::{
        CommandSpec, OutputEvent, OutputLimits, OutputStream, ProcessRuntime, RuntimeError,
        StdinMode,
    },
};

mod protocol;
mod run;

use run::{Clock, RunEntry};

pub const PROVIDER_ID: &str = "agy";
pub const EXTENSION_ID: &str = "google.google-antigravity";

/// `--app_data_dir=antigravity`에 전달하는 확장 데이터 디렉터리 이름입니다 (§7.5).
pub const EXTENSION_APP_DATA_DIR: &str = "antigravity";

/// 실환경 시험으로 기본 실행 경로를 확인한 CLI 버전입니다. 다른 버전은 `unverified`로 보고합니다.
const VERIFIED_CLI_VERSIONS: &[&str] = &["1.4.7"];

/// Stream JSON 한 줄의 상한. 상한을 넘으면 Protocol 오류로 처리합니다 (§7.3, REQ-003).
const OUTPUT_LIMITS: OutputLimits = OutputLimits {
    max_line_bytes: 4 * 1024 * 1024,
    max_buffered_bytes: 16 * 1024 * 1024,
};

/// 실행 Host의 VS Code `targetPlatform`에 맞는 확장 관리형 실행 파일 규칙입니다 (§7.5 표, PROV-01).
pub fn cli_rule(target_platform: &str) -> Option<ProviderCliRule> {
    let (os, _arch) = target_platform.split_once('-')?;
    let binary = match os {
        "win32" => "agy.exe",
        "linux" | "alpine" | "darwin" => "agy",
        _ => return None,
    };
    Some(ProviderCliRule {
        extension_id: EXTENSION_ID.into(),
        binary: CliBinaryRule::Managed {
            home_relative_path: PathBuf::from(".gemini/bin").join(binary),
        },
        binary_name: binary.into(),
        version_args: vec!["--version".into()],
    })
}

/// VS Code 확장(`google.google-antigravity`)의 대화 목록 표시 모드입니다 (Spec §7.6, DEC-08, AGENT-05).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AgyVsCodeListing {
    /// `--app_data_dir=antigravity`(`AgyDataDirMode::ExtensionShared`)를 전달해
    /// 확장과 같은 `~/.gemini/antigravity/conversation_summaries.db` 및 `conversations/<id>.db`에 기록합니다 (기본값).
    #[default]
    Listed,
    /// `--app_data_dir` 옵션을 생략(`AgyDataDirMode::DefaultCli`)해
    /// CLI 기본 디렉터리(`~/.gemini/antigravity-cli`)에 분리 기록합니다.
    /// 다만 확장 허브가 실행 중이면 확장 인덱스에 `source = CORTEX_TRAJECTORY_SOURCE_CLI`로 나타나는 것이 관찰되었고
    /// 확장 UI에서 실제로 숨겨지는지는 미검증입니다(`docs/phase-0/evidence/PH0-AGY/VSCODE_LISTING.md` §3.3).
    /// 디렉터리별로 로그인 상태가 분리되므로(`AGY-DATA-02`) 별도 CLI 로그인이 없으면 인증 실패로 보고합니다.
    Hidden,
}

impl AgyVsCodeListing {
    /// 표시 모드에 대응하는 데이터 디렉터리 모드를 돌려줍니다 (§7.5, §7.6).
    pub const fn data_dir_mode(self) -> AgyDataDirMode {
        match self {
            Self::Listed => AgyDataDirMode::ExtensionShared,
            Self::Hidden => AgyDataDirMode::DefaultCli,
        }
    }
}

impl From<AgyVsCodeListing> for AgyDataDirMode {
    fn from(listing: AgyVsCodeListing) -> Self {
        listing.data_dir_mode()
    }
}

/// Antigravity Managed 실행의 데이터 디렉터리 선택 규칙입니다 (§7.5, §7.6, AGY-DATA-01~05, DEC-08).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AgyDataDirMode {
    /// `--app_data_dir=antigravity`를 전달해 VS Code 확장의 로그인 상태와 대화 목록(`~/.gemini/antigravity`)을 공유합니다 ([`AgyVsCodeListing::Listed`]).
    #[default]
    ExtensionShared,
    /// `--app_data_dir` 옵션을 생략해 CLI 기본 디렉터리(`~/.gemini/antigravity-cli`)를 씁니다 ([`AgyVsCodeListing::Hidden`]).
    DefaultCli,
    /// 지정한 이름 또는 경로를 `--app_data_dir=<value>`로 전달합니다.
    Custom(OsString),
}

impl AgyDataDirMode {
    /// VS Code 대화 목록 표시 모드로 데이터 디렉터리 모드를 만듭니다 (§7.6, DEC-08).
    pub const fn from_vscode_listing(listing: AgyVsCodeListing) -> Self {
        listing.data_dir_mode()
    }

    /// 현재 데이터 디렉터리 모드가 대응하는 VS Code 대화 목록 표시 모드를 돌려줍니다 (§7.6).
    /// `Custom` 디렉터리는 표준 `listed`·`hidden` 모드에 해당하지 않으므로 `None`입니다.
    pub const fn vscode_listing(&self) -> Option<AgyVsCodeListing> {
        match self {
            Self::ExtensionShared => Some(AgyVsCodeListing::Listed),
            Self::DefaultCli => Some(AgyVsCodeListing::Hidden),
            Self::Custom(_) => None,
        }
    }

    pub fn cli_arg(&self) -> Option<OsString> {
        match self {
            Self::ExtensionShared => {
                Some(format!("--app_data_dir={EXTENSION_APP_DATA_DIR}").into())
            }
            Self::DefaultCli => None,
            Self::Custom(value) => {
                let mut arg = OsString::from("--app_data_dir=");
                arg.push(value);
                Some(arg)
            }
        }
    }
}

/// Run마다 Core가 정하는 실행 조건입니다. 경로·권한은 승인된 Workspace·Policy에서 나와야 합니다 (§7.2, §7.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgyLaunch {
    /// 검증된 Workspace 경로. 사용자 임의 경로를 그대로 쓰지 않습니다.
    pub cwd: PathBuf,
    pub prompt: String,
    /// `None`이면 CLI 기본 모델입니다 (예: `Some("gemini-3.8-flash-low".into())`).
    pub model: Option<String>,
    /// 추론 강도 (`low`, `medium`, `high`, `xhigh`, `max`).
    pub effort: Option<String>,
    /// 에이전트 실행 모드 (`accept-edits`, `auto`, `plan`). `None`이면 CLI 기본(`request-review`)입니다.
    /// `dangerously-skip-permissions`나 `bypassPermissions`는 거부합니다.
    pub mode: Option<String>,
    /// 추가 허용 디렉터리 (`--add-dir`).
    pub add_dirs: Vec<PathBuf>,
    /// 프롬프트 앞의 `/`가 로컬 슬래시 명령으로 해석되지 않게 막습니다 (`--disable-slash-commands`).
    pub disable_slash_commands: bool,
    /// 추가 CLI 인자. 보안·비대화형 규칙에 어긋나는 플래그는 거부합니다.
    pub extra_args: Vec<String>,
}

/// Task·Run에서 [`AgyLaunch`]를 만듭니다. Core가 승인된 Workspace·Policy·문맥 조립으로 구현합니다.
pub trait AgyLaunchResolver: Send + Sync {
    fn resolve(&self, task: &TaskSpec, context: &RunContext) -> Result<AgyLaunch, AdapterError>;
}

impl AgyLaunchResolver for AgyLaunch {
    fn resolve(&self, _task: &TaskSpec, _context: &RunContext) -> Result<AgyLaunch, AdapterError> {
        Ok(self.clone())
    }
}

pub struct AgyConfig {
    /// §7.5 탐지 결과의 실행 파일. Adapter 수명 동안 바꾸지 않습니다.
    pub executable: PathBuf,
    /// 탐지 결과의 CLI 버전 (예: `1.4.7`).
    pub cli_version: String,
    pub host_id: String,
    pub runtime: Arc<dyn ProcessRuntime>,
    pub launcher: Arc<dyn AgyLaunchResolver>,
    /// 취소 시 정상 종료를 기다리는 시간. 지나면 강제 종료합니다.
    pub cancel_grace: Duration,
    /// 데이터 디렉터리 모드 (§7.5, §7.6). 기본값 [`AgyDataDirMode::ExtensionShared`]는 [`AgyVsCodeListing::Listed`]와 같습니다.
    pub data_dir_mode: AgyDataDirMode,
    /// `--app_data_dir` 옵션을 쓰는 모드일 때 실행 전 버전 확인(§7.5 규칙 5)에서 옵션 수용이 확인되었는지 여부.
    /// `false`이면 다른 디렉터리로 조용히 바꾸지 않고 Capability를 `unsupported`로 둡니다.
    pub data_dir_option_supported: bool,
}

impl AgyConfig {
    /// VS Code 확장 대화 목록 표시 모드(`listed` 기본 / `hidden`)를 데이터 디렉터리 모드에 반영합니다 (§7.6, DEC-08).
    pub fn with_vscode_listing(mut self, listing: AgyVsCodeListing) -> Self {
        self.data_dir_mode = listing.data_dir_mode();
        self
    }

    /// 현재 데이터 디렉터리 모드가 대응하는 VS Code 대화 목록 표시 모드를 돌려줍니다 (§7.6).
    pub const fn vscode_listing(&self) -> Option<AgyVsCodeListing> {
        self.data_dir_mode.vscode_listing()
    }
}

pub struct AgyAdapter {
    config: Option<AgyConfig>,
    runs: Mutex<HashMap<String, Arc<RunEntry>>>,
    keys: Mutex<HashMap<String, String>>,
    clock: Clock,
}

impl Default for AgyAdapter {
    /// 실행 파일이 없는 Adapter. 모든 실행을 `unsupported`로 거부합니다.
    fn default() -> Self {
        Self::with_config(None)
    }
}

impl AgyAdapter {
    #[allow(private_interfaces)]
    pub fn new(config: AgyConfig) -> Self {
        Self::with_config(Some(config))
    }

    /// 실행 Host의 VS Code `targetPlatform`에 맞는 확장 실행 파일 규칙입니다 (§7.5 표).
    pub fn cli_rule(target_platform: &str) -> Option<ProviderCliRule> {
        cli_rule(target_platform)
    }

    /// 외부 호출자와 통합 시험에서 공개 타입만으로 `AgyAdapter`를 구성합니다.
    #[allow(clippy::too_many_arguments)]
    pub fn configured(
        executable: PathBuf,
        cli_version: String,
        host_id: String,
        runtime: Arc<dyn ProcessRuntime>,
        cancel_grace: Duration,
        cwd: PathBuf,
        prompt: String,
        model: Option<String>,
        effort: Option<String>,
        mode: Option<String>,
    ) -> Self {
        let launch = AgyLaunch {
            cwd,
            prompt,
            model,
            effort,
            mode,
            add_dirs: Vec::new(),
            disable_slash_commands: false,
            extra_args: Vec::new(),
        };
        Self::new(AgyConfig {
            executable,
            cli_version,
            host_id,
            runtime,
            launcher: Arc::new(launch),
            cancel_grace,
            data_dir_mode: AgyVsCodeListing::default().data_dir_mode(),
            data_dir_option_supported: true,
        })
    }

    /// VS Code 확장 대화 목록 표시 모드(`listed` 기본 / `hidden`)를 설정합니다 (Spec §7.6, DEC-08, AGENT-05).
    ///
    /// - [`AgyVsCodeListing::Listed`] (기본값): `--app_data_dir=antigravity`(`ExtensionShared`)
    /// - [`AgyVsCodeListing::Hidden`]: 옵션 생략(`DefaultCli`, `~/.gemini/antigravity-cli`)
    pub fn with_vscode_listing(mut self, listing: AgyVsCodeListing) -> Self {
        if let Some(config) = &mut self.config {
            config.data_dir_mode = listing.data_dir_mode();
        }
        self
    }

    /// 현재 Adapter에 설정된 VS Code 대화 목록 표시 모드를 돌려줍니다 (§7.6).
    pub fn vscode_listing(&self) -> Option<AgyVsCodeListing> {
        self.config.as_ref().and_then(AgyConfig::vscode_listing)
    }

    /// `--app_data_dir` 옵션을 생략하고 CLI 기본 디렉터리(`~/.gemini/antigravity-cli`, [`AgyVsCodeListing::Hidden`])를 쓰도록 설정합니다.
    pub fn with_default_cli_data_dir(self) -> Self {
        self.with_vscode_listing(AgyVsCodeListing::Hidden)
    }

    /// 지정한 `--app_data_dir=<value>` 값을 쓰도록 설정합니다.
    pub fn with_custom_data_dir(mut self, dir: impl Into<OsString>) -> Self {
        if let Some(config) = &mut self.config {
            config.data_dir_mode = AgyDataDirMode::Custom(dir.into());
        }
        self
    }

    /// 실행 전 버전 확인에서 `--app_data_dir` 옵션 수용 여부를 기록합니다 (§7.5).
    pub fn with_data_dir_option_supported(mut self, supported: bool) -> Self {
        if let Some(config) = &mut self.config {
            config.data_dir_option_supported = supported;
        }
        self
    }

    /// `--disable-slash-commands` 플래그 사용 여부를 설정합니다.
    #[allow(clippy::too_many_arguments)]
    pub fn with_disable_slash_commands(
        executable: PathBuf,
        cli_version: String,
        host_id: String,
        runtime: Arc<dyn ProcessRuntime>,
        cancel_grace: Duration,
        cwd: PathBuf,
        prompt: String,
        model: Option<String>,
        effort: Option<String>,
        mode: Option<String>,
        disable_slash_commands: bool,
        add_dirs: Vec<PathBuf>,
        extra_args: Vec<String>,
    ) -> Self {
        let launch = AgyLaunch {
            cwd,
            prompt,
            model,
            effort,
            mode,
            add_dirs,
            disable_slash_commands,
            extra_args,
        };
        Self::new(AgyConfig {
            executable,
            cli_version,
            host_id,
            runtime,
            launcher: Arc::new(launch),
            cancel_grace,
            data_dir_mode: AgyDataDirMode::ExtensionShared,
            data_dir_option_supported: true,
        })
    }

    /// `agy --app_data_dir=<dir> --version`을 실행해 비공개 데이터 디렉터리 옵션이 현재 CLI 버전에서
    /// 받아들여지는지 확인합니다 (Spec §7.5 규칙 5, AGY-DATA-01).
    pub async fn verify_extension_data_dir(
        runtime: &dyn ProcessRuntime,
        executable: &Path,
        cwd: &Path,
    ) -> Result<String, AdapterError> {
        Self::verify_data_dir_mode(runtime, executable, cwd, &AgyDataDirMode::ExtensionShared).await
    }

    #[allow(private_interfaces)]
    pub async fn verify_data_dir_mode(
        runtime: &dyn ProcessRuntime,
        executable: &Path,
        cwd: &Path,
        mode: &AgyDataDirMode,
    ) -> Result<String, AdapterError> {
        let mut args = Vec::new();
        if let Some(arg) = mode.cli_arg() {
            args.push(arg);
        }
        args.push("--version".into());
        let handle = runtime
            .spawn(CommandSpec {
                program: executable.to_path_buf(),
                args,
                cwd: cwd.to_path_buf(),
                env: vec![],
                stdin: StdinMode::Null,
                limits: OUTPUT_LIMITS,
            })
            .await
            .map_err(start_failed)?;

        let mut stdout_line = None;
        if let Ok(mut output) = handle.take_output() {
            let mut open = 2;
            while open > 0 {
                let Some(event) = output.next().await else {
                    break;
                };
                match event {
                    OutputEvent::Line {
                        stream: OutputStream::Stdout,
                        data,
                        ..
                    } if stdout_line.is_none() => {
                        if let Ok(text) = String::from_utf8(data) {
                            let trimmed = text.trim().to_owned();
                            if !trimmed.is_empty() {
                                stdout_line = Some(trimmed);
                            }
                        }
                    }
                    OutputEvent::Closed { .. } | OutputEvent::ReadError { .. } => open -= 1,
                    _ => {}
                }
            }
        }
        let status = handle.wait().await.map_err(start_failed)?;
        let _ = handle.terminate(Duration::ZERO).await;
        if status.code == Some(0)
            && let Some(version) = stdout_line
        {
            Ok(version)
        } else {
            Err(unsupported(
                "managed_run",
                "--app_data_dir option was rejected by the CLI; refusing to fall back to another data directory (§7.5)",
            ))
        }
    }

    fn with_config(config: Option<AgyConfig>) -> Self {
        Self {
            config,
            runs: Mutex::new(HashMap::new()),
            keys: Mutex::new(HashMap::new()),
            clock: Arc::new(timestamp),
        }
    }

    fn config(&self) -> Result<&AgyConfig, AdapterError> {
        self.config
            .as_ref()
            .ok_or_else(|| unsupported("start_run", "Antigravity CLI was not discovered"))
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

/// CLI 인자를 만듭니다. 프롬프트는 인자가 아니라 stdin(`--input-format stream-json`)으로 보냅니다.
fn command_args(
    config: &AgyConfig,
    launch: &AgyLaunch,
    context: &RunContext,
) -> Result<Vec<OsString>, AdapterError> {
    if config.data_dir_mode.cli_arg().is_some() && !config.data_dir_option_supported {
        return Err(unsupported(
            "managed_run",
            "--app_data_dir option is not supported by this Antigravity CLI version; refusing to fall back to another data directory (§7.5)",
        ));
    }

    if let Some(mode) = &launch.mode {
        let normalized = mode.trim();
        if normalized.eq_ignore_ascii_case("dangerously-skip-permissions")
            || normalized.eq_ignore_ascii_case("--dangerously-skip-permissions")
            || normalized.eq_ignore_ascii_case("bypassPermissions")
            || normalized.eq_ignore_ascii_case("bypass")
        {
            return Err(configuration(
                "skipping permissions is not allowed for managed Antigravity runs (§7.4)",
            ));
        }
        if !matches!(normalized, "accept-edits" | "auto" | "plan") {
            return Err(configuration(&format!(
                "unsupported Antigravity mode '{normalized}'; expected accept-edits, auto, or plan"
            )));
        }
    }

    if let Some(effort) = &launch.effort
        && !matches!(effort.as_str(), "low" | "medium" | "high" | "xhigh" | "max")
    {
        return Err(configuration(&format!(
            "unsupported Antigravity effort '{effort}'; expected low, medium, high, xhigh, or max"
        )));
    }

    for extra in &launch.extra_args {
        if extra.contains("dangerously-skip-permissions")
            || extra == "--hub"
            || extra.starts_with("--hub=")
            || extra == "--remote-control"
            || extra == "-i"
            || extra == "--prompt-interactive"
        {
            return Err(configuration(&format!(
                "disallowed Antigravity flag '{extra}' for managed runs"
            )));
        }
    }

    let mut args = Vec::new();
    if let Some(data_dir_arg) = config.data_dir_mode.cli_arg() {
        args.push(data_dir_arg);
    }

    match context.session.mode {
        SessionMode::Fresh | SessionMode::Capsule => {}
        SessionMode::Resume => {
            let source = context
                .session
                .source_session_id
                .as_deref()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| configuration("resume requires sourceSessionId"))?;
            args.extend(["--conversation".into(), source.into()]);
        }
        SessionMode::NativeFork => {
            return Err(unsupported(
                "native_fork",
                "Antigravity CLI headless print mode does not support conversation forking (/fork is rejected in print mode and --fork is absent)",
            ));
        }
    }

    args.extend([
        "--input-format".into(),
        "stream-json".into(),
        "--output-format".into(),
        "stream-json".into(),
    ]);

    if launch.disable_slash_commands {
        args.push("--disable-slash-commands".into());
    }
    if let Some(model) = &launch.model {
        args.extend(["--model".into(), model.into()]);
    }
    if let Some(effort) = &launch.effort {
        args.extend(["--effort".into(), effort.into()]);
    }
    if let Some(mode) = &launch.mode {
        args.extend(["--mode".into(), mode.into()]);
    }
    for dir in &launch.add_dirs {
        args.extend(["--add-dir".into(), dir.as_os_str().to_owned()]);
    }
    for extra in &launch.extra_args {
        args.push(extra.into());
    }
    Ok(args)
}

/// Antigravity `--input-format stream-json`의 사용자 메시지 한 줄입니다.
///
/// `agy` 1.4.7 `DecodeStreamInput`은 최상위 `"event": "user"`와 `"message": {"role": "user", "content": ...}`를 요구합니다.
fn prompt_line(prompt: &str) -> Vec<u8> {
    let mut line = serde_json::to_vec(&serde_json::json!({
        "event": "user",
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
impl AgentAdapter for AgyAdapter {
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

        if config.data_dir_mode.cli_arg().is_some() && !config.data_dir_option_supported {
            for name in [
                "managed_run",
                "event_stream",
                "cancel",
                "native_resume",
                "vscode_listing",
            ] {
                caps.insert(
                    name.into(),
                    capability(
                        CapabilityState::Unsupported,
                        EvidenceLevel::EnvironmentTested,
                        Some(
                            "--app_data_dir option is not accepted by this CLI version; refusing to fall back to another data directory (§7.5, §7.6)",
                        ),
                    ),
                );
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
                    Some("headless print mode has no live approval response path (§7.4)"),
                ),
            );
            caps.insert(
                "native_fork".into(),
                capability(
                    CapabilityState::Unsupported,
                    EvidenceLevel::EnvironmentTested,
                    Some("headless print mode rejects /fork and has no --fork flag (FORK-POC-01)"),
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
        // Spec §7.6 (DEC-08, AGENT-05): 운영자 설정이 `hidden`(`DefaultCli`)이어도 Capability는
        // 표시 수단(`--app_data_dir=antigravity`)의 지원 여부를 나타냅니다.
        let vscode_listing = if !config.data_dir_option_supported {
            capability(
                CapabilityState::Unsupported,
                EvidenceLevel::EnvironmentTested,
                Some(
                    "--app_data_dir option is not accepted by this CLI version; VS Code conversation listing requires --app_data_dir=antigravity (§7.5, §7.6)",
                ),
            )
        } else if self.verified() {
            capability(
                CapabilityState::Supported,
                EvidenceLevel::EnvironmentTested,
                Some(
                    "listed uses --app_data_dir=antigravity to record in ~/.gemini/antigravity/conversation_summaries.db (google.google-antigravity 1.9.0 / agy 1.4.7); hidden omits --app_data_dir (~/.gemini/antigravity-cli) where login state is isolated per directory and Linux Remote SSH uses <app_data_dir>/antigravity-oauth-token (AGY-DATA-02, §7.6)",
                ),
            )
        } else {
            capability(
                CapabilityState::Unverified,
                EvidenceLevel::ContractTested,
                Some(
                    "this CLI version has not been environment-tested for VS Code conversation listing (§7.6); hidden omits --app_data_dir (~/.gemini/antigravity-cli) where login state is isolated per directory (AGY-DATA-02)",
                ),
            )
        };
        caps.insert("vscode_listing".into(), vscode_listing);
        caps.insert(
            "native_resume".into(),
            capability(
                CapabilityState::Unverified,
                if self.verified() {
                    EvidenceLevel::EnvironmentTested
                } else {
                    EvidenceLevel::ContractTested
                },
                Some(
                    "Phase 0 --conversation experiment passed (PROV-05); P1 session ownership and invalid-session recovery pending",
                ),
            ),
        );
        caps.insert(
            "send_input".into(),
            capability(
                CapabilityState::Unsupported,
                EvidenceLevel::ContractTested,
                Some("follow-up turns are serialized as resumed runs after result consumption (§7.4)"),
            ),
        );
        caps.insert(
            "answer_approval".into(),
            capability(
                CapabilityState::Unsupported,
                EvidenceLevel::Documented,
                Some("headless print mode has no live approval response path; permissions are configured before start (§7.4)"),
            ),
        );
        caps.insert(
            "native_fork".into(),
            capability(
                CapabilityState::Unsupported,
                EvidenceLevel::EnvironmentTested,
                Some("headless print mode rejects /fork and has no --fork flag (FORK-POC-01)"),
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
        let args = command_args(config, &launch, context)?;
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
            Some(config.cli_version.clone()),
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
            "follow-up turns are serialized as resumed runs after result consumption (§7.4)",
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
            "headless print mode has no live approval response path (§7.4)",
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

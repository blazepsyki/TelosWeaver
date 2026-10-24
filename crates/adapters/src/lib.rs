//! Provider Adapter (Spec §7).
//!
//! 메서드가 존재한다는 것이 기능 지원을 뜻하지 않습니다. 미지원 제어는
//! [`AdapterError::UnsupportedCapability`]를 반환합니다.

use std::{collections::HashMap, ffi::OsString, fmt, path::PathBuf};

use async_trait::async_trait;
use futures::stream::BoxStream;
use telos_schema::{
    ApprovalAnswer, Capability, CliDiscovery, FailureKind, ProviderEvent, RunContext,
    RunInspection, RunStarted, TaskSpec,
};

mod agy;
pub mod claude;
mod codex;
mod discovery;
pub mod runtime;
mod unimplemented;

pub use agy::AgyAdapter;
pub use claude::ClaudeCodeAdapter;
pub use codex::CodexAdapter;
pub use discovery::NativeCliDiscoverer;
pub use unimplemented::UnsupportedAdapter;

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AdapterError {
    /// 이 Provider·버전·Host에서 지원하지 않거나 아직 구현하지 않은 제어입니다.
    UnsupportedCapability { capability: String, reason: String },
    /// §7.3 분류에 따른 실행 실패입니다. Provider 원본 오류는 `message`에 요약합니다.
    Failure { kind: FailureKind, message: String },
    /// 이 Adapter가 모르는 Run입니다. 재시작 뒤 추적을 잃은 경우도 포함합니다.
    UnknownRun { run_id: String },
    /// 같은 멱등 키로 다른 Task·Run의 시작을 요청했습니다.
    IdempotencyConflict { idempotency_key: String },
    /// 요청한 순서의 이벤트가 보존 범위를 벗어났습니다. `inspect_run` 후 다시 구독합니다.
    EventsUnavailable {
        run_id: String,
        after_sequence: u64,
        oldest_available: u64,
    },
}

impl AdapterError {
    /// 실행 실패로 분류할 수 있는 오류의 §7.3 분류입니다.
    pub fn failure_kind(&self) -> Option<FailureKind> {
        match self {
            Self::UnsupportedCapability { .. } => Some(FailureKind::UnsupportedCapability),
            Self::Failure { kind, .. } => Some(*kind),
            Self::UnknownRun { .. }
            | Self::IdempotencyConflict { .. }
            | Self::EventsUnavailable { .. } => None,
        }
    }
}

impl fmt::Display for AdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedCapability { capability, reason } => {
                write!(f, "unsupported capability '{capability}': {reason}")
            }
            Self::Failure { kind, message } => write!(f, "{kind:?}: {message}"),
            Self::UnknownRun { run_id } => write!(f, "unknown run '{run_id}'"),
            Self::IdempotencyConflict { idempotency_key } => {
                write!(
                    f,
                    "idempotency key '{idempotency_key}' was used for a different run"
                )
            }
            Self::EventsUnavailable {
                run_id,
                after_sequence,
                oldest_available,
            } => write!(
                f,
                "events after {after_sequence} of run '{run_id}' are no longer retained \
                 (oldest available: {oldest_available})"
            ),
        }
    }
}

impl std::error::Error for AdapterError {}

/// Provider CLI 탐지 (Spec §7.5, REQ-010).
///
/// - 각 Provider 모듈은 `pub fn cli_rule(target_platform: &str) -> Option<ProviderCliRule>`를 제공합니다.
///   실행 Host 플랫폼에 규칙이 없으면 `None`이며, 호출자는 `unavailable`(`unsupported_platform`)로 기록합니다.
/// - 공통 탐색(`extensions.json`·`.obsolete` 해석, 버전 폴더 비교, 파일 이름 검색, 해시)은
///   [`CliDiscoverer`] 구현이 담당합니다. 실제 구현은 PH0-HOST의 `discovery` 모듈입니다.
/// - Adapter는 탐지 결과의 `executable_path`를 생성자 등으로 받아 쓰며, 같은 Run 동안 바꾸지 않습니다 (§7.5-6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCliRule {
    /// VS Code 확장 ID (예: `openai.chatgpt`).
    pub extension_id: String,
    pub binary: CliBinaryRule,
    /// 파일 이름 검색과 PATH 대체에 쓰는 실행 파일 이름 (예: `codex.exe`).
    pub binary_name: OsString,
    /// 버전 명령 인자 (예: `--version`).
    pub version_args: Vec<OsString>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliBinaryRule {
    /// 확장 폴더 안의 상대 경로. 없으면 확장 폴더 안을 `binary_name`으로 찾고 후보가 하나일 때만 씁니다.
    InExtension { relative_path: PathBuf },
    /// 확장이 확장 폴더 밖에 내려받아 관리하는 실행 파일. [`DiscoveryRequest::home_dir`] 기준 상대 경로입니다.
    /// 확장이 설치되어 있을 때만 이 출처를 씁니다.
    Managed { home_relative_path: PathBuf },
}

/// 한 실행 Host에서의 탐지 요청. 경로는 모두 그 Host 기준입니다 (§3.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryRequest {
    pub host_id: String,
    /// 실행 Host의 VS Code `targetPlatform` (예: `win32-x64`, `linux-x64`).
    pub target_platform: String,
    /// 사용자·Host 설정의 명시 경로. 있으면 확인 실패 시에도 다른 출처로 넘어가지 않습니다.
    pub explicit_path: Option<PathBuf>,
    /// 우선순위 순서의 확장 폴더 (§7.5-1).
    pub extension_roots: Vec<PathBuf>,
    pub home_dir: PathBuf,
    /// PATH 대체에 쓸 값. `None`이면 PATH 대체를 하지 않습니다. 상대 경로 항목은 무시합니다.
    pub path_env: Option<OsString>,
    /// 결과의 `checked_at`에 그대로 씁니다 (RFC 3339).
    pub checked_at: String,
}

/// 공통 탐지 Port. 실패는 오류가 아니라 [`CliDiscovery`]의 `unavailable` 결과로 돌려줍니다.
///
/// - 해석 순서는 명시 경로 → 확장 → PATH입니다 (§7.5-4).
/// - `found`는 버전 명령 확인과 해시 계산이 끝난 결과입니다. 확인 전후 해시가 다르면 `changed_during_discovery`입니다.
/// - 실행 파일을 복사하거나 확장의 인증·상태 파일을 읽지 않습니다 (§7.5-7·8).
#[async_trait]
pub trait CliDiscoverer: Send + Sync {
    async fn discover(
        &self,
        provider_id: &str,
        rule: &ProviderCliRule,
        request: &DiscoveryRequest,
    ) -> CliDiscovery;
}

/// Core는 여러 Provider를 `Arc<dyn AgentAdapter>`로 다룹니다.
///
/// 구현은 아래 계약을 따릅니다 (G0 Adapter 실행·이벤트·취소 계약).
#[async_trait]
pub trait AgentAdapter: Send + Sync {
    fn id(&self) -> &str;

    /// 확인한 Capability만 보고합니다. 확인하지 못한 기능은 `unverified`로 두거나 생략합니다.
    async fn detect(&self, host_id: &str) -> Result<HashMap<String, Capability>, AdapterError>;

    /// - Run ID는 Core가 `context.run_id`로 발급합니다. 반환하는 `RunStarted::run_id`는 같은 값입니다.
    /// - 같은 `context.idempotency_key`와 같은 Run의 재요청은 처음의 `RunStarted`를 반환하고
    ///   프로세스를 새로 만들지 않습니다. 같은 키로 다른 Run을 요청하면 `IdempotencyConflict`입니다.
    /// - 반환 시점에 프로세스가 이미 끝났을 수 있습니다. 결과는 `events`로 확인합니다.
    /// - 오류를 반환한 Run은 이벤트를 남기지 않으며 그 Run의 `events`는 `UnknownRun`입니다.
    ///   반환 뒤 프로세스 시작이 실패하면 종료 `error`(`start_failed`)로 Run을 끝냅니다.
    async fn start_run(
        &self,
        task: &TaskSpec,
        context: &RunContext,
    ) -> Result<RunStarted, AdapterError>;

    /// - `sequence > after_sequence`인 이벤트를 순서대로 보냅니다. `after_sequence = 0`은 처음부터입니다.
    /// - 시작 직후부터 이벤트를 보존하여 구독 전에 끝난 Run도 회수할 수 있게 합니다 (§7.2).
    /// - 보존 범위를 벗어난 요청은 첫 항목으로 `EventsUnavailable`을 보냅니다.
    /// - Run의 종료 이벤트(`process_exited`, `turn_finished`, 종료 `error`)를 보낸 뒤 닫힙니다.
    ///   종료 규칙은 `telos_schema` events 모듈 문서(G0 개정 1 REQ-006)를 따르며
    ///   `telos_schema::check_run_events`로 Contract 시험합니다.
    /// - Payload 형식은 `telos_schema::ProviderEventPayload`를 따릅니다.
    fn events(
        &self,
        run_id: &str,
        after_sequence: u64,
    ) -> BoxStream<'static, Result<ProviderEvent, AdapterError>>;

    async fn send_input(
        &self,
        run_id: &str,
        input_id: &str,
        text: &str,
    ) -> Result<(), AdapterError>;

    /// 실행 중 응답 경로가 없는 Provider는 `UnsupportedCapability`를 반환합니다 (§7.4).
    async fn answer_approval(
        &self,
        run_id: &str,
        request_id: &str,
        decision: ApprovalAnswer,
    ) -> Result<(), AdapterError>;

    /// 반환값은 취소 요청 접수 여부이며 실제 종료가 아닙니다. `false`는 이미 끝난 Run입니다.
    /// 실제 종료는 `process_exited`(또는 `turn_finished`의 `cancelled`)와 `inspect_run`으로 확인합니다.
    async fn cancel_run(&self, run_id: &str) -> Result<bool, AdapterError>;

    /// 추적을 잃었으면 `Unknown`을 반환합니다. 성공으로 추정하지 않습니다.
    async fn inspect_run(&self, run_id: &str) -> Result<RunInspection, AdapterError>;
}

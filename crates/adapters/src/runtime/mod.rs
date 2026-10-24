//! 공용 프로세스 런타임 (Spec §19.5.2 공용 프로세스 경계). 소유: PH0-HOST.
//!
//! Provider Adapter는 CLI별 인자·Protocol·이벤트 의미 매핑만 담당하고, OS 프로세스의
//! 시작·입출력·종료·자식 회수는 이 모듈의 인터페이스로만 수행합니다 (§19.5.4-4).
//! Task 정책·DAG·최종 완료 판정은 이 모듈의 책임이 아닙니다.
//!
//! G0에서는 공개 인터페이스와 명시적인 Stub([`UnimplementedRuntime`])만 등록합니다.
//! G0 개정 1(REQ-002·003)에서 동시성·취소 안전성·출력 오류 계약을 확정했습니다.
//!
//! # 동시성과 취소 (REQ-002)
//! - [`ProcessHandle`]의 메서드는 모두 `&self`입니다. 같은 Handle을 `Arc`로 공유하여
//!   대기·입력·종료를 서로 다른 Task에서 동시에 호출할 수 있습니다.
//! - [`ProcessHandle::wait`]는 취소 안전합니다. Future를 버려도 프로세스 추적에 영향이 없고 다시 호출할 수 있으며,
//!   종료 후에는 항상 같은 [`ExitStatus`]를 반환합니다.
//! - [`ProcessHandle::terminate`]는 멱등입니다. 동시·반복 호출은 하나의 종료 절차를 공유하고 같은 보고를 받습니다.
//!   자연 종료와 경합하면 실제로 먼저 일어난 사건을 보고하며, 이미 끝난 프로세스에는 `Exited`를 보고합니다.
//! - 종료 후 `write_stdin`·`close_stdin`은 [`RuntimeError::StdinUnavailable`]입니다.
//!
//! # 출력 (REQ-003)
//! - 출력은 줄 단위(LF 기준, 줄바꿈 제외)로 전달하며 Protocol 해석은 Provider가 합니다.
//! - [`OutputLimits::max_line_bytes`]를 넘는 줄은 잘라서 전달하지 않습니다. 줄 전체를 버리고
//!   [`OutputEvent::LineTooLong`]을 보냅니다. Provider는 이를 Protocol 오류로 처리하고 정상 메시지로 해석하지 않습니다.
//! - 각 스트림은 정확히 한 번 [`OutputEvent::Closed`](EOF) 또는 [`OutputEvent::ReadError`]로 끝납니다.
//! - 소비되지 않은 출력이 [`OutputLimits::max_buffered_bytes`]에 도달하면 읽기를 멈춥니다(Backpressure).
//!   출력을 버리지 않으며, 소비자가 멈춰 자식이 쓰기에서 막혀도 `terminate`는 동작해야 합니다.
//! - 프로세스 종료([`ProcessHandle::wait`])와 출력 스트림 종료는 별개입니다. 남은 출력은 종료 후에도 소비할 수 있습니다.

use std::{ffi::OsString, fmt, path::PathBuf, time::Duration};

use async_trait::async_trait;
use futures::stream::BoxStream;

mod native;
pub use native::NativeRuntime;

/// 실행할 명령. 사용자 임의 경로를 검증 없이 cwd로 쓰지 않습니다 (§7.2). 경로 검증은 호출자 책임입니다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub cwd: PathBuf,
    /// `None` 값은 상속된 환경 변수를 제거합니다.
    pub env: Vec<(OsString, Option<OsString>)>,
    pub stdin: StdinMode,
    pub limits: OutputLimits,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StdinMode {
    Null,
    Piped,
}

/// 출력 상한 (§15.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputLimits {
    /// 한 줄의 최대 바이트 수(줄바꿈 제외). 초과한 줄은 [`OutputEvent::LineTooLong`]으로 보고합니다.
    pub max_line_bytes: usize,
    /// 소비되지 않은 출력의 최대 버퍼 바이트 수. 도달하면 읽기를 멈춥니다.
    pub max_buffered_bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutputStream {
    Stdout,
    Stderr,
}

/// 프로세스 출력 항목. `sequence`는 두 스트림이 공유하는 수신 순서이며 1부터 증가합니다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputEvent {
    Line {
        stream: OutputStream,
        sequence: u64,
        data: Vec<u8>,
    },
    /// 상한을 넘은 줄을 통째로 버렸습니다. 내용은 전달하지 않습니다.
    LineTooLong {
        stream: OutputStream,
        sequence: u64,
        total_bytes: u64,
    },
    /// 스트림이 정상 종료(EOF)했습니다. 이후 이 스트림의 항목은 없습니다.
    Closed { stream: OutputStream, sequence: u64 },
    /// 스트림 읽기가 실패했습니다. 이후 이 스트림의 항목은 없습니다. 프로세스 종료와는 별개입니다.
    ReadError {
        stream: OutputStream,
        sequence: u64,
        message: String,
    },
}

/// PID만으로 정체성을 확정하지 않습니다 (§14.3).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProcessIdentity {
    pub pid: u32,
    /// RFC 3339
    pub started_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminationCause {
    /// 프로세스가 스스로 종료했습니다.
    Exited,
    /// [`ProcessHandle::terminate`]의 정상 종료 요청으로 끝났습니다.
    TerminatedOnRequest,
    /// 유예 시간 뒤 강제 종료했습니다.
    Killed,
    /// 외부 요인(신호, OS)으로 끝났습니다.
    External,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExitStatus {
    pub code: Option<i32>,
    pub cause: TerminationCause,
}

/// 종료 확인 전에 Writer Lock을 해제하지 않도록 자식 회수 결과를 함께 보고합니다 (§7.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChildrenReaped {
    All,
    SomeRemaining,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminationReport {
    pub status: ExitStatus,
    pub children: ChildrenReaped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RuntimeError {
    ProgramNotFound {
        program: PathBuf,
    },
    SpawnFailed {
        program: PathBuf,
        message: String,
    },
    /// stdin이 Piped가 아니거나 이미 닫혔거나 프로세스가 끝났습니다.
    StdinUnavailable,
    OutputAlreadyTaken,
    Io {
        message: String,
    },
    /// 종료·자식 회수 결과를 확인할 수 없습니다. 성공으로 간주하지 않습니다.
    TerminationUnconfirmed {
        message: String,
    },
    Unsupported {
        operation: String,
        reason: String,
    },
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProgramNotFound { program } => {
                write!(f, "program not found: {}", program.display())
            }
            Self::SpawnFailed { program, message } => {
                write!(f, "failed to spawn {}: {message}", program.display())
            }
            Self::StdinUnavailable => {
                f.write_str("stdin is not piped, closed, or the process exited")
            }
            Self::OutputAlreadyTaken => f.write_str("output stream was already taken"),
            Self::Io { message } => write!(f, "process I/O error: {message}"),
            Self::TerminationUnconfirmed { message } => {
                write!(f, "process termination could not be confirmed: {message}")
            }
            Self::Unsupported { operation, reason } => {
                write!(f, "unsupported runtime operation '{operation}': {reason}")
            }
        }
    }
}

impl std::error::Error for RuntimeError {}

#[async_trait]
pub trait ProcessRuntime: Send + Sync {
    /// 시작 직후부터 출력을 버퍼에 보존하여 구독 전에 끝난 프로세스의 출력도 회수할 수 있게 합니다 (§7.2).
    async fn spawn(
        &self,
        spec: CommandSpec,
    ) -> Result<std::sync::Arc<dyn ProcessHandle>, RuntimeError>;
}

/// 실행 중인 프로세스 하나에 대한 공유 Handle입니다. 계약은 모듈 문서의 "동시성과 취소"를 따릅니다.
#[async_trait]
pub trait ProcessHandle: Send + Sync {
    fn identity(&self) -> &ProcessIdentity;

    /// 출력 스트림을 한 번만 가져갈 수 있습니다. 두 번째 호출은 `OutputAlreadyTaken`입니다.
    fn take_output(&self) -> Result<BoxStream<'static, OutputEvent>, RuntimeError>;

    async fn write_stdin(&self, data: &[u8]) -> Result<(), RuntimeError>;

    async fn close_stdin(&self) -> Result<(), RuntimeError>;

    /// 프로세스 종료를 기다립니다. 취소 안전하며 반복 호출할 수 있습니다.
    async fn wait(&self) -> Result<ExitStatus, RuntimeError>;

    /// 정상 종료를 요청하고 `grace` 뒤에도 살아 있으면 강제 종료합니다. 자식 프로세스까지 회수합니다.
    /// 멱등이며 동시 호출은 같은 보고를 받습니다. 결과를 확인할 수 없으면 `TerminationUnconfirmed`입니다.
    async fn terminate(&self, grace: Duration) -> Result<TerminationReport, RuntimeError>;
}

/// G0 Stub. PH0-HOST가 실제 구현으로 교체합니다.
#[derive(Debug, Default)]
pub struct UnimplementedRuntime;

#[async_trait]
impl ProcessRuntime for UnimplementedRuntime {
    async fn spawn(
        &self,
        _spec: CommandSpec,
    ) -> Result<std::sync::Arc<dyn ProcessHandle>, RuntimeError> {
        Err(RuntimeError::Unsupported {
            operation: "spawn".into(),
            reason: "process runtime not implemented yet".into(),
        })
    }
}

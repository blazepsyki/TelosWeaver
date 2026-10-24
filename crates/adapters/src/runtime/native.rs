//! Native process supervision (Spec §7.3, REQ-002/003, SAFE-03).

use super::*;
use async_trait::async_trait;
use futures::{
    FutureExt, Stream, StreamExt,
    future::{BoxFuture, Shared},
    task::AtomicWaker,
};
use std::{
    collections::VecDeque,
    fs::File,
    io::{self, Read, Write},
    pin::Pin,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicU8, Ordering},
    },
    task::{Context, Poll},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::watch;

#[cfg(windows)]
#[path = "windows.rs"]
mod platform;
#[cfg(unix)]
#[path = "unix.rs"]
mod platform;

#[derive(Debug, Default)]
pub struct NativeRuntime;

struct QueueState {
    events: VecDeque<(OutputEvent, usize)>,
    bytes: usize,
    sequence: u64,
    readers: usize,
    closed: bool,
}

struct OutputQueue {
    state: Mutex<QueueState>,
    space: Condvar,
    waker: AtomicWaker,
    limit: usize,
}

impl OutputQueue {
    fn push(&self, mut event: OutputEvent, charge: usize) -> bool {
        let mut state = self.state.lock().unwrap();
        while !state.closed && (state.bytes + charge > self.limit || state.events.len() >= 64) {
            state = self.space.wait(state).unwrap();
        }
        if state.closed {
            return false;
        }
        state.sequence += 1;
        let sequence = match &mut event {
            OutputEvent::Line { sequence, .. }
            | OutputEvent::LineTooLong { sequence, .. }
            | OutputEvent::Closed { sequence, .. }
            | OutputEvent::ReadError { sequence, .. } => sequence,
        };
        *sequence = state.sequence;
        state.bytes += charge;
        state.events.push_back((event, charge));
        drop(state);
        self.waker.wake();
        true
    }

    fn reader_done(&self) {
        self.state.lock().unwrap().readers -= 1;
        self.waker.wake();
    }
}

struct OutputReceiver(Arc<OutputQueue>);
impl Stream for OutputReceiver {
    type Item = OutputEvent;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut state = self.0.state.lock().unwrap();
        if let Some((event, charge)) = state.events.pop_front() {
            state.bytes -= charge;
            self.0.space.notify_all();
            return Poll::Ready(Some(event));
        }
        if state.readers == 0 {
            return Poll::Ready(None);
        }
        self.0.waker.register(cx.waker());
        Poll::Pending
    }
}
impl Drop for OutputReceiver {
    fn drop(&mut self) {
        self.0.state.lock().unwrap().closed = true;
        self.0.space.notify_all();
    }
}

fn collect_output(
    mut reader: impl Read,
    stream: OutputStream,
    queue: Arc<OutputQueue>,
    max_line: usize,
) {
    let mut chunk = [0; 4096];
    let mut line = Vec::with_capacity(max_line.min(4096));
    let mut length = 0_u64;
    let flush = |line: &mut Vec<u8>, length: &mut u64| {
        let event = if *length > max_line as u64 {
            line.clear();
            OutputEvent::LineTooLong {
                stream,
                sequence: 0,
                total_bytes: *length,
            }
        } else {
            OutputEvent::Line {
                stream,
                sequence: 0,
                data: std::mem::take(line),
            }
        };
        let charge = match &event {
            OutputEvent::Line { data, .. } => data.len(),
            _ => 0,
        };
        *length = 0;
        queue.push(event, charge)
    };
    'reading: loop {
        match reader.read(&mut chunk) {
            Ok(0) => {
                if length > 0 && !flush(&mut line, &mut length) {
                    break;
                }
                queue.push(
                    OutputEvent::Closed {
                        stream,
                        sequence: 0,
                    },
                    0,
                );
                break;
            }
            Ok(count) => {
                for byte in &chunk[..count] {
                    if *byte == b'\n' {
                        if !flush(&mut line, &mut length) {
                            break 'reading;
                        }
                    } else {
                        length = length.saturating_add(1);
                        if length <= max_line as u64 {
                            line.push(*byte);
                        }
                    }
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => {
                queue.push(
                    OutputEvent::ReadError {
                        stream,
                        sequence: 0,
                        message: error.to_string(),
                    },
                    0,
                );
                break;
            }
        }
    }
    queue.reader_done();
}

type ExitResult = Result<ExitStatus, RuntimeError>;
type StopFuture = Shared<BoxFuture<'static, Result<TerminationReport, RuntimeError>>>;

struct NativeHandle {
    identity: ProcessIdentity,
    control: Arc<platform::Control>,
    stdin: Arc<Mutex<Option<File>>>,
    output: Mutex<Option<OutputReceiver>>,
    exit: watch::Receiver<Option<ExitResult>>,
    cause: Arc<AtomicU8>,
    stopping: Mutex<Option<StopFuture>>,
}

impl Drop for NativeHandle {
    fn drop(&mut self) {
        // Dropping the last handle must not leave an unmanaged process tree.
        let _ = self.control.kill_tree();
    }
}

async fn exit_value(mut receiver: watch::Receiver<Option<ExitResult>>) -> ExitResult {
    receiver
        .wait_for(Option::is_some)
        .await
        .map_err(|error| RuntimeError::Io {
            message: error.to_string(),
        })?
        .as_ref()
        .unwrap()
        .clone()
}

fn io_error(error: io::Error) -> RuntimeError {
    RuntimeError::Io {
        message: error.to_string(),
    }
}

#[async_trait]
impl ProcessHandle for NativeHandle {
    fn identity(&self) -> &ProcessIdentity {
        &self.identity
    }

    fn take_output(&self) -> Result<BoxStream<'static, OutputEvent>, RuntimeError> {
        self.output
            .lock()
            .unwrap()
            .take()
            .map(StreamExt::boxed)
            .ok_or(RuntimeError::OutputAlreadyTaken)
    }

    async fn write_stdin(&self, data: &[u8]) -> Result<(), RuntimeError> {
        if self.exit.borrow().is_some() {
            return Err(RuntimeError::StdinUnavailable);
        }
        let stdin = self.stdin.clone();
        let data = data.to_vec();
        tokio::task::spawn_blocking(move || {
            let mut guard = stdin.lock().unwrap();
            let writer = guard.as_mut().ok_or(RuntimeError::StdinUnavailable)?;
            writer
                .write_all(&data)
                .and_then(|()| writer.flush())
                .map_err(io_error)
        })
        .await
        .map_err(|error| RuntimeError::Io {
            message: error.to_string(),
        })?
    }

    async fn close_stdin(&self) -> Result<(), RuntimeError> {
        if self.exit.borrow().is_some() {
            return Err(RuntimeError::StdinUnavailable);
        }
        let stdin = self.stdin.clone();
        tokio::task::spawn_blocking(move || {
            stdin
                .lock()
                .unwrap()
                .take()
                .ok_or(RuntimeError::StdinUnavailable)?;
            Ok(())
        })
        .await
        .map_err(|error| RuntimeError::Io {
            message: error.to_string(),
        })?
    }

    async fn wait(&self) -> ExitResult {
        exit_value(self.exit.clone()).await
    }

    async fn terminate(&self, grace: Duration) -> Result<TerminationReport, RuntimeError> {
        let future = {
            let mut stopping = self.stopping.lock().unwrap();
            stopping
                .get_or_insert_with(|| {
                    let control = self.control.clone();
                    let stdin = self.stdin.clone();
                    let exit = self.exit.clone();
                    let cause = self.cause.clone();
                    // Continue teardown even if every caller drops its terminate future.
                    let task = tokio::spawn(async move {
                        if exit.borrow().is_none() {
                            if control.try_wait().map_err(io_error)?.is_none() {
                                cause
                                    .compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst)
                                    .ok();
                                // A blocked stdin writer must never block termination.
                                if let Ok(mut writer) = stdin.try_lock() {
                                    writer.take();
                                }
                                control.request_stop().map_err(io_error)?;
                            }
                            if tokio::time::timeout(grace, exit_value(exit.clone()))
                                .await
                                .is_err()
                            {
                                if control.try_wait().map_err(io_error)?.is_none() {
                                    cause.store(2, Ordering::SeqCst);
                                }
                                control.kill_tree().map_err(io_error)?;
                            }
                        }
                        let status = tokio::time::timeout(Duration::from_secs(5), exit_value(exit))
                            .await
                            .map_err(|_| RuntimeError::TerminationUnconfirmed {
                                message: "process did not exit".into(),
                            })??;
                        if !control.tree_empty().map_err(io_error)? {
                            return Err(RuntimeError::TerminationUnconfirmed {
                                message: "process group is not empty".into(),
                            });
                        }
                        Ok(TerminationReport {
                            status,
                            children: ChildrenReaped::All,
                        })
                    });
                    async move {
                        task.await.map_err(|error| RuntimeError::Io {
                            message: error.to_string(),
                        })?
                    }
                    .boxed()
                    .shared()
                })
                .clone()
        };
        future.await
    }
}

pub(super) struct Spawned {
    control: Arc<platform::Control>,
    stdin: Option<File>,
    stdout: File,
    stderr: File,
    pid: u32,
}

#[async_trait]
impl ProcessRuntime for NativeRuntime {
    async fn spawn(&self, spec: CommandSpec) -> Result<Arc<dyn ProcessHandle>, RuntimeError> {
        if spec.limits.max_line_bytes == 0
            || spec.limits.max_buffered_bytes < spec.limits.max_line_bytes
        {
            return Err(RuntimeError::Io {
                message: "output limits require 0 < max_line_bytes <= max_buffered_bytes".into(),
            });
        }
        let limits = spec.limits;
        let program = spec.program.clone();
        let spawned = tokio::task::spawn_blocking(move || platform::spawn(spec))
            .await
            .map_err(|error| RuntimeError::Io {
                message: error.to_string(),
            })?
            .map_err(|error| {
                if error.kind() == io::ErrorKind::NotFound {
                    RuntimeError::ProgramNotFound {
                        program: program.clone(),
                    }
                } else {
                    RuntimeError::SpawnFailed {
                        program,
                        message: error.to_string(),
                    }
                }
            })?;
        let queue = Arc::new(OutputQueue {
            state: Mutex::new(QueueState {
                events: VecDeque::new(),
                bytes: 0,
                sequence: 0,
                readers: 2,
                closed: false,
            }),
            space: Condvar::new(),
            waker: AtomicWaker::new(),
            limit: limits.max_buffered_bytes,
        });
        let receiver = OutputReceiver(queue.clone());
        for (reader, stream) in [
            (spawned.stdout, OutputStream::Stdout),
            (spawned.stderr, OutputStream::Stderr),
        ] {
            let queue = queue.clone();
            std::thread::Builder::new()
                .name(format!("telos-output-{stream:?}"))
                .spawn(move || collect_output(reader, stream, queue, limits.max_line_bytes))
                .map_err(io_error)?;
        }
        let (sender, exit) = watch::channel(None);
        let cause = Arc::new(AtomicU8::new(0));
        let control = spawned.control.clone();
        let worker_cause = cause.clone();
        std::thread::Builder::new()
            .name("telos-process-wait".into())
            .spawn(move || {
                let result = control.wait().map_err(io_error).and_then(|status| {
                    // Collect descendants even when the parent exits without a cancel request.
                    control.kill_tree().map_err(io_error)?;
                    for _ in 0..200 {
                        if control.tree_empty().map_err(io_error)? {
                            let cause = match worker_cause.load(Ordering::SeqCst) {
                                1 => TerminationCause::TerminatedOnRequest,
                                2 => TerminationCause::Killed,
                                _ => status.cause,
                            };
                            return Ok(ExitStatus {
                                code: status.code,
                                cause,
                            });
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(RuntimeError::TerminationUnconfirmed {
                        message: "descendants did not exit".into(),
                    })
                });
                sender.send_replace(Some(result));
            })
            .map_err(io_error)?;
        Ok(Arc::new(NativeHandle {
            identity: ProcessIdentity {
                pid: spawned.pid,
                started_at: timestamp(),
            },
            control: spawned.control,
            stdin: Arc::new(Mutex::new(spawned.stdin)),
            output: Mutex::new(Some(receiver)),
            exit,
            cause,
            stopping: Mutex::new(None),
        }))
    }
}

fn timestamp() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let days = seconds / 86400;
    let z = days as i64 + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        seconds % 86400 / 3600,
        seconds % 3600 / 60,
        seconds % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    struct BrokenReader;
    impl Read for BrokenReader {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("injected read failure"))
        }
    }
    #[tokio::test]
    async fn io_failure_is_an_explicit_terminal_output_event() {
        let queue = Arc::new(OutputQueue {
            state: Mutex::new(QueueState {
                events: VecDeque::new(),
                bytes: 0,
                sequence: 0,
                readers: 1,
                closed: false,
            }),
            space: Condvar::new(),
            waker: AtomicWaker::new(),
            limit: 1024,
        });
        collect_output(BrokenReader, OutputStream::Stderr, queue.clone(), 1024);
        let events: Vec<_> = OutputReceiver(queue).collect().await;
        assert!(
            matches!(&events[..],[OutputEvent::ReadError {stream:OutputStream::Stderr,sequence:1,message}] if message=="injected read failure")
        );
    }
}

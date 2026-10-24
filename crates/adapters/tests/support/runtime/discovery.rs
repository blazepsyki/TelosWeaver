// Discovery fixtures use only public adapter interfaces (Spec §7.5, REQ-010).
#![allow(dead_code)]
use async_trait::async_trait;
use futures::{StreamExt, stream::BoxStream};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use telos_adapters::{
    CliBinaryRule, CliDiscoverer, DiscoveryRequest, NativeCliDiscoverer, ProviderCliRule,
    runtime::*,
};
use telos_schema::{CliDiscovery, CliDiscoveryFailure, CliDiscoveryOutcome, CliSource};

pub struct Files {
    pub root: PathBuf,
}
impl Files {
    pub fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "telos-discovery-{}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("home")).unwrap();
        Self { root }
    }
    pub fn write(&self, relative: impl AsRef<Path>, bytes: impl AsRef<[u8]>) -> PathBuf {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bytes).unwrap();
        path
    }
    pub fn binary(&self, relative: impl AsRef<Path>) -> PathBuf {
        let path = self.write(relative, b"test");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        path
    }
    pub fn request(&self) -> DiscoveryRequest {
        DiscoveryRequest {
            host_id: "host-fixture".into(),
            target_platform: "win32-x64".into(),
            explicit_path: None,
            extension_roots: vec![],
            home_dir: self.root.join("home"),
            path_env: None,
            checked_at: "2026-10-24T00:00:00Z".into(),
        }
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        assert!(
            self.root
                .starts_with(std::env::temp_dir().canonicalize().unwrap())
        );
        let _ = fs::remove_dir_all(&self.root);
    }
}
pub fn rule() -> ProviderCliRule {
    ProviderCliRule {
        extension_id: "sample.agent".into(),
        binary: CliBinaryRule::InExtension {
            relative_path: "bin/tool.exe".into(),
        },
        binary_name: "tool.exe".into(),
        version_args: vec!["--version".into()],
    }
}
#[derive(Debug, Clone, Copy, Default)]
pub enum Behavior {
    #[default]
    Version,
    Nonzero,
    SpawnFailure,
    ReadFailure,
    LongLine,
    InvalidUtf8,
    Empty,
    TooMuchOutput,
    ExternalExit,
    UnknownExit,
    Pending,
    MutateBinary,
}
#[derive(Default)]
pub struct Probe {
    pub programs: Mutex<Vec<PathBuf>>,
    pub terminations: Arc<AtomicUsize>,
    pub behavior: Behavior,
}
struct Handle {
    identity: ProcessIdentity,
    behavior: Behavior,
    terminations: Arc<AtomicUsize>,
}
#[async_trait]
impl ProcessRuntime for Probe {
    async fn spawn(&self, spec: CommandSpec) -> Result<Arc<dyn ProcessHandle>, RuntimeError> {
        assert_eq!(spec.args, vec![std::ffi::OsString::from("--version")]);
        assert_eq!(spec.stdin, StdinMode::Null);
        self.programs.lock().unwrap().push(spec.program.clone());
        if matches!(self.behavior, Behavior::SpawnFailure) {
            return Err(RuntimeError::Io {
                message: "fixture-secret".into(),
            });
        }
        if matches!(self.behavior, Behavior::MutateBinary) {
            fs::write(&spec.program, b"changed").unwrap();
        }
        Ok(Arc::new(Handle {
            identity: ProcessIdentity {
                pid: 1,
                started_at: "fixture".into(),
            },
            behavior: self.behavior,
            terminations: self.terminations.clone(),
        }))
    }
}
#[async_trait]
impl ProcessHandle for Handle {
    fn identity(&self) -> &ProcessIdentity {
        &self.identity
    }
    fn take_output(&self) -> Result<BoxStream<'static, OutputEvent>, RuntimeError> {
        if matches!(self.behavior, Behavior::Pending) {
            return Ok(futures::stream::pending().boxed());
        }
        let line = |sequence, data| OutputEvent::Line {
            stream: OutputStream::Stdout,
            sequence,
            data,
        };
        let events = match self.behavior {
            Behavior::ReadFailure => vec![OutputEvent::ReadError {
                stream: OutputStream::Stderr,
                sequence: 1,
                message: "fixture-secret".into(),
            }],
            Behavior::LongLine => vec![OutputEvent::LineTooLong {
                stream: OutputStream::Stdout,
                sequence: 1,
                total_bytes: 20000,
            }],
            Behavior::InvalidUtf8 => vec![line(1, vec![0xff])],
            Behavior::Empty => vec![],
            Behavior::TooMuchOutput => (1..=5)
                .map(|sequence| line(sequence, vec![b'x'; 16384]))
                .collect(),
            _ => vec![line(1, b"tool 1.2.3".to_vec())],
        };
        Ok(futures::stream::iter(events).boxed())
    }
    async fn wait(&self) -> Result<ExitStatus, RuntimeError> {
        if matches!(self.behavior, Behavior::UnknownExit) {
            return Err(RuntimeError::Io {
                message: "fixture-secret".into(),
            });
        }
        Ok(ExitStatus {
            code: Some(if matches!(self.behavior, Behavior::Nonzero) {
                1
            } else {
                0
            }),
            cause: if matches!(self.behavior, Behavior::ExternalExit) {
                TerminationCause::External
            } else {
                TerminationCause::Exited
            },
        })
    }
    async fn terminate(&self, _: Duration) -> Result<TerminationReport, RuntimeError> {
        self.terminations.fetch_add(1, Ordering::Relaxed);
        Ok(TerminationReport {
            status: ExitStatus {
                code: Some(1),
                cause: TerminationCause::Killed,
            },
            children: ChildrenReaped::All,
        })
    }
    async fn write_stdin(&self, _: &[u8]) -> Result<(), RuntimeError> {
        unreachable!()
    }
    async fn close_stdin(&self) -> Result<(), RuntimeError> {
        unreachable!()
    }
}
pub async fn discover(
    runtime: Arc<Probe>,
    rule: &ProviderCliRule,
    request: &DiscoveryRequest,
) -> CliDiscovery {
    let engine: Box<dyn CliDiscoverer> = Box::new(NativeCliDiscoverer::new(runtime));
    engine.discover("fixture-provider", rule, request).await
}
pub fn unavailable(
    record: &CliDiscovery,
    expected: CliDiscoveryFailure,
    expected_source: Option<CliSource>,
) {
    match &record.outcome {
        CliDiscoveryOutcome::Unavailable {
            failure,
            source,
            reason,
        } => {
            assert_eq!(*failure, expected);
            assert_eq!(*source, expected_source);
            assert!(!reason.contains("fixture-secret"));
        }
        other => panic!("expected unavailable, received {other:?}"),
    }
}

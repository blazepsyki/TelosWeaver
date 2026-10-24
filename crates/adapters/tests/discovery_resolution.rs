#[path = "../src/discovery/mod.rs"]
mod discovery;
use async_trait::async_trait;
use discovery::*;
use futures::{StreamExt, stream::BoxStream};
use runtime::*;
use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use telos_adapters::runtime;

#[derive(Default)]
struct Probe {
    programs: Mutex<Vec<PathBuf>>,
    fail: bool,
}
struct Handle {
    identity: ProcessIdentity,
    failed: bool,
}
#[async_trait]
impl ProcessRuntime for Probe {
    async fn spawn(&self, spec: CommandSpec) -> Result<Arc<dyn ProcessHandle>, RuntimeError> {
        self.programs.lock().unwrap().push(spec.program);
        assert_eq!(spec.args, vec![OsString::from("--version")]);
        Ok(Arc::new(Handle {
            identity: ProcessIdentity {
                pid: 1,
                started_at: "fixture".into(),
            },
            failed: self.fail,
        }))
    }
}
#[async_trait]
impl ProcessHandle for Handle {
    fn identity(&self) -> &ProcessIdentity {
        &self.identity
    }
    fn take_output(&self) -> Result<BoxStream<'static, OutputEvent>, RuntimeError> {
        Ok(futures::stream::iter(vec![OutputEvent::Line {
            stream: OutputStream::Stdout,
            sequence: 1,
            data: b"tool 1.2.3".to_vec(),
        }])
        .boxed())
    }
    async fn wait(&self) -> Result<ExitStatus, RuntimeError> {
        Ok(ExitStatus {
            code: Some(if self.failed { 1 } else { 0 }),
            cause: TerminationCause::Exited,
        })
    }
    async fn terminate(&self, _: Duration) -> Result<TerminationReport, RuntimeError> {
        Ok(TerminationReport {
            status: self.wait().await?,
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
struct Files(PathBuf);
impl Files {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "telos-resolution-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn binary(&self, path: &str) -> PathBuf {
        let path = self.0.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"fixture").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        path
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn rule<'a>(args: &'a [OsString]) -> DiscoveryRule<'a> {
    DiscoveryRule {
        extension_id: "sample.agent",
        relative_binary: Path::new("bin/tool.exe"),
        binary_name: OsStr::new("tool.exe"),
        version_args: args,
        managed_binary: None,
    }
}

#[tokio::test]
async fn explicit_extension_path_precedence_records_version_hash_and_source() {
    let files = Files::new();
    let explicit = files.binary("explicit.exe");
    let extension = files.binary("extensions/sample.agent-1.0.0/bin/tool.exe");
    let path = files.binary("path/tool.exe");
    let roots = [files.0.join("extensions")];
    let path_env = std::env::join_paths([path.parent().unwrap()]).unwrap();
    let args = [OsString::from("--version")];
    let runtime = Probe::default();
    let selected = resolve_cli(
        &runtime,
        Some(&explicit),
        &roots,
        "win32-x64",
        &path_env,
        rule(&args),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(selected.path, explicit.canonicalize().unwrap());
    assert_eq!(selected.source, SelectionSource::Explicit);
    assert_eq!(selected.cli_version, "tool 1.2.3");
    assert_eq!(selected.sha256, file_hash(&explicit).unwrap());
    assert!(selected.extension.is_none());
    let selected = resolve_cli(&runtime, None, &roots, "win32-x64", &path_env, rule(&args))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(selected.path, extension.canonicalize().unwrap());
    assert_eq!(selected.source, SelectionSource::Extension);
    assert_eq!(selected.extension.unwrap().version.to_string(), "1.0.0");
    let selected = resolve_cli(&runtime, None, &[], "win32-x64", &path_env, rule(&args))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(selected.path, path.canonicalize().unwrap());
    assert_eq!(selected.source, SelectionSource::Path);
    assert!(
        resolve_cli(
            &runtime,
            None,
            &[],
            "win32-x64",
            OsStr::new(""),
            rule(&args)
        )
        .await
        .unwrap()
        .is_none()
    );
}
#[tokio::test]
async fn explicit_invalid_path_and_failed_version_never_silently_fall_back() {
    let files = Files::new();
    let binary = files.binary("tool.exe");
    let args = [OsString::from("--version")];
    let runtime = Probe {
        fail: true,
        ..Default::default()
    };
    assert!(
        resolve_cli(
            &runtime,
            Some(&files.0.join("missing.exe")),
            &[],
            "win32-x64",
            OsStr::new(""),
            rule(&args)
        )
        .await
        .is_err()
    );
    assert!(runtime.programs.lock().unwrap().is_empty());
    assert!(
        resolve_cli(
            &runtime,
            Some(&binary),
            &[],
            "win32-x64",
            OsStr::new(""),
            rule(&args)
        )
        .await
        .is_err()
    );
}

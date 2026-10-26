use futures::StreamExt;
use std::{ffi::OsString, path::PathBuf, sync::OnceLock, time::Duration};
use telos_adapters::runtime::*;

fn fixture() -> PathBuf {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        let root = std::env::temp_dir().join(format!("telos-runtime-tests-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let output = root.join(if cfg!(windows) {
            "fixture.exe"
        } else {
            "fixture"
        });
        let source = root.join("fixture.rs");
        std::fs::write(&source, include_str!("fixtures/runtime/fixture.rs")).unwrap();
        assert!(
            std::process::Command::new("rustc")
                .arg(source)
                .arg("-o")
                .arg(&output)
                .status()
                .unwrap()
                .success()
        );
        output
    })
    .clone()
}
fn spec(mode: &str) -> CommandSpec {
    CommandSpec {
        program: fixture(),
        args: vec![mode.into()],
        cwd: std::env::temp_dir(),
        env: vec![],
        stdin: StdinMode::Piped,
        limits: OutputLimits {
            max_line_bytes: 1024,
            max_buffered_bytes: 4096,
        },
    }
}
async fn collect(handle: &std::sync::Arc<dyn ProcessHandle>) -> Vec<OutputEvent> {
    tokio::time::timeout(
        Duration::from_secs(5),
        handle.take_output().unwrap().collect(),
    )
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn early_exit_output_and_exit_code_are_retained() {
    let handle = NativeRuntime.spawn(spec("exit")).await.unwrap();
    assert_eq!(handle.wait().await.unwrap().code, Some(7));
    let events = collect(&handle).await;
    assert_eq!(events.len(), 2);
    assert!(matches!(
        handle.take_output(),
        Err(RuntimeError::OutputAlreadyTaken)
    ));
    assert_eq!(handle.wait().await.unwrap(), handle.wait().await.unwrap());
}
#[tokio::test(flavor = "multi_thread")]
async fn wait_does_not_block_stdin_or_repeatable_termination() {
    let handle = NativeRuntime.spawn(spec("echo")).await.unwrap();
    let waiting = tokio::spawn({
        let handle = handle.clone();
        async move { handle.wait().await }
    });
    handle
        .write_stdin("hello\n안녕하세요\n".as_bytes())
        .await
        .unwrap();
    handle.close_stdin().await.unwrap();
    let events = collect(&handle).await;
    assert_eq!(waiting.await.unwrap().unwrap().code, Some(0));
    let bytes: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            OutputEvent::Line {
                stream: OutputStream::Stdout,
                data,
                ..
            } => Some(data.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        bytes,
        vec![b"hello".to_vec(), "안녕하세요".as_bytes().to_vec()]
    );
    let one = handle.terminate(Duration::ZERO).await.unwrap();
    let two = handle.terminate(Duration::ZERO).await.unwrap();
    assert_eq!(one, two);
    assert_eq!(one.status.cause, TerminationCause::Exited);
}
#[tokio::test(flavor = "multi_thread")]
async fn cancelled_wait_can_be_retried_and_children_are_collected() {
    let handle = NativeRuntime.spawn(spec("tree")).await.unwrap();
    let mut stream = handle.take_output().unwrap();
    let first = tokio::time::timeout(Duration::from_secs(3), stream.next())
        .await
        .unwrap()
        .unwrap();
    let child = match first {
        OutputEvent::Line { data, .. } => String::from_utf8(data).unwrap().parse::<u32>().unwrap(),
        _ => panic!("missing child pid"),
    };
    assert!(child > 0);
    assert!(
        tokio::time::timeout(Duration::from_millis(30), handle.wait())
            .await
            .is_err()
    );
    let (one, two) = tokio::join!(
        handle.terminate(Duration::from_millis(30)),
        handle.terminate(Duration::from_secs(2))
    );
    assert_eq!(one.unwrap(), two.unwrap());
    assert_eq!(
        handle.wait().await.unwrap().cause,
        if cfg!(windows) {
            TerminationCause::Killed
        } else {
            TerminationCause::TerminatedOnRequest
        }
    );
    assert!(matches!(
        handle.write_stdin(b"late").await,
        Err(RuntimeError::StdinUnavailable)
    ));
    tokio::time::timeout(Duration::from_secs(3), stream.collect::<Vec<_>>())
        .await
        .unwrap();
}

// Spec §7.3, G1 HOST review: distinguish cooperative EOF from a forced Job teardown.
#[cfg(windows)]
#[tokio::test(flavor = "multi_thread")]
async fn windows_eof_exit_and_job_kill_report_actual_child_termination() {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::{
        Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT},
        System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
    };

    let cooperative = NativeRuntime.spawn(spec("echo")).await.unwrap();
    let report = cooperative.terminate(Duration::from_secs(2)).await.unwrap();
    assert_eq!(report.status.cause, TerminationCause::TerminatedOnRequest);
    assert_eq!(report.status.code, Some(0));
    assert_eq!(report.children, ChildrenReaped::All);
    collect(&cooperative).await;

    let handle = NativeRuntime.spawn(spec("tree")).await.unwrap();
    let mut stream = handle.take_output().unwrap();
    let event = tokio::time::timeout(Duration::from_secs(3), stream.next())
        .await
        .unwrap()
        .unwrap();
    let child_pid = match event {
        OutputEvent::Line { data, .. } => String::from_utf8(data).unwrap().parse::<u32>().unwrap(),
        _ => panic!("missing child pid"),
    };
    // Retain the original process handle so PID reuse cannot satisfy this check.
    let child = unsafe {
        let raw = OpenProcess(PROCESS_SYNCHRONIZE, 0, child_pid);
        assert!(!raw.is_null(), "{}", std::io::Error::last_os_error());
        OwnedHandle::from_raw_handle(raw)
    };
    assert_eq!(
        unsafe { WaitForSingleObject(child.as_raw_handle(), 0) },
        WAIT_TIMEOUT
    );
    let report = handle.terminate(Duration::from_millis(50)).await.unwrap();
    assert_eq!(report.status.cause, TerminationCause::Killed);
    assert_eq!(report.children, ChildrenReaped::All);
    // Confirm final OS teardown on the retained handle as well as Job accounting.
    assert_eq!(
        unsafe { WaitForSingleObject(child.as_raw_handle(), 3000) },
        WAIT_OBJECT_0
    );
    tokio::time::timeout(Duration::from_secs(3), stream.collect::<Vec<_>>())
        .await
        .unwrap();
}
#[tokio::test(flavor = "multi_thread")]
async fn output_backpressure_cannot_block_termination() {
    let handle = NativeRuntime.spawn(spec("flood")).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let report = tokio::time::timeout(
        Duration::from_secs(3),
        handle.terminate(Duration::from_millis(20)),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(report.children, ChildrenReaped::All);
    drop(handle.take_output().unwrap());
}
#[tokio::test(flavor = "multi_thread")]
async fn dropped_termination_future_still_reaps_the_process() {
    let handle = NativeRuntime.spawn(spec("sleep")).await.unwrap();
    {
        let terminating = handle.terminate(Duration::from_millis(100));
        futures::pin_mut!(terminating);
        let _ = futures::poll!(terminating);
    }
    let status = tokio::time::timeout(Duration::from_secs(3), handle.wait())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        status.cause,
        if cfg!(windows) {
            TerminationCause::Killed
        } else {
            TerminationCause::TerminatedOnRequest
        }
    );
    assert_eq!(
        handle.terminate(Duration::ZERO).await.unwrap().children,
        ChildrenReaped::All
    );
}
#[tokio::test(flavor = "multi_thread")]
async fn blocked_stdin_writer_cannot_block_termination() {
    let handle = NativeRuntime.spawn(spec("sleep")).await.unwrap();
    let writing = tokio::spawn({
        let handle = handle.clone();
        async move { handle.write_stdin(&vec![b'x'; 1024 * 1024]).await }
    });
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert_eq!(
        tokio::time::timeout(
            Duration::from_secs(3),
            handle.terminate(Duration::from_millis(20))
        )
        .await
        .unwrap()
        .unwrap()
        .children,
        ChildrenReaped::All
    );
    assert!(
        tokio::time::timeout(Duration::from_secs(3), writing)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
}
#[tokio::test(flavor = "multi_thread")]
async fn naturally_exited_parent_does_not_leave_its_child_running() {
    let handle = NativeRuntime.spawn(spec("orphan")).await.unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(3), handle.wait())
            .await
            .unwrap()
            .unwrap()
            .cause,
        TerminationCause::Exited
    );
    assert!(collect(&handle).await.iter().any(|event| matches!(
        event,
        OutputEvent::Line {
            stream: OutputStream::Stdout,
            ..
        }
    )));
}
#[tokio::test(flavor = "multi_thread")]
async fn oversized_line_is_not_forwarded_as_protocol_data() {
    let handle = NativeRuntime.spawn(spec("long")).await.unwrap();
    let events = collect(&handle).await;
    assert!(events.iter().any(|e| matches!(
        e,
        OutputEvent::LineTooLong {
            total_bytes: 20000,
            ..
        }
    )));
    assert!(
        events
            .iter()
            .any(|e| matches!(e,OutputEvent::Line {data,..} if data==b"tail"))
    );
    for (i, event) in events.iter().enumerate() {
        let sequence = match event {
            OutputEvent::Line { sequence, .. }
            | OutputEvent::LineTooLong { sequence, .. }
            | OutputEvent::ReadError { sequence, .. }
            | OutputEvent::Closed { sequence, .. } => *sequence,
        };
        assert_eq!(sequence, i as u64 + 1);
    }
}
#[tokio::test(flavor = "multi_thread")]
async fn native_argument_quoting_and_environment_override() {
    let mut command = spec("args");
    let args = ["space value", "a\"b", "trailing\\", "", "한글"];
    command.args.extend(args.iter().map(OsString::from));
    let handle = NativeRuntime.spawn(command).await.unwrap();
    let lines: Vec<_> = collect(&handle)
        .await
        .into_iter()
        .filter_map(|e| match e {
            OutputEvent::Line {
                stream: OutputStream::Stdout,
                data,
                ..
            } => Some(String::from_utf8(data).unwrap()),
            _ => None,
        })
        .collect();
    assert_eq!(lines, args);
    let mut command = spec("env");
    command
        .env
        .push(("TELOS_RUNTIME_TEST".into(), Some("test value".into())));
    let handle = NativeRuntime.spawn(command).await.unwrap();
    assert!(
        collect(&handle)
            .await
            .iter()
            .any(|e| matches!(e,OutputEvent::Line{data,..} if data==b"test value"))
    );
}
#[tokio::test(flavor = "multi_thread")]
async fn missing_program_and_invalid_limits_fail_before_spawn() {
    let mut command = spec("sleep");
    command.program = std::env::temp_dir().join("telos-missing-program-824ad.exe");
    assert!(matches!(
        NativeRuntime.spawn(command).await,
        Err(RuntimeError::ProgramNotFound { .. })
    ));
    let mut command = spec("sleep");
    command.limits.max_line_bytes = 0;
    assert!(NativeRuntime.spawn(command).await.is_err());
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn external_signal_is_not_reported_as_a_natural_exit() {
    let handle = NativeRuntime.spawn(spec("sleep")).await.unwrap();
    assert_eq!(
        unsafe { libc::kill(handle.identity().pid as i32, libc::SIGKILL) },
        0
    );
    let status = tokio::time::timeout(Duration::from_secs(3), handle.wait())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(status.code, None);
    assert_eq!(status.cause, TerminationCause::External);
}

// Spec §7.3, SAFE-03: a Unix group that ignores SIGTERM must be killed in full.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn unix_sigterm_resistance_escalates_and_removes_the_process_group() {
    let mut command = spec("sleep");
    command.program = "/bin/sh".into();
    command.args = vec!["-c".into(), "trap '' TERM; sleep 60 & echo $!; wait".into()];
    let handle = NativeRuntime.spawn(command).await.unwrap();
    let mut stream = handle.take_output().unwrap();
    let event = tokio::time::timeout(Duration::from_secs(3), stream.next())
        .await
        .unwrap()
        .unwrap();
    let child = match event {
        OutputEvent::Line { data, .. } => String::from_utf8(data).unwrap().parse::<i32>().unwrap(),
        _ => panic!("missing child pid"),
    };
    let group = handle.identity().pid as i32;
    assert_eq!(unsafe { libc::getpgid(group) }, group);
    assert_eq!(unsafe { libc::getpgid(child) }, group);
    let report = tokio::time::timeout(
        Duration::from_secs(5),
        handle.terminate(Duration::from_millis(50)),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(report.status.cause, TerminationCause::Killed);
    assert_eq!(report.children, ChildrenReaped::All);
    assert_eq!(unsafe { libc::kill(-group, 0) }, -1);
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ESRCH)
    );
    assert_eq!(unsafe { libc::kill(child, 0) }, -1);
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ESRCH)
    );
    tokio::time::timeout(Duration::from_secs(3), stream.collect::<Vec<_>>())
        .await
        .unwrap();
}

// Subprocess fixture: its abrupt death bypasses all runtime destructors (REQ-023).
#[test]
fn runtime_owner_fixture() {
    let Some(report) = std::env::var_os("TELOS_RUNTIME_OWNER_REPORT") else {
        return;
    };
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let mode = std::env::var("TELOS_RUNTIME_OWNER_MODE").unwrap();
        let mut command = spec(&mode);
        command.program = std::env::var_os("TELOS_RUNTIME_FIXTURE").unwrap().into();
        let handle = NativeRuntime.spawn(command).await.unwrap();
        let mut output = handle.take_output().unwrap();
        let event = output.next().await.unwrap();
        let child = match event {
            OutputEvent::Line { data, .. } => String::from_utf8(data).unwrap(),
            _ => panic!("missing child pid"),
        };
        std::fs::write(report, format!("{} {child}", handle.identity().pid)).unwrap();
        std::future::pending::<()>().await;
    });
}

fn process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::{Foundation::*, System::Threading::*};
        let process = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if process.is_null() {
            return false;
        }
        let alive = WaitForSingleObject(process, 0) == WAIT_TIMEOUT;
        CloseHandle(process);
        alive
    }
}

fn stop_fixture(pid: u32) {
    #[cfg(unix)]
    unsafe {
        libc::kill(pid as i32, libc::SIGKILL);
    }
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::{Foundation::CloseHandle, System::Threading::*};
        let process = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if !process.is_null() {
            TerminateProcess(process, 1);
            CloseHandle(process);
        }
    }
}

fn owner_crash(mode: &str) {
    struct Cleanup {
        owner: std::process::Child,
        pids: Vec<u32>,
        report: PathBuf,
    }
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = self.owner.kill();
            let _ = self.owner.wait();
            for &pid in &self.pids {
                if process_alive(pid) {
                    stop_fixture(pid);
                }
            }
            let _ = std::fs::remove_file(&self.report);
        }
    }
    let report = std::env::temp_dir().join(format!("telos-owner-{}-{mode}", std::process::id()));
    let _ = std::fs::remove_file(&report);
    let executable = fixture();
    let owner = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "runtime_owner_fixture", "--nocapture"])
        .env("TELOS_RUNTIME_OWNER_REPORT", &report)
        .env("TELOS_RUNTIME_OWNER_MODE", mode)
        .env("TELOS_RUNTIME_FIXTURE", executable)
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let mut cleanup = Cleanup {
        owner,
        pids: vec![],
        report,
    };
    let start = std::time::Instant::now();
    while !cleanup.report.exists() {
        assert!(
            cleanup.owner.try_wait().unwrap().is_none(),
            "fixture owner exited"
        );
        assert!(
            start.elapsed() < Duration::from_secs(15),
            "fixture ready timeout"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    cleanup.pids = std::fs::read_to_string(&cleanup.report)
        .unwrap()
        .split_whitespace()
        .map(|pid| pid.parse().unwrap())
        .collect();
    assert_eq!(cleanup.pids.len(), 2);
    assert!(cleanup.pids.iter().all(|&pid| process_alive(pid)));
    cleanup.owner.kill().unwrap();
    cleanup.owner.wait().unwrap();
    let start = std::time::Instant::now();
    while cleanup.pids.iter().any(|&pid| process_alive(pid))
        && start.elapsed() < Duration::from_secs(5)
    {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        cleanup.pids.iter().all(|&pid| !process_alive(pid)),
        "owner crash left processes: {:?}",
        cleanup.pids
    );
}

// Spec §3, §7.3, SAFE-03, REQ-023: no owner Drop can execute after SIGKILL/TerminateProcess.
#[test]
fn owner_crash_removes_provider_and_grandchild() {
    owner_crash("tree");
}

#[cfg(target_os = "linux")]
#[test]
fn owner_crash_removes_descendant_in_another_session() {
    owner_crash("detached-tree");
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn natural_exit_reaps_descendant_in_another_session() {
    let handle = NativeRuntime.spawn(spec("detached-orphan")).await.unwrap();
    let events = collect(&handle).await;
    let child = events
        .iter()
        .find_map(|event| match event {
            OutputEvent::Line { data, .. } => String::from_utf8_lossy(data).parse::<u32>().ok(),
            _ => None,
        })
        .unwrap();
    assert_eq!(handle.wait().await.unwrap().code, Some(0));
    assert!(!process_alive(child));
}

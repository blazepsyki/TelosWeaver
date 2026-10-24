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

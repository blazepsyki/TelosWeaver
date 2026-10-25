//! Real Linux daemon and private Unix socket (Spec §3.3, §19.1, SAFE-03, REQ-019).
#![cfg(target_os = "linux")]

use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::{
        fs::{FileTypeExt, MetadataExt, PermissionsExt},
        net::UnixStream,
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use telos_schema::{api::*, fixtures::*};

struct Daemon(Child);
impl Drop for Daemon {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}
fn private(path: &Path, expected: u32) {
    let metadata = std::fs::metadata(path).unwrap();
    assert_eq!(metadata.uid(), unsafe { libc::geteuid() });
    assert_eq!(metadata.permissions().mode() & 0o777, expected);
    println!(
        "mode={expected:04o} uid={} path={}",
        metadata.uid(),
        path.display()
    );
}
fn call(stream: &mut BufReader<UnixStream>, method: &str, params: Value) -> Value {
    let frame = encode_frame(&json!({
        "protocolVersion":API_PROTOCOL_VERSION,"requestId":method,"method":method,"params":params
    }))
    .unwrap();
    stream.get_mut().write_all(frame.as_bytes()).unwrap();
    let mut line = String::new();
    assert!(stream.read_line(&mut line).unwrap() > 0);
    let response: Value = serde_json::from_str(&line).unwrap();
    println!("response {response}");
    response
}

#[test]
#[ignore = "requires Linux XDG_RUNTIME_DIR; starts a real no-policy daemon, no model calls"]
fn real_daemon_xdg_permissions_auth_health_deny_runs_and_cleanup() {
    let root =
        std::env::temp_dir().join(telos_core::execution::new_id("telos-host-linux").unwrap());
    let project = root.join("project");
    std::fs::create_dir_all(&project).unwrap();
    let project = project.canonicalize().unwrap();
    let key = project_key(&normalize_project_root(project.to_str().unwrap(), false));
    let runtime = PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").unwrap()).join("telos-weaver");
    let socket = runtime.join(format!("{key}.sock"));
    let token_path = root.join("state/telos-weaver").join(&key).join("api-token");
    println!("temporary={} projectKey={key}", root.display());
    assert!(!socket.exists());
    let mut daemon = Daemon(
        Command::new(env!("CARGO_BIN_EXE_telos-weaver"))
            .args(["serve", "--project-root"])
            .arg(&project)
            .args(["--project-id", PROJECT_ID, "--host-id", HOST_ID])
            // Scope state isolation to this child; retain the real host runtime directory.
            .env("XDG_STATE_HOME", root.join("state"))
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    println!("daemonPid={}", daemon.0.id());
    let deadline = Instant::now() + Duration::from_secs(10);
    let stream = loop {
        assert!(
            daemon.0.try_wait().unwrap().is_none(),
            "daemon exited before ready"
        );
        if token_path.is_file()
            && let Ok(stream) = UnixStream::connect(&socket)
        {
            break stream;
        }
        assert!(Instant::now() < deadline, "daemon readiness timeout");
        std::thread::sleep(Duration::from_millis(20));
    };
    private(&runtime, 0o700);
    private(token_path.parent().unwrap(), 0o700);
    private(&token_path, 0o600);
    private(&socket, 0o600);
    assert!(std::fs::metadata(&socket).unwrap().file_type().is_socket());
    assert!(!token_path.starts_with(&project));
    let token = std::fs::read_to_string(&token_path).unwrap();
    assert_eq!(token.len(), 64);
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut stream = BufReader::new(stream);
    assert_eq!(
        call(&mut stream, "core.health", json!({}))["error"]["code"],
        "unauthenticated"
    );
    assert_eq!(
        call(
            &mut stream,
            "core.hello",
            json!({"clientName":"ph0-host-linux","clientVersion":"0","token":"wrong"})
        )["error"]["code"],
        "unauthenticated"
    );
    assert!(
        call(
            &mut stream,
            "core.hello",
            json!({"clientName":"ph0-host-linux","clientVersion":"0","token":token})
        )
        .get("result")
        .is_some()
    );
    let health = call(&mut stream, "core.health", json!({}));
    assert_eq!(health["result"]["status"], "degraded");
    let components = health["result"]["components"].as_array().unwrap();
    for name in ["execution", "scheduler", "persistence", "rn"] {
        assert!(
            components.iter().any(|c| c["name"] == name),
            "missing {name}"
        );
    }
    let rn = components.iter().find(|c| c["name"] == "rn").unwrap();
    assert_eq!(rn["status"], "degraded");
    assert_eq!(
        rn["detail"],
        "not configured (the operator policy has no rn section)"
    );
    let task = task_spec();
    let session = run_context(&task).session;
    let denied = call(
        &mut stream,
        "runs.start",
        json!(RunsStartParams {
            task,
            session,
            idempotency_key: "host-linux-deny".into()
        }),
    );
    assert_eq!(denied["error"]["code"], "unavailable");
    assert_eq!(
        denied["error"]["message"],
        "trusted run policy is not configured"
    );
    assert_eq!(
        unsafe { libc::kill(daemon.0.id() as i32, libc::SIGTERM) },
        0
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = daemon.0.try_wait().unwrap() {
            assert!(status.success());
            println!("daemonExit={status} socketRemoved={}", !socket.exists());
            break;
        }
        assert!(Instant::now() < deadline, "daemon shutdown timeout");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!socket.exists());
    drop(stream);
    drop(daemon);
    std::fs::remove_dir_all(&root).unwrap();
    println!("temporaryRemoved=true");
}

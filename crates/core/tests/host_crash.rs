//! Core crash with a real Claude foreground tool tree (Spec §3, §14.2, SAFE-03, REQ-023).

#[allow(dead_code)]
#[path = "phase0/client.rs"]
mod client;

use serde_json::{Value, json};
use std::{
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use telos_schema::{
    WorkspaceMode,
    api::{normalize_project_root, project_key},
    fixtures,
};

fn alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .ok()
            .and_then(|s| {
                s.rsplit_once(')')
                    .map(|(_, tail)| !tail.trim_start().starts_with('Z'))
            })
            .unwrap_or(false)
    }
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::{Foundation::*, System::Threading::*};
        let process = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if process.is_null() {
            return false;
        }
        let result = WaitForSingleObject(process, 0) == WAIT_TIMEOUT;
        CloseHandle(process);
        result
    }
}

fn kill_owned_probe(pid: u32) {
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

struct Cleanup {
    core: Child,
    pids: Vec<u32>,
}
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = self.core.kill();
        let _ = self.core.wait();
        for &pid in &self.pids {
            if alive(pid) {
                kill_owned_probe(pid);
            }
        }
    }
}

// Only this test's Core and reported tool ancestry are stopped, never user sessions.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "runs real Core and Claude haiku; requires Python on the host"]
async fn core_crash_terminates_real_provider_and_tool_descendants() {
    let root =
        std::env::temp_dir().join(telos_core::execution::new_id("telos-host-crash").unwrap());
    for name in ["project", "workspace", "state", "runtime"] {
        std::fs::create_dir_all(root.join(name)).unwrap();
    }
    let project = root.join("project").canonicalize().unwrap();
    let workspace = root.join("workspace").canonicalize().unwrap();
    let key = project_key(&normalize_project_root(
        project.to_str().unwrap(),
        cfg!(windows),
    ));
    let host = if cfg!(windows) {
        "windows-local"
    } else {
        "psyki-nas"
    };
    let policy = root.join("policy.json");
    std::fs::write(
        &policy,
        serde_json::to_vec(&json!({
            "revision":"req023-test", "permissionProfiles":["phase0"],
            "workspaces":[{"id":"ws-crash", "path":workspace, "writable":true}],
            "claude":{"models":["haiku"], "permissionMode":"acceptEdits", "allowedTools":["Bash"],
                "disallowedTools":["WebFetch","WebSearch"], "settingSources":"user"}
        }))
        .unwrap(),
    )
    .unwrap();
    std::fs::write(
        workspace.join("crash_probe.py"),
        include_str!("fixtures/host/crash_probe.py"),
    )
    .unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_telos-weaver"));
    command
        .args(["serve", "--project-root"])
        .arg(&project)
        .args([
            "--project-id",
            fixtures::PROJECT_ID,
            "--host-id",
            host,
            "--policy",
        ])
        .arg(&policy)
        .env("LOCALAPPDATA", root.join("state"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env("XDG_RUNTIME_DIR", root.join("runtime"))
        .stdout(Stdio::from(
            std::fs::File::create(root.join("core.stdout.log")).unwrap(),
        ))
        .stderr(Stdio::from(
            std::fs::File::create(root.join("core.stderr.log")).unwrap(),
        ));
    let mut cleanup = Cleanup {
        core: command.spawn().unwrap(),
        pids: vec![],
    };
    println!(
        "host={host} temporary={} corePid={}",
        root.display(),
        cleanup.core.id()
    );
    let token_path = root.join("state/telos-weaver").join(&key).join("api-token");
    let deadline = Instant::now() + Duration::from_secs(20);
    while !token_path.exists() {
        assert!(
            cleanup.core.try_wait().unwrap().is_none(),
            "Core exited: {}",
            root.display()
        );
        assert!(Instant::now() < deadline, "Core readiness timeout");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    #[cfg(windows)]
    let endpoint = format!(r"\\.\pipe\telos-weaver-{key}");
    #[cfg(unix)]
    let endpoint = root
        .join("runtime/telos-weaver")
        .join(format!("{key}.sock"))
        .to_str()
        .unwrap()
        .to_owned();
    let mut client = client::Client::connect(&endpoint).await;
    client
        .hello(std::fs::read_to_string(token_path).unwrap().trim())
        .await;
    let mut task = fixtures::task_spec();
    task.id = "req023-real".into();
    task.agent.provider = "claude".into();
    task.permission_profile_id = "phase0".into();
    task.workspace.mode = WorkspaceMode::Directory;
    task.workspace.host_id = host.into();
    task.workspace.workspace_id = Some("ws-crash".into());
    task.workspace.read_only = false;
    let python = if cfg!(windows) { "python" } else { "python3" };
    task.objective = format!(
        "Use Bash exactly once to run `{python} crash_probe.py {}` in the current working directory. Run it in the foreground and wait; do not use background execution or other tools. This disposable test intentionally waits.",
        cleanup.core.id()
    );
    let started = client
        .call(
            "runs.start",
            json!({"task":task,"session":{"mode":"fresh"},"idempotencyKey":"req023:1"}),
        )
        .await;
    let run_id = started["result"]["runId"]
        .as_str()
        .expect("Run started")
        .to_owned();
    client.subscribe(&run_id).await;
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let inspect = client.call("runs.inspect", json!({"runId":run_id})).await;
        assert_eq!(inspect["result"]["status"], "Running", "{inspect}");
        if workspace.join("probe.json").exists() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "tool did not report its tree: {}",
            root.display()
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    for notification in &client.notifications {
        if let Some(id) = notification["params"]["event"]["nativeSessionId"].as_str() {
            println!("claude-session: {id}");
            break;
        }
    }
    let probe: Value =
        serde_json::from_slice(&std::fs::read(workspace.join("probe.json")).unwrap()).unwrap();
    assert_eq!(probe["core"], cleanup.core.id());
    cleanup.pids = probe["pids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|pid| pid.as_u64().unwrap() as u32)
        .collect();
    assert!(
        cleanup.pids.len() >= 3,
        "Provider, tool and tool child must be present"
    );
    assert!(
        cleanup.pids.iter().all(|&pid| alive(pid)),
        "tree must be live before Core crash"
    );
    println!("runId={run_id} livePids={:?}", cleanup.pids);
    let start = Instant::now();
    cleanup.core.kill().unwrap();
    cleanup.core.wait().unwrap();
    while cleanup.pids.iter().any(|&pid| alive(pid)) && start.elapsed() < Duration::from_secs(5) {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let remaining: Vec<_> = cleanup
        .pids
        .iter()
        .copied()
        .filter(|&pid| alive(pid))
        .collect();
    println!(
        "core crash elapsedMs={} remainingPids={remaining:?}",
        start.elapsed().as_millis()
    );
    assert!(
        remaining.is_empty(),
        "Core crash left a running provider/tool tree"
    );
    // Keep isolated logs and the native session for INTEGRATION's merge-time cleanup.
}

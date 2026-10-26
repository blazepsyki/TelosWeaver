//! Unix process-group containment (Spec §7.3, SAFE-03).

use super::{CommandSpec, ExitStatus, Spawned, StdinMode, TerminationCause};
use std::{
    fs::File,
    io,
    os::{fd::OwnedFd, unix::process::CommandExt},
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[cfg(target_os = "linux")]
#[path = "linux_guard.rs"]
mod guard;

pub struct Control {
    child: Mutex<Child>,
    pgid: i32,
    status: Mutex<Option<ExitStatus>>,
    retired: AtomicBool,
    #[cfg(target_os = "linux")]
    receipt: Mutex<File>,
}
pub fn spawn(spec: CommandSpec) -> io::Result<Spawned> {
    let mut command = Command::new(spec.program.canonicalize()?);
    command
        .args(spec.args)
        .current_dir(spec.cwd.canonicalize()?)
        .stdin(if spec.stdin == StdinMode::Piped {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    for (key, value) in spec.env {
        if let Some(value) = value {
            command.env(key, value);
        } else {
            command.env_remove(key);
        }
    }
    #[cfg(target_os = "linux")]
    let mut receipt = guard::prepare(&mut command)?;
    let mut child = command.spawn()?;
    let pgid = child.id() as i32;
    #[cfg(target_os = "linux")]
    let pid = match guard::provider_pid(&mut receipt) {
        Ok(pid) => pid,
        Err(error) => {
            unsafe {
                libc::kill(-pgid, libc::SIGKILL);
            }
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };
    #[cfg(not(target_os = "linux"))]
    let pid = child.id();
    let stdin = child
        .stdin
        .take()
        .map(|pipe| File::from(OwnedFd::from(pipe)));
    let stdout = File::from(OwnedFd::from(child.stdout.take().unwrap()));
    let stderr = File::from(OwnedFd::from(child.stderr.take().unwrap()));
    Ok(Spawned {
        control: Arc::new(Control {
            child: Mutex::new(child),
            pgid,
            status: Mutex::new(None),
            retired: AtomicBool::new(false),
            #[cfg(target_os = "linux")]
            receipt: Mutex::new(receipt),
        }),
        stdin,
        stdout,
        stderr,
        pid,
    })
}
impl Control {
    pub fn try_wait(&self) -> io::Result<Option<ExitStatus>> {
        let mut cached = self.status.lock().unwrap();
        if cached.is_none()
            && let Some(status) = self.child.lock().unwrap().try_wait()?
        {
            #[cfg(target_os = "linux")]
            {
                use std::io::Read;
                let mut confirmed = [0];
                if self
                    .receipt
                    .lock()
                    .unwrap()
                    .read_exact(&mut confirmed)
                    .is_err()
                    || confirmed != [1]
                {
                    // A dead guard without its receipt cannot establish tree completion.
                    unsafe {
                        libc::kill(-self.pgid, libc::SIGKILL);
                    }
                    return Err(io::Error::other(
                        "Linux guard did not confirm descendant termination",
                    ));
                }
                self.retired.store(true, Ordering::Release);
            }
            *cached = Some(ExitStatus {
                code: status.code(),
                cause: if status.code().is_some() {
                    TerminationCause::Exited
                } else {
                    TerminationCause::External
                },
            });
        }
        Ok(*cached)
    }
    pub fn wait(&self) -> io::Result<ExitStatus> {
        loop {
            if let Some(code) = self.try_wait()? {
                return Ok(code);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn signal(&self, signal: i32) -> io::Result<()> {
        // Serialize signaling with reaping: the guard PID reserves the PGID
        // until try_wait records its receipt (Spec §14.3, REQ-023).
        let _status = self.status.lock().unwrap();
        if self.retired.load(Ordering::Acquire) {
            return Ok(());
        }
        if unsafe { libc::kill(-self.pgid, signal) } == 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ESRCH) {
            self.retired.store(true, Ordering::Release);
            Ok(())
        } else {
            Err(error)
        }
    }
    pub fn request_stop(&self) -> io::Result<()> {
        self.signal(libc::SIGTERM)
    }
    pub fn kill_tree(&self) -> io::Result<()> {
        self.signal(libc::SIGKILL)
    }
    pub fn tree_empty(&self) -> io::Result<bool> {
        if self.retired.load(Ordering::Acquire) {
            return Ok(true);
        }
        if unsafe { libc::kill(-self.pgid, 0) } == 0 {
            return Ok(false);
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ESRCH) {
            self.retired.store(true, Ordering::Release);
            Ok(true)
        } else {
            Err(error)
        }
    }
}
impl Drop for Control {
    fn drop(&mut self) {
        // A cancelled spawn future also drops this guard before a NativeHandle exists.
        let _ = self.kill_tree();
        let _ = self.child.get_mut().unwrap().wait();
    }
}

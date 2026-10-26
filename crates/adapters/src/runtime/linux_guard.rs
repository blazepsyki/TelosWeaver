//! Per-run subreaper (Spec §3, §7.3, SAFE-03, REQ-023).
//!
//! The Command child becomes a guard and forks the actual executable before exec.
//! Only syscall-style operations run after fork: no Rust locks, allocation or logging.
//! The guard's PID reserves the provider PGID until Core reaps the guard.

use std::{
    fs::File,
    io::{self, Read},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::process::CommandExt,
    },
    process::Command,
};

pub fn prepare(command: &mut Command) -> io::Result<File> {
    let mut fds = [-1; 2];
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let read = unsafe { File::from_raw_fd(fds[0]) };
    let write = unsafe { File::from_raw_fd(fds[1]) };
    let owner = unsafe { libc::getpid() };
    let owner_group = unsafe { libc::getpgrp() };
    let mut limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    if unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: enter uses only process-local scalar/stack data and async-signal-safe
    // libc/syscall operations. Only the executable child returns to Command's exec.
    unsafe {
        command.pre_exec(move || enter(owner, owner_group, write.as_raw_fd(), limit.rlim_max));
    }
    Ok(read)
}

pub fn provider_pid(report: &mut File) -> io::Result<u32> {
    let mut bytes = [0; 4];
    report.read_exact(&mut bytes)?;
    let pid = i32::from_ne_bytes(bytes);
    if pid <= 0 {
        return Err(io::Error::other("invalid Linux guard provider pid"));
    }
    Ok(pid as u32)
}

fn enter(owner: i32, owner_group: i32, report: i32, max_fd: libc::rlim_t) -> io::Result<()> {
    unsafe {
        if libc::getppid() != owner {
            return Err(io::Error::from_raw_os_error(libc::ECANCELED));
        }
        if libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) != 0 {
            return Err(io::Error::last_os_error());
        }
        let fd = libc::open(
            c"/proc/thread-self/children".as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC,
        );
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        libc::close(fd);
        libc::signal(libc::SIGCHLD, libc::SIG_DFL);
        let mut gate = [-1; 2];
        if libc::pipe2(gate.as_mut_ptr(), libc::O_CLOEXEC) != 0 {
            return Err(io::Error::last_os_error());
        }
        let provider = libc::fork();
        if provider < 0 {
            return Err(io::Error::last_os_error());
        }
        if provider == 0 {
            libc::close(report);
            libc::close(gate[1]);
            let mut armed = [0];
            let count = loop {
                let count = libc::read(gate[0], armed.as_mut_ptr().cast(), 1);
                if count < 0 && *libc::__errno_location() == libc::EINTR {
                    continue;
                }
                break count;
            };
            libc::close(gate[0]);
            if count != 1 || armed != [1] {
                return Err(io::Error::from_raw_os_error(libc::ECANCELED));
            }
            return Ok(());
        }
        // Leave the provider PG before it can be terminated. The guard remains a
        // direct Core child: getppid observes process death, not spawning-thread death.
        let separated = libc::setpgid(0, owner_group) == 0;
        for signal in [libc::SIGPIPE, libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
            libc::signal(signal, libc::SIG_IGN);
        }
        let bytes = provider.to_ne_bytes();
        let reported =
            libc::write(report, bytes.as_ptr().cast(), bytes.len()) == bytes.len() as isize;
        libc::close(gate[0]);
        if separated && reported && libc::getppid() == owner {
            libc::write(gate[1], [1_u8].as_ptr().cast(), 1);
        }
        libc::close(gate[1]);
        close_other_fds(report, max_fd);
        if separated && reported {
            loop {
                let mut info: libc::siginfo_t = std::mem::zeroed();
                if libc::getppid() != owner {
                    break;
                }
                let result = libc::waitid(
                    libc::P_PID,
                    provider as u32,
                    &mut info,
                    libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                );
                if result == 0 && info.si_pid() != 0 {
                    break;
                }
                if result < 0 && *libc::__errno_location() != libc::EINTR {
                    break;
                }
                pause();
            }
        }
        if separated {
            libc::kill(-libc::getpid(), libc::SIGKILL);
        }
        let mut status = 0;
        let confirmed = reap_tree(provider, &mut status);
        if confirmed {
            libc::write(report, [1_u8].as_ptr().cast(), 1);
        }
        libc::close(report);
        if !confirmed {
            libc::_exit(125);
        }
        if libc::WIFEXITED(status) {
            libc::_exit(libc::WEXITSTATUS(status));
        }
        if libc::WIFSIGNALED(status) {
            let signal = libc::WTERMSIG(status);
            libc::signal(signal, libc::SIG_DFL);
            let mut mask: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut mask);
            libc::sigaddset(&mut mask, signal);
            libc::sigprocmask(libc::SIG_UNBLOCK, &mask, std::ptr::null_mut());
            libc::kill(libc::getpid(), signal);
        }
        libc::_exit(125);
    }
}

fn close_other_fds(keep: i32, max_fd: libc::rlim_t) {
    unsafe {
        let first = libc::syscall(libc::SYS_close_range, 0_u32, keep as u32 - 1, 0_u32);
        let last = libc::syscall(libc::SYS_close_range, keep as u32 + 1, u32::MAX, 0_u32);
        if first != 0 || last != 0 {
            // Older kernels still work; obtain the bound before fork, never via std here.
            for fd in 0..max_fd.min(i32::MAX as u64) as i32 {
                if fd != keep {
                    libc::close(fd);
                }
            }
        }
    }
}

fn pause() {
    let delay = libc::timespec {
        tv_sec: 0,
        tv_nsec: 10_000_000,
    };
    unsafe {
        libc::nanosleep(&delay, std::ptr::null_mut());
    }
}

fn reap_tree(provider: i32, provider_status: &mut i32) -> bool {
    let mut saw_provider = false;
    loop {
        // children is not a stable snapshot. Re-read after adoption and never
        // declare completion from this file: only waitpid's ECHILD confirms it.
        if !kill_children() {
            return false;
        }
        loop {
            let mut status = 0;
            let pid = unsafe { libc::waitpid(-1, &mut status, libc::WNOHANG) };
            if pid > 0 {
                if pid == provider {
                    *provider_status = status;
                    saw_provider = true;
                }
                continue;
            }
            if pid == 0 {
                break;
            }
            let error = unsafe { *libc::__errno_location() };
            if error == libc::EINTR {
                continue;
            }
            return error == libc::ECHILD && saw_provider;
        }
        pause();
    }
}

fn kill_children() -> bool {
    unsafe {
        let fd = libc::open(
            c"/proc/thread-self/children".as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC,
        );
        if fd < 0 {
            return false;
        }
        let mut bytes = [0_u8; 4096];
        let mut pid = 0_i32;
        let result = loop {
            let count = libc::read(fd, bytes.as_mut_ptr().cast(), bytes.len());
            if count < 0 {
                if *libc::__errno_location() == libc::EINTR {
                    continue;
                }
                break false;
            }
            for &byte in &bytes[..count as usize] {
                if byte.is_ascii_digit() {
                    pid = pid.saturating_mul(10).saturating_add((byte - b'0') as i32);
                } else if pid > 0 {
                    libc::kill(pid, libc::SIGKILL);
                    pid = 0;
                }
            }
            if count == 0 {
                if pid > 0 {
                    libc::kill(pid, libc::SIGKILL);
                }
                break true;
            }
        };
        libc::close(fd);
        result
    }
}

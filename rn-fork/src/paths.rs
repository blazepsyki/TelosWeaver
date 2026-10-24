//! Where telos-rn keeps its state: `~/.telos-rn`, or `TELOS_RN_HOME` (handy for running two "machines" side by
//! side). On Windows, `~` is the profile folder.
//!
//! (TelosWeaver fork: the state never shares or moves over anything of an official Reason Network install, under
//! `~/.reason/network`, or of the engine's earlier name.)

use crate::AGENT_NAME;
use std::path::{Path, PathBuf};

/// The environment variables' prefix: `TELOS_RN_<name>`.
const ENV_PREFIX: &str = "TELOS_RN_";

pub fn user_home() -> PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// The name of a setting in the environment: `TELOS_RN_<name>`.
pub fn env_name(name: &str) -> String {
    format!("{ENV_PREFIX}{name}")
}

/// A setting from the environment: `TELOS_RN_<name>`.
pub fn env_var(name: &str) -> Result<String, std::env::VarError> {
    std::env::var(env_name(name))
}

pub fn env_var_os(name: &str) -> Option<std::ffi::OsString> {
    std::env::var_os(env_name(name))
}

pub fn home() -> PathBuf {
    env_var_os("HOME").map(PathBuf::from).unwrap_or_else(|| user_home().join(format!(".{AGENT_NAME}")))
}

/// `~/.telos-rn/network.sock`, unless that is too long for a Unix socket address (104 bytes on macOS): then a short
/// path in a private `/tmp/telos-rn-<uid>` directory, derived from the home directory so every client agrees on it.
#[cfg(unix)]
pub fn socket() -> PathBuf {
    socket_in(&home())
}

#[cfg(unix)]
fn socket_in(home: &Path) -> PathBuf {
    let path = home.join("network.sock");
    if path.as_os_str().len() < 100 {
        return path;
    }
    short_dir().join(format!("{:016x}.sock", hash(&path)))
}

/// A named pipe, named for the home directory so every client agrees on it.
#[cfg(windows)]
pub fn socket() -> PathBuf {
    pipe_in(&home())
}

/// Named for the directory as Windows knows it, not as it was spelled: `C:/Users/alex/.telos-rn` and
/// `c:\users\alex\.telos-rn\` are one directory, with one lock file, so they must be one pipe. (Before the
/// directory exists, as spelled; the daemon creating it names the pipe by what it then finds.)
#[cfg(windows)]
fn pipe_in(home: &Path) -> PathBuf {
    let dir = home.canonicalize().unwrap_or_else(|_| home.to_path_buf());
    let dir = dir.to_str().and_then(|s| s.strip_prefix(r"\\?\")).map_or(dir.clone(), PathBuf::from);
    PathBuf::from(format!(r"\\.\pipe\{AGENT_NAME}-{:016x}", hash(&dir)))
}

/// FNV-1a: stable across builds, unlike `DefaultHasher`.
fn hash(path: &Path) -> u64 {
    path.as_os_str()
        .as_encoded_bytes()
        .iter()
        .fold(0xcbf29ce484222325u64, |h, &b| (h ^ b as u64).wrapping_mul(0x100000001b3))
}

#[cfg(unix)]
fn short_dir() -> PathBuf {
    PathBuf::from(format!("/tmp/{AGENT_NAME}-{}", unsafe { libc::getuid() }))
}

/// Makes sure the socket's directory exists and that nobody else can get at it.
#[cfg(unix)]
pub fn ensure_socket_dir() -> std::io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    let dir = socket().parent().map(PathBuf::from).unwrap_or_else(home);
    if dir != short_dir() {
        return ensure_home();
    }
    let _ = std::fs::DirBuilder::new().mode(0o700).create(&dir);
    let meta = std::fs::symlink_metadata(&dir)?;
    if !meta.is_dir() || meta.uid() != unsafe { libc::getuid() } || meta.mode() & 0o077 != 0 {
        return Err(std::io::Error::other(format!("{} is not a private directory of yours; remove it", dir.display())));
    }
    Ok(())
}

pub fn db() -> PathBuf {
    home().join("network.db")
}

pub fn config() -> PathBuf {
    home().join("config.toml")
}

pub fn log() -> PathBuf {
    home().join("daemon.log")
}

/// Files that came with messages, one folder per message.
pub fn inbox() -> PathBuf {
    home().join("inbox")
}

pub fn lock() -> PathBuf {
    home().join("daemon.lock")
}

/// Claude Code's own directory (`CLAUDE_CONFIG_DIR`, else `~/.claude`).
pub fn claude_dir() -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from).unwrap_or_else(|| user_home().join(".claude"))
}

/// Claude Code's registry of live sessions, one `<pid>.json` per session.
pub fn claude_sessions() -> PathBuf {
    claude_dir().join("sessions")
}

/// Only for you: mode 0700 (on Windows, the profile folder already is).
pub fn ensure_home() -> std::io::Result<()> {
    let mut dir = std::fs::DirBuilder::new();
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut dir, 0o700);
    dir.recursive(true).create(home())
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_state_is_its_own() {
        // Never an official Reason Network install's state, nor its environment.
        let home = super::user_home();
        if super::env_var_os("HOME").is_none() {
            assert_eq!(super::home(), home.join(".telos-rn"));
        }
        assert_ne!(super::home(), home.join(".reason").join("network"));
        assert_eq!(super::env_name("HOME"), "TELOS_RN_HOME");
    }

    #[test]
    #[cfg(windows)]
    fn every_spelling_of_the_home_names_one_pipe() {
        let dir = std::env::temp_dir().join(format!("telos-rn-home-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let spelled = dir.to_str().unwrap();
        let slashes = spelled.replace('\\', "/");
        let shouting = format!("{}\\", spelled.to_uppercase());
        assert_eq!(super::pipe_in(&dir), super::pipe_in(std::path::Path::new(&slashes)));
        assert_eq!(super::pipe_in(&dir), super::pipe_in(std::path::Path::new(&shouting)));
        assert!(super::pipe_in(&dir).to_str().unwrap().starts_with(r"\\.\pipe\telos-rn-"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

//! The daemon's local socket: a Unix socket, or a named pipe on Windows.

use crate::paths;
use anyhow::{Context, Result};
use std::io;

#[cfg(unix)]
pub use tokio::net::UnixStream as Stream;
#[cfg(windows)]
pub use tokio::net::windows::named_pipe::NamedPipeClient as Stream;

/// The daemon's end of a client's connection.
#[cfg(unix)]
pub use tokio::net::UnixStream as Accepted;
#[cfg(windows)]
pub use tokio::net::windows::named_pipe::NamedPipeServer as Accepted;

#[cfg(unix)]
pub async fn connect() -> io::Result<Stream> {
    Stream::connect(paths::socket()).await
}

#[cfg(windows)]
pub async fn connect() -> io::Result<Stream> {
    use std::os::windows::io::AsRawHandle;
    use tokio::net::windows::named_pipe::ClientOptions;
    let mut tries = 0;
    let pipe = loop {
        match ClientOptions::new().open(paths::socket()) {
            // Every instance is taken for a moment while the daemon opens the next one.
            Err(e)
                if e.raw_os_error() == Some(windows_sys::Win32::Foundation::ERROR_PIPE_BUSY as i32) && tries < 20 =>
            {
                tries += 1
            }
            r => break r?,
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    };
    // Anyone on the machine can make a pipe by that name: talk only to the daemon that holds our lock.
    let mut server = 0;
    // SAFETY: asks about the pipe handle we hold.
    unsafe { windows_sys::Win32::System::Pipes::GetNamedPipeServerProcessId(pipe.as_raw_handle(), &mut server) };
    if crate::client::running_daemon() != Some(server) {
        return Err(io::Error::other(format!("{} is not this user's network daemon", paths::socket().display())));
    }
    Ok(pipe)
}

/// Where the daemon takes connections. Only the daemon holding the lock opens it. Only this OS user can connect
/// (TelosWeaver, PH0-RN-06: a user-only DACL on Windows, 0600 on Unix; see `private`).
pub struct Listener {
    #[cfg(unix)]
    inner: tokio::net::UnixListener,
    #[cfg(windows)]
    next: Accepted,
    #[cfg(windows)]
    path: std::path::PathBuf,
}

impl Listener {
    /// The agents' socket.
    pub fn bind() -> Result<Self> {
        Self::bind_at(paths::socket(), true)
    }

    /// The TelosWeaver RPC socket (PH0-RN-06).
    pub fn bind_rpc() -> Result<Self> {
        Self::bind_at(paths::rpc_socket(), false)
    }

    #[cfg(unix)]
    pub(crate) fn bind_at(sock: std::path::PathBuf, _agents: bool) -> Result<Self> {
        paths::ensure_socket_dir()?;
        let _ = std::fs::remove_file(&sock);
        let inner = tokio::net::UnixListener::bind(&sock).with_context(|| format!("binding {}", sock.display()))?;
        crate::private::protect(&sock)?;
        Ok(Listener { inner })
    }

    /// The first instance of the pipe, so nobody else holds its name.
    #[cfg(windows)]
    pub(crate) fn bind_at(path: std::path::PathBuf, _agents: bool) -> Result<Self> {
        let next = crate::private::pipe(&path, true).with_context(|| format!("creating {}", path.display()))?;
        Ok(Listener { next, path })
    }

    #[cfg(unix)]
    pub async fn accept(&mut self) -> io::Result<Accepted> {
        Ok(self.inner.accept().await?.0)
    }

    /// Each client gets an instance of its own: once one connects, the next waits for whoever comes after.
    #[cfg(windows)]
    pub async fn accept(&mut self) -> io::Result<Accepted> {
        loop {
            let connected = self.next.connect().await;
            let this = std::mem::replace(&mut self.next, crate::private::pipe(&self.path, false)?);
            // One that left before we got to it is dropped.
            if connected.is_ok() {
                return Ok(this);
            }
        }
    }
}

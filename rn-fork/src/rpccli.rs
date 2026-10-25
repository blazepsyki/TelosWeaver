//! `telos-rn rpc`: one TelosWeaver RPC request from the command line, for testing (PH0-RN-06). It reads where the RPC
//! is and its token from the state folder, says `rn.hello` (its result on stderr), sends the request and prints the
//! reply; with `--listen` it then prints the notifications that come for that long. Core speaks the RPC itself
//! (PH0-RN-07).

use crate::paths;
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::time::Duration;
use telos_schema::rn::{RN_PROTOCOL_VERSION, RnRpcInfo};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

pub async fn run(method: &str, params: Option<&str>, listen: u64) -> Result<()> {
    let info: RnRpcInfo = serde_json::from_str(
        &std::fs::read_to_string(paths::rpc_info()).context("no TelosWeaver RPC here (is the daemon running?)")?,
    )?;
    let token = std::fs::read_to_string(paths::rpc_token())?.trim().to_string();
    let params: Value = match params {
        Some(p) => serde_json::from_str(p).context("params are JSON")?,
        None => json!({}),
    };
    let stream = connect(&info).await?;
    let (rd, mut wr) = tokio::io::split(stream);
    let mut lines = BufReader::new(rd).lines();
    let mut send = async |id: &str, method: &str, params: Value| -> Result<()> {
        let frame =
            json!({"protocolVersion": RN_PROTOCOL_VERSION, "requestId": id, "method": method, "params": params});
        wr.write_all(format!("{frame}\n").as_bytes()).await?;
        Ok(())
    };
    let hello = json!({"clientName": "telos-rn rpc", "clientVersion": env!("CARGO_PKG_VERSION"), "token": token});
    send("hello", "rn.hello", hello).await?;
    send("1", method, params).await?;
    let mut failed = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30 + listen);
    let mut answered = false;
    let mut until = None;
    loop {
        let stop = until.unwrap_or(deadline);
        let Ok(next) = tokio::time::timeout_at(stop, lines.next_line()).await else { break };
        let Some(line) = next? else { break };
        let v: Value = serde_json::from_str(&line)?;
        match v.get("requestId").and_then(Value::as_str) {
            Some("hello") if v.get("error").is_some() => {
                println!("{}", serde_json::to_string_pretty(&v)?);
                bail!("rn.hello was refused");
            }
            // On stderr, apart from the reply: who this engine says it is, and Core's mailbox endpoint.
            Some("hello") => eprintln!("{}", serde_json::to_string(&v["result"])?),
            Some(_) => {
                println!("{}", serde_json::to_string_pretty(&v)?);
                failed = v.get("error").is_some();
                answered = true;
                if listen == 0 {
                    break;
                }
                until = Some(tokio::time::Instant::now() + Duration::from_secs(listen));
            }
            None => println!("{}", serde_json::to_string(&v)?),
        }
    }
    if !answered {
        bail!("no answer from the RPC");
    }
    if failed {
        std::process::exit(2);
    }
    Ok(())
}

#[cfg(unix)]
async fn connect(info: &RnRpcInfo) -> Result<tokio::net::UnixStream> {
    Ok(tokio::net::UnixStream::connect(&info.endpoint).await?)
}

/// The pipe, if the engine that wrote `rpc.json` holds it (anyone can make a pipe by that name).
#[cfg(windows)]
async fn connect(info: &RnRpcInfo) -> Result<tokio::net::windows::named_pipe::NamedPipeClient> {
    use std::os::windows::io::AsRawHandle;
    let pipe = tokio::net::windows::named_pipe::ClientOptions::new().open(&info.endpoint)?;
    let mut server = 0;
    // SAFETY: asks about the pipe handle we hold.
    unsafe { windows_sys::Win32::System::Pipes::GetNamedPipeServerProcessId(pipe.as_raw_handle(), &mut server) };
    if server != info.pid {
        bail!("{} is not held by the engine that wrote rpc.json", info.endpoint);
    }
    Ok(pipe)
}

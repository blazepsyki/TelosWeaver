//! TelosWeaver Core Daemon 진입점. Local Core API와 Scheduler는 아직 구현되지 않았습니다.

use std::process::ExitCode;

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("--version" | "-V") => {
            println!("{} {}", telos_core::SERVICE_ID, env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("{}: daemon is not implemented yet", telos_core::SERVICE_ID);
            ExitCode::FAILURE
        }
    }
}

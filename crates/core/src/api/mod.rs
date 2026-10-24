//! Local Core API 서버 (Spec §3.3, §17). 소유: PH0-HOST.
//! Wire types follow `telos_schema::api`; policy authorization is injected by the caller.

mod security;
mod server;

pub use server::{ApiServer, ApiServerConfig};

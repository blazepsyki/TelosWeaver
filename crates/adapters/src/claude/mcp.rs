//! Task MCP의 Run 단위 주입 (Spec §9.2, H0 D4, PH1-CLAUDE P4·L1).
//!
//! `RunContext.taskMcp`가 있으면 그 Run에만 `--mcp-config <파일>`로 MCP 서버 하나를 연결합니다.
//! - 파일은 자격 파일과 같은 디렉터리(Core 상태 디렉터리)에 만들고 Run이 끝나면 지웁니다. Workspace에는 쓰지 않습니다.
//! - 파일·인자에는 서버 명령과 자격 파일 **경로**만 들어갑니다. Adapter는 자격 파일을 열지 않습니다.
//! - 사용자·프로젝트의 MCP 설정은 읽지도 쓰지도 않습니다. Adapter는 항상 `--strict-mcp-config`로 실행하므로
//!   그 Run에는 주입한 서버만 연결됩니다(2.1.325 실험: 없으면 사용자 설정의 서버가 함께 연결됨).
//! - 주입한 서버의 도구만 `mcp__<서버 이름>` 규칙으로 추가 허용합니다.

use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use telos_schema::TaskMcpBinding;

use super::configuration;
use crate::AdapterError;

/// MCP 서버 프로세스에 자격 파일 경로를 넘기는 환경 변수입니다. 계약 상수를 그대로 씁니다 (REQ1-003).
pub use telos_schema::TASK_MCP_CREDENTIAL_FILE_ENV;

/// Run 하나의 주입 파일입니다. 버려질 때 파일을 지웁니다.
#[derive(Debug)]
pub(crate) struct Injection {
    config_path: PathBuf,
    server_name: String,
}

impl Injection {
    /// 연결 정보를 검사하고 설정 파일을 씁니다. 프로세스를 띄우기 전에 부릅니다.
    pub fn prepare(binding: &TaskMcpBinding, run_id: &str) -> Result<Self, AdapterError> {
        let name = &binding.server_name;
        // 도구 이름이 `mcp__<서버>__<도구>`이므로 서버 이름에 다른 문자를 받지 않습니다.
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            || name.contains("__")
        {
            return Err(configuration(
                "taskMcp.serverName must be ASCII letters, digits, '-' or single '_'",
            ));
        }
        if binding.command.is_empty() {
            return Err(configuration("taskMcp.command is empty"));
        }
        let credential = Path::new(&binding.credential_file);
        if !credential.is_absolute() {
            return Err(configuration(
                "taskMcp.credentialFile must be an absolute path",
            ));
        }
        // 존재만 확인합니다. 내용은 MCP 서버 프로세스만 읽습니다.
        if !credential.is_file() {
            return Err(configuration("taskMcp.credentialFile does not exist"));
        }
        let directory = credential
            .parent()
            .ok_or_else(|| configuration("taskMcp.credentialFile has no parent directory"))?;
        let safe_run: String = run_id
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        // 같은 Run의 시작 요청이 겹쳐도 서로의 파일을 지우지 않도록 프로세스 안에서 번호를 붙입니다.
        static SERIAL: AtomicU64 = AtomicU64::new(1);
        let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
        let config_path = directory.join(format!("claude-mcp-{safe_run}-{serial}.json"));
        let config = serde_json::json!({
            "mcpServers": {
                name: {
                    "type": "stdio",
                    "command": binding.command,
                    "args": binding.args,
                    "env": { TASK_MCP_CREDENTIAL_FILE_ENV: binding.credential_file },
                }
            }
        });
        write_private(&config_path, config.to_string().as_bytes()).map_err(|error| {
            configuration(&format!("could not write the task MCP config: {error}"))
        })?;
        Ok(Self {
            config_path,
            server_name: name.clone(),
        })
    }

    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    /// 주입한 서버의 도구 전체를 허용하는 권한 규칙입니다. 도구별 범위는 Core가 자격으로 집행합니다.
    pub fn allowed_tool(&self) -> String {
        format!("mcp__{}", self.server_name)
    }
}

impl Drop for Injection {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.config_path);
    }
}

/// 현재 사용자만 읽고 쓰는 파일로 씁니다. Windows에서는 상태 디렉터리의 권한을 따릅니다.
fn write_private(path: &Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(data)
}

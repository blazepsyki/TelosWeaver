//! 시험용 stdio MCP 서버 (PH1-CLAUDE P4·L1). 도구 하나(`telos_probe`)만 제공합니다.
//!
//! Core의 Task MCP 서버(`telos-weaver`의 stdio 하위 명령)가 아직 없어, Adapter의 Run 단위 주입을
//! 실환경에서 확인하는 데 씁니다. 표준 라이브러리만 쓰며 실환경 시험이 `rustc`로 빌드합니다.
//!
//! ```text
//! task_mcp_server --reply <도구 응답 문구> --log <호출 기록 파일>
//! ```
//!
//! 자격 파일 경로는 환경 변수 `TELOS_TASK_MCP_CREDENTIAL_FILE`로 받습니다. 파일을 읽을 수 있는지와 크기만
//! 기록하고 내용은 응답·기록에 넣지 않습니다.

use std::{
    fs::OpenOptions,
    io::{self, BufRead, Write},
};

const CREDENTIAL_ENV: &str = "TELOS_TASK_MCP_CREDENTIAL_FILE";
const TOOL: &str = "telos_probe";

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let value = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let reply = value("--reply").unwrap_or_else(|| "PROBE_OK".into());
    let log = value("--log");
    let record = |line: String| {
        if let Some(path) = &log
            && let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path)
        {
            let _ = writeln!(file, "{line}");
        }
    };
    record(format!("start {}", credential_state()));

    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let fields = members(&line);
        let field = |key: &str| fields.iter().find(|(k, _)| k == key).map(|(_, v)| *v);
        // `id`가 없으면 알림입니다. 응답하지 않습니다.
        let (Some(id), Some(method)) = (field("id"), field("method")) else {
            continue;
        };
        let params = field("params").map(members).unwrap_or_default();
        let param = |key: &str| params.iter().find(|(k, _)| k == key).map(|(_, v)| *v);
        let body = match method {
            "\"initialize\"" => format!(
                r#""result":{{"protocolVersion":{},"capabilities":{{"tools":{{}}}},"serverInfo":{{"name":"telos-task-mcp-fixture","version":"0.0.0"}}}}"#,
                param("protocolVersion").unwrap_or("\"2024-11-05\"")
            ),
            "\"ping\"" => r#""result":{}"#.to_owned(),
            "\"tools/list\"" => format!(
                r#""result":{{"tools":[{{"name":"{TOOL}","description":"Returns the TelosWeaver probe token. Takes no arguments.","inputSchema":{{"type":"object","properties":{{}},"additionalProperties":false}}}}]}}"#
            ),
            "\"tools/call\"" => {
                let name = param("name").unwrap_or("null");
                record(format!("call tool={name} {}", credential_state()));
                if name == format!("\"{TOOL}\"") {
                    format!(
                        r#""result":{{"content":[{{"type":"text","text":{}}}],"isError":false}}"#,
                        quote(&reply)
                    )
                } else {
                    r#""error":{"code":-32602,"message":"unknown tool"}"#.to_owned()
                }
            }
            _ => r#""error":{"code":-32601,"message":"method not found"}"#.to_owned(),
        };
        if writeln!(stdout, r#"{{"jsonrpc":"2.0","id":{id},{body}}}"#).is_err()
            || stdout.flush().is_err()
        {
            break;
        }
    }
    record("stop".into());
}

/// 자격 파일의 상태만 돌려줍니다. 내용은 다루지 않습니다.
fn credential_state() -> String {
    match std::env::var(CREDENTIAL_ENV) {
        Ok(path) => match std::fs::metadata(&path) {
            Ok(meta) => format!(
                "credential_env=set credential_readable={} credential_bytes={}",
                std::fs::File::open(&path).is_ok(),
                meta.len()
            ),
            Err(_) => "credential_env=set credential_readable=false credential_bytes=0".into(),
        },
        Err(_) => "credential_env=unset credential_readable=false credential_bytes=0".into(),
    }
}

fn quote(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// JSON 객체의 최상위 멤버를 (키, 값 원문)으로 나눕니다. 객체가 아니거나 형식이 틀리면 읽은 데까지만 돌려줍니다.
fn members(json: &str) -> Vec<(String, &str)> {
    let bytes = json.as_bytes();
    let mut out = Vec::new();
    let mut i = skip_space(bytes, 0);
    if bytes.get(i) != Some(&b'{') {
        return out;
    }
    i += 1;
    loop {
        i = skip_space(bytes, i);
        if bytes.get(i) != Some(&b'"') {
            return out;
        }
        let key_end = skip_value(bytes, i);
        let key = json[i + 1..key_end.saturating_sub(1).max(i + 1)].to_owned();
        i = skip_space(bytes, key_end);
        if bytes.get(i) != Some(&b':') {
            return out;
        }
        i = skip_space(bytes, i + 1);
        let end = skip_value(bytes, i);
        if end <= i {
            return out;
        }
        out.push((key, &json[i..end]));
        i = skip_space(bytes, end);
        if bytes.get(i) != Some(&b',') {
            return out;
        }
        i += 1;
    }
}

fn skip_space(bytes: &[u8], mut i: usize) -> usize {
    while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
        i += 1;
    }
    i
}

/// `i`에서 시작하는 값 하나의 끝(다음 위치)을 돌려줍니다.
fn skip_value(bytes: &[u8], mut i: usize) -> usize {
    match bytes.get(i) {
        Some(b'"') => {
            i += 1;
            while let Some(&b) = bytes.get(i) {
                i += if b == b'\\' { 2 } else { 1 };
                if b == b'"' {
                    return i;
                }
            }
            bytes.len()
        }
        Some(b'{' | b'[') => {
            let mut depth = 0_usize;
            while let Some(&b) = bytes.get(i) {
                match b {
                    b'"' => {
                        i = skip_value(bytes, i);
                        continue;
                    }
                    b'{' | b'[' => depth += 1,
                    b'}' | b']' => {
                        depth -= 1;
                        if depth == 0 {
                            return i + 1;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            bytes.len()
        }
        _ => {
            while bytes
                .get(i)
                .is_some_and(|b| !matches!(b, b',' | b'}' | b']') && !b.is_ascii_whitespace())
            {
                i += 1;
            }
            i
        }
    }
}

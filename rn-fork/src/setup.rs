//! `setup`: wires Reason Network into every agent on this machine (MCP server, plus hooks where the agent has them).
//! `setup --remove` undoes it. Every file it edits is backed up once, as `<file>.bak-telos-rn`. Also `doctor`.
//! (TelosWeaver fork: no login service; the daemon starts when a command needs it. Wiring is under `telos-rn` only:
//! an official Reason Network install's `reason-network` entries are never touched.)

use crate::{AGENT_NAME, CLI, agent, client::Link, paths, proto::*, render};
use anyhow::{Context, Result, anyhow};
use serde_json::{Map, Value, json};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

mod zcode;

/// What happened to one agent.
struct Report {
    agent: &'static str,
    lines: Vec<String>,
    notes: Vec<String>,
}

pub fn exe() -> Result<PathBuf> {
    let exe = crate::util::current_exe()?;
    // Windows reports it in full already; its canonical form (`\\?\C:\…`) is not for other programs' configs.
    if cfg!(windows) {
        return Ok(exe);
    }
    Ok(exe.canonicalize()?)
}

/// The command line agents run through a shell: `'/path/to/telos-rn' hook codex stop`. On Windows that shell is
/// PowerShell (`& 'C:\Users\Alex Smith\…\telos-rn.exe' hook codex stop`), and a path that needs no quotes also runs in
/// cmd.exe.
pub fn shell_cmd(exe: &Path, args: &str) -> String {
    let p = exe.display().to_string();
    let plain = |extra: &str| p.chars().all(|c| c.is_ascii_alphanumeric() || extra.contains(c));
    let quoted = if cfg!(windows) {
        if plain(r"\:._-") { p.clone() } else { format!("& '{}'", p.replace('\'', "''")) }
    } else if plain("/._-") {
        p.clone()
    } else {
        format!("'{}'", p.replace('\'', "'\\''"))
    };
    format!("{quoted} {args}")
}

fn home() -> PathBuf {
    paths::user_home()
}

fn codex_home() -> PathBuf {
    std::env::var_os("CODEX_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".codex"))
}

fn gemini_home() -> PathBuf {
    std::env::var_os("GEMINI_CLI_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".gemini"))
}

fn opencode_home() -> PathBuf {
    home().join(".config").join("opencode")
}

fn pi_home() -> PathBuf {
    std::env::var_os("PI_CODING_AGENT_DIR").map(PathBuf::from).unwrap_or_else(|| home().join(".pi").join("agent"))
}

fn read_json(path: &Path) -> Result<Value> {
    match std::fs::read_to_string(path) {
        Ok(s) if s.trim().is_empty() => Ok(json!({})),
        Ok(s) => serde_json::from_str(&s).with_context(|| format!("{} is not plain JSON", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(e.into()),
    }
}

/// Writes `content` if it differs, after backing up the original once. Returns whether anything changed.
fn write_file(path: &Path, content: &str, dry: bool) -> Result<bool> {
    if std::fs::read_to_string(path).ok().as_deref() == Some(content) {
        return Ok(false);
    }
    if dry {
        return Ok(true);
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let backup = PathBuf::from(format!("{}.bak-telos-rn", path.display()));
    // A file that already runs this binary was backed up (or created) when it was first wired in.
    let wired = exe().is_ok_and(|exe| has(path.to_path_buf(), &me(&exe)));
    if path.exists() && !backup.exists() && !wired {
        std::fs::copy(path, &backup)?;
    }
    // Write then rename, so an agent never reads a half-written file.
    let tmp = PathBuf::from(format!("{}.tmp-telos-rn", path.display()));
    std::fs::write(&tmp, content)?;
    // Keep its permissions (a config holding tokens may be 0600).
    if let Ok(meta) = std::fs::metadata(path) {
        std::fs::set_permissions(&tmp, meta.permissions())?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(true)
}

fn write_json(path: &Path, v: &Value, dry: bool) -> Result<bool> {
    // Removing Reason Network from a config that never existed leaves an empty shell: don't create a file for that.
    if !path.exists() && hollow(v) {
        return Ok(false);
    }
    write_file(path, &(serde_json::to_string_pretty(v)? + "\n"), dry)
}

/// Only empty objects and arrays.
fn hollow(v: &Value) -> bool {
    match v {
        Value::Object(o) => o.values().all(hollow),
        Value::Array(a) => a.is_empty(),
        Value::Null => true,
        _ => false,
    }
}

/// `v` as a JSON object (creating it if absent); anything else is left alone and reported.
fn obj(v: &mut Value) -> Result<&mut Map<String, Value>> {
    if v.is_null() {
        *v = json!({});
    }
    if !v.is_object() {
        return Err(anyhow!("expected a JSON object, found {v}; not touching it"));
    }
    Ok(v.as_object_mut().expect("checked above"))
}

/// Hook groups in the Claude Code style (`{matcher, hooks: [{type, command, ...}]}`), used by Codex and Gemini.
/// Replaces any earlier groups of ours (or the upstream engine's) for `agent`, keeps everything else.
fn merge_hook_groups(hooks: &mut Map<String, Value>, agent: &str, groups: Vec<(&str, Value)>, remove: bool) {
    let marker = format!(" hook {agent} ");
    let mine = |v: &Value| v.to_string().contains(&marker);
    for (_, list) in hooks.iter_mut() {
        if let Some(arr) = list.as_array_mut() {
            // In a group, drop only our handlers (someone may have added theirs to the same group).
            for g in arr.iter_mut() {
                if let Some(inner) = g.get_mut("hooks").and_then(Value::as_array_mut) {
                    inner.retain(|h| !mine(h));
                }
            }
            arr.retain(|g| match g.get("hooks").and_then(Value::as_array) {
                Some(inner) => !inner.is_empty(),
                None => !mine(g),
            });
        }
    }
    hooks.retain(|_, list| list.as_array().is_none_or(|a| !a.is_empty()));
    if remove {
        return;
    }
    for (event, group) in groups {
        let list = hooks.entry(event.to_string()).or_insert_with(|| json!([]));
        if let Some(arr) = list.as_array_mut() {
            arr.push(group);
        }
    }
}

fn changed(path: &Path, did: bool) -> String {
    let p = crate::util::tilde(&path.display().to_string());
    if did { format!("updated {p}") } else { format!("{p} unchanged") }
}

// ---- Claude Code ----

fn claude(exe: &Path, remove: bool, dry: bool) -> Result<Option<Report>> {
    let Some(bin) = agent::find_bin("claude") else { return Ok(None) };
    let mut r = Report { agent: "Claude Code", lines: Vec::new(), notes: Vec::new() };
    // A settings file it can't read is left alone: the rest of the setup still goes ahead.
    if let Err(e) = claude_allow_ask(remove, dry, &mut r) {
        r.notes.push(format!("couldn't allow ask_user in Claude Code's settings ({e:#}): it asks once to allow it"));
    }
    let get = |name: &str| -> Result<(bool, bool)> {
        let out = Command::new(&bin).args(["mcp", "get", name]).stdin(std::process::Stdio::null()).output()?;
        let existing = out.status.success();
        Ok((existing, existing && contains(&String::from_utf8_lossy(&out.stdout), &exe.display().to_string())))
    };
    let drop = |name: &str| -> Result<()> {
        if !dry {
            Command::new(&bin)
                .args(["mcp", "remove", name, "--scope", "user"])
                .stdin(std::process::Stdio::null())
                .output()?;
        }
        Ok(())
    };
    let (existing, current) = get(AGENT_NAME)?;
    if remove || (existing && !current) {
        if existing {
            drop(AGENT_NAME)?;
        }
        if remove {
            r.lines.push(if existing {
                format!("removed the {AGENT_NAME} MCP server")
            } else {
                "nothing to remove".into()
            });
            return Ok(Some(r));
        }
    }
    if current {
        r.lines.push(format!("{AGENT_NAME} MCP server already set"));
    } else {
        if !dry {
            let out = Command::new(&bin)
                .args(["mcp", "add", "--scope", "user", AGENT_NAME, "--"])
                .arg(exe)
                .arg("mcp")
                .stdin(std::process::Stdio::null())
                .output()?;
            if !out.status.success() {
                return Err(anyhow!("claude mcp add failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
            }
        }
        r.lines.push(format!("added the {AGENT_NAME} MCP server (user scope)"));
    }
    Ok(Some(r))
}

/// `ask_user` only shows the person a pop-up, so it is allowed: it never waits on a prompt in a terminal nobody is
/// watching. The other tools keep Claude Code's own permission prompts.
fn claude_allow_ask(remove: bool, dry: bool, r: &mut Report) -> Result<()> {
    let rule = format!("mcp__{AGENT_NAME}__ask_user");
    let path = paths::claude_dir().join("settings.json");
    let mut v = read_json(&path)?;
    let perms = obj(obj(&mut v)?.entry("permissions").or_insert_with(|| json!({})))?;
    let Some(allow) = perms.entry("allow").or_insert_with(|| json!([])).as_array_mut() else {
        return Err(anyhow!("{}: permissions.allow isn't a list", path.display()));
    };
    if remove {
        allow.retain(|x| *x != rule);
    } else if !allow.iter().any(|x| *x == rule) {
        allow.push(json!(rule));
    }
    // Nothing of the user's own left: no empty lists behind.
    if allow.is_empty() {
        perms.remove("allow");
    }
    if perms.is_empty() {
        obj(&mut v)?.remove("permissions");
    }
    r.lines.push(changed(&path, write_json(&path, &v, dry)?));
    Ok(())
}

// ---- Codex ----

fn codex(exe: &Path, remove: bool, dry: bool) -> Result<Option<Report>> {
    let dir = codex_home();
    if agent::find_bin("codex").is_none() && !dir.exists() {
        return Ok(None);
    }
    let mut r = Report { agent: "Codex", lines: Vec::new(), notes: Vec::new() };

    let cfg_path = dir.join("config.toml");
    let text = std::fs::read_to_string(&cfg_path).unwrap_or_default();
    let mut doc: toml_edit::DocumentMut = text.parse().with_context(|| format!("parsing {}", cfg_path.display()))?;
    if remove && let Some(servers) = doc.get_mut("mcp_servers").and_then(|t| t.as_table_like_mut()) {
        servers.remove(AGENT_NAME);
    }
    if !remove {
        let servers = doc.entry("mcp_servers").or_insert(toml_edit::table());
        if let Some(t) = servers.as_table_mut() {
            t.set_implicit(true);
        }
        let mut t = toml_edit::Table::new();
        t["command"] = toml_edit::value(exe.display().to_string());
        t["args"] = toml_edit::value(toml_edit::Array::from_iter(["mcp"]));
        let mut env = toml_edit::InlineTable::new();
        env.insert("TELOS_RN_AGENT", "codex".into());
        if let Ok(h) = crate::paths::env_var("HOME") {
            env.insert("TELOS_RN_HOME", h.into());
        }
        t["env"] = toml_edit::value(env);
        // Room for check_inbox(wait_secs) long-polls.
        t["tool_timeout_sec"] = toml_edit::value(660);
        t["default_tools_approval_mode"] = toml_edit::value("approve");
        servers[AGENT_NAME] = toml_edit::Item::Table(t);
    }
    r.lines.push(changed(&cfg_path, write_file(&cfg_path, &doc.to_string(), dry)?));

    let hooks_path = dir.join("hooks.json");
    let mut v = read_json(&hooks_path)?;
    let group = |event: &str, matcher: bool, timeout: u64| {
        let mut g = json!({ "hooks": [{ "type": "command", "command": shell_cmd(exe, &format!("hook codex {event}")), "timeout": timeout }] });
        if matcher {
            g["matcher"] = json!("*");
        }
        g
    };
    let groups = vec![
        ("SessionStart", group("session-start", true, 10)),
        ("UserPromptSubmit", group("user-prompt-submit", false, 10)),
        ("PostToolUse", group("post-tool-use", true, 10)),
        ("Stop", group("stop", false, 10)),
        ("SessionEnd", group("session-end", false, 3)),
    ];
    merge_hook_groups(obj(obj(&mut v)?.entry("hooks").or_insert_with(|| json!({})))?, "codex", groups, remove);
    let did = write_json(&hooks_path, &v, dry)?;
    r.lines.push(changed(&hooks_path, did));
    if did && !remove {
        r.notes.push("Codex asks once to trust Reason Network's hooks: choose \"Trust all and continue\".".into());
    }
    Ok(Some(r))
}

// ---- Grok CLI ----

fn grok_home() -> PathBuf {
    std::env::var_os("GROK_HOME").filter(|h| !h.is_empty()).map(PathBuf::from).unwrap_or_else(|| home().join(".grok"))
}

/// grok reads the MCP server from `config.toml` and Claude-Code-style hook groups from `hooks/*.json` (global hook
/// files are always trusted, so no prompt).
fn grok(exe: &Path, remove: bool, dry: bool) -> Result<Option<Report>> {
    let dir = grok_home();
    if agent::find_bin("grok").is_none() && !dir.exists() {
        return Ok(None);
    }
    let mut r = Report { agent: "Grok CLI", lines: Vec::new(), notes: Vec::new() };

    let cfg_path = dir.join("config.toml");
    let text = std::fs::read_to_string(&cfg_path).unwrap_or_default();
    let mut doc: toml_edit::DocumentMut = text.parse().with_context(|| format!("parsing {}", cfg_path.display()))?;
    if remove {
        if let Some(servers) = doc.get_mut("mcp_servers").and_then(|t| t.as_table_like_mut()) {
            servers.remove(AGENT_NAME);
        }
    } else {
        let servers = doc.entry("mcp_servers").or_insert(toml_edit::table());
        if let Some(t) = servers.as_table_mut() {
            t.set_implicit(true);
        }
        let mut t = toml_edit::Table::new();
        t["command"] = toml_edit::value(exe.display().to_string());
        t["args"] = toml_edit::value(toml_edit::Array::from_iter(["mcp"]));
        let mut env = toml_edit::InlineTable::new();
        env.insert("TELOS_RN_AGENT", "grok".into());
        if let Ok(h) = crate::paths::env_var("HOME") {
            env.insert("TELOS_RN_HOME", h.into());
        }
        t["env"] = toml_edit::value(env);
        t["enabled"] = toml_edit::value(true);
        // Room for check_inbox(wait_secs) long-polls.
        t["tool_timeout_sec"] = toml_edit::value(660);
        servers[AGENT_NAME] = toml_edit::Item::Table(t);
    }
    grok_allow(&mut doc, remove);
    r.lines.push(changed(&cfg_path, write_file(&cfg_path, &doc.to_string(), dry)?));

    let hooks_path = dir.join(format!("hooks/{AGENT_NAME}.json"));
    let mut v = read_json(&hooks_path)?;
    // Stop and PostToolUse default to a 10 minute timeout in grok; the others to 5 seconds.
    let group = |event: &str, matcher: bool, timeout: u64| {
        let mut g = json!({ "hooks": [{ "type": "command", "command": shell_cmd(exe, &format!("hook grok {event}")), "timeout": timeout }] });
        if matcher {
            g["matcher"] = json!("*");
        }
        g
    };
    let groups = vec![
        ("SessionStart", group("session-start", false, 10)),
        ("UserPromptSubmit", group("user-prompt-submit", false, 10)),
        ("PostToolUse", group("post-tool-use", true, 10)),
        ("Stop", group("stop", false, 10)),
        ("SessionEnd", group("session-end", false, 3)),
    ];
    merge_hook_groups(obj(obj(&mut v)?.entry("hooks").or_insert_with(|| json!({})))?, "grok", groups, remove);
    if remove && hollow(&v) {
        if hooks_path.exists() {
            if !dry {
                std::fs::remove_file(&hooks_path)?;
            }
            r.lines.push(format!("removed {}", crate::util::tilde(&hooks_path.display().to_string())));
        }
    } else {
        let did = write_json(&hooks_path, &v, dry)?;
        r.lines.push(changed(&hooks_path, did));
    }
    Ok(Some(r))
}

/// grok asks before every MCP tool call unless a rule allows it.
fn grok_allow(doc: &mut toml_edit::DocumentMut, remove: bool) {
    let rule = format!("MCPTool({AGENT_NAME}__*)");
    if remove {
        if let Some(permission) = doc.get_mut("permission").and_then(|p| p.as_table_like_mut()) {
            if let Some(allow) = permission.get_mut("allow").and_then(|a| a.as_array_mut()) {
                allow.retain(|v| v.as_str() != Some(rule.as_str()));
                if allow.is_empty() {
                    permission.remove("allow");
                }
            }
            if permission.is_empty() {
                doc.remove("permission");
            }
        }
        return;
    }
    let allow = doc.entry("permission").or_insert(toml_edit::table())["allow"]
        .or_insert(toml_edit::value(toml_edit::Array::new()));
    if let Some(allow) = allow.as_array_mut()
        && !allow.iter().any(|v| v.as_str() == Some(rule.as_str()))
    {
        allow.push(rule);
    }
}

// ---- Cursor ----

fn cursor(exe: &Path, remove: bool, dry: bool) -> Result<Option<Report>> {
    let dir = home().join(".cursor");
    if !dir.exists() && agent::find_bin("cursor-agent").is_none() {
        return Ok(None);
    }
    let mut r = Report { agent: "Cursor", lines: Vec::new(), notes: Vec::new() };
    let mcp_path = dir.join("mcp.json");
    let mut v = read_json(&mcp_path)?;
    let servers = obj(obj(&mut v)?.entry("mcpServers").or_insert_with(|| json!({})))?;
    if remove {
        servers.remove(AGENT_NAME);
    } else {
        servers.insert(
            AGENT_NAME.into(),
            json!({ "type": "stdio", "command": exe.display().to_string(), "args": ["mcp"], "env": { "TELOS_RN_AGENT": "cursor" } }),
        );
    }
    r.lines.push(changed(&mcp_path, write_json(&mcp_path, &v, dry)?));

    let hooks_path = dir.join("hooks.json");
    let mut v = read_json(&hooks_path)?;
    obj(&mut v)?.entry("version").or_insert(json!(1));
    let hooks = obj(obj(&mut v)?.entry("hooks").or_insert_with(|| json!({})))?;
    let entry = |event: &str| json!({ "command": shell_cmd(exe, &format!("hook cursor {event}")), "timeout": 10 });
    let mut ours = vec![
        ("sessionStart", entry("session-start")),
        ("beforeSubmitPrompt", entry("user-prompt-submit")),
        ("postToolUse", entry("post-tool-use")),
        ("stop", entry("stop")),
        ("sessionEnd", entry("session-end")),
    ];
    ours[2].1["matcher"] = json!("*");
    merge_hook_groups(hooks, "cursor", ours, remove);
    r.lines.push(changed(&hooks_path, write_json(&hooks_path, &v, dry)?));
    Ok(Some(r))
}

// ---- Gemini CLI ----

fn gemini(exe: &Path, remove: bool, dry: bool) -> Result<Option<Report>> {
    let dir = gemini_home();
    if !dir.exists() && agent::find_bin("gemini").is_none() {
        return Ok(None);
    }
    let mut r = Report { agent: "Gemini CLI", lines: Vec::new(), notes: Vec::new() };
    let path = dir.join("settings.json");
    let mut v = read_json(&path)?;
    let servers = obj(obj(&mut v)?.entry("mcpServers").or_insert_with(|| json!({})))?;
    if remove {
        servers.remove(AGENT_NAME);
    } else {
        servers.insert(
            AGENT_NAME.into(),
            json!({ "command": exe.display().to_string(), "args": ["mcp"], "env": { "TELOS_RN_AGENT": "gemini" }, "timeout": 660000, "trust": true }),
        );
    }
    let group = |event: &str, timeout: u64| json!({ "matcher": "*", "hooks": [{ "type": "command", "name": format!("{AGENT_NAME}-{event}"), "command": shell_cmd(exe, &format!("hook gemini {event}")), "timeout": timeout }] });
    let groups = vec![
        ("SessionStart", group("session-start", 10000)),
        ("BeforeAgent", group("user-prompt-submit", 10000)),
        ("AfterTool", group("post-tool-use", 10000)),
        ("AfterAgent", group("stop", 10000)),
        ("SessionEnd", group("session-end", 5000)),
    ];
    merge_hook_groups(obj(obj(&mut v)?.entry("hooks").or_insert_with(|| json!({})))?, "gemini", groups, remove);
    let did = write_json(&path, &v, dry)?;
    r.lines.push(changed(&path, did));
    if did && !remove {
        r.notes.push("Gemini CLI only runs Reason Network in trusted folders (`/permissions trust`).".into());
    }
    Ok(Some(r))
}

// ---- GitHub Copilot CLI ----

fn copilot(exe: &Path, remove: bool, dry: bool) -> Result<Option<Report>> {
    let dir = home().join(".copilot");
    if !dir.exists() && agent::find_bin("copilot").is_none() {
        return Ok(None);
    }
    let mut r = Report { agent: "Copilot CLI", lines: Vec::new(), notes: Vec::new() };
    let mcp_path = dir.join("mcp-config.json");
    let mut v = read_json(&mcp_path)?;
    let servers = obj(obj(&mut v)?.entry("mcpServers").or_insert_with(|| json!({})))?;
    if remove {
        servers.remove(AGENT_NAME);
    } else {
        servers.insert(
            AGENT_NAME.into(),
            json!({ "type": "local", "command": exe.display().to_string(), "args": ["mcp"], "env": { "TELOS_RN_AGENT": "copilot" }, "tools": ["*"], "timeout": 660000 }),
        );
    }
    r.lines.push(changed(&mcp_path, write_json(&mcp_path, &v, dry)?));

    let hooks_path = dir.join("hooks").join(format!("{AGENT_NAME}.json"));
    if remove {
        if hooks_path.exists() {
            if !dry {
                std::fs::remove_file(&hooks_path)?;
            }
            r.lines.push(format!("removed {}", crate::util::tilde(&hooks_path.display().to_string())));
        }
    } else {
        let shell = if cfg!(windows) { "powershell" } else { "bash" };
        let entry = |event: &str| json!([{ "type": "command", shell: shell_cmd(exe, &format!("hook copilot {event}")), "timeoutSec": 10 }]);
        let v = json!({ "version": 1, "hooks": {
            "sessionStart": entry("session-start"),
            "userPromptSubmitted": entry("user-prompt-submit"),
            "postToolUse": entry("post-tool-use"),
            "agentStop": entry("stop"),
            "sessionEnd": entry("session-end"),
        }});
        r.lines.push(changed(&hooks_path, write_json(&hooks_path, &v, dry)?));
    }
    Ok(Some(r))
}

// ---- opencode ----

fn opencode(exe: &Path, remove: bool, dry: bool) -> Result<Option<Report>> {
    let dir = opencode_home();
    if !dir.exists() && agent::find_bin("opencode").is_none() {
        return Ok(None);
    }
    let mut r = Report { agent: "opencode", lines: Vec::new(), notes: Vec::new() };
    let path = dir.join("opencode.json");
    if dir.join("opencode.jsonc").exists() && !path.exists() {
        r.notes.push(format!(
            "opencode: you use opencode.jsonc, which Reason Network doesn't edit. Add under \"mcp\": \"{AGENT_NAME}\": {{\"type\": \"local\", \"command\": [\"{}\", \"mcp\"], \"environment\": {{\"TELOS_RN_AGENT\": \"opencode\"}}}}",
            exe.display()
        ));
    } else {
        let mut v = read_json(&path)?;
        let mcp = obj(obj(&mut v)?.entry("mcp").or_insert_with(|| json!({})))?;
        if remove {
            mcp.remove(AGENT_NAME);
        } else {
            mcp.insert(
                AGENT_NAME.into(),
                json!({ "type": "local", "command": [exe.display().to_string(), "mcp"], "environment": { "TELOS_RN_AGENT": "opencode" }, "enabled": true }),
            );
        }
        r.lines.push(changed(&path, write_json(&path, &v, dry)?));
    }
    let plugin = dir.join("plugins").join(format!("{AGENT_NAME}.ts"));
    if remove {
        if plugin.exists() {
            if !dry {
                std::fs::remove_file(&plugin)?;
            }
            r.lines.push(format!("removed {}", crate::util::tilde(&plugin.display().to_string())));
        }
    } else {
        let code = include_str!("opencode-plugin.ts")
            .replace("__ENGINE_EXE__", &exe.display().to_string().replace('\\', "\\\\").replace('"', "\\\""));
        r.lines.push(changed(&plugin, write_file(&plugin, &code, dry)?));
    }
    Ok(Some(r))
}

// ---- pi ----

/// One extension does it all: it adds Reason Network's MCP server to each session (deferred, so the tools stay out of
/// the model's context until it looks for them) and delivers messages.
fn pi(exe: &Path, remove: bool, dry: bool) -> Result<Option<Report>> {
    if !pi_home().exists() && agent::find_bin("pi").is_none() {
        return Ok(None);
    }
    let mut r = Report { agent: "pi", lines: Vec::new(), notes: Vec::new() };
    let ext = pi_home().join("extensions").join(format!("{AGENT_NAME}.ts"));
    if remove {
        if ext.exists() {
            if !dry {
                std::fs::remove_file(&ext)?;
            }
            r.lines.push(format!("removed {}", crate::util::tilde(&ext.display().to_string())));
        }
    } else {
        let code = include_str!("pi-extension.ts")
            .replace("__ENGINE_EXE__", &exe.display().to_string().replace('\\', "\\\\").replace('"', "\\\""));
        r.lines.push(changed(&ext, write_file(&ext, &code, dry)?));
    }
    Ok(Some(r))
}

// ---- DeepSeek Harness (dsh) ----

const DSH_BEGIN: &str = "# === telos-rn MCP BEGIN ===";
const DSH_END: &str = "# === telos-rn MCP END ===";

fn dsh_home() -> PathBuf {
    // dsh takes a blank `DSH_HOME` for unset, spaces included.
    std::env::var_os("DSH_HOME")
        .filter(|h| !h.to_string_lossy().trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".dsh"))
}

/// dsh applies this file on top of every profile, those made later too.
fn dsh_patch() -> PathBuf {
    dsh_home().join("cordis.patch.yml")
}

/// The binary's path as the block writes it: a YAML single-quoted string doubles its quotes.
fn dsh_command(exe: &Path) -> String {
    exe.display().to_string().replace('\'', "''")
}

/// Our MCP server as an entry of dsh's MCP client plugin. Tool calls get as long as `check_inbox(wait_secs)` may wait
/// (dsh gives them one minute otherwise).
fn dsh_block(exe: &Path) -> String {
    let command = dsh_command(exe);
    format!(
        "{DSH_BEGIN}\n- insert:\n    - id: mcp-{AGENT_NAME}\n      name: '@deepseek-ai/dsh-mcp-client'\n      config:\n        \
         serverName: {AGENT_NAME}\n        transport: stdio\n        command: '{command}'\n        args:\n          - mcp\n        \
         env:\n          TELOS_RN_AGENT: dsh\n        toolCallTimeoutMs: 660000\n{DSH_END}\n"
    )
}

fn dsh_wired(text: &str, exe: &Path) -> bool {
    dsh_has_server(text) && contains(&dsh_entries(text).join("\n"), &dsh_command(exe))
}

/// The lines of a patch file that are neither blank nor comments.
fn dsh_entries(text: &str) -> Vec<&str> {
    text.lines().filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#')).collect()
}

/// Whether an entry of the file (not a comment) names an MCP server `telos-rn`, quoted or not.
fn dsh_has_server(text: &str) -> bool {
    dsh_entries(text).iter().any(|l| {
        let line = l.trim_start().trim_start_matches("- ").trim_start();
        line.strip_prefix("serverName:").is_some_and(|name| {
            name.split(" #").next().unwrap_or_default().trim().trim_matches(['\'', '"']) == AGENT_NAME
        })
    })
}

/// A patch file's text with `block` in it (in place of the one it has), or without ours when `remove`. `None`: nothing
/// to change. dsh wants a top-level list (`- …` from the first column), so a file that is anything else is left
/// alone, as is one with a server of that name the user wrote; both come back as an error saying so.
fn dsh_patch_text(text: &str, block: &str, remove: bool) -> Result<Option<String>> {
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let block = block.replace('\n', eol);
    let marked = text.find(DSH_BEGIN).and_then(|b| {
        let e = b + text[b..].find(DSH_END)? + DSH_END.len();
        let e = e + text[e..].chars().take_while(|c| matches!(c, '\r' | '\n')).take(eol.len()).count();
        Some((b, e))
    });
    let new = match marked {
        Some((b, e)) if remove => {
            let before =
                text[..b].strip_suffix(&format!("{eol}{eol}")).map_or(&text[..b], |s| &text[..s.len() + eol.len()]);
            let rest = format!("{before}{}", &text[e..]);
            if dsh_entries(&rest).is_empty() {
                // Nothing but comments is not a list: dsh refuses to start on it.
                let gap = if rest.is_empty() || rest.ends_with('\n') { "" } else { eol };
                format!("{rest}{gap}[]{eol}")
            } else {
                rest
            }
        }
        Some((b, e)) => format!("{}{block}{}", &text[..b], &text[e..]),
        None if remove => return Ok(None),
        None => {
            if dsh_has_server(text) {
                return Err(anyhow!("it already has a {AGENT_NAME} MCP server of your own; left as is"));
            }
            let entries = dsh_entries(text);
            match entries.first().copied() {
                None => format!("{text}{}{block}", if text.is_empty() || text.ends_with('\n') { "" } else { eol }),
                Some(first) if first.trim_end() == "[]" && entries.len() == 1 => {
                    let rest: Vec<&str> = text.lines().filter(|l| l.trim_end() != "[]").collect();
                    // A file that was only the list becomes our block, with no blank line above it.
                    if rest.is_empty() { block } else { format!("{}{eol}{block}", rest.join(eol)) }
                }
                Some(first) if first.starts_with("- ") => {
                    let gap = if text.ends_with('\n') { eol.to_string() } else { format!("{eol}{eol}") };
                    format!("{text}{gap}{block}")
                }
                Some(_) => {
                    return Err(anyhow!(
                        "it isn't a top-level list of patch entries (`- …` from the first column); add the {AGENT_NAME} MCP entry yourself"
                    ));
                }
            }
        }
    };
    Ok((new != text).then_some(new))
}

fn dsh(exe: &Path, remove: bool, dry: bool) -> Result<Option<Report>> {
    if !dsh_home().exists() && agent::find_bin("dsh").is_none() {
        return Ok(None);
    }
    let mut r = Report { agent: "DeepSeek Harness", lines: Vec::new(), notes: Vec::new() };
    let path = dsh_patch();
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
    };
    match dsh_patch_text(&text, &dsh_block(exe), remove) {
        Ok(Some(new)) => {
            r.lines.push(changed(&path, write_file(&path, &new, dry)?));
            if !remove {
                r.notes.push(
                    "DeepSeek Harness reads Reason Network messages when it checks its inbox; start a new dsh session to load the MCP server."
                        .into(),
                );
            }
        }
        Ok(None) => r.lines.push(changed(&path, false)),
        // Skipped, not set up: `apply` says so, and the wizard doesn't count it among the agents it set up.
        Err(e) => return Err(anyhow!("{}: {e:#}", crate::util::tilde(&path.display().to_string()))),
    }
    Ok(Some(r))
}

type AgentFn = fn(&Path, bool, bool) -> Result<Option<Report>>;

struct Agent {
    id: &'static str,
    name: &'static str,
    /// How quickly it sees messages, and anything to know.
    hint: &'static str,
    installed: fn() -> bool,
    wired: fn(&Path) -> bool,
    apply: AgentFn,
}

/// Whether `text` holds `needle`. Windows paths are the same whatever their case (`D:\Program Files` is
/// `D:\program files`), and the case a program is started with depends on how it was found.
fn contains(text: &str, needle: &str) -> bool {
    if cfg!(windows) { text.to_lowercase().contains(&needle.to_lowercase()) } else { text.contains(needle) }
}

fn has(path: PathBuf, needle: &str) -> bool {
    std::fs::read_to_string(path).is_ok_and(|s| contains(&s, needle))
}

/// Our path as a JSON config has it (with Windows' backslashes escaped).
fn me(exe: &Path) -> String {
    let s = json!(exe.display().to_string()).to_string();
    s[1..s.len() - 1].to_string()
}

const AGENTS: [Agent; 10] = [
    Agent {
        id: "claude",
        name: "Claude Code",
        hint: "messages arrive instantly",
        installed: || agent::find_bin("claude").is_some(),
        wired: |_| {
            agent::find_bin("claude").is_some_and(|b| {
                Command::new(b)
                    .args(["mcp", "get", AGENT_NAME])
                    .stdin(std::process::Stdio::null())
                    .output()
                    .is_ok_and(|o| o.status.success())
            })
        },
        apply: claude,
    },
    Agent {
        id: "codex",
        name: "Codex",
        hint: "instant; asks once to trust the hooks",
        installed: || agent::find_bin("codex").is_some() || codex_home().exists(),
        // TOML keeps a Windows path as it is ('C:\…').
        wired: |exe| {
            has(codex_home().join("config.toml"), &exe.display().to_string())
                && has(codex_home().join("hooks.json"), "hook codex")
        },
        apply: codex,
    },
    Agent {
        id: "grok",
        name: "Grok CLI",
        hint: "at its next step or end of turn",
        installed: || agent::find_bin("grok").is_some() || grok_home().exists(),
        wired: |exe| {
            has(grok_home().join("config.toml"), &exe.display().to_string())
                && has(grok_home().join(format!("hooks/{AGENT_NAME}.json")), "hook grok")
        },
        apply: grok,
    },
    Agent {
        id: "cursor",
        name: "Cursor",
        hint: "sees messages at its next step",
        installed: || home().join(".cursor").exists() || agent::find_bin("cursor-agent").is_some(),
        wired: |exe| has(home().join(".cursor/mcp.json"), &me(exe)),
        apply: cursor,
    },
    Agent {
        id: "zcode",
        name: "ZCode Desktop",
        hint: "sees messages when it checks its inbox; reload MCP after setup",
        installed: zcode::installed,
        wired: zcode::wired,
        apply: zcode::apply,
    },
    Agent {
        id: "gemini",
        name: "Gemini CLI",
        hint: "at its next step; trusted folders only",
        installed: || gemini_home().exists() || agent::find_bin("gemini").is_some(),
        wired: |exe| has(gemini_home().join("settings.json"), &me(exe)),
        apply: gemini,
    },
    Agent {
        id: "copilot",
        name: "Copilot CLI",
        hint: "sees messages at its next step",
        installed: || home().join(".copilot").exists() || agent::find_bin("copilot").is_some(),
        wired: |exe| has(home().join(".copilot/mcp-config.json"), &me(exe)),
        apply: copilot,
    },
    Agent {
        id: "opencode",
        name: "opencode",
        hint: "messages arrive within seconds",
        installed: || opencode_home().exists() || agent::find_bin("opencode").is_some(),
        wired: |_| opencode_home().join(format!("plugins/{AGENT_NAME}.ts")).exists(),
        apply: opencode,
    },
    Agent {
        id: "pi",
        name: "pi",
        hint: "messages arrive within seconds",
        installed: || pi_home().exists() || agent::find_bin("pi").is_some(),
        wired: |_| pi_home().join(format!("extensions/{AGENT_NAME}.ts")).exists(),
        apply: pi,
    },
    Agent {
        id: "dsh",
        name: "DeepSeek Harness",
        hint: "sees messages when it checks its inbox",
        installed: || dsh_home().exists() || agent::find_bin("dsh").is_some(),
        wired: |exe| std::fs::read_to_string(dsh_patch()).is_ok_and(|t| dsh_wired(&t, exe)),
        apply: dsh,
    },
];

/// An agent found on this machine.
pub struct Found {
    pub id: &'static str,
    pub name: &'static str,
    pub hint: &'static str,
    pub wired: bool,
}

pub fn detect(exe: &Path) -> Vec<Found> {
    AGENTS
        .iter()
        .filter(|a| (a.installed)())
        .map(|a| Found { id: a.id, name: a.name, hint: a.hint, wired: (a.wired)(exe) })
        .collect()
}

/// What `apply` did: a line per agent, and things the user should know.
#[derive(Default)]
pub struct Applied {
    pub lines: Vec<String>,
    pub notes: Vec<String>,
    /// The agents set up (or removed) without trouble.
    pub agents: Vec<&'static str>,
}

/// Wires Reason Network into the `install` agents and out of the `remove` ones.
pub fn apply(exe: &Path, install: &[&str], remove: &[&str], dry: bool) -> Applied {
    let mut out = Applied::default();
    for a in &AGENTS {
        let removing = remove.contains(&a.id);
        if !removing && !install.contains(&a.id) {
            continue;
        }
        match (a.apply)(exe, removing, dry) {
            Ok(Some(r)) => {
                out.lines.push(format!("{}: {}", r.agent, r.lines.join("; ")));
                out.notes.extend(r.notes);
                out.agents.push(a.name);
            }
            Ok(None) => {}
            Err(e) => {
                out.lines.push(format!("{}: skipped ({e:#})", a.name));
                out.notes.push(format!("{} not set up: {e:#}", a.name));
            }
        }
    }
    out
}

/// Non-interactive setup: every agent found (or, with `remove`, every agent wired).
pub fn run(dry: bool, remove: bool) -> Result<()> {
    let exe = exe()?;
    let found = detect(&exe);
    let verb = if dry {
        "Would change"
    } else if remove {
        "Removing Reason Network"
    } else {
        "Setting up Reason Network"
    };
    println!("{verb} ({}):", crate::util::tilde(&exe.display().to_string()));
    if found.is_empty() && !remove {
        println!(
            "  no supported agents found (Claude Code, Codex, Grok CLI, Cursor, ZCode Desktop, Gemini CLI, Copilot CLI, opencode, pi, DeepSeek Harness)"
        );
    }
    let ids: Vec<&str> = found.iter().map(|f| f.id).collect();
    let (install, removing): (&[&str], &[&str]) = if remove { (&[], &ids) } else { (&ids, &[]) };
    let out = apply(&exe, install, removing, dry);
    for l in &out.lines {
        println!("  {l}");
    }
    if !out.notes.is_empty() {
        println!();
        for n in &out.notes {
            println!("- {n}");
        }
    }
    Ok(())
}

/// Read-only checks, one line each.
pub async fn doctor() -> Result<()> {
    let exe = exe()?;
    println!("telos-rn {} at {}", env!("CARGO_PKG_VERSION"), crate::util::tilde(&exe.display().to_string()));
    let link = Link::new(None);
    match link.call::<TeamInfo>(Request::Team).await {
        Ok(t) => {
            let online = t.machines.iter().filter(|m| !m.is_me && m.online).count();
            let others = t.machines.len().saturating_sub(1);
            let team = crate::shown(&t.name);
            println!("✓ daemon running; team {team} with {others} other machine(s), {online} connected");
            print!("{}", crate::shown(&render::team(&t)).lines().skip(1).map(|l| format!("{l}\n")).collect::<String>());
            if let Some(u) = &t.udp {
                println!("  {}", render::udp_port(u));
            }
            if t.relay_connected == Some(false) {
                let relay = crate::shown(&t.relay.clone().unwrap_or_else(render::default_relays));
                println!(
                    "  ! can't reach the team's relay {relay}: until it's up, teammates are only found on this network"
                );
            }
            if others > 0 && online == 0 {
                println!(
                    "  ! no teammate connected: check your internet, or that their machines are on (`{CLI} doctor` there)"
                );
                // Nothing tells a machine that was off when the team changed relays where the team went.
                println!(
                    "  ! if the team changed relays while this machine was off, set the same one here: `{CLI} relay <url>`"
                );
                println!("    (or `default` for Reason's); `{CLI} team` on a teammate's machine shows which");
            }
        }
        Err(e) if e.to_string().contains("not in a team") => {
            println!("✓ daemon running");
            println!("- not in a team yet: run `{CLI}`");
        }
        Err(e) => println!("✗ daemon: {}", crate::shown(&format!("{e:#}"))),
    }
    let cfg = crate::config::Config::load();
    let (odd, off): (Vec<_>, Vec<_>) = cfg.folders.iter().partition(|e| cfg.odd_folders().contains(e));
    if !off.is_empty() {
        println!("- Reason Network is off in {}", off.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>().join(", "));
    }
    for (k, v) in odd {
        let why = if v == "off" { "not a full path" } else { "\"off\" is the only setting so far" };
        println!("  ! folders: \"{k}\" = \"{v}\" is ignored: {why}");
    }
    for f in detect(&exe) {
        if f.wired {
            println!("✓ {}: wired up", f.name);
        } else {
            println!("- {}: installed, not on Reason Network (`{CLI} setup`)", f.name);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grok_allow_rule() {
        let mut doc: toml_edit::DocumentMut = "[permission]\nallow = [\"Bash(ls)\"]\n".parse().unwrap();
        grok_allow(&mut doc, false);
        grok_allow(&mut doc, false);
        assert_eq!(doc.to_string(), "[permission]\nallow = [\"Bash(ls)\", \"MCPTool(telos-rn__*)\"]\n");
        grok_allow(&mut doc, true);
        assert_eq!(doc.to_string(), "[permission]\nallow = [\"Bash(ls)\"]\n");
        let mut doc = toml_edit::DocumentMut::new();
        grok_allow(&mut doc, false);
        assert!(doc.to_string().contains("MCPTool(telos-rn__*)"));
        grok_allow(&mut doc, true);
        assert_eq!(doc.to_string(), "");
    }

    #[test]
    fn hook_groups_merge_and_remove() {
        let mut hooks = serde_json::from_value::<Map<String, Value>>(json!({
            "Stop": [{ "hooks": [{ "type": "command", "command": "other-tool stop" }] }],
            "PostToolUse": [{ "matcher": "*", "hooks": [{ "type": "command", "command": "/x/telos-rn hook codex post-tool-use" }] }],
        }))
        .unwrap();
        let ours =
            |e: &str| json!({ "hooks": [{ "type": "command", "command": format!("/y/telos-rn hook codex {e} ") }] });
        merge_hook_groups(
            &mut hooks,
            "codex",
            vec![("Stop", ours("stop")), ("PostToolUse", ours("post-tool-use"))],
            false,
        );
        assert_eq!(hooks["Stop"].as_array().unwrap().len(), 2, "keeps other tools' hooks");
        assert_eq!(hooks["PostToolUse"].as_array().unwrap().len(), 1, "replaces our earlier hook");
        merge_hook_groups(&mut hooks, "codex", vec![], true);
        assert_eq!(hooks["Stop"].as_array().unwrap().len(), 1);
        assert!(!hooks.contains_key("PostToolUse"));
    }

    const DSH_PATCH: &str = "# Your patch layer\n\n- id: agent-loop\n  config:\n    maxParallelToolCalls: 20\n";

    #[test]
    fn dsh_patch_gets_our_block_once_and_loses_it_again() {
        let block = dsh_block(Path::new("/usr/local/bin/telos-rn"));
        let with = dsh_patch_text(DSH_PATCH, &block, false).unwrap().unwrap();
        assert!(with.starts_with(DSH_PATCH) && with.contains(&block), "{with}");
        assert_eq!(dsh_patch_text(&with, &block, false).unwrap(), None, "already there");
        let moved = dsh_block(Path::new("/opt/telos-rn"));
        let updated = dsh_patch_text(&with, &moved, false).unwrap().unwrap();
        assert!(updated.contains("/opt/telos-rn") && !updated.contains("/usr/local/bin"), "{updated}");
        assert_eq!(dsh_patch_text(&with, &block, true).unwrap().as_deref(), Some(DSH_PATCH), "removal restores it");
        assert_eq!(dsh_patch_text(DSH_PATCH, &block, true).unwrap(), None, "nothing to remove");
    }

    #[test]
    fn dsh_patch_stays_a_list_when_our_block_goes() {
        let block = dsh_block(Path::new("/x/telos-rn"));
        // What dsh ships: comments and an empty list. Without the list dsh refuses to start.
        let shipped = "# patch layer\n[]\n";
        let with = dsh_patch_text(shipped, &block, false).unwrap().unwrap();
        assert!(!with.contains("[]") && with.contains(&block), "{with}");
        assert_eq!(dsh_patch_text(&with, &block, true).unwrap().as_deref(), Some(shipped));
        // A file made for our block, or an empty one, goes back to an empty list as well.
        let made = dsh_patch_text("", &block, false).unwrap().unwrap();
        assert_eq!(made, block);
        assert_eq!(dsh_patch_text(&made, &block, true).unwrap().as_deref(), Some("[]\n"));
    }

    #[test]
    fn dsh_patch_only_takes_a_top_level_list() {
        let block = dsh_block(Path::new("/x/telos-rn"));
        // Appending to any of these would leave a file dsh can't parse.
        assert!(dsh_patch_text("  - id: a\n    name: b\n", &block, false).is_err(), "indented list");
        assert!(dsh_patch_text("---\n[]\n", &block, false).is_err(), "document marker");
        assert!(dsh_patch_text("agents: []\n", &block, false).is_err(), "a mapping");
        let crlf = dsh_patch_text(&DSH_PATCH.replace('\n', "\r\n"), &block, false).unwrap().unwrap();
        assert!(!crlf.replace("\r\n", "").contains('\n'), "line ends stay CRLF");
    }

    #[test]
    fn dsh_patch_leaves_a_server_of_the_users_own_alone() {
        let block = dsh_block(Path::new("/x/telos-rn"));
        let own = "- insert:\n    - id: my-rn\n      name: '@deepseek-ai/dsh-mcp-client'\n      config:\n        serverName: telos-rn\n";
        let why = dsh_patch_text(own, &block, false).unwrap_err().to_string();
        assert!(why.contains("of your own"), "{why}");
        assert_eq!(dsh_patch_text(own, &block, true).unwrap(), None, "`--remove` leaves it");
        // However YAML quotes the name; a comment about it is not a server.
        for quoted in ["\"telos-rn\"", "'telos-rn'", "telos-rn # mine"] {
            let own = format!("- insert:\n    - config:\n        serverName: {quoted}\n");
            assert!(dsh_patch_text(&own, &block, false).is_err(), "{quoted}");
        }
        let commented = "# serverName: telos-rn\n- id: agent-loop\n";
        assert!(dsh_patch_text(commented, &block, false).unwrap().unwrap().contains(&block), "commented out");
        assert!(!dsh_wired("# serverName: telos-rn\n", Path::new("/x/telos-rn")));
    }

    #[test]
    fn dsh_patch_that_is_only_the_empty_list_round_trips() {
        let block = dsh_block(Path::new("/x/telos-rn"));
        let with = dsh_patch_text("[]\n", &block, false).unwrap().unwrap();
        assert_eq!(with, block, "no blank line above our block");
        assert_eq!(dsh_patch_text(&with, &block, true).unwrap().as_deref(), Some("[]\n"));
    }

    #[test]
    fn dsh_wiring_is_found_through_the_yaml_quote() {
        let exe = Path::new(r"C:\Users\O'Neil\telos-rn.exe");
        let block = dsh_block(exe);
        assert!(block.contains(r"'C:\Users\O''Neil\telos-rn.exe'"), "{block}");
        assert!(dsh_wired(&block, exe));
        assert!(!dsh_wired(&block, Path::new(r"C:\Users\other\telos-rn.exe")));
        assert!(!dsh_wired("", exe));
    }

    #[test]
    #[cfg(unix)]
    fn quoting() {
        assert_eq!(shell_cmd(Path::new("/usr/local/bin/telos-rn"), "mcp"), "/usr/local/bin/telos-rn mcp");
        assert_eq!(shell_cmd(Path::new("/Users/a b/telos-rn"), "mcp"), "'/Users/a b/telos-rn' mcp");
    }

    #[test]
    #[cfg(windows)]
    fn quoting() {
        assert_eq!(
            shell_cmd(Path::new(r"C:\Users\alex\.local\bin\telos-rn.exe"), "mcp"),
            r"C:\Users\alex\.local\bin\telos-rn.exe mcp"
        );
        assert_eq!(
            shell_cmd(Path::new(r"C:\Users\Alex O'Neil\telos-rn.exe"), "mcp"),
            r"& 'C:\Users\Alex O''Neil\telos-rn.exe' mcp"
        );
        assert_eq!(me(Path::new(r"C:\a\telos-rn.exe")), r"C:\\a\\telos-rn.exe");
    }

    #[test]
    #[cfg(windows)]
    fn paths_match_whatever_their_case() {
        let exe = r"D:\Program Files\telos-rn\telos-rn.exe";
        assert!(contains(r"command = 'D:\program files\telos-rn\telos-rn.exe'", exe));
        assert!(contains(r#""command": "d:\\PROGRAM FILES\\telos-rn\\telos-rn.exe""#, &me(Path::new(exe))));
        assert!(!contains(r"D:\program files\telos-rn\other.exe", exe));
        // It folds case beyond ASCII, as Windows does.
        assert!(contains(r"command = 'C:\USERS\JÖRG\telos-rn.exe'", r"C:\Users\Jörg\telos-rn.exe"));
    }
}

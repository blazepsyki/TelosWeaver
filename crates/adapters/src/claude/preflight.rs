//! Capability 판정 근거와 사전 점검 (Spec §7.1, H0 D11·D14, PH1-CLAUDE P2·P3·L4).
//!
//! - 수단이 있는 기능의 `supported`/`unverified`는 [`telos_schema::judge`]로 정합니다. 입력은 같은 플랫폼에서
//!   통과한 실환경 증거([`builtin_evidence`])의 버전, 지금 `--version`이 보고한 버전, 그 기능에 필요한 점검만
//!   모은 [`PreflightReport`]입니다.
//! - 사전 점검은 모델을 호출하지 않습니다. `--version` 출력 형식, `--help`의 옵션 정의, 그리고 Adapter가 실제로
//!   쓰는 옵션을 모두 붙인 실행 한 번입니다. 그 실행은 없는 세션(`--resume <0 UUID>`)을 가리켜 옵션 해석 뒤
//!   "No conversation found"로 끝나며 Turn을 만들지 않습니다(2.1.325에서 `num_turns: 0`, 사용량 0, 세션 미저장 확인).
//!   CLI는 모르는 옵션·잘못된 값을 `error: ...` 줄로 거부하므로 그 줄이 가리키는 옵션을 실패로 기록합니다.

use std::{
    ffi::OsString,
    path::Path,
    time::{Duration, Instant},
};

use futures::StreamExt;
use telos_schema::{
    CapabilityEvidence, PreflightCheck, PreflightReport, PreflightResult, capability_keys as keys,
};

use super::PROVIDER_ID;
use crate::runtime::{
    CommandSpec, OutputEvent, OutputLimits, OutputStream, ProcessRuntime, StdinMode,
};

const EVIDENCE_PHASE0: &str = "docs/phase-0/evidence/PH0-CLAUDE";
const EVIDENCE_PHASE0_LINUX: &str = "docs/phase-0/evidence/PH0-INTEGRATION/README.md §9.4";
const EVIDENCE_H0_A1: &str = "docs/phase-1/H0_DECISIONS.md A1";
const EVIDENCE_PHASE1: &str = "docs/phase-1/evidence/PH1-CLAUDE/README.md";

const WINDOWS: (&str, &str) = ("windows-local", "win32-x64");
const LINUX: (&str, &str) = ("psyki-nas", "linux-x64");

/// (Host·플랫폼, CLI 버전, 확인 날짜, 근거, [(키, 시험 이름)]). 실행해 통과한 시험만 적습니다.
type EvidenceGroup = (
    (&'static str, &'static str),
    &'static str,
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str)],
);

const BASIC: &[(&str, &str)] = &[
    (keys::MANAGED_RUN, "env_completed_run"),
    (keys::EVENT_STREAM, "env_completed_run"),
    (keys::CANCEL, "env_cancel_terminates_process_tree"),
    (keys::NATIVE_RESUME, "env_resume_and_fork"),
    (keys::NATIVE_FORK, "env_resume_and_fork"),
];
const LISTING: &[(&str, &str)] = &[(keys::VSCODE_LISTING, "env_vscode_listing")];
/// Adapter 경로(`approval_required` → `answer_approval`)의 시험입니다. CLI만 직접 확인한
/// `env_permission_prompt_tool_offers_live_approval`은 이 키의 근거로 쓰지 않습니다.
const APPROVAL: &[(&str, &str)] = &[(keys::ANSWER_APPROVAL, "env_live_approval_allow_and_deny")];

const EVIDENCE: &[EvidenceGroup] = &[
    (WINDOWS, "2.1.310", "2026-10-24", EVIDENCE_PHASE0, BASIC),
    (WINDOWS, "2.1.310", "2026-10-25", EVIDENCE_PHASE0, LISTING),
    (LINUX, "2.1.310", "2026-10-26", EVIDENCE_PHASE0_LINUX, BASIC),
    (
        LINUX,
        "2.1.310",
        "2026-10-26",
        EVIDENCE_PHASE0_LINUX,
        LISTING,
    ),
    (WINDOWS, "2.1.318", "2026-11-07", EVIDENCE_H0_A1, BASIC),
    (WINDOWS, "2.1.318", "2026-11-07", EVIDENCE_H0_A1, LISTING),
    (WINDOWS, "2.1.325", "2026-11-17", EVIDENCE_PHASE1, BASIC),
    (WINDOWS, "2.1.325", "2026-11-17", EVIDENCE_PHASE1, LISTING),
    (WINDOWS, "2.1.325", "2026-11-17", EVIDENCE_PHASE1, APPROVAL),
];

/// 이 Adapter와 함께 기록한 실환경 시험 통과 증거입니다 (Spec §7.1 `CapabilityEvidence`, L4).
/// Core가 `provider_capabilities`에 쌓은 증거는 `ClaudeCodeAdapter::with_evidence`로 더합니다.
pub fn builtin_evidence() -> Vec<CapabilityEvidence> {
    EVIDENCE
        .iter()
        .flat_map(|((host, platform), version, date, reference, tests)| {
            tests.iter().map(move |(key, test)| CapabilityEvidence {
                key: (*key).into(),
                provider: PROVIDER_ID.into(),
                host_id: (*host).into(),
                platform: (*platform).into(),
                cli_version: (*version).into(),
                extension_version: Some((*version).into()),
                test: (*test).into(),
                passed: true,
                checked_at: format!("{date}T00:00:00Z"),
                evidence_ref: (*reference).into(),
            })
        })
        .collect()
}

/// Adapter가 실행되는 Host의 VS Code `targetPlatform` 표기입니다. CLI는 Adapter와 같은 Host에서 실행합니다 (§3.3).
pub fn host_platform() -> String {
    let os = match std::env::consts::OS {
        "windows" => "win32",
        "macos" => "darwin",
        other => other,
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        other => other,
    };
    format!("{os}-{arch}")
}

/// `2.1.325 (Claude Code)` → `2.1.325`. 기대 형식이 아니면 `None`입니다.
pub(crate) fn parse_version(output: &str) -> Option<&str> {
    let (version, product) = output.trim().split_once(' ')?;
    let mut parts = version.split('.');
    let numeric = |part: Option<&str>| {
        part.is_some_and(|p| p.chars().next().is_some_and(|c| c.is_ascii_digit()))
    };
    (product == "(Claude Code)"
        && numeric(parts.next())
        && numeric(parts.next())
        && numeric(parts.next()))
    .then_some(version)
}

/// 점검 하나가 어떤 기능에 필요한지입니다. 관계없는 옵션이 없어졌다고 다른 기능을 `unverified`로 내리지 않습니다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Need {
    /// 모든 Managed Run
    Base,
    /// `native_resume`과 `native_fork`
    Resume,
    /// `native_fork`
    Fork,
    /// `answer_approval`
    Approval,
}

impl Need {
    fn applies_to(self, key: &str) -> bool {
        match self {
            Self::Base => true,
            Self::Resume => key == keys::NATIVE_RESUME || key == keys::NATIVE_FORK,
            Self::Fork => key == keys::NATIVE_FORK,
            Self::Approval => key == keys::ANSWER_APPROVAL,
        }
    }
}

/// 없는 세션 ID입니다. 옵션 해석이 끝난 뒤 CLI가 Turn 없이 끝나게 합니다.
const NO_SESSION: &str = "00000000-0000-4000-8000-000000000000";

/// Adapter가 실제로 쓰는 옵션과 점검에 붙이는 값입니다 (`command_args`와 같은 옵션).
const PROBE_OPTIONS: &[(&str, &[&str], Need)] = &[
    ("-p", &[], Need::Base),
    ("--input-format", &["stream-json"], Need::Base),
    ("--output-format", &["stream-json"], Need::Base),
    ("--verbose", &[], Need::Base),
    ("--strict-mcp-config", &[], Need::Base),
    ("--mcp-config", &[r#"{"mcpServers":{}}"#], Need::Base),
    ("--permission-mode", &["default"], Need::Base),
    ("--model", &["haiku"], Need::Base),
    ("--setting-sources", &["user"], Need::Base),
    ("--tools", &[""], Need::Base),
    ("--allowedTools", &["Read"], Need::Base),
    ("--disallowedTools", &["Bash"], Need::Base),
    ("--permission-prompt-tool", &["stdio"], Need::Approval),
    ("--resume", &[NO_SESSION], Need::Resume),
    ("--fork-session", &[], Need::Fork),
];

/// `--help`에 정의가 있어야 하는 옵션입니다. `--permission-prompt-tool`은 2.1.325 도움말에 정의 줄이 없어
/// (다른 옵션 설명에만 나옴) 위 실행 점검으로만 확인합니다.
const HELP_FLAGS: &[(&str, Need)] = &[
    ("--print", Need::Base),
    ("--input-format", Need::Base),
    ("--output-format", Need::Base),
    ("--verbose", Need::Base),
    ("--strict-mcp-config", Need::Base),
    ("--mcp-config", Need::Base),
    ("--permission-mode", Need::Base),
    ("--model", Need::Base),
    ("--setting-sources", Need::Base),
    ("--tools", Need::Base),
    ("--allowedTools", Need::Base),
    ("--disallowedTools", Need::Base),
    ("--resume", Need::Resume),
    ("--fork-session", Need::Fork),
];

const CHECK_TIMEOUT: Duration = Duration::from_secs(20);
const CHECK_LIMITS: OutputLimits = OutputLimits {
    max_line_bytes: 256 * 1024,
    max_buffered_bytes: 1024 * 1024,
};

/// 사전 점검 결과와 각 점검이 필요한 기능입니다.
#[derive(Debug, Clone)]
pub(crate) struct Preflight {
    /// `--version`이 지금 보고한 버전(예: `2.1.325`). 읽지 못하면 `None`이며 어떤 증거와도 맞지 않습니다.
    pub version: Option<String>,
    provider_version: String,
    host_id: String,
    checked_at: String,
    results: Vec<(Need, PreflightResult)>,
}

impl Preflight {
    /// 전체 점검 결과입니다.
    pub fn report(&self) -> PreflightReport {
        self.build(|_| true)
    }

    /// `key` 기능에 필요한 점검만 모은 결과입니다. [`telos_schema::judge`]에 넘깁니다.
    pub fn report_for(&self, key: &str) -> PreflightReport {
        self.build(|need| need.applies_to(key))
    }

    fn build(&self, include: impl Fn(Need) -> bool) -> PreflightReport {
        PreflightReport {
            provider: PROVIDER_ID.into(),
            host_id: self.host_id.clone(),
            cli_version: self.provider_version.clone(),
            results: self
                .results
                .iter()
                .filter(|(need, _)| include(*need))
                .map(|(_, result)| result.clone())
                .collect(),
            checked_at: self.checked_at.clone(),
        }
    }
}

pub(crate) struct PreflightRequest<'a> {
    pub runtime: &'a dyn ProcessRuntime,
    pub executable: &'a Path,
    /// 탐지 때의 버전 문자열. `--version`을 읽지 못했을 때 보고서에 적습니다.
    pub discovered_version: &'a str,
    pub host_id: &'a str,
    pub env: Vec<(OsString, Option<OsString>)>,
    pub checked_at: String,
}

pub(crate) async fn run(request: PreflightRequest<'_>) -> Preflight {
    let mut results = Vec::new();
    let capture = |args: Vec<OsString>| capture(&request, args);

    let version_output = capture(vec!["--version".into()]).await;
    let observed = version_output
        .as_ref()
        .ok()
        .and_then(|out| out.stdout.first())
        .cloned();
    let version = observed
        .as_deref()
        .and_then(parse_version)
        .map(str::to_owned);
    results.push((
        Need::Base,
        PreflightResult {
            check: PreflightCheck::VersionOutput,
            passed: version.is_some(),
            detail: match (&version, &version_output) {
                (Some(_), _) => None,
                (None, Ok(_)) => Some(format!(
                    "unexpected --version output: {}",
                    shorten(observed.as_deref().unwrap_or("<empty>"))
                )),
                (None, Err(error)) => Some(error.clone()),
            },
        },
    ));

    let help = capture(vec!["--help".into()]).await;
    let defined = help.as_ref().map(|out| help_flags(&out.stdout));
    for (flag, need) in HELP_FLAGS {
        let (passed, detail) = match &defined {
            Ok(flags) if flags.iter().any(|f| f == flag) => (true, None),
            Ok(_) => (false, Some("not defined in --help".to_owned())),
            Err(error) => (false, Some((*error).clone())),
        };
        results.push((
            *need,
            PreflightResult {
                check: PreflightCheck::HelpContains {
                    flag: (*flag).into(),
                },
                passed,
                detail,
            },
        ));
    }

    // 모르는 옵션은 한 번에 하나만 보고되므로, 거부된 옵션을 빼고 남은 옵션으로 다시 확인합니다.
    let mut active: Vec<usize> = (0..PROBE_OPTIONS.len()).collect();
    let mut failed: Vec<(usize, String)> = Vec::new();
    while !active.is_empty() {
        let args = active
            .iter()
            .flat_map(|&i| {
                let (name, values, _) = PROBE_OPTIONS[i];
                std::iter::once(name).chain(values.iter().copied())
            })
            .map(OsString::from)
            .collect();
        let rejected: Vec<(usize, String)> = match capture(args).await {
            Err(error) => active.iter().map(|&i| (i, error.clone())).collect(),
            Ok(out) => match out.error_line() {
                None => break,
                Some(line) => {
                    let named: Vec<usize> = active
                        .iter()
                        .copied()
                        .filter(|&i| names_option(line, PROBE_OPTIONS[i].0))
                        .collect();
                    // 어느 옵션인지 알 수 없는 오류는 남은 옵션 전체를 확인하지 못한 것으로 둡니다.
                    let targets = if named.is_empty() {
                        active.clone()
                    } else {
                        named
                    };
                    targets.into_iter().map(|i| (i, shorten(line))).collect()
                }
            },
        };
        for (index, detail) in rejected {
            if !active.contains(&index) {
                continue;
            }
            active.retain(|&i| i != index);
            failed.push((index, detail));
            // `--fork-session`은 `--resume` 없이 쓸 수 없습니다.
            if PROBE_OPTIONS[index].0 == "--resume"
                && let Some(fork) = active
                    .iter()
                    .copied()
                    .find(|&i| PROBE_OPTIONS[i].0 == "--fork-session")
            {
                active.retain(|&i| i != fork);
                failed.push((fork, "requires --resume, which was rejected".into()));
            }
        }
    }
    for (index, (name, values, need)) in PROBE_OPTIONS.iter().enumerate() {
        let detail = failed
            .iter()
            .find(|(i, _)| *i == index)
            .map(|(_, d)| d.clone());
        let option = option_label(name, values);
        results.push((
            *need,
            PreflightResult {
                check: PreflightCheck::OptionAccepted { option },
                passed: detail.is_none(),
                detail,
            },
        ));
    }

    Preflight {
        provider_version: observed
            .filter(|_| version.is_some())
            .unwrap_or_else(|| request.discovered_version.to_owned()),
        version,
        host_id: request.host_id.to_owned(),
        checked_at: request.checked_at,
        results,
    }
}

struct Captured {
    stdout: Vec<String>,
    stderr: Vec<String>,
}

impl Captured {
    /// CLI가 옵션·설정을 거부할 때 내는 `error: ...` 줄입니다.
    fn error_line(&self) -> Option<&str> {
        self.stderr
            .iter()
            .chain(&self.stdout)
            .map(|line| line.trim())
            .find(|line| {
                line.get(..6)
                    .is_some_and(|head| head.eq_ignore_ascii_case("error:"))
            })
    }
}

/// 짧게 끝나는 명령을 실행해 출력을 모읍니다. stdin은 주지 않으며 시간 제한을 넘기면 끝냅니다.
async fn capture(request: &PreflightRequest<'_>, args: Vec<OsString>) -> Result<Captured, String> {
    let handle = request
        .runtime
        .spawn(CommandSpec {
            program: request.executable.to_path_buf(),
            args,
            cwd: std::env::temp_dir(),
            env: request.env.clone(),
            stdin: StdinMode::Null,
            limits: CHECK_LIMITS,
        })
        .await
        .map_err(|error| format!("could not run the CLI: {error}"))?;
    let mut output = handle
        .take_output()
        .map_err(|error| format!("could not read CLI output: {error}"))?;
    let mut captured = Captured {
        stdout: Vec::new(),
        stderr: Vec::new(),
    };
    let deadline = Instant::now() + CHECK_TIMEOUT;
    let mut open = 2;
    while open > 0 {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let Ok(event) = tokio::time::timeout(remaining, output.next()).await else {
            let _ = handle.terminate(Duration::ZERO).await;
            return Err(format!(
                "the CLI did not finish within {}s",
                CHECK_TIMEOUT.as_secs()
            ));
        };
        match event {
            None => break,
            Some(OutputEvent::Line { stream, data, .. }) => {
                let line = String::from_utf8_lossy(&data).trim_end().to_owned();
                match stream {
                    OutputStream::Stdout => captured.stdout.push(line),
                    OutputStream::Stderr => captured.stderr.push(line),
                }
            }
            Some(OutputEvent::LineTooLong { .. }) => {}
            Some(OutputEvent::Closed { .. } | OutputEvent::ReadError { .. }) => open -= 1,
        }
    }
    // 남은 자식까지 회수합니다. 이미 끝났으면 아무 일도 하지 않습니다.
    let _ = handle.terminate(Duration::ZERO).await;
    Ok(captured)
}

/// 보고서에 적는 옵션 표기입니다. 값에 따라 수용 여부가 갈리는 옵션은 점검한 값을 함께 적습니다.
fn option_label(name: &str, values: &[&str]) -> String {
    const WITH_VALUE: &[&str] = &[
        "--input-format",
        "--output-format",
        "--permission-mode",
        "--permission-prompt-tool",
    ];
    match values.first() {
        Some(value) if WITH_VALUE.contains(&name) => format!("{name} {value}"),
        _ => name.to_owned(),
    }
}

/// `--help`에서 옵션 정의 줄(두 칸 들여쓰기 뒤 `-`)의 옵션 이름을 모읍니다. 설명 속에 언급된 옵션은 세지 않습니다.
fn help_flags(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter(|line| line.starts_with("  -"))
        .flat_map(|line| {
            line.split_whitespace()
                .map(|token| token.trim_end_matches(','))
                .take_while(|token| token.starts_with('-'))
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect()
}

/// 오류 줄의 따옴표 안(`'--allowedTools, --allowed-tools <tools...>'`)에 옵션 이름이 있는지 봅니다.
fn names_option(line: &str, name: &str) -> bool {
    line.split('\'').nth(1).is_some_and(|quoted| {
        quoted
            .split([',', ' ', '<', '['])
            .any(|token| token == name)
    })
}

fn shorten(text: &str) -> String {
    match text.char_indices().nth(200) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_output_must_match_the_expected_format() {
        assert_eq!(parse_version("2.1.325 (Claude Code)\n"), Some("2.1.325"));
        assert_eq!(parse_version("2.1.325"), None);
        assert_eq!(parse_version("claude 2.1.325 (Claude Code)"), None);
        assert_eq!(parse_version("2.1 (Claude Code)"), None);
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn help_flags_come_from_definition_lines_only() {
        let lines: Vec<String> = [
            "Options:",
            "  --allowedTools, --allowed-tools <tools...>",
            "      Comma or space-separated list",
            "  -p, --print                           Print response and exit",
            "  --permission-prompts <target>         Who answers: \"host\" (the SDK host or",
            "                                        --permission-prompt-tool) or \"none\"",
        ]
        .map(String::from)
        .to_vec();
        let flags = help_flags(&lines);
        assert_eq!(
            flags,
            [
                "--allowedTools",
                "--allowed-tools",
                "-p",
                "--print",
                "--permission-prompts"
            ]
        );
    }

    #[test]
    fn option_errors_name_the_rejected_option() {
        assert!(names_option(
            "error: unknown option '--fork-session'",
            "--fork-session"
        ));
        let choice = "error: option '--permission-mode <mode>' argument 'default' is invalid.";
        assert!(names_option(choice, "--permission-mode"));
        assert!(!names_option(choice, "-p"));
        let alias = "error: option '--allowedTools, --allowed-tools <tools...>' argument missing";
        assert!(names_option(alias, "--allowedTools"));
        assert!(!names_option(alias, "--tools"));
        assert!(!names_option(
            "error: unknown option '--strict-mcp-config'",
            "--mcp-config"
        ));
    }

    #[test]
    fn builtin_evidence_is_recorded_per_platform_and_version() {
        let evidence = builtin_evidence();
        assert!(
            evidence
                .iter()
                .all(|e| e.passed && e.provider == PROVIDER_ID)
        );
        let has = |key: &str, platform: &str, version: &str| {
            evidence
                .iter()
                .any(|e| e.key == key && e.platform == platform && e.cli_version == version)
        };
        assert!(has(keys::NATIVE_FORK, "win32-x64", "2.1.318"));
        assert!(has(keys::MANAGED_RUN, "linux-x64", "2.1.310"));
        assert!(has(keys::VSCODE_LISTING, "linux-x64", "2.1.310"));
        // Adapter 승인 경로는 Linux와 2.1.318에서 실행하지 않았습니다.
        assert!(!has(keys::ANSWER_APPROVAL, "linux-x64", "2.1.310"));
        assert!(!has(keys::ANSWER_APPROVAL, "win32-x64", "2.1.318"));
    }
}

use std::fs;
use std::path::Path;

use rust_multi_cli_harness::Config;

#[test]
fn parses_tools_toml_with_expected_tools() {
    let path = Path::new("tools.toml");
    let content = fs::read_to_string(path).expect("tools.toml should be readable");

    let parsed = Config::from_toml_str(&content).expect("tools.toml should parse");

    assert_eq!(parsed.version, 1);
    assert!(parsed.supervisor.is_some());
    assert_eq!(parsed.tools.len(), 3);
    assert_eq!(parsed.tools[0].id, "antigravity-cli");
    assert_eq!(parsed.tools[1].id, "codex-cli");
    assert_eq!(parsed.tools[2].id, "copilot-cli");

    let supervisor = parsed.supervisor.as_ref().expect("supervisor should parse");
    assert_eq!(supervisor.command, "codex");
    assert!(supervisor.args.contains(&"exec".to_owned()));
    assert!(supervisor.args.contains(&"--json".to_owned()));

    assert!(supervisor
        .parser
        .script
        .contains(r#""summary": stdout_text"#));
    assert!(supervisor
        .args
        .iter()
        .any(|arg| arg.contains("\"actions\"")));
    assert!(!supervisor.parser.script.contains("check for code logic"));

    assert_eq!(parsed.tools[0].command, "agy");
    assert_eq!(parsed.tools[0].args.last().map(String::as_str), Some("-p"));
    assert_eq!(parsed.tools[1].command, "codex");
    assert_eq!(
        parsed.tools[1].args,
        vec!["exec", "--sandbox", "read-only", "--json"]
    );
    assert_eq!(parsed.tools[2].command, "copilot");
    assert_eq!(
        parsed.tools[2].args,
        vec!["--output-format", "json", "--allow-all-tools", "-p"]
    );

    assert_eq!(parsed.tools[0].parser.entrypoint, "parse");
    assert!(parsed.tools[0].parser.script.contains("stdout_text"));
    assert!(parsed.tools[1].parser.script.contains("exit_code != 0"));
    assert!(parsed.tools[2].parser.script.contains("exit_code != 0"));
}

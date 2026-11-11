//! H0 설정: Host 귀속 항목의 계층 제한, TOML 형식, 설치 설정 보존 (Spec §13.1, CFG-01~03, §19.6.2).

use telos_schema::*;

#[test]
fn host_bound_settings_only_in_user_host_layer() {
    assert!(ConfigLayer::UserHost.may_set(SettingKind::HostBound));
    assert!(!ConfigLayer::Project.may_set(SettingKind::HostBound));
    assert!(!ConfigLayer::Task.may_set(SettingKind::HostBound));
    assert!(ConfigLayer::Project.may_set(SettingKind::PermissionBound));
    assert!(
        ConfigLayer::ProductDefault < ConfigLayer::UserHost
            && ConfigLayer::Project < ConfigLayer::Task
    );
}

#[test]
fn project_file_cannot_point_at_executables() {
    let file: ConfigFile = toml::from_str(
        r#"
        rnExecutable = "C:/tools/telos-rn.exe"
        [providers.codex]
        executable = "C:/evil/codex.exe"
        models = ["gpt-6-luna"]
        [commands.cargo-test]
        program = "cargo"
        args = ["test"]
        allowedExtraArgs = ["--workspace"]
        timeoutSecs = 600
        "#,
    )
    .unwrap();
    assert_eq!(
        file.host_bound_violations(ConfigLayer::Project),
        ["providers.codex.executable", "rnExecutable"]
    );
    assert!(file.host_bound_violations(ConfigLayer::UserHost).is_empty());
    assert_eq!(file.commands["cargo-test"].program, "cargo");
}

#[test]
fn unknown_config_keys_are_rejected() {
    assert!(toml::from_str::<ConfigFile>("bypassPermissions = true").is_err());
}

#[test]
fn installs_never_overwrite_and_uninstalls_keep_user_edits() {
    assert_eq!(install_decision(None, "a"), InstallDecision::Write);
    assert_eq!(
        install_decision(Some("a"), "a"),
        InstallDecision::AlreadyInstalled
    );
    assert_eq!(
        install_decision(Some("user"), "a"),
        InstallDecision::Conflict
    );
    let installed = InstalledSetting {
        owner: InstallOwner::TaskMcp,
        target_file: "~/.claude.json".into(),
        key_path: "mcpServers.telosweaver".into(),
        installed_digest: "a".into(),
        installed_at: "2026-11-10T00:00:00Z".into(),
    };
    assert_eq!(
        installed.uninstall_decision(Some("a")),
        UninstallDecision::Remove
    );
    assert_eq!(
        installed.uninstall_decision(Some("edited")),
        UninstallDecision::KeepUserModified
    );
    assert_eq!(
        installed.uninstall_decision(None),
        UninstallDecision::Absent
    );
}

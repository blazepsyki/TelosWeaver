//! Linux Remote SSH discovery without model calls (Spec §7.5, PROV-01, REQ-010).
#![cfg(target_os = "linux")]

use std::{path::PathBuf, sync::Arc};
use telos_adapters::{
    CliDiscoverer, DiscoveryRequest, NativeCliDiscoverer, agy, claude, codex,
    runtime::NativeRuntime,
};
use telos_schema::{BinaryLocation, CliSource, ExtensionResolution};

#[tokio::test]
#[ignore = "requires the psyki-nas Remote SSH extensions; runs only --version"]
async fn remote_ssh_active_extensions_and_managed_agy() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let root = home.join(".vscode-server/extensions");
    let metadata: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("extensions.json")).unwrap()).unwrap();
    let obsolete: serde_json::Value = match std::fs::read(root.join(".obsolete")) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => serde_json::json!({}),
        Err(error) => panic!("obsolete metadata: {error}"),
    };
    let request = DiscoveryRequest {
        host_id: "psyki-nas".into(),
        target_platform: "linux-x64".into(),
        explicit_path: None,
        extension_roots: vec![root.clone()],
        home_dir: home.clone(),
        path_env: std::env::var_os("PATH"),
        checked_at: "2026-10-25T00:00:00Z".into(),
    };
    let discoverer = NativeCliDiscoverer::new(Arc::new(NativeRuntime));
    for (provider, rule, expected_version, expected_cli) in [
        (
            "claude",
            claude::cli_rule("linux-x64").unwrap(),
            "2.1.310",
            "2.1.310 (Claude Code)",
        ),
        (
            "codex",
            codex::cli_rule("linux-x64").unwrap(),
            "26.1021.22027",
            "codex-cli 0.164.0-alpha.11.1",
        ),
        (
            "antigravity",
            agy::cli_rule("linux-x64").unwrap(),
            "1.9.0",
            "1.4.7",
        ),
    ] {
        let mut installed: Vec<_> = std::fs::read_dir(&root)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(&format!("{}-", rule.extension_id)))
            .collect();
        installed.sort();
        println!(
            "installed {provider}: {}",
            serde_json::to_string(&installed).unwrap()
        );
        if provider != "antigravity" {
            assert!(installed.len() >= 2, "requires two installed versions");
        }
        let active = metadata
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["identifier"]["id"] == rule.extension_id)
            .unwrap();
        let relative = active["relativeLocation"].as_str().unwrap();
        println!(
            "active {provider}: {relative}; obsolete={}",
            obsolete[relative]
        );
        assert_eq!(active["version"], expected_version);
        assert_ne!(obsolete[relative], true);
        let record = discoverer.discover(provider, &rule, &request).await;
        println!("discovery {}", serde_json::to_string(&record).unwrap());
        let cli = record.found().expect("CLI must be found");
        assert_eq!(cli.source, CliSource::Extension);
        assert_eq!(cli.cli_version, expected_cli);
        assert_eq!(cli.sha256.len(), 64);
        let extension = cli.extension.as_ref().unwrap();
        assert_eq!(extension.version, expected_version);
        assert_eq!(extension.resolution, ExtensionResolution::Metadata);
        assert_eq!(PathBuf::from(&extension.install_path), root.join(relative));
        if provider == "antigravity" {
            assert_eq!(cli.location, BinaryLocation::ManagedPath);
            assert_eq!(
                PathBuf::from(&cli.executable_path),
                home.join(".gemini/bin/agy")
            );
        } else {
            assert_eq!(cli.location, BinaryLocation::RelativePath);
            assert!(PathBuf::from(&cli.executable_path).starts_with(root.join(relative)));
        }
    }
}

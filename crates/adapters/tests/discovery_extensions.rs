#[path = "support/runtime/discovery.rs"]
mod support;
use std::{fs, path::Path, sync::Arc};
use support::*;
use telos_schema::{
    BinaryLocation, CliDiscoveryFailure as Failure, CliSource, ExtensionResolution,
};

#[tokio::test]
async fn numeric_version_platform_and_obsolete_are_reported_through_public_api() {
    let files = Files::new();
    for folder in [
        "sample.agent-1.9.0-win32-x64",
        "sample.agent-1.10.0-win32-x64",
        "sample.agent-8.0.0-linux-x64",
        "sample.agent-9.0.0-win32-x64",
    ] {
        files.binary(format!("extensions/{folder}/bin/tool.exe"));
    }
    files.write(
        "extensions/.obsolete",
        br#"{"sample.agent-9.0.0-win32-x64":true}"#,
    );
    let mut request = files.request();
    request.extension_roots = vec![files.root.join("extensions")];
    let record = discover(Arc::new(Probe::default()), &rule(), &request).await;
    assert_eq!(record.provider_id, "fixture-provider");
    assert_eq!(record.host_id, request.host_id);
    assert_eq!(record.checked_at, request.checked_at);
    let cli = record.found().unwrap();
    assert_eq!(cli.source, CliSource::Extension);
    assert_eq!(cli.location, BinaryLocation::RelativePath);
    let extension = cli.extension.as_ref().unwrap();
    assert_eq!(extension.extension_id, "sample.agent");
    assert_eq!(extension.version, "1.10.0");
    assert_eq!(extension.target_platform.as_deref(), Some("win32-x64"));
    assert_eq!(extension.resolution, ExtensionResolution::FolderScan);
    assert_eq!(
        Path::new(&cli.executable_path),
        files
            .root
            .join("extensions/sample.agent-1.10.0-win32-x64/bin/tool.exe")
            .canonicalize()
            .unwrap()
    );
}
#[tokio::test]
async fn active_metadata_overrides_newer_folders_and_reports_platform() {
    let files = Files::new();
    files.binary("extensions/sample.agent-1.0.0/bin/tool.exe");
    files.binary("extensions/sample.agent-9.0.0/bin/tool.exe");
    files.write("extensions/extensions.json",br#"[{"identifier":{"id":"SAMPLE.AGENT"},"version":"1.0.0","relativeLocation":"sample.agent-1.0.0","metadata":{"targetPlatform":"win32-x64"}}]"#);
    let mut request = files.request();
    request.extension_roots = vec![files.root.join("extensions")];
    let record = discover(Arc::new(Probe::default()), &rule(), &request).await;
    let extension = record.found().unwrap().extension.as_ref().unwrap();
    assert_eq!(extension.version, "1.0.0");
    assert_eq!(extension.resolution, ExtensionResolution::Metadata);
    assert_eq!(extension.target_platform.as_deref(), Some("win32-x64"));
}
#[tokio::test]
async fn universal_and_unsuffixed_packages_omit_target_platform() {
    for (folder,metadata) in [("sample.agent-1.0.0-universal",Some(br#"[{"identifier":{"id":"sample.agent"},"version":"1.0.0","relativeLocation":"sample.agent-1.0.0-universal","metadata":{"targetPlatform":"universal"}}]"#.as_slice())),("sample.agent-1.0.0",None)] {
        let files=Files::new();files.binary(format!("extensions/{folder}/bin/tool.exe"));if let Some(metadata)=metadata {files.write("extensions/extensions.json",metadata);}
        let mut request=files.request();request.extension_roots=vec![files.root.join("extensions")];
        let record=discover(Arc::new(Probe::default()),&rule(),&request).await;assert!(record.found().unwrap().extension.as_ref().unwrap().target_platform.is_none());
    }
}

// REQ-013(3), Spec §7.5: active universal metadata must win over newer folders.
#[tokio::test]
async fn undefined_platform_metadata_preserves_active_selection() {
    for target in ["win32-x64", "linux-x64", "darwin-arm64"] {
        let files = Files::new();
        files.binary("extensions/sample.agent-1.0.0/bin/tool.exe");
        files.binary("extensions/sample.agent-9.0.0/bin/tool.exe");
        files.write(
            "extensions/extensions.json",
            br#"[{"identifier":{"id":"sample.agent"},"version":"1.0.0","relativeLocation":"sample.agent-1.0.0","metadata":{"targetPlatform":"undefined"}}]"#,
        );
        let mut request = files.request();
        request.target_platform = target.into();
        request.extension_roots = vec![files.root.join("extensions")];
        let record = discover(Arc::new(Probe::default()), &rule(), &request).await;
        let extension = record.found().unwrap().extension.as_ref().unwrap();
        assert_eq!(extension.version, "1.0.0", "{target}");
        assert_eq!(extension.resolution, ExtensionResolution::Metadata);
        assert_eq!(extension.target_platform, None);
    }
}

#[tokio::test]
async fn undefined_platform_metadata_preserves_ambiguity_and_obsolete_rejection() {
    for obsolete in [false, true] {
        let files = Files::new();
        files.binary("extensions/sample.agent-1.0.0/bin/tool.exe");
        let entry = serde_json::json!({
            "identifier": {"id": "sample.agent"},
            "version": "1.0.0",
            "relativeLocation": "sample.agent-1.0.0",
            "metadata": {"targetPlatform": "undefined"}
        });
        let entries = if obsolete {
            files.write("extensions/.obsolete", br#"{"sample.agent-1.0.0":true}"#);
            serde_json::json!([entry])
        } else {
            serde_json::json!([entry.clone(), entry])
        };
        files.write(
            "extensions/extensions.json",
            serde_json::to_vec(&entries).unwrap(),
        );
        let mut request = files.request();
        request.extension_roots = vec![files.root.join("extensions")];
        let runtime = Arc::new(Probe::default());
        let record = discover(runtime.clone(), &rule(), &request).await;
        unavailable(
            &record,
            if obsolete {
                Failure::InvalidMetadata
            } else {
                Failure::Ambiguous
            },
            Some(CliSource::Extension),
        );
        assert!(runtime.programs.lock().unwrap().is_empty());
    }
}
#[tokio::test]
async fn malformed_metadata_and_ambiguity_are_classified_without_executing() {
    for (file, content, expected) in [
        (
            "extensions.json",
            "[fixture-secret",
            Failure::InvalidMetadata,
        ),
        (
            "extensions.json",
            r#"[{"identifier":{"id":"sample.agent"}},{"identifier":{"id":"sample.agent"}}]"#,
            Failure::Ambiguous,
        ),
        (
            "extensions.json",
            r#"[{"identifier":{"id":"sample.agent"},"version":"1.0.0","relativeLocation":"../outside"}]"#,
            Failure::InvalidMetadata,
        ),
        (
            "extensions.json",
            r#"[{"identifier":{"id":"sample.agent"},"version":"1.0.0","relativeLocation":"active","metadata":{"targetPlatform":1}}]"#,
            Failure::InvalidMetadata,
        ),
        (
            "extensions.json",
            r#"[{"identifier":{"id":"sample.agent"},"version":"invalid","relativeLocation":"active"}]"#,
            Failure::InvalidMetadata,
        ),
        (".obsolete", "[]", Failure::InvalidMetadata),
        (
            ".obsolete",
            r#"{"active":"true"}"#,
            Failure::InvalidMetadata,
        ),
    ] {
        let files = Files::new();
        files.write(format!("extensions/{file}"), content);
        let mut request = files.request();
        request.extension_roots = vec![files.root.join("extensions")];
        let runtime = Arc::new(Probe::default());
        let record = discover(runtime.clone(), &rule(), &request).await;
        unavailable(&record, expected, Some(CliSource::Extension));
        assert!(runtime.programs.lock().unwrap().is_empty());
    }
}
#[tokio::test]
async fn obsolete_active_metadata_and_equal_version_candidates_are_rejected() {
    let files = Files::new();
    files.binary("extensions/sample.agent-1.0.0/bin/tool.exe");
    files.write("extensions/.obsolete", br#"{"sample.agent-1.0.0":true}"#);
    files.write("extensions/extensions.json",br#"[{"identifier":{"id":"sample.agent"},"version":"1.0.0","relativeLocation":"sample.agent-1.0.0"}]"#);
    let mut request = files.request();
    request.extension_roots = vec![files.root.join("extensions")];
    unavailable(
        &discover(Arc::new(Probe::default()), &rule(), &request).await,
        Failure::InvalidMetadata,
        Some(CliSource::Extension),
    );
    fs::remove_file(files.root.join("extensions/extensions.json")).unwrap();
    fs::remove_file(files.root.join("extensions/.obsolete")).unwrap();
    files.binary("extensions/sample.agent-1.0.0-win32-x64/bin/tool.exe");
    unavailable(
        &discover(Arc::new(Probe::default()), &rule(), &request).await,
        Failure::Ambiguous,
        Some(CliSource::Extension),
    );
}
#[tokio::test]
async fn name_search_reports_location_hash_and_rejects_multiple_candidates() {
    let files = Files::new();
    let binary = files.binary("extensions/sample.agent-1.0.0/nested/tool.exe");
    let mut request = files.request();
    request.extension_roots = vec![files.root.join("extensions")];
    let runtime = Arc::new(Probe::default());
    let record = discover(runtime.clone(), &rule(), &request).await;
    let cli = record.found().unwrap();
    assert_eq!(cli.location, BinaryLocation::NameSearch);
    assert_eq!(
        Path::new(&cli.executable_path),
        binary.canonicalize().unwrap()
    );
    assert_eq!(
        cli.sha256,
        "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
    );
    files.binary("extensions/sample.agent-1.0.0/another/tool.exe");
    unavailable(
        &discover(runtime.clone(), &rule(), &request).await,
        Failure::Ambiguous,
        Some(CliSource::Extension),
    );
    assert_eq!(runtime.programs.lock().unwrap().len(), 1);
}
#[tokio::test]
async fn relative_path_entries_are_ignored_even_when_the_file_exists() {
    let files = Files::new();
    let mut request = files.request();
    let mut rule = rule();
    rule.binary_name = "Cargo.toml".into();
    let relative = Path::new(".");
    assert!(relative.join("Cargo.toml").is_file());
    request.path_env = Some(relative.as_os_str().into());
    let runtime = Arc::new(Probe::default());
    unavailable(
        &discover(runtime.clone(), &rule, &request).await,
        Failure::NotFound,
        None,
    );
    assert!(runtime.programs.lock().unwrap().is_empty());
}
#[tokio::test]
async fn extension_roots_follow_host_priority_and_other_platforms_are_excluded() {
    let files = Files::new();
    let chosen = files.binary("first/sample.agent-1.0.0/bin/tool.exe");
    files.binary("second/sample.agent-9.0.0/bin/tool.exe");
    let mut request = files.request();
    request.extension_roots = vec![files.root.join("first"), files.root.join("second")];
    let record = discover(Arc::new(Probe::default()), &rule(), &request).await;
    assert_eq!(
        Path::new(&record.found().unwrap().executable_path),
        chosen.canonicalize().unwrap()
    );
    files.binary("foreign/sample.agent-10.0.0-linux-x64/bin/tool.exe");
    request.extension_roots = vec![files.root.join("foreign")];
    unavailable(
        &discover(Arc::new(Probe::default()), &rule(), &request).await,
        Failure::NotFound,
        None,
    );
}

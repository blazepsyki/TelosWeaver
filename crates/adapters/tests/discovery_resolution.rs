#[path = "support/runtime/discovery.rs"]
mod support;
use std::{
    path::Path,
    sync::{Arc, atomic::Ordering},
};
use support::*;
use telos_adapters::{CliBinaryRule, CliDiscoverer, NativeCliDiscoverer, runtime::NativeRuntime};
use telos_schema::{BinaryLocation, CliDiscoveryFailure as Failure, CliSource};

#[tokio::test]
async fn explicit_extension_path_precedence_uses_public_records() {
    let files = Files::new();
    let explicit = files.binary("explicit.exe");
    let extension = files.binary("extensions/sample.agent-1.0.0/bin/tool.exe");
    let path = files.binary("path/tool.exe");
    let mut request = files.request();
    request.explicit_path = Some(explicit.clone());
    request.extension_roots = vec![files.root.join("extensions")];
    request.path_env = Some(std::env::join_paths([path.parent().unwrap()]).unwrap());
    let runtime = Arc::new(Probe::default());
    for (expected, source, location) in [
        (explicit, CliSource::Explicit, BinaryLocation::Direct),
        (
            extension,
            CliSource::Extension,
            BinaryLocation::RelativePath,
        ),
        (path, CliSource::Path, BinaryLocation::Direct),
    ] {
        let record = discover(runtime.clone(), &rule(), &request).await;
        let cli = record.found().unwrap();
        assert_eq!(
            Path::new(&cli.executable_path),
            expected.canonicalize().unwrap()
        );
        assert_eq!(cli.source, source);
        assert_eq!(cli.location, location);
        assert_eq!(cli.cli_version, "tool 1.2.3");
        assert_eq!(
            cli.sha256,
            "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
        );
        assert_eq!(cli.extension.is_some(), source == CliSource::Extension);
        if request.explicit_path.take().is_none() {
            request.extension_roots.clear();
        }
    }
    request.path_env = None;
    unavailable(
        &discover(runtime.clone(), &rule(), &request).await,
        Failure::NotFound,
        None,
    );
    assert_eq!(runtime.programs.lock().unwrap().len(), 3);
}
#[tokio::test]
async fn invalid_explicit_path_and_failed_version_do_not_fall_back() {
    let files = Files::new();
    let path = files.binary("path/tool.exe");
    let mut request = files.request();
    request.path_env = Some(path.parent().unwrap().as_os_str().into());
    request.explicit_path = Some(files.root.join("missing.exe"));
    let runtime = Arc::new(Probe::default());
    unavailable(
        &discover(runtime.clone(), &rule(), &request).await,
        Failure::InvalidPath,
        Some(CliSource::Explicit),
    );
    assert!(runtime.programs.lock().unwrap().is_empty());
    request.explicit_path = Some(files.binary("explicit.exe"));
    let runtime = Arc::new(Probe {
        behavior: Behavior::Nonzero,
        ..Default::default()
    });
    unavailable(
        &discover(runtime.clone(), &rule(), &request).await,
        Failure::VersionCheckFailed,
        Some(CliSource::Explicit),
    );
    assert_eq!(runtime.programs.lock().unwrap().len(), 1);
    assert_eq!(runtime.terminations.load(Ordering::Relaxed), 1);
}
#[tokio::test]
async fn managed_binary_requires_an_installed_extension_and_reports_location() {
    let files = Files::new();
    let managed = files.binary("home/.gemini/bin/tool.exe");
    let mut rule = rule();
    rule.binary = CliBinaryRule::Managed {
        home_relative_path: ".gemini/bin/tool.exe".into(),
    };
    let mut request = files.request();
    request.extension_roots = vec![files.root.join("extensions")];
    let runtime = Arc::new(Probe::default());
    unavailable(
        &discover(runtime.clone(), &rule, &request).await,
        Failure::NotFound,
        None,
    );
    assert!(runtime.programs.lock().unwrap().is_empty());
    std::fs::create_dir_all(files.root.join("extensions/sample.agent-1.0.0")).unwrap();
    let record = discover(runtime.clone(), &rule, &request).await;
    let cli = record.found().unwrap();
    assert_eq!(
        Path::new(&cli.executable_path),
        managed.canonicalize().unwrap()
    );
    assert_eq!(cli.source, CliSource::Extension);
    assert_eq!(cli.location, BinaryLocation::ManagedPath);
    assert!(cli.extension.is_some());
    rule.binary = CliBinaryRule::Managed {
        home_relative_path: "../outside.exe".into(),
    };
    unavailable(
        &discover(runtime, &rule, &request).await,
        Failure::InvalidPath,
        Some(CliSource::Extension),
    );
}
#[tokio::test]
async fn changed_executable_is_a_typed_failure() {
    let files = Files::new();
    let mut request = files.request();
    request.explicit_path = Some(files.binary("tool.exe"));
    let runtime = Arc::new(Probe {
        behavior: Behavior::MutateBinary,
        ..Default::default()
    });
    unavailable(
        &discover(runtime, &rule(), &request).await,
        Failure::ChangedDuringDiscovery,
        Some(CliSource::Explicit),
    );
}
#[tokio::test]
async fn version_start_output_and_exit_failures_are_classified_and_cleaned_up() {
    let files = Files::new();
    let mut request = files.request();
    request.explicit_path = Some(files.binary("tool.exe"));
    for behavior in [
        Behavior::SpawnFailure,
        Behavior::ReadFailure,
        Behavior::LongLine,
        Behavior::InvalidUtf8,
        Behavior::Empty,
        Behavior::TooMuchOutput,
        Behavior::ExternalExit,
        Behavior::UnknownExit,
    ] {
        let runtime = Arc::new(Probe {
            behavior,
            ..Default::default()
        });
        let record = discover(runtime.clone(), &rule(), &request).await;
        unavailable(
            &record,
            Failure::VersionCheckFailed,
            Some(CliSource::Explicit),
        );
        assert_eq!(
            runtime.terminations.load(Ordering::Relaxed),
            usize::from(!matches!(behavior, Behavior::SpawnFailure)),
            "{behavior:?}"
        );
    }
}
#[tokio::test]
async fn version_timeout_requests_process_cleanup() {
    let files = Files::new();
    let mut request = files.request();
    request.explicit_path = Some(files.binary("tool.exe"));
    let runtime = Arc::new(Probe {
        behavior: Behavior::Pending,
        ..Default::default()
    });
    let record = tokio::time::timeout(
        std::time::Duration::from_secs(7),
        discover(runtime.clone(), &rule(), &request),
    )
    .await
    .unwrap();
    unavailable(
        &record,
        Failure::VersionCheckFailed,
        Some(CliSource::Explicit),
    );
    assert_eq!(runtime.terminations.load(Ordering::Relaxed), 1);
}
#[tokio::test]
async fn unsupported_platform_io_error_and_invalid_rule_are_distinct() {
    let files = Files::new();
    let mut request = files.request();
    request.target_platform = "unknown-x64".into();
    let runtime = Arc::new(Probe::default());
    unavailable(
        &discover(runtime.clone(), &rule(), &request).await,
        Failure::UnsupportedPlatform,
        None,
    );
    request.target_platform = "win32-x64".into();
    request.extension_roots = vec![files.write("not-a-directory", b"test")];
    unavailable(
        &discover(runtime.clone(), &rule(), &request).await,
        Failure::IoError,
        Some(CliSource::Extension),
    );
    request.extension_roots.clear();
    let mut rule = rule();
    rule.binary_name = "../tool.exe".into();
    unavailable(
        &discover(runtime.clone(), &rule, &request).await,
        Failure::InvalidPath,
        None,
    );
    assert!(runtime.programs.lock().unwrap().is_empty());
}
#[cfg(windows)]
#[tokio::test]
async fn non_utf8_windows_path_is_rejected_before_execution() {
    use std::os::windows::ffi::OsStringExt;
    let files = Files::new();
    let name = std::ffi::OsString::from_wide(&[
        b'x' as u16,
        0xd800,
        b'.' as u16,
        b'e' as u16,
        b'x' as u16,
        b'e' as u16,
    ]);
    let binary = files.binary(Path::new(&name));
    let mut request = files.request();
    request.explicit_path = Some(binary);
    let runtime = Arc::new(Probe::default());
    unavailable(
        &discover(runtime.clone(), &rule(), &request).await,
        Failure::InvalidPath,
        Some(CliSource::Explicit),
    );
    assert!(runtime.programs.lock().unwrap().is_empty());
}
#[tokio::test(flavor = "multi_thread")]
async fn public_discoverer_probes_a_real_local_process() {
    let files = Files::new();
    let source = files.write("fixture.rs", include_str!("fixtures/runtime/fixture.rs"));
    let executable = files.root.join("tool.exe");
    assert!(
        std::process::Command::new("rustc")
            .arg(source)
            .arg("-o")
            .arg(&executable)
            .status()
            .unwrap()
            .success()
    );
    let mut request = files.request();
    request.explicit_path = Some(executable.clone());
    let record = NativeCliDiscoverer::new(Arc::new(NativeRuntime))
        .discover("fixture-provider", &rule(), &request)
        .await;
    let cli = record.found().unwrap();
    assert_eq!(cli.cli_version, "tool 1.2.3");
    assert_eq!(
        Path::new(&cli.executable_path),
        executable.canonicalize().unwrap()
    );
    assert_eq!(cli.sha256.len(), 64);
    assert!(
        cli.sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    );
}

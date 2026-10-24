#[path = "../src/discovery/mod.rs"]
#[allow(dead_code)]
mod discovery;
use std::{
    ffi::OsStr,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use telos_adapters::runtime;

fn root() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "telos-discovery-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&root).unwrap();
    root
}
#[test]
fn numeric_version_platform_and_obsolete_are_respected() {
    let root = root();
    for name in [
        "sample.agent-1.9.0-win32-x64",
        "sample.agent-1.10.0-win32-x64",
        "sample.agent-8.0.0-linux-x64",
        "sample.agent-9.0.0-win32-x64",
    ] {
        fs::create_dir(root.join(name)).unwrap();
    }
    fs::write(
        root.join(".obsolete"),
        r#"{"sample.agent-9.0.0-win32-x64":true}"#,
    )
    .unwrap();
    let chosen = discovery::discover_extension(&root, "sample.agent", "win32-x64")
        .unwrap()
        .unwrap();
    assert_eq!(chosen.version.to_string(), "1.10.0");
    assert!(!chosen.from_metadata);
    assert_eq!(chosen.id, "sample.agent");
}
#[test]
fn active_metadata_overrides_directory_versions_and_cannot_escape() {
    let root = root();
    fs::create_dir(root.join("sample.agent-1.0.0")).unwrap();
    fs::create_dir(root.join("sample.agent-9.0.0")).unwrap();
    fs::write(root.join("extensions.json"),r#"[{"identifier":{"id":"sample.agent"},"version":"1.0.0","relativeLocation":"sample.agent-1.0.0"}]"#).unwrap();
    let chosen = discovery::discover_extension(&root, "sample.agent", "win32-x64")
        .unwrap()
        .unwrap();
    assert!(chosen.from_metadata);
    assert!(chosen.path.ends_with("sample.agent-1.0.0"));
    fs::write(root.join("extensions.json"),r#"[{"identifier":{"id":"sample.agent"},"version":"1.0.0","relativeLocation":"../outside"}]"#).unwrap();
    assert!(discovery::discover_extension(&root, "sample.agent", "win32-x64").is_err());
}
#[test]
fn broken_or_ambiguous_metadata_is_not_silently_ignored() {
    let root = root();
    fs::write(root.join("extensions.json"), "[").unwrap();
    assert!(discovery::discover_extension(&root, "sample.agent", "win32-x64").is_err());
    fs::write(root.join("extensions.json"), "[]").unwrap();
    fs::write(root.join(".obsolete"), "[]").unwrap();
    assert!(discovery::discover_extension(&root, "sample.agent", "win32-x64").is_err());
    fs::write(root.join(".obsolete"), "{}").unwrap();
    fs::write(
        root.join("extensions.json"),
        r#"[{"identifier":{"id":"sample.agent"}},{"identifier":{"id":"sample.agent"}}]"#,
    )
    .unwrap();
    assert!(
        discovery::discover_extension(&root, "sample.agent", "win32-x64")
            .unwrap_err()
            .to_string()
            .contains("multiple active")
    );
}
#[test]
fn binary_search_requires_unique_native_candidate() {
    let root = root();
    let name = if cfg!(windows) { "tool.exe" } else { "tool" };
    fs::create_dir(root.join("nested")).unwrap();
    let binary = root.join("nested").join(name);
    fs::write(&binary, b"test").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let chosen = discovery::find_binary(&root, std::path::Path::new("missing"), OsStr::new(name))
        .unwrap()
        .unwrap();
    assert_eq!(chosen, binary.canonicalize().unwrap());
    assert_eq!(
        discovery::file_hash(&chosen).unwrap(),
        "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
    );
    fs::create_dir(root.join("another")).unwrap();
    fs::write(root.join("another").join(name), b"other").unwrap();
    assert!(
        discovery::find_binary(&root, std::path::Path::new("missing"), OsStr::new(name)).is_err()
    );
}
#[test]
fn path_search_ignores_relative_cwd_entries() {
    let root = root();
    let binary = root.join("tool.exe");
    fs::write(&binary, b"x").unwrap();
    let env = std::env::join_paths([std::path::Path::new("."), root.as_path()]).unwrap();
    assert_eq!(
        discovery::path_candidates(&env, OsStr::new("tool.exe")),
        vec![binary]
    );
}

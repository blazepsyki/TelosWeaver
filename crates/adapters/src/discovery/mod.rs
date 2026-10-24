//! Extension discovery engine (Spec §7.5). Public DTO registration is REQ-010.

use crate::runtime::{CommandSpec, OutputEvent, OutputLimits, ProcessRuntime, StdinMode};
use futures::StreamExt;
use semver::Version;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    ffi::{OsStr, OsString},
    fs, io,
    path::{Component, Path, PathBuf},
    time::Duration,
};

// These are engine-local records, not a replacement for the pending shared DTO.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExtensionInstall {
    pub id: String,
    pub version: Version,
    pub path: PathBuf,
    pub from_metadata: bool,
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
fn json_file(path: &Path) -> io::Result<Option<Value>> {
    use io::Read;
    match fs::File::open(path) {
        Ok(file) => {
            let mut bytes = Vec::new();
            file.take(4 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
            if bytes.len() > 4 * 1024 * 1024 {
                return Err(invalid("extension metadata exceeds 4 MiB"));
            }
            serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|e| invalid(e.to_string()))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}
fn confined(root: &Path, relative: &Path) -> io::Result<PathBuf> {
    if relative.is_absolute()
        || relative.components().any(|p| {
            matches!(
                p,
                Component::ParentDir | Component::Prefix(_) | Component::RootDir
            )
        })
    {
        return Err(invalid("extension path escapes its root"));
    }
    let resolved = root.join(relative).canonicalize()?;
    if !resolved.starts_with(root) {
        return Err(invalid("extension symlink escapes its root"));
    }
    Ok(resolved)
}
fn folder_version(name: &str, id: &str, target: &str) -> Option<Version> {
    let suffix = name.strip_prefix(&format!("{id}-"))?;
    for platform in [
        "win32-x64",
        "win32-arm64",
        "linux-x64",
        "linux-arm64",
        "darwin-x64",
        "darwin-arm64",
        "alpine-x64",
        "alpine-arm64",
        "universal",
        "web",
    ] {
        if let Some(version) = suffix.strip_suffix(&format!("-{platform}")) {
            return (platform == target || platform == "universal")
                .then(|| Version::parse(version).ok())
                .flatten();
        }
    }
    Version::parse(suffix).ok()
}

pub(crate) fn discover_extension(
    root: &Path,
    id: &str,
    target: &str,
) -> io::Result<Option<ExtensionInstall>> {
    let root = match root.canonicalize() {
        Ok(root) => root,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let obsolete = json_file(&root.join(".obsolete"))?.unwrap_or_else(|| serde_json::json!({}));
    if !obsolete.is_object() {
        return Err(invalid(".obsolete must be an object"));
    }
    let is_obsolete = |name: &str| obsolete.get(name).and_then(Value::as_bool) == Some(true);
    if let Some(metadata) = json_file(&root.join("extensions.json"))? {
        let entries = metadata
            .as_array()
            .ok_or_else(|| invalid("extensions.json must be an array"))?;
        let matches: Vec<_> = entries
            .iter()
            .filter(|entry| {
                entry
                    .pointer("/identifier/id")
                    .and_then(Value::as_str)
                    .is_some_and(|value| value.eq_ignore_ascii_case(id))
            })
            .filter(|entry| {
                entry
                    .pointer("/metadata/targetPlatform")
                    .and_then(Value::as_str)
                    .is_none_or(|value| value == target || value == "universal")
            })
            .collect();
        if matches.len() > 1 {
            return Err(invalid("multiple active extension metadata entries"));
        }
        if let Some(entry) = matches.first() {
            let relative = entry
                .get("relativeLocation")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("active extension has no relativeLocation"))?;
            if is_obsolete(relative) {
                return Err(invalid("active extension is marked obsolete"));
            }
            let version = entry
                .get("version")
                .and_then(Value::as_str)
                .and_then(|v| Version::parse(v).ok())
                .ok_or_else(|| invalid("invalid active extension version"))?;
            let path = confined(&root, Path::new(relative))?;
            if !path.is_dir() {
                return Err(invalid("active extension is not a directory"));
            }
            return Ok(Some(ExtensionInstall {
                id: id.into(),
                version,
                path,
                from_metadata: true,
            }));
        }
    }
    let mut candidates = Vec::new();
    for entry in fs::read_dir(&root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if is_obsolete(name) {
            continue;
        }
        if let Some(version) = folder_version(name, id, target) {
            candidates.push(ExtensionInstall {
                id: id.into(),
                version,
                path: confined(&root, Path::new(name))?,
                from_metadata: false,
            });
        }
    }
    candidates.sort_by(|a, b| a.version.cmp(&b.version));
    let Some(best) = candidates.pop() else {
        return Ok(None);
    };
    if candidates
        .last()
        .is_some_and(|other| other.version == best.version)
    {
        return Err(invalid("ambiguous extension version"));
    }
    Ok(Some(best))
}

pub(crate) fn find_binary(
    extension: &Path,
    relative: &Path,
    name: &OsStr,
) -> io::Result<Option<PathBuf>> {
    let root = extension.canonicalize()?;
    match confined(&root, relative) {
        Ok(candidate) if candidate.is_file() => return validate_binary(&candidate).map(Some),
        Ok(_) => return Err(invalid("expected binary is not a file")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let mut pending = vec![(root.clone(), 0)];
    let mut found = Vec::new();
    let mut visited = 0;
    while let Some((directory, depth)) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            visited += 1;
            if visited > 10000 {
                return Err(invalid("extension search exceeds entry limit"));
            }
            let kind = entry.file_type()?;
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() && depth < 8 {
                pending.push((entry.path(), depth + 1));
            }
            if kind.is_file() && entry.file_name() == name {
                found.push(entry.path());
            }
        }
    }
    match found.len() {
        0 => Ok(None),
        1 => validate_binary(&found[0]).map(Some),
        _ => Err(invalid("multiple executable candidates")),
    }
}

pub(crate) fn validate_binary(path: &Path) -> io::Result<PathBuf> {
    let path = path.canonicalize()?;
    if !path.is_file() {
        return Err(invalid("executable is not a regular file"));
    }
    #[cfg(windows)]
    if !path
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|ext| ext.eq_ignore_ascii_case("exe") || ext.eq_ignore_ascii_case("com"))
    {
        return Err(invalid("native Windows executable required"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if fs::metadata(&path)?.permissions().mode() & 0o111 == 0 {
            return Err(invalid("file is not executable"));
        }
    }
    Ok(path)
}

pub(crate) fn file_hash(path: &Path) -> io::Result<String> {
    use io::Read;
    let mut file = fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut chunk = [0_u8; 65536];
    loop {
        let count = file.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        digest.update(&chunk[..count]);
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

pub(crate) fn path_candidates(path_env: &OsStr, binary_name: &OsStr) -> Vec<PathBuf> {
    std::env::split_paths(path_env)
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join(binary_name))
        .filter(|path| path.is_file())
        .collect()
}

// Internal engine input/output only. REQ-010 owns the cross-crate DTO and registration.
pub(crate) struct DiscoveryRule<'a> {
    pub extension_id: &'a str,
    pub relative_binary: &'a Path,
    pub binary_name: &'a OsStr,
    pub version_args: &'a [OsString],
    pub managed_binary: Option<&'a Path>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelectionSource {
    Explicit,
    Extension,
    Path,
}
#[derive(Debug, Clone)]
pub(crate) struct SelectedCli {
    pub path: PathBuf,
    pub source: SelectionSource,
    pub extension: Option<ExtensionInstall>,
    pub cli_version: String,
    pub sha256: String,
}

pub(crate) async fn resolve_cli(
    runtime: &dyn ProcessRuntime,
    explicit: Option<&Path>,
    extension_roots: &[PathBuf],
    target: &str,
    path_env: &OsStr,
    rule: DiscoveryRule<'_>,
) -> io::Result<Option<SelectedCli>> {
    let mut selected = None;
    if let Some(path) = explicit {
        selected = Some((validate_binary(path)?, SelectionSource::Explicit, None));
    }
    if selected.is_none() {
        for root in extension_roots {
            if let Some(extension) = discover_extension(root, rule.extension_id, target)? {
                let binary = if let Some(path) = rule.managed_binary {
                    if path.exists() {
                        Some(validate_binary(path)?)
                    } else {
                        None
                    }
                } else {
                    find_binary(&extension.path, rule.relative_binary, rule.binary_name)?
                };
                if let Some(path) = binary {
                    selected = Some((path, SelectionSource::Extension, Some(extension)));
                    break;
                }
            }
        }
    }
    if selected.is_none()
        && let Some(candidate) = path_candidates(path_env, rule.binary_name)
            .into_iter()
            .next()
    {
        selected = Some((validate_binary(&candidate)?, SelectionSource::Path, None));
    }
    let Some((path, source, extension)) = selected else {
        return Ok(None);
    };
    let sha256 = file_hash(&path)?;
    let cli_version = probe_version(runtime, &path, rule.version_args).await?;
    if file_hash(&path)? != sha256 {
        return Err(invalid("executable changed during discovery"));
    }
    Ok(Some(SelectedCli {
        path,
        source,
        extension,
        cli_version,
        sha256,
    }))
}

async fn probe_version(
    runtime: &dyn ProcessRuntime,
    path: &Path,
    args: &[OsString],
) -> io::Result<String> {
    let process = runtime
        .spawn(CommandSpec {
            program: path.into(),
            args: args.to_vec(),
            cwd: std::env::temp_dir(),
            env: vec![],
            stdin: StdinMode::Null,
            limits: OutputLimits {
                max_line_bytes: 16384,
                max_buffered_bytes: 32768,
            },
        })
        .await
        .map_err(|_| invalid("version process could not start"))?;
    let result = tokio::time::timeout(Duration::from_secs(5), async {
        let mut output = process
            .take_output()
            .map_err(|_| invalid("version output unavailable"))?;
        let mut bytes = Vec::new();
        while let Some(event) = output.next().await {
            match event {
                OutputEvent::Line { data, .. } => {
                    if bytes.len() + data.len() + 1 > 65536 {
                        return Err(invalid("version output exceeds the limit"));
                    }
                    bytes.extend(data);
                    bytes.push(b'\n');
                }
                OutputEvent::Closed { .. } => {}
                OutputEvent::LineTooLong { .. } | OutputEvent::ReadError { .. } => {
                    return Err(invalid("invalid version output"));
                }
            }
        }
        let status = process
            .wait()
            .await
            .map_err(|_| invalid("version process exit is unknown"))?;
        if status.code != Some(0) {
            return Err(invalid("version process failed"));
        }
        let text = String::from_utf8(bytes).map_err(|_| invalid("version output is not UTF-8"))?;
        text.lines()
            .find(|line| !line.trim().is_empty())
            .map(|line| line.trim().to_owned())
            .ok_or_else(|| invalid("empty version output"))
    })
    .await
    .unwrap_or_else(|_| {
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "version probe timed out",
        ))
    });
    if result.is_err() {
        let _ = process.terminate(Duration::ZERO).await;
    }
    result
}

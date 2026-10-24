//! CLI discovery through the REQ-010 public contract (Spec §7.5).

use crate::{
    CliBinaryRule, CliDiscoverer, DiscoveryRequest, ProviderCliRule,
    runtime::{CommandSpec, OutputEvent, OutputLimits, ProcessRuntime, StdinMode},
};
use async_trait::async_trait;
use futures::StreamExt;
use semver::Version;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    ffi::OsStr,
    fs, io,
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use telos_schema::{
    BinaryLocation, CliDiscovery, CliDiscoveryFailure, CliDiscoveryOutcome, CliSource,
    DiscoveredCli, ExtensionInstall, ExtensionResolution,
};

/// Shared discovery engine. Provider modules supply rules; the Host supplies paths and time.
pub struct NativeCliDiscoverer {
    runtime: Arc<dyn ProcessRuntime>,
}
impl NativeCliDiscoverer {
    pub fn new(runtime: Arc<dyn ProcessRuntime>) -> Self {
        Self { runtime }
    }
}
#[async_trait]
impl CliDiscoverer for NativeCliDiscoverer {
    async fn discover(
        &self,
        provider_id: &str,
        rule: &ProviderCliRule,
        request: &DiscoveryRequest,
    ) -> CliDiscovery {
        let outcome = match resolve_cli(self.runtime.as_ref(), rule, request).await {
            Ok(Some(cli)) => CliDiscoveryOutcome::Found { cli },
            Ok(None) => CliDiscoveryOutcome::Unavailable {
                failure: CliDiscoveryFailure::NotFound,
                source: None,
                reason: "no executable candidate was found".into(),
            },
            Err(error) => CliDiscoveryOutcome::Unavailable {
                failure: error.failure,
                source: error.source,
                reason: error.reason.into(),
            },
        };
        CliDiscovery {
            provider_id: provider_id.into(),
            host_id: request.host_id.clone(),
            checked_at: request.checked_at.clone(),
            outcome,
        }
    }
}

#[derive(Debug)]
struct DiscoveryError {
    failure: CliDiscoveryFailure,
    source: Option<CliSource>,
    reason: &'static str,
}
type DiscoveryResult<T> = Result<T, DiscoveryError>;
fn failure(kind: CliDiscoveryFailure, reason: &'static str) -> DiscoveryError {
    DiscoveryError {
        failure: kind,
        source: None,
        reason,
    }
}
fn io_failure(_: io::Error) -> DiscoveryError {
    failure(CliDiscoveryFailure::IoError, "filesystem operation failed")
}
impl DiscoveryError {
    fn at(mut self, source: CliSource) -> Self {
        self.source = Some(source);
        self
    }
}

const TARGET_PLATFORMS: &[&str] = &[
    "win32-x64",
    "win32-arm64",
    "win32-ia32",
    "linux-x64",
    "linux-arm64",
    "linux-armhf",
    "darwin-x64",
    "darwin-arm64",
    "alpine-x64",
    "alpine-arm64",
];

struct ExtensionCandidate {
    version: Version,
    path: PathBuf,
    target_platform: Option<String>,
    resolution: ExtensionResolution,
}
struct LocatedBinary {
    path: PathBuf,
    location: BinaryLocation,
}

fn json_file(path: &Path) -> DiscoveryResult<Option<Value>> {
    use io::Read;
    match fs::File::open(path) {
        Ok(file) => {
            let mut bytes = Vec::new();
            file.take(4 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(io_failure)?;
            if bytes.len() > 4 * 1024 * 1024 {
                return Err(failure(
                    CliDiscoveryFailure::InvalidMetadata,
                    "extension metadata exceeds 4 MiB",
                ));
            }
            // Do not expose parser diagnostics containing metadata fragments.
            serde_json::from_slice(&bytes).map(Some).map_err(|_| {
                failure(
                    CliDiscoveryFailure::InvalidMetadata,
                    "extension metadata is invalid JSON",
                )
            })
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io_failure(error)),
    }
}
fn confined(
    root: &Path,
    relative: &Path,
    invalid_kind: CliDiscoveryFailure,
) -> DiscoveryResult<Option<PathBuf>> {
    if relative.is_absolute()
        || relative.components().any(|part| {
            matches!(
                part,
                Component::ParentDir | Component::Prefix(_) | Component::RootDir
            )
        })
    {
        return Err(failure(invalid_kind, "relative path escapes its root"));
    }
    let resolved = match root.join(relative).canonicalize() {
        Ok(path) => path,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io_failure(error)),
    };
    if !resolved.starts_with(root) {
        return Err(failure(invalid_kind, "resolved path escapes its root"));
    }
    Ok(Some(resolved))
}
fn folder_version(name: &str, id: &str, target: &str) -> Option<(Version, Option<String>)> {
    let suffix = name.strip_prefix(&format!("{id}-"))?;
    for platform in TARGET_PLATFORMS.iter().copied().chain(["universal", "web"]) {
        if let Some(version) = suffix.strip_suffix(&format!("-{platform}")) {
            if platform != target && platform != "universal" {
                return None;
            }
            return Version::parse(version)
                .ok()
                .map(|version| (version, (platform != "universal").then(|| platform.into())));
        }
    }
    Version::parse(suffix).ok().map(|version| (version, None))
}
fn discover_extension(
    root: &Path,
    id: &str,
    target: &str,
) -> DiscoveryResult<Option<ExtensionCandidate>> {
    let root = match root.canonicalize() {
        Ok(root) => root,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io_failure(error)),
    };
    let obsolete = json_file(&root.join(".obsolete"))?.unwrap_or_else(|| serde_json::json!({}));
    let obsolete = obsolete.as_object().ok_or_else(|| {
        failure(
            CliDiscoveryFailure::InvalidMetadata,
            ".obsolete must be an object",
        )
    })?;
    if obsolete.values().any(|flag| !flag.is_boolean()) {
        return Err(failure(
            CliDiscoveryFailure::InvalidMetadata,
            ".obsolete flags must be boolean",
        ));
    }
    let is_obsolete = |name: &str| obsolete.get(name).and_then(Value::as_bool) == Some(true);
    if let Some(metadata) = json_file(&root.join("extensions.json"))? {
        let entries = metadata.as_array().ok_or_else(|| {
            failure(
                CliDiscoveryFailure::InvalidMetadata,
                "extensions.json must be an array",
            )
        })?;
        let mut matches = Vec::new();
        for entry in entries {
            if !entry
                .pointer("/identifier/id")
                .and_then(Value::as_str)
                .is_some_and(|value| value.eq_ignore_ascii_case(id))
            {
                continue;
            }
            let platform = match entry.pointer("/metadata/targetPlatform") {
                None => None,
                Some(Value::String(value)) => Some(value.as_str()),
                Some(_) => {
                    return Err(failure(
                        CliDiscoveryFailure::InvalidMetadata,
                        "invalid extension platform metadata",
                    ));
                }
            };
            // Spec §7.5, REQ-013(3): VS Code also serializes universal as "undefined".
            if platform.is_none_or(|platform| {
                platform == target || matches!(platform, "universal" | "undefined")
            }) {
                matches.push((entry, platform));
            }
        }
        if matches.len() > 1 {
            return Err(failure(
                CliDiscoveryFailure::Ambiguous,
                "multiple active extension metadata entries",
            ));
        }
        if let Some((entry, platform)) = matches.first() {
            let relative = entry
                .get("relativeLocation")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    failure(
                        CliDiscoveryFailure::InvalidMetadata,
                        "active extension has no relativeLocation",
                    )
                })?;
            if is_obsolete(relative) {
                return Err(failure(
                    CliDiscoveryFailure::InvalidMetadata,
                    "active extension is marked obsolete",
                ));
            }
            let version = entry
                .get("version")
                .and_then(Value::as_str)
                .and_then(|version| Version::parse(version).ok())
                .ok_or_else(|| {
                    failure(
                        CliDiscoveryFailure::InvalidMetadata,
                        "invalid active extension version",
                    )
                })?;
            let path = confined(
                &root,
                Path::new(relative),
                CliDiscoveryFailure::InvalidMetadata,
            )?
            .ok_or_else(|| {
                failure(
                    CliDiscoveryFailure::InvalidMetadata,
                    "active extension directory is missing",
                )
            })?;
            if !path.is_dir() {
                return Err(failure(
                    CliDiscoveryFailure::InvalidMetadata,
                    "active extension is not a directory",
                ));
            }
            let target_platform = match platform {
                Some("universal" | "undefined") => None,
                Some(platform) => Some((*platform).into()),
                None => folder_version(relative, id, target).and_then(|(_, platform)| platform),
            };
            return Ok(Some(ExtensionCandidate {
                version,
                path,
                target_platform,
                resolution: ExtensionResolution::Metadata,
            }));
        }
    }
    let mut candidates = Vec::new();
    for entry in fs::read_dir(&root).map_err(io_failure)? {
        let entry = entry.map_err(io_failure)?;
        if !entry.file_type().map_err(io_failure)?.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if is_obsolete(name) {
            continue;
        }
        if let Some((version, target_platform)) = folder_version(name, id, target) {
            let path = confined(&root, Path::new(name), CliDiscoveryFailure::InvalidMetadata)?
                .ok_or_else(|| {
                    failure(
                        CliDiscoveryFailure::InvalidMetadata,
                        "extension directory disappeared",
                    )
                })?;
            candidates.push(ExtensionCandidate {
                version,
                path,
                target_platform,
                resolution: ExtensionResolution::FolderScan,
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
        return Err(failure(
            CliDiscoveryFailure::Ambiguous,
            "ambiguous extension version",
        ));
    }
    Ok(Some(best))
}

fn validate_binary(path: &Path) -> DiscoveryResult<PathBuf> {
    let path = path.canonicalize().map_err(|error| match error.kind() {
        io::ErrorKind::NotFound | io::ErrorKind::InvalidInput | io::ErrorKind::NotADirectory => {
            failure(
                CliDiscoveryFailure::InvalidPath,
                "executable path is invalid",
            )
        }
        _ => io_failure(error),
    })?;
    if !path.is_file() {
        return Err(failure(
            CliDiscoveryFailure::InvalidPath,
            "executable is not a regular file",
        ));
    }
    path_text(&path)?;
    #[cfg(windows)]
    if !path
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|ext| ext.eq_ignore_ascii_case("exe") || ext.eq_ignore_ascii_case("com"))
    {
        return Err(failure(
            CliDiscoveryFailure::InvalidPath,
            "native Windows executable required",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if fs::metadata(&path)
            .map_err(io_failure)?
            .permissions()
            .mode()
            & 0o111
            == 0
        {
            return Err(failure(
                CliDiscoveryFailure::InvalidPath,
                "file is not executable",
            ));
        }
    }
    Ok(path)
}
fn path_text(path: &Path) -> DiscoveryResult<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| failure(CliDiscoveryFailure::InvalidPath, "path is not UTF-8"))
}
fn find_binary(
    extension: &Path,
    relative: &Path,
    name: &OsStr,
) -> DiscoveryResult<Option<LocatedBinary>> {
    if let Some(candidate) = confined(extension, relative, CliDiscoveryFailure::InvalidMetadata)? {
        return validate_binary(&candidate).map(|path| {
            Some(LocatedBinary {
                path,
                location: BinaryLocation::RelativePath,
            })
        });
    }
    let mut pending = vec![(extension.to_owned(), 0)];
    let mut found = Vec::new();
    let mut visited = 0;
    while let Some((directory, depth)) = pending.pop() {
        for entry in fs::read_dir(directory).map_err(io_failure)? {
            let entry = entry.map_err(io_failure)?;
            visited += 1;
            if visited > 10000 {
                return Err(failure(
                    CliDiscoveryFailure::IoError,
                    "extension search exceeds entry limit",
                ));
            }
            let kind = entry.file_type().map_err(io_failure)?;
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() && depth < 8 {
                let path = entry.path().canonicalize().map_err(io_failure)?;
                if !path.starts_with(extension) {
                    return Err(failure(
                        CliDiscoveryFailure::InvalidMetadata,
                        "search directory escapes extension",
                    ));
                }
                pending.push((path, depth + 1));
            }
            if kind.is_file() && entry.file_name() == name {
                found.push(entry.path());
            }
        }
    }
    match found.len() {
        0 => Ok(None),
        1 => validate_binary(&found[0]).map(|path| {
            Some(LocatedBinary {
                path,
                location: BinaryLocation::NameSearch,
            })
        }),
        _ => Err(failure(
            CliDiscoveryFailure::Ambiguous,
            "multiple executable candidates",
        )),
    }
}
fn file_hash(path: &Path) -> DiscoveryResult<String> {
    use io::Read;
    let mut file = fs::File::open(path).map_err(io_failure)?;
    let mut digest = Sha256::new();
    let mut chunk = [0_u8; 65536];
    loop {
        let count = file.read(&mut chunk).map_err(io_failure)?;
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

async fn resolve_cli(
    runtime: &dyn ProcessRuntime,
    rule: &ProviderCliRule,
    request: &DiscoveryRequest,
) -> DiscoveryResult<Option<DiscoveredCli>> {
    if !TARGET_PLATFORMS.contains(&request.target_platform.as_str()) {
        return Err(failure(
            CliDiscoveryFailure::UnsupportedPlatform,
            "unsupported execution platform",
        ));
    }
    // A bare name would search the current directory; it must stay a file name.
    let name = Path::new(&rule.binary_name);
    if name.components().count() != 1
        || !matches!(name.components().next(), Some(Component::Normal(_)))
    {
        return Err(failure(
            CliDiscoveryFailure::InvalidPath,
            "binary_name must be a file name",
        ));
    }
    let mut selected = None;
    if let Some(path) = &request.explicit_path {
        let path = validate_binary(path).map_err(|error| error.at(CliSource::Explicit))?;
        selected = Some((
            LocatedBinary {
                path,
                location: BinaryLocation::Direct,
            },
            CliSource::Explicit,
            None,
        ));
    }
    if selected.is_none() {
        for root in &request.extension_roots {
            if let Some(extension) =
                discover_extension(root, &rule.extension_id, &request.target_platform)
                    .map_err(|error| error.at(CliSource::Extension))?
            {
                let located = match &rule.binary {
                    CliBinaryRule::InExtension { relative_path } => {
                        find_binary(&extension.path, relative_path, &rule.binary_name)
                    }
                    CliBinaryRule::Managed { home_relative_path } => {
                        let home = request
                            .home_dir
                            .canonicalize()
                            .map_err(io_failure)
                            .map_err(|error| error.at(CliSource::Extension))?;
                        match confined(&home, home_relative_path, CliDiscoveryFailure::InvalidPath)
                            .map_err(|error| error.at(CliSource::Extension))?
                        {
                            Some(path) => validate_binary(&path).map(|path| {
                                Some(LocatedBinary {
                                    path,
                                    location: BinaryLocation::ManagedPath,
                                })
                            }),
                            None => Ok(None),
                        }
                    }
                }
                .map_err(|error| error.at(CliSource::Extension))?;
                if let Some(binary) = located {
                    selected = Some((binary, CliSource::Extension, Some(extension)));
                    break;
                }
            }
        }
    }
    if selected.is_none()
        && let Some(path_env) = &request.path_env
    {
        for directory in std::env::split_paths(path_env).filter(|dir| dir.is_absolute()) {
            let candidate = directory.join(&rule.binary_name);
            match fs::metadata(&candidate) {
                Ok(_) => {
                    let path =
                        validate_binary(&candidate).map_err(|error| error.at(CliSource::Path))?;
                    selected = Some((
                        LocatedBinary {
                            path,
                            location: BinaryLocation::Direct,
                        },
                        CliSource::Path,
                        None,
                    ));
                    break;
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(io_failure(error).at(CliSource::Path)),
            }
        }
    }
    let Some((binary, source, extension)) = selected else {
        return Ok(None);
    };
    let executable_path = path_text(&binary.path).map_err(|error| error.at(source))?;
    let extension = extension
        .map(|extension| {
            Ok(ExtensionInstall {
                extension_id: rule.extension_id.clone(),
                version: extension.version.to_string(),
                install_path: path_text(&extension.path)?,
                target_platform: extension.target_platform,
                resolution: extension.resolution,
            })
        })
        .transpose()
        .map_err(|error: DiscoveryError| error.at(source))?;
    let sha256 = file_hash(&binary.path).map_err(|error| error.at(source))?;
    let cli_version = probe_version(runtime, &binary.path, &rule.version_args)
        .await
        .map_err(|error| error.at(source))?;
    if file_hash(&binary.path).map_err(|error| error.at(source))? != sha256 {
        return Err(failure(
            CliDiscoveryFailure::ChangedDuringDiscovery,
            "executable changed during discovery",
        )
        .at(source));
    }
    Ok(Some(DiscoveredCli {
        source,
        location: binary.location,
        executable_path,
        extension,
        cli_version,
        sha256,
    }))
}

async fn probe_version(
    runtime: &dyn ProcessRuntime,
    path: &Path,
    args: &[std::ffi::OsString],
) -> DiscoveryResult<String> {
    let version_failure = |reason| failure(CliDiscoveryFailure::VersionCheckFailed, reason);
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
        .map_err(|_| version_failure("version process could not start"))?;
    let result = tokio::time::timeout(Duration::from_secs(5), async {
        let mut output = process
            .take_output()
            .map_err(|_| version_failure("version output unavailable"))?;
        let mut bytes = Vec::new();
        while let Some(event) = output.next().await {
            match event {
                OutputEvent::Line { data, .. } => {
                    if bytes.len() + data.len() + 1 > 65536 {
                        return Err(version_failure("version output exceeds the limit"));
                    }
                    bytes.extend(data);
                    bytes.push(b'\n');
                }
                OutputEvent::Closed { .. } => {}
                OutputEvent::LineTooLong { .. } | OutputEvent::ReadError { .. } => {
                    return Err(version_failure("invalid version output"));
                }
            }
        }
        let status = process
            .wait()
            .await
            .map_err(|_| version_failure("version process exit is unknown"))?;
        if status.code != Some(0) || status.cause != crate::runtime::TerminationCause::Exited {
            return Err(version_failure("version process failed"));
        }
        let text =
            String::from_utf8(bytes).map_err(|_| version_failure("version output is not UTF-8"))?;
        text.lines()
            .find(|line| !line.trim().is_empty())
            .map(|line| line.trim().to_owned())
            .ok_or_else(|| version_failure("empty version output"))
    })
    .await
    .unwrap_or_else(|_| Err(version_failure("version probe timed out")));
    if result.is_err() {
        let _ = process.terminate(Duration::ZERO).await;
    }
    result
}

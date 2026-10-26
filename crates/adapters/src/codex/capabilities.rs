//! Version/Host evidence for Capability reporting (Spec §7.1, C3-X1/X2/X5).
use super::{CodexConfig, CodexVscodeListing, timestamp};
use std::{collections::HashMap, io::Read, path::Path};
use telos_schema::{Capability, CapabilityState, EvidenceLevel, capability_keys as keys};

const CLI: &str = "codex-cli 0.164.0-alpha.11.1";
const EXTENSION: &str = "26.1021.22027";
const BASIC_VERIFIED: &[(&str, &str)] = &[("win32-x64", CLI), ("linux-x64", CLI)];
// Add only after the corresponding live control experiment has passed.
const STEERING_VERIFIED: &[(&str, &str)] = &[("win32-x64", CLI)];
const APPROVAL_VERIFIED: &[(&str, &str)] = &[("win32-x64", CLI)];

#[derive(Debug, Clone)]
pub(super) struct ExtensionIdentity {
    pub id: String,
    pub version: String,
    pub platform: String,
}

pub(super) fn extension_identity(executable: &Path) -> Option<ExtensionIdentity> {
    let executable = executable.canonicalize().ok()?;
    for directory in executable.parent()?.ancestors() {
        let Ok(file) = std::fs::File::open(directory.join("package.json")) else {
            continue;
        };
        let mut bytes = vec![];
        file.take(1024 * 1024 + 1).read_to_end(&mut bytes).ok()?;
        if bytes.len() > 1024 * 1024 {
            continue;
        }
        let Ok(package) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            continue;
        };
        if package["publisher"] != "openai" || package["name"] != "chatgpt" {
            continue;
        }
        return Some(ExtensionIdentity {
            id: "openai.chatgpt".into(),
            version: package["version"].as_str()?.into(),
            platform: package
                .pointer("/__metadata/targetPlatform")?
                .as_str()?
                .into(),
        });
    }
    None
}

fn recorded(table: &[(&str, &str)], platform: &str, version: &str) -> bool {
    table.contains(&(platform, version))
}

pub(super) fn report(
    config: &CodexConfig,
    host: &str,
    platform: &str,
    listing: CodexVscodeListing,
) -> HashMap<String, Capability> {
    let extension = extension_identity(&config.executable);
    with_evidence(config, host, platform, listing, extension.as_ref())
}

pub(super) fn with_evidence(
    config: &CodexConfig,
    host: &str,
    platform: &str,
    listing: CodexVscodeListing,
    extension: Option<&ExtensionIdentity>,
) -> HashMap<String, Capability> {
    let mut capabilities = HashMap::new();
    for &key in keys::ALL {
        let verified = match key {
            keys::SEND_INPUT => recorded(STEERING_VERIFIED, platform, &config.cli_version),
            keys::ANSWER_APPROVAL => recorded(APPROVAL_VERIFIED, platform, &config.cli_version),
            keys::VSCODE_LISTING => {
                recorded(BASIC_VERIFIED, platform, &config.cli_version)
                    && extension.is_some_and(|e| {
                        e.id == "openai.chatgpt" && e.version == EXTENSION && e.platform == platform
                    })
            }
            keys::MANAGED_RUN
            | keys::EVENT_STREAM
            | keys::CANCEL
            | keys::NATIVE_RESUME
            | keys::NATIVE_FORK => recorded(BASIC_VERIFIED, platform, &config.cli_version),
            _ => false,
        };
        let state = if key == keys::VSCODE_LISTING && listing == CodexVscodeListing::Hidden {
            CapabilityState::Unsupported
        } else if verified {
            CapabilityState::Supported
        } else {
            CapabilityState::Unverified
        };
        let reason = match key {
            keys::VSCODE_LISTING if listing == CodexVscodeListing::Hidden => "hidden creation has no verified native mechanism and is rejected; threadSource does not provide a hidden source".into(),
            keys::VSCODE_LISTING if verified && platform == "win32-x64" => "Windows x64 extension 26.1021.22027 / CLI 0.164.0-alpha.11.1: default DB listing passed; UI listing/opening user-confirmed on 2026-10-25; live refresh unverified".into(),
            keys::VSCODE_LISTING if verified => "Linux x64 extension 26.1021.22027 / CLI 0.164.0-alpha.11.1: default DB listing passed on psyki-nas; UI listing/opening and live refresh unverified".into(),
            keys::VSCODE_LISTING => "this platform/CLI/extension identity combination has not been verified for the default VS Code catalog".into(),
            keys::SEND_INPUT if verified => "Windows x64 CLI 0.164.0-alpha.11.1 / gpt-6-luna low: turn/steer updated the active Native Turn; one turn_finished and valid common events confirmed; later follow-up Turns are not included".into(),
            keys::ANSWER_APPROVAL if verified => "Windows x64 CLI 0.164.0-alpha.11.1 / gpt-6-luna low: interactive command approval Allow/Deny controlled a disposable file write; only this command-approval path is environment-tested; Core approval policy remains separate".into(),
            keys::SEND_INPUT | keys::ANSWER_APPROVAL => format!("{key} has not been environment-tested for {platform} / {}", config.cli_version),
            _ if verified => format!("basic execution, normalized stream, cancellation, native Resume/Fork passed on {platform} / {}; turn selection and session ownership restrictions still apply", config.cli_version),
            _ => format!("this platform/CLI combination has not been environment-tested: {platform} / {}", config.cli_version),
        };
        capabilities.insert(
            key.into(),
            Capability {
                state,
                provider_version: config.cli_version.clone(),
                host_id: host.into(),
                checked_at: timestamp(),
                evidence_level: if verified {
                    EvidenceLevel::EnvironmentTested
                } else {
                    EvidenceLevel::MockTested
                },
                reason: Some(reason),
            },
        );
    }
    capabilities
}

//! H0 Capability 판정: 버전 규칙, 사전 점검, Core 집행과 전략 대체 (Spec §7.1, §8.2, §19.6.2).

use telos_schema::*;

fn preflight(passed: bool) -> PreflightReport {
    PreflightReport {
        provider: "agy".into(),
        host_id: "windows-local".into(),
        cli_version: "1.5.3".into(),
        results: vec![
            PreflightResult {
                check: PreflightCheck::VersionOutput,
                passed: true,
                detail: None,
            },
            PreflightResult {
                check: PreflightCheck::OptionAccepted {
                    option: "--app_data_dir=antigravity".into(),
                },
                passed,
                detail: None,
            },
        ],
        checked_at: "2026-11-10T00:00:00Z".into(),
    }
}

#[test]
fn major_minor_reads_prefixed_versions() {
    assert_eq!(major_minor("1.5.3"), Some((1, 5)));
    assert_eq!(major_minor("2.1.318 (Claude Code)"), Some((2, 1)));
    assert_eq!(major_minor("codex-cli 0.164.0-alpha.11.1"), Some((0, 164)));
    assert_eq!(major_minor("unknown"), None);
}

#[test]
fn same_line_needs_a_passing_preflight() {
    let tested = ["1.5.2"];
    assert_eq!(judge(&tested, "1.5.2", None), CapabilityState::Supported);
    assert_eq!(
        judge(&tested, "1.5.3", Some(&preflight(true))),
        CapabilityState::Supported
    );
    assert_eq!(
        judge(&tested, "1.5.3", Some(&preflight(false))),
        CapabilityState::Unverified
    );
    assert_eq!(judge(&tested, "1.5.3", None), CapabilityState::Unverified);
    assert_eq!(
        judge(&tested, "1.6.0", Some(&preflight(true))),
        CapabilityState::Unverified
    );
    assert_eq!(
        judge(&[], "1.5.3", Some(&preflight(true))),
        CapabilityState::Unverified
    );
}

#[test]
fn empty_preflight_does_not_pass() {
    let mut report = preflight(true);
    report.results.clear();
    assert!(!report.passed());
}

#[test]
fn core_uses_only_supported_and_falls_back_to_capsule() {
    assert!(CapabilityState::Supported.permits_use());
    assert!(!CapabilityState::Unverified.permits_use());
    assert!(!CapabilityState::Unsupported.permits_use());
    use CapabilityState::*;
    assert_eq!(
        session_fallback(SessionMode::NativeFork, Supported, Unsupported),
        Some((SessionMode::Capsule, capability_keys::NATIVE_FORK))
    );
    assert_eq!(
        session_fallback(SessionMode::Resume, Unverified, Supported),
        Some((SessionMode::Capsule, capability_keys::NATIVE_RESUME))
    );
    assert_eq!(
        session_fallback(SessionMode::Resume, Supported, Unsupported),
        None
    );
    assert_eq!(
        session_fallback(SessionMode::Fresh, Unsupported, Unsupported),
        None
    );
}

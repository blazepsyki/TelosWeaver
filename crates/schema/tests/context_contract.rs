//! H0 문맥: 막는 입력 문제, 전략 대체 기록, 사용량 Unknown (Spec §6, §8.1~8.3, §19.6.2).

use serde_json::json;
use telos_schema::*;

fn snapshot(issues: Vec<InputIssue>) -> ContextSnapshot {
    ContextSnapshot {
        snapshot_id: "ctx-1".into(),
        run_id: "run-1".into(),
        task_id: "task-1".into(),
        strategy: SessionMode::Capsule,
        fallback: Some(StrategyFallback {
            requested: SessionMode::NativeFork,
            used: SessionMode::Capsule,
            reason: "headless fork is unsupported".into(),
            capability: Some(capability_keys::NATIVE_FORK.into()),
        }),
        instructions: vec![InstructionSource {
            path: "AGENTS.md".into(),
            digest: "sha256:1".into(),
            scope: "project".into(),
        }],
        inputs: vec![],
        selected: vec![SelectedMaterial {
            reference: "docs/spec.md".into(),
            trust: SourceTrust::Material,
            reason: "target document".into(),
        }],
        exclusions: vec!["target/".into()],
        environment: EnvironmentFacts {
            host_id: "windows-local".into(),
            workspace_id: "ws-1".into(),
            cwd: None,
            executable_path: "agy.exe".into(),
            cli_version: "1.5.3".into(),
            tools: vec![],
        },
        git: None,
        contract: ContractDigests {
            plan_revision: 1,
            task_revision: 1,
            policy_revision: "policy-1".into(),
            config_digest: "sha256:c".into(),
            permission_digest: "sha256:p".into(),
            budget_digest: "sha256:b".into(),
            validation_digest: "sha256:v".into(),
        },
        issues,
        assumptions: vec![],
        capsule: Some(ArtifactVersionRef {
            artifact_id: "capsule".into(),
            version_id: "v1".into(),
        }),
        created_at: "2026-11-10T00:00:00Z".into(),
    }
}

#[test]
fn only_blocking_issues_hold_execution() {
    let soft = InputIssue {
        kind: InputIssueKind::Stale,
        subject: "notes".into(),
        detail: "older than producer revision".into(),
        blocking: false,
    };
    assert!(!snapshot(vec![soft.clone()]).blocks_execution());
    let missing = InputIssue {
        kind: InputIssueKind::Missing,
        subject: "required-input".into(),
        detail: "not readable".into(),
        blocking: true,
    };
    assert!(snapshot(vec![soft, missing]).blocks_execution());
}

#[test]
fn fallback_is_recorded_not_hidden() {
    let value = serde_json::to_value(snapshot(vec![])).unwrap();
    assert_eq!(
        value["fallback"],
        json!({"requested": "native-fork", "used": "capsule", "reason": "headless fork is unsupported", "capability": "native_fork"})
    );
    assert_eq!(value["selected"][0]["trust"], "material");
}

#[test]
fn unknown_usage_stays_absent_not_zero() {
    let record = UsageRecord {
        usage_id: "u-1".into(),
        plan_id: "plan-1".into(),
        task_id: "task-1".into(),
        run_id: "run-1".into(),
        native_turn_id: None,
        provider: "agy".into(),
        role: UsageRole::Master,
        usage: UsagePayload {
            scope: UsageScope::Delta,
            source: UsageSource::ProviderReported,
            input_tokens: None,
            output_tokens: Some(10),
            cache_read_tokens: None,
            cache_write_tokens: None,
            cache_included_in_input: None,
            cost_usd: None,
        },
        included_in_parent: false,
        recorded_at: "2026-11-10T00:00:00Z".into(),
    };
    let value = serde_json::to_value(&record).unwrap();
    assert!(value["usage"].get("inputTokens").is_none());
    assert_eq!(value["role"], "master");
}

//! H0 마스터 결정·정책: 권한 교집합, 예산 예약, 승인 요청 경로 (Spec §4.3, §13, §19.6.2).

use telos_schema::*;

fn perms(providers: &[&str], network: bool, master: &[&str]) -> PermissionSet {
    PermissionSet {
        providers: providers.iter().map(|s| s.to_string()).collect(),
        host_ids: vec!["windows-local".into()],
        workspace_modes: vec![WorkspaceMode::Directory, WorkspaceMode::GitWorktree],
        writable_workspace_ids: vec!["ws-1".into()],
        commands: vec!["cargo-test".into()],
        network,
        master_approvable_actions: master.iter().map(|s| s.to_string()).collect(),
    }
}

fn budget() -> BudgetProfile {
    BudgetProfile {
        id: "default".into(),
        limits: ControlLimits::default(),
        run_timeout_secs: 600,
        wait_timeout_secs: 300,
        plan_timeout_secs: None,
        max_tokens: None,
        max_cost_usd: None,
        require_exact_cost: false,
        master_reserve_tasks: 2,
        master_reserve_tokens: None,
    }
}

#[test]
fn permissions_only_narrow() {
    let user = perms(&["claude", "codex", "agy"], true, &["git-merge"]);
    let project = perms(
        &["claude", "codex"],
        false,
        &["git-merge", "workspace-write"],
    );
    let task = perms(&["codex"], true, &[]);
    let effective = PermissionSet::intersect_all(&[user, project, task]);
    assert_eq!(effective.providers, ["codex"]);
    assert!(!effective.network, "one scope denies network");
    assert!(effective.master_approvable_actions.is_empty());
    assert_eq!(PermissionSet::intersect_all(&[]), PermissionSet::default());
}

#[test]
fn workers_cannot_spend_the_master_reserve() {
    let profile = budget(); // maxPlanTasks 32, master reserve 2
    let ledger = BudgetLedger {
        plan_id: "plan-1".into(),
        tasks_reserved: 10,
        tasks_consumed: 20,
        tokens_measurable: false,
        tokens_consumed: 0,
    };
    assert!(
        !ledger.can_reserve_tasks(&profile, 1, false),
        "30 + 1 > 32 - 2"
    );
    assert!(ledger.can_reserve_tasks(&profile, 2, true));
    assert!(!ledger.can_reserve_tasks(&profile, 3, true));
    assert!(
        !ledger.can_reserve_tasks(&profile, u32::MAX, true),
        "no overflow"
    );
}

#[test]
fn exact_cost_plans_need_measurable_usage() {
    let mut profile = budget();
    assert!(profile.may_start(false));
    profile.require_exact_cost = true;
    assert!(!profile.may_start(false));
    assert!(profile.may_start(true));
}

#[test]
fn approval_requests_route_by_answerability_and_delegation() {
    let p = perms(&["codex"], false, &["workspace-write"]);
    assert_eq!(
        ProviderApprovalRequest::route_for(false, "workspace-write", &p),
        ApprovalRoute::Unanswerable
    );
    assert_eq!(
        ProviderApprovalRequest::route_for(true, "workspace-write", &p),
        ApprovalRoute::Master
    );
    assert_eq!(
        ProviderApprovalRequest::route_for(true, "network", &p),
        ApprovalRoute::User
    );
}

#[test]
fn approval_event_extra_fields_are_optional() {
    let old =
        serde_json::json!({"requestId": "r", "action": "a", "summary": "s", "answerable": true});
    let parsed: ApprovalRequiredPayload = serde_json::from_value(old).unwrap();
    assert_eq!(parsed.target, None);
    assert_eq!(parsed.args_digest, None);
}

// §4.3, REQ1-003: Provider 승인 요청의 행위 낱말과 인자 Digest 규칙
#[test]
fn approval_actions_and_args_digest_are_shared_rules() {
    assert_eq!(
        approval_actions::ALL,
        [
            "command_execution",
            "file_change",
            "mcp_tool_call",
            "permissions",
            "tool_use"
        ]
    );
    // 마스터가 대신 승인할 수 있는 Provider 행위는 같은 낱말로 적습니다.
    let permissions = perms(&["claude"], false, &[approval_actions::FILE_CHANGE]);
    assert_eq!(
        ProviderApprovalRequest::route_for(true, approval_actions::FILE_CHANGE, &permissions),
        ApprovalRoute::Master
    );
    assert_eq!(
        ProviderApprovalRequest::route_for(true, approval_actions::COMMAND_EXECUTION, &permissions),
        ApprovalRoute::User
    );

    // 객체 키 순서와 공백은 Digest에 영향이 없고, 값이 바뀌면 Digest가 바뀝니다.
    let a: serde_json::Value = serde_json::from_str(
        r#"{"file_path":"a.txt","content":"hi","n":[1,2,{"y":true,"x":null}]}"#,
    )
    .unwrap();
    let b: serde_json::Value = serde_json::from_str(
        r#"{ "n": [1, 2, {"x": null, "y": true}], "content": "hi", "file_path": "a.txt" }"#,
    )
    .unwrap();
    assert_eq!(args_digest(&a), args_digest(&b));
    let mut c = a.clone();
    c["content"] = "bye".into();
    assert_ne!(args_digest(&a), args_digest(&c));
    // 정렬한 압축 표기 `{"a":1}`의 SHA-256
    assert_eq!(
        args_digest(&serde_json::json!({"a": 1})),
        "sha256:015abd7f5cc57a2dd94b7590f04ad8084273905ee33ec5cebeae62276a97f862"
    );
}

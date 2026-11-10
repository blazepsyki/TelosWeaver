//! H0 Task MCP: 도구 이름, 위임 입력의 권한 필드 거부, 보고 형식, 연결 정보 (Spec §9.2·§9.3, §19.6.2).

use serde_json::json;
use telos_schema::*;

fn delegate_json() -> serde_json::Value {
    json!({
        "idempotencyKey": "child-1",
        "type": "review",
        "objective": "Review the draft",
        "agent": {"provider": "codex", "role": "reviewer"},
        "workspace": {"mode": "none", "hostId": "windows-local", "readOnly": true}
    })
}

#[test]
fn tool_names_match_spec() {
    let names: Vec<_> = [
        TaskTool::DelegateTask,
        TaskTool::GetTaskResult,
        TaskTool::ListTasks,
        TaskTool::CancelTask,
        TaskTool::GetAgentStatus,
        TaskTool::ReportTaskResult,
    ]
    .iter()
    .map(|t| serde_json::to_value(t).unwrap())
    .collect();
    assert_eq!(
        names,
        [
            "delegate_task",
            "get_task_result",
            "list_tasks",
            "cancel_task",
            "get_agent_status",
            "report_task_result"
        ]
    );
}

#[test]
fn delegate_input_defaults_and_rejects_permission_fields() {
    let input: DelegateTaskInput = serde_json::from_value(delegate_json()).unwrap();
    assert!(!input.wait_for_result);
    assert_eq!(input.cancel_propagation, CancelPropagation::Descendants);
    assert!(input.outputs.is_empty());
    // 자식의 권한·예산은 Core가 부모 범위에서 정합니다. 호출자가 넣을 수 없습니다.
    for field in [
        "permissionProfileId",
        "budgetProfileId",
        "completionPolicyId",
    ] {
        let mut value = delegate_json();
        value[field] = json!("wide");
        assert!(
            serde_json::from_value::<DelegateTaskInput>(value).is_err(),
            "{field}"
        );
    }
}

#[test]
fn reported_outputs_are_tagged_by_source() {
    let input: ReportTaskResultInput = serde_json::from_value(json!({
        "idempotencyKey": "report-1",
        "summary": "done",
        "outputs": [
            {"source": "workspace-file", "outputName": "draft", "relativePath": "out/draft.md"},
            {"source": "inline", "outputName": "notes", "contentType": "text/plain", "text": "ok"}
        ]
    }))
    .unwrap();
    assert!(matches!(
        input.outputs[0],
        ReportedOutput::WorkspaceFile { .. }
    ));
    assert!(matches!(input.outputs[1], ReportedOutput::Inline { .. }));
}

#[test]
fn binding_carries_a_credential_path_not_a_secret() {
    let binding = TaskMcpBinding {
        server_name: "telosweaver".into(),
        command: "telos-weaver".into(),
        args: vec!["mcp".into()],
        credential_file: "C:/state/run-1/mcp-credential".into(),
    };
    let value = serde_json::to_value(&binding).unwrap();
    let keys: Vec<_> = value.as_object().unwrap().keys().cloned().collect();
    assert_eq!(keys.len(), 4);
    assert!(keys.iter().all(|k| !k.to_lowercase().contains("token")));
    let mut context = telos_schema::fixtures::run_context(&telos_schema::fixtures::task_spec());
    assert!(
        serde_json::to_value(&context)
            .unwrap()
            .get("taskMcp")
            .is_none()
    );
    context.task_mcp = Some(binding);
    assert_eq!(
        serde_json::to_value(&context).unwrap()["taskMcp"]["serverName"],
        "telosweaver"
    );
}

#[test]
fn tool_errors_use_snake_case_codes() {
    let error = TaskToolError {
        code: TaskToolErrorCode::LimitExceeded,
        message: "depth".into(),
        capability: None,
    };
    assert_eq!(
        serde_json::to_value(&error).unwrap()["code"],
        "limit_exceeded"
    );
}

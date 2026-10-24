//! 마스터 결정 검증 (Spec §4.2 집행 순서의 검사 단계).
//!
//! 순서: 호출자·소유권 → Plan·Epoch·Policy Revision → Payload Schema → kind 지원 여부 →
//! kind별 권한·한도·Task 트리·의존성 검사 → 신뢰된 조회. 처음 실패한 검사의 사유로 거부합니다.
//! 멱등 키 처리와 집행·기록은 Master Controller가 합니다.

use std::collections::{HashMap, HashSet};

use telos_schema::{
    CreateTasksPayload, DecisionContext, DecisionPayload, DecisionRejection, MasterDecision,
    MasterDecisionKind, RejectionCode, RequestCancelPayload, RequestRunPayload, SessionMode,
    TaskStatus,
};

use super::{LookupDenial, TrustedLookup};

/// Phase 0에서 집행하는 kind입니다(배정표 §6). 나머지는 Payload 검사 후 `unsupported_kind`로 거부합니다.
pub const PHASE0_EXECUTED_KINDS: &[MasterDecisionKind] = &[
    MasterDecisionKind::CreateTasks,
    MasterDecisionKind::RequestRun,
    MasterDecisionKind::RequestCancel,
];

/// 결정을 보낸 쪽입니다. Core가 실제 마스터 Run에서 회수한 결정에만 Run ID를 채웁니다.
#[derive(Debug, Clone, Copy)]
pub struct Caller<'a> {
    /// 이 Plan의 현재 활성 마스터 Run.
    pub active_master_run_id: &'a str,
}

fn reject(
    code: RejectionCode,
    message: impl Into<String>,
    field: Option<String>,
) -> DecisionRejection {
    DecisionRejection {
        code,
        message: message.into(),
        field,
    }
}

pub fn validate(
    decision: &MasterDecision,
    context: &DecisionContext,
    caller: Caller<'_>,
    lookup: &dyn TrustedLookup,
) -> Result<DecisionPayload, DecisionRejection> {
    if decision.master_run_id != caller.active_master_run_id {
        return Err(reject(
            RejectionCode::UnauthorizedCaller,
            "decision does not come from the active master run",
            None,
        ));
    }
    if decision.plan_id != context.plan_id {
        return Err(reject(
            RejectionCode::UnauthorizedCaller,
            "decision targets another plan",
            None,
        ));
    }
    if decision.controller_epoch != context.controller_epoch {
        return Err(reject(
            RejectionCode::StaleControllerEpoch,
            "controller epoch has changed",
            None,
        ));
    }
    if decision.plan_revision != context.plan_revision {
        return Err(reject(
            RejectionCode::StalePlanRevision,
            "plan revision has changed",
            None,
        ));
    }
    if decision.policy_revision != context.policy_revision {
        return Err(reject(
            RejectionCode::StalePolicyRevision,
            "policy revision has changed",
            None,
        ));
    }
    let payload = DecisionPayload::from_decision(decision).map_err(|error| {
        reject(
            RejectionCode::InvalidPayload,
            format!("payload does not match kind: {error}"),
            None,
        )
    })?;
    if !PHASE0_EXECUTED_KINDS.contains(&decision.kind) {
        return Err(reject(
            RejectionCode::UnsupportedKind,
            "this decision kind is not executed in Phase 0",
            None,
        ));
    }
    match &payload {
        DecisionPayload::CreateTasks(p) => check_create_tasks(p, context, lookup)?,
        DecisionPayload::RequestRun(p) => check_request_run(p, context)?,
        DecisionPayload::RequestCancel(p) => check_request_cancel(p, context)?,
        _ => unreachable!("only executed kinds reach kind-specific checks"),
    }
    Ok(payload)
}

fn check_create_tasks(
    payload: &CreateTasksPayload,
    context: &DecisionContext,
    lookup: &dyn TrustedLookup,
) -> Result<(), DecisionRejection> {
    if payload.tasks.is_empty() {
        return Err(reject(
            RejectionCode::InvalidPayload,
            "no tasks to create",
            Some("/tasks".into()),
        ));
    }
    let total = context.tasks.len() + payload.tasks.len();
    if total > context.limits.max_plan_tasks as usize {
        return Err(reject(
            RejectionCode::LimitExceeded,
            format!(
                "plan would have {total} tasks (limit {})",
                context.limits.max_plan_tasks
            ),
            Some("/tasks".into()),
        ));
    }
    let existing: HashMap<&str, u32> = context
        .tasks
        .iter()
        .map(|t| (t.id.as_str(), t.delegation_depth))
        .collect();
    let mut batch: HashMap<&str, usize> = HashMap::new();
    for (index, task) in payload.tasks.iter().enumerate() {
        let field = |name: &str| Some(format!("/tasks/{index}/{name}"));
        if task.id.is_empty()
            || existing.contains_key(task.id.as_str())
            || batch.insert(&task.id, index).is_some()
        {
            return Err(reject(
                RejectionCode::InvalidPayload,
                "task id is empty or already used",
                field("id"),
            ));
        }
        if task.project_id != context.project_id || task.plan_id != context.plan_id {
            return Err(reject(
                RejectionCode::PermissionDenied,
                "task belongs to another project or plan",
                field("planId"),
            ));
        }
        if task.agent.role == "master" {
            return Err(reject(
                RejectionCode::PermissionDenied,
                "a master cannot create another controller task",
                field("agent/role"),
            ));
        }
        let denied = |what: &str, name: &str| {
            Err(reject(
                RejectionCode::PermissionDenied,
                format!("{what} is not allowed"),
                field(name),
            ))
        };
        if !context.allowed_providers.contains(&task.agent.provider) {
            return denied("provider", "agent/provider");
        }
        if !context.allowed_host_ids.contains(&task.workspace.host_id) {
            return denied("host", "workspace/hostId");
        }
        if !context
            .allowed_workspace_modes
            .contains(&task.workspace.mode)
        {
            return denied("workspace mode", "workspace/mode");
        }
        if !context
            .allowed_permission_profile_ids
            .contains(&task.permission_profile_id)
        {
            return denied("permission profile", "permissionProfileId");
        }
        if !context
            .allowed_budget_profile_ids
            .contains(&task.budget_profile_id)
        {
            return denied("budget profile", "budgetProfileId");
        }
    }
    // 위임 깊이: 부모는 기존 Task이거나 같은 결정의 앞선 Task입니다.
    let mut depth: HashMap<&str, u32> = existing.clone();
    for (index, task) in payload.tasks.iter().enumerate() {
        let d = match &task.parent_task_id {
            None => 0,
            Some(parent) => match depth.get(parent.as_str()) {
                Some(parent_depth) => parent_depth + 1,
                None => {
                    return Err(reject(
                        RejectionCode::UnknownTask,
                        "parent task does not exist",
                        Some(format!("/tasks/{index}/parentTaskId")),
                    ));
                }
            },
        };
        if d > context.limits.max_delegation_depth {
            return Err(reject(
                RejectionCode::LimitExceeded,
                format!(
                    "delegation depth {d} exceeds {}",
                    context.limits.max_delegation_depth
                ),
                Some(format!("/tasks/{index}/parentTaskId")),
            ));
        }
        depth.insert(&task.id, d);
    }
    for (index, task) in payload.tasks.iter().enumerate() {
        for (n, dependency) in task.dependencies.iter().enumerate() {
            if !existing.contains_key(dependency.as_str())
                && !batch.contains_key(dependency.as_str())
            {
                return Err(reject(
                    RejectionCode::UnknownTask,
                    "dependency does not exist",
                    Some(format!("/tasks/{index}/dependencies/{n}")),
                ));
            }
        }
    }
    if has_cycle(payload) {
        return Err(reject(
            RejectionCode::DependencyCycle,
            "task dependencies form a cycle",
            Some("/tasks".into()),
        ));
    }
    for (index, task) in payload.tasks.iter().enumerate() {
        if let Err(denial) = lookup.check_task(task) {
            let field = Some(format!("/tasks/{index}"));
            return Err(match denial {
                LookupDenial::Denied(m) => reject(RejectionCode::PermissionDenied, m, field),
                LookupDenial::Unavailable(m) => reject(RejectionCode::PolicyUnavailable, m, field),
                LookupDenial::Unsupported(m) => reject(RejectionCode::CheckUnsupported, m, field),
            });
        }
    }
    Ok(())
}

/// 같은 결정 안의 의존성 순환을 찾습니다. 기존 Task는 이미 검증된 DAG이므로 새 Task 사이만 봅니다.
fn has_cycle(payload: &CreateTasksPayload) -> bool {
    let edges: HashMap<&str, Vec<&str>> = payload
        .tasks
        .iter()
        .map(|t| {
            (
                t.id.as_str(),
                t.dependencies.iter().map(String::as_str).collect(),
            )
        })
        .collect();
    fn visit<'a>(
        node: &'a str,
        edges: &HashMap<&'a str, Vec<&'a str>>,
        visiting: &mut HashSet<&'a str>,
        done: &mut HashSet<&'a str>,
    ) -> bool {
        if done.contains(node) {
            return false;
        }
        if !visiting.insert(node) {
            return true;
        }
        let cyclic = edges
            .get(node)
            .is_some_and(|next| next.iter().any(|n| visit(n, edges, visiting, done)));
        visiting.remove(node);
        done.insert(node);
        cyclic
    }
    let (mut visiting, mut done) = (HashSet::new(), HashSet::new());
    edges
        .keys()
        .any(|node| visit(node, &edges, &mut visiting, &mut done))
}

fn check_request_run(
    payload: &RequestRunPayload,
    context: &DecisionContext,
) -> Result<(), DecisionRejection> {
    let task = context
        .tasks
        .iter()
        .find(|t| t.id == payload.task_id && t.revision == payload.task_revision)
        .ok_or_else(|| {
            reject(
                RejectionCode::UnknownTask,
                "task or task revision does not exist",
                Some("/taskId".into()),
            )
        })?;
    if task.status != TaskStatus::Ready {
        return Err(reject(
            RejectionCode::InvalidPayload,
            format!("task is {:?}, not Ready", task.status),
            Some("/taskId".into()),
        ));
    }
    if payload.session.mode == SessionMode::Capsule {
        return Err(reject(
            RejectionCode::UnsupportedCapability,
            "capsule sessions are not supported in Phase 0",
            Some("/session/mode".into()),
        ));
    }
    Ok(())
}

fn check_request_cancel(
    payload: &RequestCancelPayload,
    context: &DecisionContext,
) -> Result<(), DecisionRejection> {
    if !context.tasks.iter().any(|t| t.id == payload.task_id) {
        return Err(reject(
            RejectionCode::UnknownTask,
            "task does not exist",
            Some("/taskId".into()),
        ));
    }
    Ok(())
}

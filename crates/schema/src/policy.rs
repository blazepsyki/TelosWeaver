//! Spec §4.3 Worker 요청·승인, §13.1 권한 교집합, §13.2 예산 (H0 마스터 결정·정책)
//!
//! 마스터 결정 8종의 Payload는 `decision.rs`, 승인 기록은 `ApprovalDecision`이 원본입니다.
//! 이 모듈은 결정 집행에 필요한 권한·예산 판정과 Provider 승인 요청의 상관을 정합니다.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ApprovalAnswer, ControlLimits, WorkspaceMode};

/// 한 범위(사용자·Host·프로젝트·Plan·부모 위임·Task 요청)가 허용하는 것입니다.
/// 최종 권한은 모든 범위의 교집합이며 저장소 설정·외부 메시지만으로 넓히지 않습니다 (§13.1, CFG-02).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PermissionSet {
    pub providers: Vec<String>,
    pub host_ids: Vec<String>,
    pub workspace_modes: Vec<WorkspaceMode>,
    /// 쓰기를 허용하는 Workspace ID. 없으면 읽기 전용입니다.
    pub writable_workspace_ids: Vec<String>,
    /// 실행해도 되는 검증 명령 이름 (H0 D8)
    pub commands: Vec<String>,
    pub network: bool,
    /// 마스터가 사용자 대신 승인할 수 있는 개별 행위(예: `git-merge`). 여기 없는 행위는 사용자 전용입니다 (§4.3).
    pub master_approvable_actions: Vec<String>,
}

fn intersect<T: PartialEq + Clone>(a: &[T], b: &[T]) -> Vec<T> {
    a.iter().filter(|x| b.contains(x)).cloned().collect()
}

impl PermissionSet {
    pub fn intersect(&self, other: &Self) -> Self {
        Self {
            providers: intersect(&self.providers, &other.providers),
            host_ids: intersect(&self.host_ids, &other.host_ids),
            workspace_modes: intersect(&self.workspace_modes, &other.workspace_modes),
            writable_workspace_ids: intersect(
                &self.writable_workspace_ids,
                &other.writable_workspace_ids,
            ),
            commands: intersect(&self.commands, &other.commands),
            network: self.network && other.network,
            master_approvable_actions: intersect(
                &self.master_approvable_actions,
                &other.master_approvable_actions,
            ),
        }
    }

    /// 범위 목록 전체의 교집합입니다. 범위가 하나도 없으면 아무것도 허용하지 않습니다.
    pub fn intersect_all(scopes: &[Self]) -> Self {
        match scopes.split_first() {
            None => Self::default(),
            Some((first, rest)) => rest.iter().fold(first.clone(), |acc, s| acc.intersect(s)),
        }
    }

    pub fn is_master_approvable(&self, action: &str) -> bool {
        self.master_approvable_actions.iter().any(|a| a == action)
    }
}

/// 실행 시간·대기·재시도·토큰·비용의 상한입니다. Timeout은 작업 프로파일별 필수 양수입니다 (§13.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BudgetProfile {
    pub id: String,
    pub limits: ControlLimits,
    pub run_timeout_secs: u64,
    pub wait_timeout_secs: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_timeout_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_cost_usd: Option<f64>,
    /// 참이면 사용량을 정확히 잴 수 없는 Provider로 이 Plan을 시작하지 않습니다.
    pub require_exact_cost: bool,
    /// 마스터 판단용으로 예약하는 몫(Task 수·토큰). Worker가 쓰지 못합니다.
    pub master_reserve_tasks: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub master_reserve_tokens: Option<u64>,
}

impl BudgetProfile {
    /// 계측할 수 없는 사용량으로 시작해도 되는지입니다 (§13.2).
    pub fn may_start(&self, usage_measurable: bool) -> bool {
        usage_measurable || !self.require_exact_cost
    }
}

/// Plan 예산 장부입니다. 새 작업은 예약하고, 끝나면 실제 소비로 바꿉니다.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BudgetLedger {
    pub plan_id: String,
    pub tasks_reserved: u32,
    pub tasks_consumed: u32,
    /// 보고되지 않은 사용량이 있으면 거짓입니다. 이때 토큰 합계를 정확한 집행 근거로 쓰지 않습니다.
    pub tokens_measurable: bool,
    pub tokens_consumed: u64,
}

impl BudgetLedger {
    /// Task `count`개를 예약할 수 있는지입니다. Worker 요청은 마스터 예약분을 넘보지 못합니다.
    pub fn can_reserve_tasks(&self, profile: &BudgetProfile, count: u32, for_master: bool) -> bool {
        let used = self.tasks_reserved + self.tasks_consumed;
        let cap = if for_master {
            profile.limits.max_plan_tasks
        } else {
            profile
                .limits
                .max_plan_tasks
                .saturating_sub(profile.master_reserve_tasks)
        };
        used.checked_add(count).is_some_and(|total| total <= cap)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ApprovalRoute {
    /// 위임 범위 안: 마스터가 `answer_action`으로 답합니다.
    Master,
    /// 사용자 전용·권한 확대: 마스터가 사용자에게 올립니다.
    User,
    /// Provider가 실행 중 응답을 받을 수 없음: 거부·재계획합니다 (§7.4).
    Unanswerable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub enum ProviderApprovalState {
    Pending,
    Answered,
    /// 기한 안에 답이 없어 거부로 처리했습니다.
    Expired,
    /// 응답 전에 Run이 끝났습니다.
    Abandoned,
}

/// Worker Run 중 Provider가 보낸 승인 요청입니다 (`ApprovalRequired` 이벤트, 이월 P1-2).
/// 응답은 실행 직전에 대상·인자 Digest·정책을 다시 확인한 뒤 Adapter `answer_approval`로 보냅니다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProviderApprovalRequest {
    /// Core가 발급하는 ID. 마스터의 `answer_action.requestId`가 가리킵니다.
    pub request_id: String,
    pub run_id: String,
    pub task_id: String,
    /// Provider 이벤트의 요청 ID (`ApprovalRequiredPayload::request_id`)
    pub native_request_id: String,
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args_digest: Option<String>,
    pub summary: String,
    pub route: ApprovalRoute,
    pub state: ProviderApprovalState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<ApprovalAnswer>,
    /// 답을 기록한 `ApprovalDecision`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<String>,
    pub expires_at: String,
    pub created_at: String,
}

impl ProviderApprovalRequest {
    /// 경로 판정입니다. 응답 경로가 없으면 무조건 `Unanswerable`, 마스터 위임 행위면 `Master`, 그 밖은 `User`입니다.
    pub fn route_for(answerable: bool, action: &str, permissions: &PermissionSet) -> ApprovalRoute {
        if !answerable {
            ApprovalRoute::Unanswerable
        } else if permissions.is_master_approvable(action) {
            ApprovalRoute::Master
        } else {
            ApprovalRoute::User
        }
    }
}

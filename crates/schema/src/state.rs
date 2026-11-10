//! Spec §5.1 식별자, §8.1 Session·Run·Turn, §9.3 대기, §9.4 Task·Run 상태 (H0 Task·Run·Session 모델)
//!
//! 영속 기록과 Local API가 함께 쓰는 Wire 형식입니다. 전이 규칙은 [`TaskStatus::can_transition_to`]가
//! 원본이며 Scheduler(PH1-SCHED)는 이 규칙 밖의 전이를 만들지 않습니다.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ExecutionOutcome, RunStatus, SessionMode, TaskSpec, TaskStatus};

/// 사용자 목표와 승인된 실행 구성입니다. 수정은 새 Revision이며 실행 중 묵시적으로 바꾸지 않습니다 (§5.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub id: String,
    pub project_id: String,
    pub revision: u32,
    pub goal: String,
    pub constraints: Vec<String>,
    pub status: PlanStatus,
    /// 마스터 제어권의 세대입니다. 재개·교체 시 올리고 이전 세대의 결정을 집행하지 않습니다 (§4.1, §14.3).
    pub controller_epoch: u64,
    /// 마스터 실행을 기록하는 제어용 Task (`role: master`, §4.1)
    pub master_task_id: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub enum PlanStatus {
    Active,
    /// 마스터 유실 등으로 새 결정을 보류한 상태입니다. 허용된 기존 Run은 정책에 따라 계속됩니다 (§14.2).
    Held,
    Completed,
    Failed,
    Cancelled,
}

/// Wait* 상태에서 무엇을 기다리는지와 응답 뒤 돌아갈 상태입니다 (§9.3, §9.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WaitState {
    /// 응답 뒤 복귀할 상태. `Running`·`Executed`·`Validating`·`Verified` 중 하나입니다.
    pub resume_status: TaskStatus,
    /// 기다리는 대상(자식 Task ID, 결정·질문 ID 등)
    pub waiting_for: Vec<String>,
    /// 기한이 지나면 계약에 정한 경로(실패·재계획·사용자 질문)를 따릅니다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline: Option<String>,
}

/// 부모 취소 시 자식 처리입니다. 기본은 자손 전파이며 예외는 계약에 기록합니다 (§9.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum CancelPropagation {
    #[default]
    Descendants,
    Independent,
}

/// 영속 Task 기록입니다. 상태 변경은 `state_version`의 조건부 갱신으로만 합니다 (§9.5, §14.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TaskRecord {
    pub spec: TaskSpec,
    pub status: TaskStatus,
    /// 상태가 바뀔 때마다 1씩 올립니다. 갱신 요청은 읽은 값과 같을 때만 적용합니다.
    pub state_version: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait: Option<WaitState>,
    #[serde(default)]
    pub cancel_propagation: CancelPropagation,
    /// 위임 깊이. 최상위 Task는 0입니다(`TaskSummary::delegation_depth`와 같은 기준, `ControlLimits::max_delegation_depth`).
    pub delegation_depth: u32,
    pub created_at: String,
    pub updated_at: String,
}

/// Run 하나는 Task의 실행 시도 하나입니다. 재시도는 같은 Task의 새 Run입니다 (§5.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunRecord {
    pub run_id: String,
    pub task_id: String,
    pub task_revision: u32,
    /// 같은 Task Revision 안에서 1부터 올립니다.
    pub attempt: u32,
    pub status: RunStatus,
    pub state_version: u64,
    pub provider: String,
    pub host_id: String,
    pub session_mode: SessionMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<ExecutionOutcome>,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
}

/// Provider 대화입니다. 여러 Run이 재사용할 수 있고 Run과 1:1로 가정하지 않습니다 (§5.1, §8.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionRecord {
    /// Core가 발급하는 ID
    pub session_id: String,
    pub provider: String,
    pub host_id: String,
    pub native_session_id: String,
    /// 이 Session에 쓸 수 있는 Task입니다. 다른 Task의 Resume은 소유권 확인을 거칩니다 (§8.1).
    pub owner_task_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<String>,
    /// Native Fork의 분기 기준. Provider가 지정하지 못하면 지원 가능한 기준을 적습니다 (§8.2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fork_basis: Option<String>,
    pub created_by_run_id: String,
    pub created_at: String,
}

/// Run이 Session을 어떻게 썼는지입니다 (`run_sessions`, §8.1·§14.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunSession {
    pub run_id: String,
    pub session_id: String,
    pub usage: SessionUse,
    pub native_turn_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum SessionUse {
    Created,
    Resumed,
    Forked,
}

impl TaskStatus {
    /// 더 이상 상태가 바뀌지 않습니다. 새 결과는 새 Task Revision 또는 새 Task로 실행합니다 (§11.3).
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Skipped | Self::Cancelled
        )
    }

    pub fn is_waiting(self) -> bool {
        matches!(
            self,
            Self::WaitingChild | Self::WaitingMaster | Self::WaitingUser
        )
    }

    /// §9.4의 전이 표입니다. 같은 상태로의 전이는 변경이 아니므로 허용하지 않습니다.
    /// Wait* 상태의 복귀는 `WaitState::resume_status`로 정한 상태만 허용하며 그 검사는 호출자가 합니다.
    pub fn can_transition_to(self, next: Self) -> bool {
        use TaskStatus::*;
        if self == next || self.is_terminal() {
            return false;
        }
        // 취소는 외부 실행이 없으면 바로 Cancelled, 실행·검증 중이면 Cancelling을 거칩니다.
        let cancel = match self {
            Created | Blocked | Ready | Executed | Verified => next == Cancelled,
            Running | Validating | WaitingChild | WaitingMaster | WaitingUser => next == Cancelling,
            _ => false,
        };
        cancel
            || match self {
                Created => matches!(next, Blocked | Ready),
                Blocked => matches!(next, Ready | Skipped | Failed),
                Ready => matches!(next, Running | Blocked | Skipped),
                Running => matches!(
                    next,
                    Executed
                        | WaitingChild
                        | WaitingMaster
                        | WaitingUser
                        | Ready
                        | Failed
                        | Interrupted
                        | NeedsReconciliation
                ),
                WaitingChild | WaitingMaster | WaitingUser => matches!(
                    next,
                    Running
                        | Executed
                        | Validating
                        | Verified
                        | WaitingChild
                        | WaitingMaster
                        | WaitingUser
                        | Ready
                        | Failed
                        | Interrupted
                ),
                Executed => matches!(
                    next,
                    Validating | Completed | WaitingMaster | WaitingUser | Ready | Failed
                ),
                Validating => matches!(
                    next,
                    Verified | WaitingMaster | WaitingUser | Ready | Failed | Interrupted
                ),
                Verified => matches!(next, Completed | WaitingMaster | WaitingUser | Ready),
                Cancelling => matches!(next, Cancelled | NeedsReconciliation),
                Interrupted => matches!(next, Ready | Failed | NeedsReconciliation | Cancelled),
                NeedsReconciliation => matches!(next, Ready | Executed | Failed | Cancelled),
                Completed | Failed | Skipped | Cancelled => false,
            }
    }
}

impl RunStatus {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Cancelled | Self::Unknown
        )
    }

    /// Run 상태 전이입니다. `Unknown`은 종료 결과를 확정할 수 없을 때이며 상태 대조로만 해소합니다 (§9.4, §14.3).
    pub fn can_transition_to(self, next: Self) -> bool {
        use RunStatus::*;
        if self == next || self.is_terminal() {
            return false;
        }
        match self {
            Starting => matches!(
                next,
                Running | Failed | Cancelling | Cancelled | Interrupted | Unknown
            ),
            Running => matches!(
                next,
                Waiting | Succeeded | Failed | Cancelling | Interrupted | Unknown
            ),
            Waiting => matches!(next, Running | Failed | Cancelling | Interrupted | Unknown),
            // 취소 요청 전에 프로세스가 끝났을 수 있어 실제 결과를 기록합니다.
            // Task는 취소 의도가 우선이며 Completed로 되돌리지 않습니다 (§9.5).
            Cancelling => matches!(next, Cancelled | Succeeded | Failed | Unknown),
            Interrupted => matches!(next, Succeeded | Failed | Cancelled | Unknown),
            Succeeded | Failed | Cancelled | Unknown => false,
        }
    }
}

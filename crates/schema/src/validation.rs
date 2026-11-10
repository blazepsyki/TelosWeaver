//! Spec §11 Validation·승인 (H0 산출물·검증)
//!
//! 검증과 승인은 고정된 Artifact Version과 계약 Revision에 묶입니다. 대상이 바뀌면 새 검증·승인이 필요합니다 (§11.3).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ApprovalAnswer, ArtifactVersionRef, ValidationContract};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ValidationVerdict {
    Passed,
    Failed,
    Error,
    Inconclusive,
    Skipped,
}

impl ValidationVerdict {
    /// 필수 검사는 `Passed`만 통과로 인정합니다. 실행 오류와 판정 불가를 성공으로 표시하지 않습니다.
    pub fn passes_required_check(self) -> bool {
        self == Self::Passed
    }
}

/// `command` 검증의 `ValidationContract::parameters` 형식입니다. 설정에 등록된 명령 이름과 그 명령이 허용한 인자만 씁니다 (H0 D8).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommandValidationParameters {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub enum ValidationJobStatus {
    Queued,
    Running,
    Cancelling,
    Cancelled,
    /// 판정이 나왔습니다(`verdict`). 실행 오류·판정 불가도 이 상태에서 `Error`·`Inconclusive`로 기록합니다.
    Finished,
}

/// Core가 실행하는 검증 하나입니다. 계약은 시작 때의 Revision으로 고정합니다 (§11.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ValidationJob {
    pub validation_id: String,
    pub task_id: String,
    pub task_revision: u32,
    pub contract: ValidationContract,
    pub targets: Vec<ArtifactVersionRef>,
    pub status: ValidationJobStatus,
    pub state_version: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verdict: Option<ValidationVerdict>,
    /// 근거(로그·보고서) Artifact
    #[serde(default)]
    pub evidence: Vec<ArtifactVersionRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
}

/// 계획 승인·결과 검토·개별 행위 실행 승인을 구분합니다 (§11.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ApprovalKind {
    Plan,
    ResultReview,
    Action,
}

/// 개별 행위의 대상과 인자입니다. 인자 원문 대신 Digest를 고정합니다.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalAction {
    /// 예: `git-merge`, `provider-tool`, `workspace-write`
    pub action: String,
    pub target: String,
    pub args_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "actor",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum ApprovalActor {
    User,
    /// 위임받은 범위의 마스터 판단 (§4.3)
    Master {
        master_run_id: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub enum ApprovalState {
    Pending,
    Granted,
    Denied,
    Expired,
    Revoked,
    /// 한 번 쓰는 승인을 사용했습니다. 같은 승인을 다시 적용하지 않습니다.
    Consumed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalDecision {
    pub approval_id: String,
    pub kind: ApprovalKind,
    pub plan_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contract_revision: Option<u32>,
    pub targets: Vec<ArtifactVersionRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<ApprovalAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decided_by: Option<ApprovalActor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<ApprovalAnswer>,
    /// `human`으로 요구된 검토 등 사용자만 결정할 수 있는 승인입니다. 마스터가 대신하지 않습니다 (§11.2).
    pub user_only: bool,
    pub policy_revision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    pub state: ApprovalState,
    pub state_version: u64,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decided_at: Option<String>,
}

impl ApprovalDecision {
    /// 실행 직전 검사입니다. 허용된 승인이고, 대상 Version·행위·인자 Digest·Policy Revision이 모두 같을 때만 적용합니다.
    /// 사용자 전용 승인은 사용자가 결정한 경우에만 인정합니다. 기한 확인은 호출자가 현재 시각으로 합니다 (§11.2).
    pub fn permits(
        &self,
        targets: &[ArtifactVersionRef],
        action: Option<&ApprovalAction>,
        policy_revision: &str,
    ) -> bool {
        let key = |r: &ArtifactVersionRef| (r.artifact_id.clone(), r.version_id.clone());
        let mut mine: Vec<_> = self.targets.iter().map(key).collect();
        let mut theirs: Vec<_> = targets.iter().map(key).collect();
        mine.sort();
        theirs.sort();
        let actor_ok = !self.user_only || matches!(self.decided_by, Some(ApprovalActor::User));
        self.state == ApprovalState::Granted
            && self.answer == Some(ApprovalAnswer::Allow)
            && actor_ok
            && mine == theirs
            && self.action.as_ref() == action
            && self.policy_revision == policy_revision
    }
}

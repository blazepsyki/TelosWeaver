//! Spec §6 실행 전 맥락, §8.1 문맥 전략, §8.2 분기, §8.3 사용량 (H0 문맥)
//!
//! Snapshot은 Run 시작 전에 고정하며 이후 바꾸지 않습니다. 필수 지침·입력을 읽지 못하면 조용히 생략하지 않습니다.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ArtifactVersionRef, GitBaseline, SessionMode, UsagePayload};

/// 자료의 출처 종류입니다. 자료 본문의 지시를 사용자 권한이나 정책 변경으로 승격하지 않습니다 (§6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum SourceTrust {
    /// 적용하는 사용자·프로젝트 지침
    Instruction,
    /// 분석·작업 대상 문서와 파일
    Material,
    ToolOutput,
    ExternalMessage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InstructionSource {
    pub path: String,
    pub digest: String,
    /// 예: `user`, `project`, `directory:<경로>`
    pub scope: String,
}

/// Run 시작 때 실제 Version으로 해석한 입력입니다 (§5.2 `task-output` 고정).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedInput {
    pub name: String,
    pub version: ArtifactVersionRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub producer_task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub producer_task_revision: Option<u32>,
}

/// 모델에 전달하려고 고른 자료와 이유입니다. Workspace 전체를 무조건 넣지 않습니다 (§6.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SelectedMaterial {
    pub reference: String,
    pub trust: SourceTrust,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentFacts {
    pub host_id: String,
    pub workspace_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    pub executable_path: String,
    pub cli_version: String,
    #[serde(default)]
    pub tools: Vec<String>,
}

/// 실행 계약의 고정 값입니다. 최종 설정·권한·예산·검증 계약의 SHA-256과 Policy Revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContractDigests {
    pub plan_revision: u32,
    pub task_revision: u32,
    pub policy_revision: String,
    pub config_digest: String,
    pub permission_digest: String,
    pub budget_digest: String,
    pub validation_digest: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum InputIssueKind {
    Missing,
    Conflict,
    Stale,
    Unreadable,
}

/// 입력 문제입니다. `blocking`이면 Core가 실행을 보류하고, 아니면 마스터가 질문·제한적 실행·실패를 정합니다 (§6.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InputIssue {
    pub kind: InputIssueKind,
    pub subject: String,
    pub detail: String,
    pub blocking: bool,
}

/// 요청한 문맥 전략을 쓰지 못해 다른 전략으로 실행한 기록입니다. 성공한 것처럼 표시하지 않습니다 (§8.2 6번, E2E-13).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StrategyFallback {
    pub requested: SessionMode,
    pub used: SessionMode,
    pub reason: String,
    /// 거부된 기능의 공통 Capability 키
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContextSnapshot {
    pub snapshot_id: String,
    pub run_id: String,
    pub task_id: String,
    pub strategy: SessionMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<StrategyFallback>,
    pub instructions: Vec<InstructionSource>,
    pub inputs: Vec<ResolvedInput>,
    pub selected: Vec<SelectedMaterial>,
    /// 일부러 넣지 않은 범위와 이유
    pub exclusions: Vec<String>,
    pub environment: EnvironmentFacts,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git: Option<GitBaseline>,
    pub contract: ContractDigests,
    pub issues: Vec<InputIssue>,
    /// 알려진 가정과 미해결 질문
    pub assumptions: Vec<String>,
    /// `capsule` 전략에서 전달한 Capsule Artifact
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capsule: Option<ArtifactVersionRef>,
    pub created_at: String,
}

impl ContextSnapshot {
    /// Core가 판정한 막는 문제가 있으면 실행을 보류합니다.
    pub fn blocks_execution(&self) -> bool {
        self.issues.iter().any(|i| i.blocking)
    }
}

/// 다른 Provider 인계·Fork 미지원·긴 이력 축소에 쓰는 Capsule 내용입니다. Artifact(`structured`)로 저장합니다 (§8.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CapsuleContent {
    pub goal: String,
    pub decisions: Vec<String>,
    pub evidence: Vec<ArtifactVersionRef>,
    pub constraints: Vec<String>,
    pub open_questions: Vec<String>,
    /// 원래 이력의 출처와 생략 범위
    pub source_summary: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum UsageRole {
    Master,
    Worker,
    Validator,
}

/// Provider가 보고한 사용량 한 건입니다. 값이 없으면 0이 아니라 Unknown입니다 (§8.3, §15.2).
/// `included_in_parent`가 참이면 부모 기록에 이미 포함된 값이므로 다시 합산하지 않습니다.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UsageRecord {
    pub usage_id: String,
    pub plan_id: String,
    pub task_id: String,
    pub run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_turn_id: Option<String>,
    pub provider: String,
    pub role: UsageRole,
    pub usage: UsagePayload,
    pub included_in_parent: bool,
    pub recorded_at: String,
}

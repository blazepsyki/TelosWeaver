# H0 뒤 병렬 구현 착수 지시 (Phase 1)

| 항목 | 값 |
|---|---|
| 작성 | PH1-INTEGRATION (`claude-opus-5.5/session-c9336d2c`), 2026-11-16 |
| 기준 | `master`의 H0 계약 기준 Commit `bac970e`, Spec v1.5.11, rustc 1.100.0·Node 24.22.1 |
| 게이트 | H0 통과(2026-11-16 사용자 확정). 다음 게이트는 H1 수직 경로(Spec §19.6.4 3번) |
| 배정 | 담당 지정은 사용자가 합니다. 이 문서는 지정된 담당이 착수할 때 읽는 지시이며 배정 기록이 아닙니다. 배정의 단일 원본은 [배정표](../../WORK_ALLOCATION.md)입니다 |

## 1. 공통 규칙

1. **읽을 것:** [AGENTS.md](../../../../AGENTS.md), Spec §19.6과 §19.5.4·§19.5.5, [배정표](../../WORK_ALLOCATION.md), [H0 결정](../../H0_DECISIONS.md), 이 문서의 자기 작업 절, [수용 대조표](../../H0_ACCEPTANCE_MAP.md)의 자기 행.
2. **착수 기록:** 대표 저장소 `master`에서 자기 행이 `unassigned`인지 확인하고 담당·Branch·Worktree·기준 Commit을 적어 단독 커밋합니다(`unassigned → in-progress`, §7 변경 기록 한 줄, 표 행 사이 빈 줄 금지).
3. **작업 위치:** Worktree `D:\worktree\TelosWeaver\ph1-<작업>`, Branch `ph1/<작업>`(예: `ph1-persist`, `ph1/persist`). 대표 저장소 작업 트리에서 코드를 수정하지 않습니다. push하지 않습니다.
4. **계약 확인이 첫 작업입니다.** H0 계약은 사용 작업이 미배정인 상태에서 통합 담당이 검토 대행으로 확인했습니다. 구현 전에 자기 작업의 계약 타입(`crates/schema/src/`)·Core 인터페이스(`crates/core/src/<모듈>/mod.rs`)·Contract 시험을 읽고, 그대로 구현할 수 있으면 배정표 §7에 "계약 확인"을 적습니다. 바꿔야 하면 구현보다 먼저 배정표 §4에 변경 요청(`REQ1-NNN`: 필요한 필드·메서드, 이유, 영향 작업, 시험 계획)을 올립니다. 임시 계약을 자기 Branch에서 먼저 쓰지 않습니다.
5. **수정 범위:** Spec §19.6.3의 독점 경로만 수정합니다. `crates/schema/**`·`schemas/**`(CONTRACT), `crates/core/src/lib.rs`·`main.rs`·`Cargo.toml` 의존성·다른 모듈의 `mod.rs`(INTEGRATION)는 변경 요청으로 처리합니다. 새 Crate 의존성이 필요하면 이름·버전·이유를 요청에 적습니다.
6. **모듈 연결:** 다른 Core 모듈은 그 모듈의 공개 Trait으로만 씁니다(`Arc<dyn Trait>` 주입). 상대 구현이 없으면 자기 시험 안에서 그 Trait의 시험 대역을 만들어 개발합니다. 자기 Trait의 메서드 서명은 계약이므로 임의로 바꾸지 않습니다. `mod.rs`에 구현 파일(`mod sqlite;` 등)을 추가하는 것은 자기 범위입니다.
7. **Stub과 표시:** 구현을 등록하기 전에는 가용 기능으로 표시하지 않습니다. 미지원·미검증으로 남기는 기능은 사유·재확인 조건과 함께 통합 담당에게 알립니다([지원 백로그](../../../SUPPORT_BACKLOG.md)).
8. **시험:** `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`, Extension은 `npm test`. 수용 대조표에서 자기 행의 시험 수준(C·I·E)을 채울 시험을 만듭니다. Mock 통과와 실환경 통과를 구분하고, 실행하지 않은 시험을 통과로 적지 않습니다.
9. **실환경 시험:** 가벼운 모델(Phase 0과 같은 시험 모델, H0 D10)과 저장소 밖 일회용 임시 폴더만 씁니다. 사용자의 기존 대화·전역 MCP/Hook 설정을 바꾸지 않습니다. 세션 ID를 증거에 남기고 병합 시점에 정리합니다(Claude 프로젝트 폴더 삭제, Codex 공식 Archive, Antigravity 파일 삭제와 [요약 정리 도구](agy_summary_cleanup.py)). CLI로 다른 에이전트에 위임할 때는 [CLI 세션 표시 문서](../../../AGENT_CLI_VSCODE_SESSIONS.md)를 따릅니다.
10. **증거:** `docs/phase-1/evidence/<작업 ID>/README.md`에 [증거 형식](../../../phase-0/evidence/PH0-INTEGRATION/EVIDENCE_TEMPLATE.md)(Spec §19.5.8)으로 수정 파일·구현 기능·Stub/미지원·시험(명령·날짜·Host·결과·종류)·남은 요청을 적습니다.
11. **완료:** 자기 행을 `review`로 바꾸고 통합을 요청합니다. Worktree는 그대로 둡니다. 병합·정리·`integrated`/`verified` 전환은 통합 담당이 합니다.
12. **참고 자료(요구사항 아님):** [NAC 분석](../../../research/NAC_ANALYSIS.md) §5·§6·§8과 [Harness 논문 분석](../../../research/HARNESS_PAPER_ANALYSIS.md) §6·§8은 결과 확정 경계·Capsule 구성·장애 시험에 대한 제안입니다. 기존 계약 안에서 구현 선택에 참고할 수 있고, 계약·Spec을 바꾸는 제안은 변경 요청으로 올립니다.

## 2. 진행 순서

| 단계 | 작업 | 조건 |
|---|---|---|
| 1차 (지금 착수 가능) | PERSIST, SCHED, CONTEXT, WORKSPACE, ARTIFACT, CONFIG, RN, CODEX, CLAUDE, AGY | H0 계약 위에서 병렬 구현 |
| 2차 (Mock으로 먼저 개발 가능, 실제 연결은 통합 뒤) | MCP·VALIDATION·MASTER ← SCHED·ARTIFACT 구현 통합 뒤 연결. HOST ← PERSIST·SCHED 통합 뒤 연결. EXT ← HOST API 확장 통합 뒤 연결 | Spec §19.6.4 2번 |
| H1 | 마스터 → Worker Provider 하나에 위임 → Workspace → Artifact → 영속 기록 → Core 재시작 | PERSIST·SCHED·MASTER·HOST·WORKSPACE·ARTIFACT 확인 |

H1에 가장 먼저 필요한 것은 PERSIST와 SCHED입니다. 담당 수가 부족하면 PERSIST → SCHED → ARTIFACT·WORKSPACE → HOST·MASTER 순으로 먼저 배정하는 것을 권합니다.

## 3. 작업별 지시

각 절의 "계약"은 `crates/schema/src/`의 파일과 `crates/schema/tests/`의 Contract 시험, "인터페이스"는 구현할 Trait입니다. 요구사항 전체는 Spec §19.6.3과 수용 대조표의 자기 행을 따릅니다.

### 3.1 PH1-PERSIST

- **경로:** `crates/core/src/persistence/**`, `crates/core/tests/persistence_*.rs`
- **인터페이스:** `persistence::Store`(동기). `apply(WriteBatch)` 한 번이 한 Transaction이며, 쓰기마다 예상 `stateVersion`을 확인하고 하나라도 어긋나면 전체를 적용하지 않습니다. 같은 Transaction에 `JournalEntry`(빈틈없는 `sequence`)를 추가합니다.
- **계약:** `journal.rs`(`CORE_DB_SCHEMA_VERSION`, `RunIntent`·`IntentPhase`, `IdempotencyRecord`·`check_idempotency`, `db_open_decision`), `state.rs`, `journal_contract.rs`. Spec §14.
- **결정:** 프로젝트별 SQLite 파일, 사용자 상태 디렉터리(H0 D1). SQLite Crate와 Migration 방식은 담당이 제안해 의존성 변경 요청으로 올립니다.
- **완료 기준:** 조건부 쓰기·잠금 충돌·멱등 재생/충돌, 미완 의도·잠금 조회, `journal_after`의 보존 범위 밖 `cursor_expired`, 새 Schema 거부, 기록 실패 시 `Io` 반환(호출자가 후속 실행 보류), 프로세스 재시작 뒤 확정 기록 보존.

### 3.2 PH1-SCHED

**통합됨(REQ1-002, 2026-11-17).** 구현 `scheduler::TaskScheduler`, 증거 [PH1-SCHED](../PH1-SCHED/README.md), 규칙은 Spec §9.5 "Scheduler 집행 규칙"과 §14.3 "재시작 대조". 다른 작업이 Scheduler와 맞물리는 지점은 다음과 같습니다.

- **HOST:** `RunExecutor`의 `start`·`cancel`·`probe`를 구현하고, Provider 이벤트로 Run 종료를 판정해 `Scheduler::on_run_finished(RunReport)`로 보고합니다. `enforce_deadlines`를 주기적으로, `recover`를 Core 시작 때 부릅니다.
- **MASTER:** 정식 정책으로 `SchedulerPolicy`(`budget_profile`·`child_profiles`·`authorize_run`·`completion_rule`)를 구현하고, 결정을 `register_tasks`·`request_run`·`request_rework`·`cancel_task`·`complete_task`·`begin_wait`/`end_wait`로 집행합니다(`Actor::Master`의 Epoch 확인은 Scheduler가 함).
- **MCP:** `delegate_task`·`cancel_task`를 `Actor::Worker`로 부릅니다. 지금 실행할 수 없는 위임은 `SchedulerError::Unavailable`입니다.
- **VALIDATION:** 검증 Job을 만들기 전에 `begin_validation`, 판정을 저장한 뒤 `on_validation_finished`를 부릅니다.
- **CONTEXT:** §3.3의 Snapshot ID 규칙을 지킵니다.
- **Provider 승인 응답:** Scheduler를 거치지 않습니다. 승인 대기 중의 Task 상태만 `begin_wait`·`end_wait`로 표시합니다.

### 3.3 PH1-CONTEXT

- **경로:** `crates/core/src/context/**`, `crates/core/tests/context_*.rs`
- **인터페이스:** `context::ContextBuilder`. 돌려주는 Snapshot의 `runId`는 그 Run, `snapshotId`는 `RunContext.contextSnapshotId`와 같아야 합니다(Scheduler가 Run마다 먼저 정함, REQ1-002). 다르면 Scheduler가 실행하지 않습니다
- **계약:** `context.rs`(`ContextSnapshot`·`InputIssue`·`StrategyFallback`·`CapsuleContent`·`UsageRecord`), `judgment.rs`(`session_fallback`), `context_contract.rs`. Spec §6·§8, 지원 백로그 SB-11.
- **완료 기준:** 실행 전 Snapshot 고정, 필수 입력 누락·충돌·오래됨·읽기 실패 구분과 `blocking` 판정, 신뢰 구분(지침과 자료 분리), Fresh/Capsule 구성과 대체 사유 기록, Capsule의 `structured` Artifact 저장, 사용량 기록(Unknown은 0으로 적지 않음, 부모 포함분 중복 합산 금지).

### 3.4 PH1-WORKSPACE

- **경로:** `crates/core/src/workspace/**`, `crates/core/tests/workspace_*.rs`
- **인터페이스:** `workspace::WorkspaceManager`
- **계약:** `workspace.rs`(`WorkspaceRecord`·`GitBaseline`·`ResourceLock::can_take_over`·`LockMode::conflicts_with`·`DiffSummary`·`SourceMergeRequest`, `INJECTED_WORKSPACE_PATHS`), `workspace_contract.rs`. Spec §10·§14.3.
- **결정:** Worktree는 상태 디렉터리 아래, 승인 뒤 로컬 Merge Commit, push 없음(H0 D9).
- **완료 기준:** none/directory/git-worktree, 같은 Worktree의 동시 Writer 차단, Base·Dirty·기존 변경 기록과 보존, 주입 경로를 뺀 Diff, Revision 고정, 승인 Revision과 현재 Revision이 다르면 반영 거부, `Retained` 보존과 정리, 소유자 생존을 확인하지 못한 잠금은 넘겨받지 않음. Windows와 Linux 경로를 모두 시험합니다.

### 3.5 PH1-ARTIFACT

- **경로:** `crates/core/src/artifact/**`, `crates/core/tests/artifact_*.rs`
- **인터페이스:** `artifact::ArtifactStore`(동기)
- **계약:** `artifact.rs`(`ArtifactVersion`·`ArtifactHolder`·`ArtifactReference`·`DEFAULT_ARTIFACT_MAX_BYTES`), `task.rs`(OutputContract·InputBinding·`ArtifactVersionRef`), `mcp.rs`(`ReportedOutput`), `verification_contract.rs`. Spec §5.3·§10.
- **결정:** 내용 해시 주소 저장, 기본 상한 64 MiB(H0 D2).
- **완료 기준:** 텍스트·파일·JSON·Diff 등록·조회, 같은 내용 공유와 Version 기록 분리, 읽을 때 Checksum 확인, 상한 초과 거부, 참조가 남은 내용 보존과 참조 없는 내용 정리, Output 이름·Producer 기반 InputBinding 해석, Workspace 밖 경로 보고 거부.

### 3.6 PH1-CONFIG

- **경로:** `crates/core/src/config/**`, `crates/core/tests/config_*.rs`
- **인터페이스:** `config::ConfigService`(동기)
- **계약:** `config.rs`(`ConfigFile`·`ConfigLayer::may_set`·`host_bound_violations`·`ResolvedConfig`·`InstalledSetting`·`install_decision`·`uninstall_decision`), `policy.rs`(`PermissionSet`), `config_contract.rs`. Spec §13.1.
- **결정:** TOML, 사용자 `%APPDATA%\telos-weaver\config.toml`과 프로젝트 `.telos-weaver/config.toml`(H0 D7), 이름 붙인 검증 명령만(H0 D8), 기본 마스터 Claude `sonnet`(H0 D6).
- **완료 기준:** 계층 합성·출처·Digest, 권한 상한의 교집합(하위 계층이 넓히지 못함), Host 귀속 항목이 프로젝트 계층에 있으면 로드 거부, 정의되지 않은 키 거부, 설치 항목 기록과 사용자 변경 보존. Phase 0 운영자 정책 JSON에서 이 설정으로 바꾸는 전환은 통합 담당과 함께 합니다.

### 3.7 PH1-RN

- **경로:** `rn-fork/**`, `crates/rn-client/**`
- **계약:** `rn.rs`, `api.rs`의 `rn.status`·`rn.send`, `config.rs`의 설치 설정 소유권(`InstallOwner::RnFork`). Spec §12, CFG-03, SAFE-11·12·14.
- **완료 기준:** 설치·제거 때 MCP/Hook 설정의 소유권·충돌·사용자 후속 변경 보존(E2E-09), 통제된 RPC 전송과 TTL·중복 처리, 비호환·장애 때 Degraded 동작, DEC-07 제거 경로 회귀(RN-09~12). 사용자 전역 설정은 사용자의 명시적 실행·확인 범위에서만 바꿉니다.

### 3.8 PH1-CODEX · PH1-CLAUDE · PH1-AGY

배정(2026-11-16 사용자 지시): PH1-CODEX는 Codex, PH1-AGY는 Antigravity가 각자 맡고, PH1-CLAUDE는 통합 담당이 Claude 서브 에이전트로 진행합니다.

**공통**

- **경로:** Spec §19.5.3의 자기 Provider 경로만 수정합니다(`crates/adapters/src/<provider>.rs`, `crates/adapters/src/<provider>/**`, `crates/adapters/tests/<provider>_*.rs`, `crates/adapters/tests/fixtures/<provider>/**`). `crates/adapters/src/lib.rs`(Trait)·`runtime`·`discovery`·`crates/schema/**`는 변경 요청으로 처리합니다.
- **Worktree·Branch:** `D:\worktree\TelosWeaver\ph1-<provider>`, `ph1/<provider>`, 기준은 최신 `master`.
- **계약:** `provider.rs`(`RunContext.taskMcp`·`ApprovalRequiredPayload`의 `target`·`argsDigest`), `mcp.rs`(`TaskMcpBinding`: 서버 이름·명령·인자·자격 파일 경로), `judgment.rs`(`CapabilityEvidence`·`PreflightCheck`·`PreflightReport`·`judge`), `capability_keys`, `context.rs`(`CapsuleContent`). Spec §7.1·§7.3·§9.2·§19.6.6.

| # | 할 일 | 완료 기준 |
|---|---|---|
| P1 | **계약 확인** | 위 계약 타입과 Contract 시험(`mcp_contract.rs`·`judgment_contract.rs`·`policy_contract.rs`)을 읽고 그대로 구현할 수 있는지 판단. 바꿀 점은 구현 전에 배정표 §4 변경 요청 |
| P2 | **Capability 판정 기준 적용**(이월 P1-1, SB-10, H0 D11·D14) | 수단이 있는 기능의 `supported`/`unverified`를 Adapter 안의 고정 버전 비교 대신 `telos_schema::judge(통과한 버전 목록, 현재 버전, 사전 점검)`로 정합니다. 같은 버전이면 `supported`, 같은 major.minor이고 사전 점검을 통과하면 `supported`, 그 밖은 `unverified`. 실환경에서 통과한 `native_resume`·`native_fork`는 세 Provider가 같은 기준으로 판정(세부 제한은 `reason`과 실행 전 거부). 수단이 없는 기능은 계속 `unsupported`와 사유 |
| P3 | **사전 점검**(모델 호출 없음) | `detect` 때 `--version` 출력 형식, Adapter가 실제로 쓰는 옵션의 수용, `--help`의 필수 옵션을 확인해 `PreflightReport`를 만듭니다. 옵션이 없어진 CLI를 Mock으로 재현해 `unverified`로 떨어지는 시험 포함 |
| P4 | **Task MCP Run 단위 주입** | `RunContext.taskMcp`가 `Some`이면 그 Run에만 MCP 서버를 연결하고, `None`이면 지금과 같게 실행합니다. 사용자 전역 설정 파일을 읽기만 하고 쓰지 않습니다. 자격 원문을 설정 파일·인자·로그에 넣지 않고 `credentialFile` 경로만 MCP 서버 프로세스의 환경 변수 `telos_schema::TASK_MCP_CREDENTIAL_FILE_ENV`로 넘깁니다(REQ1-003). Core의 Task MCP 서버는 아직 없으므로 시험용 stdio MCP 서버(도구 하나)를 `tests/fixtures/<provider>/`에 두고 실환경에서 Worker가 그 도구를 호출하는지 확인합니다. Run이 끝나면 주입 파일을 정리합니다 |
| P5 | **Capsule 입력**(SB-11) | `SessionMode::Capsule`은 새 세션으로 시작하고 Core가 만든 Capsule 본문을 입력에 포함합니다. `capsuleVersionId`가 없으면(`SessionRequest::lacks_capsule_version`) 실행 전에 거부합니다(REQ1-003). `fresh`와 구분되는 점(본문 전달, 이전 세션 미사용)을 시험으로 고정합니다. Capsule 조립은 CONTEXT의 일이며 Adapter는 받은 입력을 전달만 합니다. 전달 경계가 지금 `TaskSpec`·`RunContext`로 부족하면 변경 요청 |
| P6 | **승인 요청의 대상·Digest** | `approval_required`를 내는 Provider는 `action`을 `telos_schema::approval_actions`의 낱말로, `argsDigest`를 `telos_schema::args_digest(인자 원문 JSON)`로 채우고 `target`도 채웁니다(REQ1-003). Core가 실행 직전에 같은 대상·인자인지 다시 확인하는 데 씁니다 |
| P7 | **증거** | `docs/phase-1/evidence/<작업 ID>/README.md`. 시험마다 Host·CLI 버전·종류(Mock/실환경)·세션 ID. 지원 백로그에 반영할 판정 변화(SB 번호별)를 따로 정리해 통합 담당에게 전달 |

**Codex (PH1-CODEX)**

| # | 할 일 | 완료 기준 |
|---|---|---|
| C1 | P4의 주입 수단 | App Server Thread 시작의 설정 재정의(`mcp_servers`)로 그 Thread에만 연결. `~/.codex/config.toml`을 바꾸지 않음을 시험 전후 비교로 확인 |
| C2 | P6 | `crates/adapters/src/codex/run.rs`의 `target: None, args_digest: None` Stub을 실제 값으로 교체 |
| C3 | SB-08 | Linux(`psyki-nas`)에서 `send_input`·`answer_approval` 실환경 시험. 서버 공용 Clone은 통합 담당이 다시 만든 뒤 진행(그 전까지 미검증으로 기록) |
| C4 | SB-09 | 명령 실행 외 승인(`fileChange`·권한 확장·MCP 도구 호출)의 종류별 실환경 결과 또는 미검증 사유. Task MCP 도구 호출이 승인 요청을 만드는지는 P4와 함께 확인 |
| C5 | 이월 P1-3 | Codex의 진행 중 조정(`turn/steer`)이 Phase 1에서도 Run당 Turn 하나 계약(§7.3) 안에 있는지 재확인 |

**Claude (PH1-CLAUDE)**

| # | 할 일 | 완료 기준 |
|---|---|---|
| L1 | P4의 주입 수단 | `--mcp-config <Run 상태 파일>`. 사용자·프로젝트의 기존 MCP 설정과 섞이는 범위를 확인해 기록하고(`--strict-mcp-config` 사용 여부 판단), 주입한 서버의 도구만 추가로 허용 |
| L2 | 실시간 승인(이월 P1-2, SB-07) | `--permission-prompt-tool stdio`로 받은 승인 요청을 `approval_required` 이벤트(P6 포함)로 내고, `answer_approval`의 Allow/Deny를 CLI에 돌려줍니다. 응답 전 Run 취소, 모르는 요청 ID, 같은 요청 중복 응답을 시험. 승인 경로가 없는 실행에서는 지금처럼 정책에 따라 거부 |
| L3 | 진행 중 조정 검토(이월 P1-3, SB-06) | 같은 Turn에 입력을 반영하는 수단이 2.1.318에 있는지 문서·실험으로 확인. 없으면 `send_input`은 계속 `UnsupportedCapability`이고 후속 입력은 Resume Run이라는 판정과 근거를 기록 |
| L4 | 검증 버전 목록 | 2.1.318 재시험 결과(H0 A1)를 `CapabilityEvidence` 형태의 근거로 반영 |

**Antigravity (PH1-AGY)**

| # | 할 일 | 완료 기준 |
|---|---|---|
| A1 | P4의 주입 수단(H0 D5) | Run Workspace의 `.agents/plugins/<이름>/{plugin.json, mcp_config.json}`과 세션 한정 `--settings '{"permissions":{"allow":["mcp(<server>/<tool>)"]}}'`. 사용자 설정(`~/.gemini/**`의 설정 파일)을 바꾸지 않음을 확인. 주입 경로는 `telos_schema::INJECTED_WORKSPACE_PATHS`(`.agents/`)와 일치시키고 Run 종료 때 지웁니다. 이미 `.agents/`가 있는 Workspace에서는 사용자 파일을 건드리지 않고 자기 플러그인 폴더만 만들고 지웁니다 |
| A2 | 자동 업데이트 대응(H0 D14) | CLI가 하루에도 버전이 바뀌므로 P2·P3가 실제로 동작하는지 1.5.x에서 확인(같은 major.minor + 사전 점검 통과 → `supported`) |
| A3 | 진행 중 조정 검토(이월 P1-3, SB-06) | Claude L3와 같은 방식으로 판정·근거 기록 |
| A4 | 시험 정리 | 시험 세션 정리 함수에 요약 원본 정리를 넣습니다([D15 기록](D15_AGY_SUMMARIES.md), [정리 도구](agy_summary_cleanup.py)). 시험 뒤 확장 대화 목록에 빈 항목이 남지 않는지 확인 |
| A5 | 미지원 재확인 | SB-01·02·04(Headless Fork·실행 중 승인 응답·특정 Turn 기준 재개)가 현재 CLI `--help`에서 달라졌는지만 확인하고, 달라지지 않았으면 미지원 유지 |

### 3.9 2차 작업 (Mock으로 먼저 개발)

| 작업 | 경로 | 인터페이스·계약 | 실제 연결 조건 |
|---|---|---|---|
| PH1-MCP | `crates/core/src/mcp/**`, `crates/core/tests/mcp_*.rs` | `mcp::TaskMcpService`, `mcp.rs`(도구 6종·`TaskMcpScope`), `api.rs`의 `tasks.tool`·`ConnectionScope::TaskMcp`. H0 D4 | SCHED 통합 뒤 |
| PH1-VALIDATION | `crates/core/src/validation/**`, `crates/core/tests/validation_*.rs` | `validation::Validator`·`ApprovalService`, `validation.rs`(`ValidationJob`·`ApprovalDecision::permits`). H0 D8 | SCHED·ARTIFACT 통합 뒤 |
| PH1-MASTER | Spec §19.5.3의 PH0-MASTER 경로 | 결정 8종을 `Scheduler`로 집행, `scheduler::SchedulerPolicy` 구현, `policy.rs`(`PermissionSet`·`BudgetProfile`·`ProviderApprovalRequest::route_for`). H0 D3·D6 | SCHED·ARTIFACT 통합 뒤 |
| PH1-HOST | Spec §19.5.3의 PH0-HOST 경로 | `scheduler::RunExecutor`(`start`·`cancel`·`probe`) 구현과 Run 종료 보고(`Scheduler::on_run_finished`), `api.rs` Phase 1 메서드와 API 버전 `1.0` 상향(EXT와 같은 변경, H0 D13), `journal.subscribe`. 이월 P1-5 | PERSIST·SCHED 통합 뒤 |
| PH1-EXT | `extension/**` | 생성 타입(`extension/src/generated/`)만 사용. QuickInput과 TreeView, Webview 없음(H0 D12) | HOST API 확장 통합 뒤 |

## 4. 통합 담당이 하는 일

- 배정표 관리, 변경 요청 판정, `lib.rs`·`main.rs` 등록과 의존성 반영, 병합과 Worktree 정리
- `core/integration`의 Phase 0 임시 구성(운영자 정책·Launcher·RN 연결)을 소유 모듈 구현이 통합된 뒤 전환
- H1 수직 경로 Harness(`crates/core/tests/phase1/`)와 수용 대조표 결과 기록
- `psyki-nas`의 `~/TelosWeaver` 공용 Clone 재생성(삭제된 상태, Linux 시험 전에 필요)

# 통합 담당 인계 (2026-11-17)

| 항목 | 값 |
|---|---|
| 작성 | `claude-opus-5.5/session-c9336d2c` (PH1-INTEGRATION·PH1-CONTRACT·PH1-PERSIST·PH1-SCHED 담당), 사용자 지시로 다른 모델에 인계 |
| 기준 | `master` `e25a102`(= `origin/master`), 공통 계약 기준 Commit `f743ef4`, Spec v1.5.12 |
| 인계 대상 | 이 담당이 맡던 네 작업 전부와, 서브 에이전트에 맡겼던 PH1-CLAUDE의 검수·통합 |
| 원본 | 배정·게이트의 단일 원본은 [배정표](../../WORK_ALLOCATION.md)입니다. 이 문서는 배정표와 저장소에 아직 없는 상태·판단만 적습니다 |

## 1. 받는 쪽이 먼저 할 일

1. [AGENTS.md](../../../../AGENTS.md), Spec §19.5.4·§19.5.5·§19.6, [배정표](../../WORK_ALLOCATION.md), [H0 결정](../../H0_DECISIONS.md), [착수 지시](H0_KICKOFF.md), 이 문서를 읽습니다.
2. 배정표에서 `claude-opus-5.5/session-c9336d2c`가 담당인 행(INTEGRATION·CONTRACT·PERSIST·SCHED·CLAUDE)의 담당자 ID를 자기 ID로 바꾸고 §7에 담당 변경을 기록합니다(대표 저장소 `master`에서 단독 커밋).
3. §3의 순서대로 진행합니다. A(SCHED 마무리)와 B(CLAUDE 검수)는 서로 독립이라 어느 쪽을 먼저 해도 됩니다.

## 2. 현재 상태

### 2.1 게이트와 작업

| 대상 | 상태 | 위치 |
|---|---|---|
| H0 공통 계약 | 통과 확정(2026-11-16 사용자) | 계약 10종 `bac970e`, REQ1-001 포함 기준 `f743ef4` |
| H1 수직 경로 | 준비 중 | PERSIST 완료, SCHED 구현 끝·통합 전. WORKSPACE·ARTIFACT·MASTER·HOST는 미배정 |
| PH1-CONTRACT | `integrated` | 이후 계약 변경 요청(`REQ1-NNN`) 처리를 계속 맡는 역할 |
| PH1-PERSIST | `integrated` (`f743ef4`) | [증거](../PH1-PERSIST/README.md). Linux 미검증 |
| PH1-SCHED | `in-progress`, **구현·시험 끝, 문서·통합 남음** | Branch `ph1/sched` `4751f30`, Worktree `D:\worktree\TelosWeaver\ph1-sched` |
| PH1-CLAUDE | 서브 에이전트 작업 끝, **검수·통합 남음** | Branch `ph1/claude` `8a32cdf`(커밋 3개), Worktree `D:\worktree\TelosWeaver\ph1-claude` |
| PH1-CODEX·PH1-AGY | `reserved` | 사용자가 Codex·Antigravity에 각자 시키기로 함. 착수 여부는 배정표 행으로 확인 |
| 나머지 10개 작업 | `unassigned` | 담당 지정은 사용자 몫 |

두 Worktree 모두 `git status`가 깨끗합니다. 대표 저장소 작업 트리도 깨끗합니다. `master`는 `origin/master`와 같습니다(사용자가 push함).

### 2.2 `ph1/sched`의 검사 결과 (`4751f30`, 2026-11-17, Windows 로컬, rustc 1.100.0)

| 검사 | 결과 |
|---|---|
| `cargo test --workspace` | 283 통과, 0 실패, 42 제외(`#[ignore]` 실환경 시험) |
| `cargo clippy --workspace --all-targets -- -D warnings` | 경고 없음 |
| `cargo fmt --all --check` | 차이 없음 |
| `scheduler_tasks` 9개 · `scheduler_runs` 15개 · `scheduler_recovery` 8개 | 모두 통과 |

`ph1/sched`는 Schema를 바꾸지 않았으므로 Schema 재생성과 Extension 시험은 다시 돌리지 않았습니다.

## 3. 남은 일

### A. PH1-SCHED 마무리 (Branch `ph1/sched`)

코드와 시험은 끝났습니다. 아래가 남았습니다.

1. **REQ1-002를 배정표 §4에 등록합니다.** H0에서 검토 대행으로 쓴 `Scheduler` 인터페이스를 구현하면서 바꾼 내용입니다(§4.1). HOST·MASTER·MCP·VALIDATION이 아직 착수 전이라 영향받는 구현은 없지만, 착수 지시와 Spec에 적힌 인터페이스 설명이 달라집니다.
2. **Spec에 반영합니다**(새 개정 v1.5.13, 변경 기록 행 추가). 사용자의 상시 지시는 "문서에 반영되지 않은 설계는 반영하고 계속"입니다. 반영할 곳과 내용은 §4입니다.
   - §9.4 또는 §9.5: Scheduler가 정한 규칙(§4.2)
   - §14.3: 재시작 대조의 판정(§4.3)
   - §19.6.2 "Core 서비스 인터페이스" 문단: `Scheduler`·`RunExecutor`·`SchedulerPolicy` 설명 갱신
3. **문서를 맞춥니다.**
   - [H0_KICKOFF.md](H0_KICKOFF.md) §3.2(SCHED 인터페이스), §3.3(Snapshot ID 규칙, §4.1의 5번), §3.9(HOST는 `RunExecutor`의 `start`·`cancel`·`probe`만 구현하고 Run 종료를 `on_run_finished`로 보고, 승인 응답 라우팅은 MASTER·HOST가 따로 정함, MASTER는 `SchedulerPolicy` 구현)
   - `crates/core/README.md`의 `scheduler` 행(구현됨, Daemon 미연결)
   - `crates/core/src/context/mod.rs`의 `ContextBuilder::build` 주석에 Snapshot ID 규칙 한 줄(§4.1의 5번). 이 파일은 CONTEXT 담당 경로이므로 REQ1-002에 포함해 처리합니다
4. **증거 문서**를 `docs/phase-1/evidence/PH1-SCHED/README.md`에 씁니다. 형식은 [PERSIST 증거](../PH1-PERSIST/README.md)를 따르고, "미구현·미검증" 표에 §4.4를 옮깁니다.
5. 공통 검사 3종을 다시 돌리고 커밋한 뒤, `master`에 `git merge --no-ff ph1/sched`로 병합하고 병합 뒤 검사를 한 번 더 돌립니다.
6. 배정표를 단독 커밋으로 갱신합니다(SCHED `integrated`, REQ1-002 완료, §5 "Task·Run·Session" 행에 구현·시험 경로, 계약 기준 Commit을 병합 Commit으로, §7 기록).
7. Worktree와 Branch를 지웁니다(`git worktree remove`, `git branch -d`).

### B. PH1-CLAUDE 검수와 통합 (Branch `ph1/claude`)

서브 에이전트의 최종 보고는 Branch 안의 `docs/phase-1/evidence/PH1-CLAUDE/README.md`(359줄)에 들어 있습니다. 아직 아무도 검수하지 않았습니다. 보고 내용을 그대로 믿지 말고 아래를 직접 확인합니다.

1. **범위 확인:** `git diff --name-only c5963c5..ph1/claude`가 독점 경로(`crates/adapters/src/claude.rs`, `crates/adapters/src/claude/**`, `crates/adapters/tests/claude_*.rs`, `crates/adapters/tests/fixtures/claude/**`, `docs/phase-1/evidence/PH1-CLAUDE/**`)만 담는지 봅니다. 보고는 14개 파일이며 모두 독점 경로 안이라고 합니다.
2. **검사 재실행:** 보고는 245 통과·0 실패·50 제외, clippy·fmt 통과입니다. `master`가 그 뒤 PERSIST를 받았으므로 병합 결과에서 다시 돌립니다. 충돌은 예상하지 않습니다(겹치는 파일 없음).
3. **코드 검수에서 볼 점**(보고가 스스로 밝힌 것):
   - 지시에 없던 동작 변경 2건: 승인 대기 중 취소 때 `interrupt` 제어 요청을 먼저 보냄, 주입한 Task MCP 서버가 연결되지 않으면 `configuration` 오류로 Run을 끝냄
   - `detect`가 CLI를 3번 실행하게 됨(1~4초)
   - 설치된 CLI가 2.1.318이 아니라 2.1.325였고, 2.1.318 증거는 H0 A1 기록을 옮긴 것(이 작업에서 재실행 안 함)
   - 증거를 플랫폼으로만 고르고 Host ID는 맞추지 않음(계약 문구는 "같은 Host·플랫폼")
   - 실시간 승인은 Adapter 단위 설정이고 기본값은 이전 동작. 켜려면 `core/integration/mod.rs`에서 `.with_approvals(ClaudeApprovals::Prompt)`
   - 앞의 커밋 2개는 메시지에 "진행 중"이 남아 있음(중단 대비로 그렇게 시킨 것, 문제 아님)
4. **변경 요청 초안 3건을 판정합니다**(CONTRACT 역할). 셋 다 구현을 막지는 않지만 Codex·Antigravity Adapter와 맞춰야 합니다.
   - 초안 A: Task MCP 자격 파일 경로를 MCP 서버 프로세스에 넘기는 환경 변수 이름(`TELOS_TASK_MCP_CREDENTIAL_FILE`)을 `telos-schema`에 상수로 둘지
   - 초안 B: `approval_required`의 `action` 낱말 목록과 `argsDigest` 계산 규칙(`sha256:<hex>`, 키 정렬 압축 JSON)
   - 초안 C: `capsule` 모드에서 `capsuleVersionId`가 필수인지
   - A와 B는 PH1-CODEX·PH1-AGY가 다르게 구현하기 전에 정하는 것이 좋습니다. 채택하면 `REQ1-NNN`으로 등록하고 [착수 지시](H0_KICKOFF.md) §3.8에 반영합니다.
5. **지원 백로그 반영:** 보고의 제안(SB-07 Adapter 부분 해소, SB-10 Claude 해소, SB-06·SB-04 유지와 근거 갱신, SB-11 문구 갱신, Linux 미검증 새 항목 3건)을 근거를 확인한 뒤 [SUPPORT_BACKLOG.md](../../../SUPPORT_BACKLOG.md)에 반영합니다. 백로그 상태 변경은 통합 담당만 합니다.
6. 병합(`--no-ff`), 배정표 단독 커밋(CLAUDE `integrated`, §5 관련 행, §7), Worktree·Branch 삭제.
7. **시험 세션 정리**(병합 시점에 하는 사용자 규칙): `~/.claude/projects`에서 이름에 `telos-claude-env`가 들어간 폴더 25개(세션 52개, 목록은 증거 문서 §8)를 지웁니다. `%TEMP%`의 `telos-claude-env\`, `telos-claude-env-state\`, `telos-claude-env-run1.log`, `telos-claude-env-run2.log`, `telos-claude-ws-test.log`도 지웁니다. 사용자의 다른 Claude 대화 폴더는 건드리지 않습니다. 지우기 전에 대상 목록을 출력해 확인합니다.

### C. 그 뒤

1. **담당 지정 요청:** H1에는 WORKSPACE·ARTIFACT·MASTER·HOST가 더 필요합니다. 사용자에게 배정을 요청합니다. 권장 순서는 ARTIFACT·WORKSPACE → HOST·MASTER입니다.
2. **Daemon 연결:** `SqliteStore`와 `TaskScheduler`는 라이브러리로만 있습니다. `telos-weaver serve`는 여전히 Phase 0의 메모리 상태로 동작합니다. HOST가 `RunExecutor`를 구현하고 Local API를 확장할 때 통합 담당이 `main.rs`·`core/integration`을 전환합니다. 그 전에는 재시작 복구나 Task 관리가 된다고 표시하지 않습니다.
3. **H1 Harness:** `crates/core/tests/phase1/`에 마스터 → Worker 위임 → Workspace → Artifact → 영속 기록 → Core 재시작 경로를 만듭니다(Spec §19.6.4 3번).
4. **Linux:** `psyki-nas`의 공용 Clone `~/TelosWeaver`가 지워진 상태입니다. 사용자 지시가 있으면 `master`로 다시 만들고, PERSIST·SCHED 시험과 Provider의 Linux 시험(Codex SB-08 등)을 실행합니다. 절차는 [LINUX_REMOTE_HOST.md](../../../phase-0/LINUX_REMOTE_HOST.md)입니다.
5. **수용 대조표:** [H0_ACCEPTANCE_MAP.md](../../H0_ACCEPTANCE_MAP.md)의 결과 열은 H3에서 채웁니다.
6. **사용자에게 남아 있는 확인:** Antigravity 확장 대화 목록에 빈 시험 항목이 더 보이지 않는지(H0 D15 정리 결과).

## 4. SCHED에서 정했지만 아직 Spec에 없는 내용

### 4.1 인터페이스 변경 (REQ1-002로 등록할 것)

`crates/core/src/scheduler/mod.rs`의 주석이 원문입니다.

1. `on_run_event(ProviderEvent)` → `on_run_finished(RunReport)`. Provider 이벤트 해석과 Run 종료 판정은 HOST(실행 감독)가 하고, Scheduler는 확정된 종료·중단 보고만 받습니다. `RunReport.status`는 `Succeeded`·`Failed`·`Cancelled`·`Unknown`·`Interrupted` 중 하나입니다.
2. `create_task` → `register_tasks`(마스터·사용자가 `TaskSpec` 목록을 등록)와 `delegate_task`(Worker의 Task MCP 위임)로 나눔. `request_run`은 `SessionRequest`를 받고 `RunAdmission::{Started, Queued}`를 돌려줌.
3. 추가: `begin_validation`, `on_validation_finished(task_id, validation_id)`(판정은 저장소에서 읽음), `begin_wait`·`end_wait`, `complete_task`(`propose_completion`의 이름 변경), `reconcile_task`, `enforce_deadlines`.
4. 삭제: `Scheduler::answer_provider_approval`, `RunExecutor::answer_approval`. Scheduler가 쓰지 않는 호출이었습니다. Provider 승인 응답의 경로(HOST ↔ MASTER/VALIDATION)는 그 작업들이 착수할 때 계약 `ProviderApprovalRequest` 위에서 정합니다. 승인 대기 중의 Task 상태는 `begin_wait`·`end_wait`로 표시합니다.
5. `ContextBuilder::build`가 돌려주는 Snapshot은 `run_id`가 그 Run이고 `snapshot_id`가 `RunContext.context_snapshot_id`와 같아야 합니다. Scheduler가 ID를 먼저 정해 넘기고, 다르면 실행하지 않습니다.
6. 새 Port `SchedulerPolicy`(MASTER의 정식 정책이 구현): `budget_profile`, `child_profiles`(자식의 권한·예산·완료 정책을 부모 범위 안에서 정함), `authorize_run`(Host·Provider·권한·Session 소유권), `completion_rule`(결과 검토 승인 필요 여부).
7. `SchedulerError`에 `DependencyCycle`·`StartFailed{confirmed}`·`Conflict` 추가. `RecoveryReport`의 항목을 `returned_to_ready`·`interrupted`·`needs_reconciliation`·`released_locks`로 변경.

### 4.2 Scheduler가 집행하는 규칙

- **단일 소유권:** 프로젝트 DB를 독점한 Core 프로세스 안에서 `TaskScheduler` 하나가 모든 변경을 한 줄로 처리합니다. 한 요청의 상태 변경은 저장소의 한 배치입니다.
- **등록:** 의존 Task가 모두 `Completed`면 `Ready`, 아니면 `Blocked`로 저장합니다(`Created`는 저장하지 않음). 순환·없는 선행 Task·중복 ID·다른 Plan/Project는 요청 전체를 거부합니다. 위임 깊이는 부모 사슬로 세며 최상위가 0입니다.
- **예산 장부:** 등록하면 `tasksReserved` +1, Task가 종료 상태가 되면 예약에서 소비로 옮깁니다. 취소된 Task도 소비로 셉니다(만들고 취소하는 반복으로 상한을 피하지 못하게). Worker 위임은 마스터 예약분을 뺀 범위만 씁니다. 토큰·비용은 Scheduler가 다루지 않습니다.
- **Run 시작 순서:** 정책 확인 → 동시성 → Workspace 준비(잠금) → 문맥 Snapshot → [Task `Running` + Run `Starting` + 의도 `recorded` + Snapshot + 멱등 키]를 한 배치 → 의도 `dispatched` → Adapter 시작 → [의도 `started` + Run `Running` + Session 기록].
- **동시성:** Provider·Host별로 끝이 확정되지 않은 실행 의도 수를 셉니다. 자식을 기다리는 부모, 추적을 잃은 Run, 결과 불명 Run도 자리를 차지합니다. 마스터·사용자의 요청은 대기열(메모리, 요청 순서)에 넣고 자리가 나면 시작합니다. Worker의 위임은 대기열 없이 바로 `Unavailable`입니다.
- **잠금:** Workspace Writer 잠금은 `WorkspaceManager::prepare`가 잡고, Run이 확정 종료할 때 Scheduler가 종료 기록과 같은 배치에서 그 Run의 잠금을 모두 풉니다. `resume`은 Session Writer 잠금을 함께 잡습니다. 결과를 모르는 Run의 잠금은 대조가 끝날 때까지 둡니다.
- **재실행은 요청이 있을 때만:** 실패 뒤 Task를 `Ready`로 되돌릴 뿐 Scheduler가 스스로 새 Run을 만들지 않습니다. 부작용 확인은 다시 요청하는 마스터·사용자의 판단이고, 새 Run의 Snapshot이 Workspace의 현재 상태를 기록합니다.
- **재시도:** 같은 Task Revision의 실패 Run 수가 `executionRetryLimit` 이하이고 실패 종류가 `authentication`·`configuration`·`unsupported_capability`·`tool_denied`가 아니면 `Ready`, 아니면 `Failed`.
- **재작업:** 같은 Revision의 성공 Run 수가 `reworkLimit` 이하일 때만 `Ready`. 필수 검증이 실패하면 자동으로 이 경로를 탑니다. 실행 오류·판정 불가·건너뜀·취소된 검증은 통과가 아니며 `WaitingMaster`(복귀 `Validating`)로 둡니다.
- **완료:** `complete_task`만 `Completed`를 만듭니다. 필수 검증이 있으면 `Verified`에서만, 없으면 `Executed`에서 가능하고, 완료 정책이 요구하면 그 Revision의 허용된 결과 검토 승인이 있어야 합니다. 완료 뒤 의존 Task를 풀고, `WaitingChild` 부모는 기다리던 자식이 모두 종료 상태가 되면 복귀시킵니다(자식의 Run 성공만으로는 깨우지 않음).
- **취소:** 실행이 없는 상태는 바로 `Cancelled`, 실행·검증·대기 중은 `Cancelling`으로 두고 Run 정지를 요청합니다. 멈출 Run이 없으면 바로 `Cancelled`입니다. `cancelPropagation`이 `descendants`인 자손에 전파하고 `independent` 자식과 그 아래는 건드리지 않습니다. `Cancelling` Task는 Run이 성공으로 끝나도 `Cancelled`입니다. Task 취소 없이 Run만 취소된 경우에도 Task를 취소로 확정합니다.
- **이전 Run의 보고:** Task의 가장 최근 Run이 아닌 Run의 보고와 이미 끝난 Run의 보고는 Run 기록만 갱신하거나 무시하고 Task를 바꾸지 않습니다.
- **기한:** 실행 시간 상한을 넘긴 Run은 `Cancelling`으로 적고 정지를 요청하며, 그 뒤의 정지 보고는 취소가 아니라 `Failed`(Timeout)로 기록해 재시도 규칙을 따릅니다. `WaitingChild`의 기한이 지나면 부모는 결과 없이 복귀합니다. 마스터·사용자 응답 대기는 복귀 상태가 `Running`이면 복귀(호출자가 승인 요청을 거부로 답함), 아니면 `Failed`입니다.
- **주체:** 마스터의 요청은 Plan이 `Active`이고 `controllerEpoch`가 같을 때만 받습니다. Worker는 살아 있는 자기 Run의 자격으로 위임과 자기 자손의 취소만 할 수 있습니다.

### 4.3 재시작 대조 (`recover`)

| 남은 실행 의도 | 처리 |
|---|---|
| `recorded` (전달 전) | 외부 실행이 없었음. 의도를 `abandoned`, Run을 `Cancelled`, Task를 `Ready`로 되돌리고 잠금을 풉니다. 프로세스를 찾지 않습니다 |
| `dispatched`·`started`, 프로세스 살아 있음 | Run·Task를 `Interrupted`로 둡니다. P0의 Core는 출력을 다시 받을 수 없습니다. 잠금·의도는 그대로 |
| `dispatched`·`started`, 프로세스 없음 또는 확인 불가, 증거 없음 | Run을 `Unknown`, Task를 `NeedsReconciliation`으로 둡니다. 다시 실행하지 않습니다. 잠금·의도는 그대로 |
| 취소 중이던 Task | 정지를 확인하지 못했으므로 `Cancelled`로 확정하지 않고 `NeedsReconciliation` |

주인 Run이 없거나 확정 종료한 잠금만 풉니다. `Interrupted`·`NeedsReconciliation` Task는 `reconcile_task`(다시 실행·결과 채택·실패·취소)로만 풀리며, 그때 남은 Run·의도·잠금을 정리합니다. 대조를 반복해도 결과가 같고 기록이 늘지 않습니다.

### 4.4 SCHED의 미구현·미검증 (증거 문서에 옮길 것)

| 항목 | 내용 |
|---|---|
| Daemon 연결 | `TaskScheduler`는 라이브러리로만 있음. `main.rs`·Local API와 연결 전 |
| 실제 의존 모듈 | 시험은 실제 `SqliteStore`와 대역(`RunExecutor`·`ContextBuilder`·`WorkspaceManager`·`SchedulerPolicy`)으로 실행. 실제 Provider·Workspace·문맥과의 결합은 H1에서 확인 |
| Task MCP 연결 발급 | `task_mcp`가 있으면 `issue_binding`·`revoke`를 부르는 코드가 있으나 시험하지 않음(시험은 `None`). `remainingChildren` 값은 임시 계산 |
| 대기열 | 메모리에만 있음. Core 재시작 뒤 Task는 `Ready`로 남고 다시 요청해야 함 |
| 검증 취소 | `Validating` Task를 취소하면 상태만 바꿈. 돌고 있는 검증 Job을 멈추는 것은 VALIDATION의 일 |
| `DelegateTaskInput.deadline` | 부모의 대기 기한으로만 씀. 자식 Task 자체의 기한은 저장할 필드가 없음 |
| 승인 조건 | 완료 때 결과 검토 승인의 Task·Revision·상태만 확인. 대상 Artifact Version 일치(`ApprovalDecision::permits`)는 VALIDATION·MASTER 몫 |
| 토큰·비용 예산 | 다루지 않음(Task 수 상한만). 사용량 기록은 CONTEXT |
| 저장소 호출 | 동기 `Store`를 비동기 함수 안에서 직접 부름. 부하가 문제면 `spawn_blocking`으로 옮길 것 |
| Linux | 미실행 |

## 5. 환경과 절차에서 알아둘 것

- **cargo는 PowerShell에서 실행합니다.** 이 머신의 Git Bash에서는 `link.exe`가 Git의 coreutils로 잡혀 링크가 실패합니다(`/usr/bin/link: extra operand`). 예: `Set-Location D:\worktree\TelosWeaver\ph1-sched; cargo test --workspace`. Git 명령과 문서 편집은 Git Bash로 해도 됩니다.
- **Python:** Git Bash의 `python`은 동작하지 않는 별칭입니다. `/c/Users/Psyki-HOME/AppData/Local/Python/bin/python`을 씁니다. 파일을 쓸 때 `newline='\n'`을 지정하지 않으면 CRLF가 됩니다.
- **대표 저장소 작업 트리는 다른 에이전트와 함께 씁니다.** 코드 변경은 `D:\worktree\TelosWeaver\<작업>` Worktree에서 하고, `master`에서는 병합 Commit과 문서·배정표 커밋만 만듭니다. 병합을 staged 상태로 남겨 두지 않습니다.
- **배정표 수정은 `master`에서 단독 커밋**입니다. §7 표의 행 사이에 빈 줄을 넣지 않습니다.
- **기본 Branch는 `master`**입니다. `main`을 만들지 않습니다. **push는 사용자가 요청할 때만** 합니다. push 전에는 `docs/archive`가 추적되지 않는지와 64자리 16진 토큰 문자열이 없는지 확인합니다.
- **병합 뒤**에는 Worktree와 Branch를 지우고, 그 작업이 만든 시험 세션을 정리합니다(Claude 프로젝트 폴더 삭제, Codex는 공식 Archive, Antigravity는 파일 삭제와 [요약 정리 도구](agy_summary_cleanup.py)).
- **Spec 변경**은 사용자 합의가 원칙입니다. 사용자는 구현 중 정한 설계를 Spec에 반영하고 계속하라고 지시했습니다. 요구사항이나 사용자 결정(H0 D1~D15)을 바꾸는 변경은 먼저 묻습니다.
- **사용자에게 보이는 글**은 모두 한국어, 존대, 꾸밈 없는 문체입니다. 사용자의 전역 에이전트 설정(`~/.claude/**`, `~/.codex/**`, `~/.gemini/**`)은 바꾸지 않습니다.
- **서브 에이전트**는 사용자가 요청할 때만 씁니다. PH1-CLAUDE를 서브 에이전트로 한 것은 사용자의 명시적 지시였습니다.
- **실환경 시험**은 가벼운 모델(Claude `haiku`, Codex `gpt-6-luna` low, Antigravity Flash low)과 저장소 밖 일회용 폴더만 씁니다(H0 D10).

## 6. 이번 담당 기간의 커밋 (참고)

| 내용 | Commit |
|---|---|
| H0 계약 10종·Core 인터페이스 병합 | `bac970e` |
| H0 통과 확정 기록, 착수 지시 | `aab7ec5`, `df2a5e5`, `c5963c5` |
| PH1-PERSIST(`SqliteStore`), REQ1-001, Spec v1.5.12 | `fc4b79e`, `77dfabb`, 병합 `f743ef4` |
| PH1-SCHED 구현·시험(통합 전) | `ph1/sched` `4751f30` |
| PH1-CLAUDE(서브 에이전트, 검수 전) | `ph1/claude` `a24ffed`, `c7ebf8a`, `8a32cdf` |

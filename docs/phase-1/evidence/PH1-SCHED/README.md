# PH1-SCHED 검증 기록

> Spec §19.5.8 형식입니다. 실행하지 않은 시험을 통과로 적지 않습니다.

| 항목 | 값 |
|---|---|
| 작업 ID | PH1-SCHED |
| 담당 | `claude-opus-5.5/session-c9336d2c` (사용자 지정 2026-11-16, 통합 담당이 직접. 2026-11-17 인계 문서로 모델 교체 뒤 이어서 마무리) |
| Branch / Worktree | `ph1/sched` / `D:\worktree\TelosWeaver\ph1-sched` |
| 기준 Commit | `e25a102` (PERSIST 통합 `f743ef4` 뒤) |
| 결과 Commit | 배정표의 PH1-SCHED 행에 기록 |
| 계약 기준 | `f743ef4` + REQ1-002 |

## 1. 수정 파일

- 독점 경로: `crates/core/src/scheduler/mod.rs`, `crates/core/src/scheduler/service.rs`, `crates/core/src/scheduler/time.rs`, `crates/core/tests/scheduler_tasks.rs`, `crates/core/tests/scheduler_runs.rs`, `crates/core/tests/scheduler_recovery.rs`, `crates/core/tests/fixtures/scheduler/mod.rs`, 이 문서
- REQ1-002로 처리한 다른 경로: `crates/core/src/context/mod.rs`(주석 한 줄), `docs/DEVELOPMENT_v1.5.md`(v1.5.13), `crates/core/README.md`, `docs/phase-1/evidence/PH1-INTEGRATION/H0_KICKOFF.md`
- `telos-schema`(Wire 계약)는 바꾸지 않았습니다. Schema 생성물 변화가 없습니다.

## 2. 구현한 기능

`scheduler::TaskScheduler`가 `Scheduler` Trait을 구현합니다. 규칙의 원문은 Spec §9.5 "Scheduler 집행 규칙"과 §14.3 "재시작 대조"입니다.

- **계약 확인:** H0 인터페이스를 Spec §9와 대조해 REQ1-002로 바꿨습니다(§6).
- **단일 처리 순서:** 모든 변경이 하나의 문(`tokio::sync::Mutex`)을 차례로 지나고, 한 요청의 상태 변경은 저장소의 한 배치입니다.
- **등록:** 의존성·순환·중복 ID·Plan/Project 검사, 위임 깊이, Plan Task 수 상한, 멱등 재생.
- **Run 시작:** 정책 확인 → 동시성 → Workspace 준비 → Snapshot → [Task·Run·의도 `recorded`·Snapshot·멱등 키] 한 배치 → `dispatched` → Adapter 시작 → `started`. 막는 입력 문제는 `Blocked`, 전략 대체는 실제 쓴 전략으로 전달, Session 기록(`created`·`resumed`·`forked`), `resume`의 Session Writer 잠금.
- **동시성:** Provider·Host별 미확정 실행 의도 수, 마스터·사용자 요청의 메모리 대기열, Worker 위임의 즉시 거부.
- **종료 반영:** Run 종료 보고로 Run·의도·잠금·Task를 한 배치로 갱신, 재시도·재작업 상한, 이전 Run과 중복 보고 무시.
- **검증·완료:** 검증 시작, 필수 검증의 마지막 Job으로 판정, 완료 정책의 결과 검토 승인 확인, 후속 Task 해제, `WaitingChild` 부모 복귀.
- **취소:** 접수와 정지 구분, 자손 전파(`independent` 제외), 늦은 성공 차단, 대기열에서 제거.
- **위임:** 정책이 정한 Profile, 깊이·Worker 몫 상한, 지금 실행할 수 없으면 등록하지 않고 거부, 시작 실패한 자식 정리, 부모 대기.
- **기한:** 실행 시간 상한(정지 뒤 Timeout 실패), 대기 기한.
- **재시작 대조:** 의도 단계와 프로세스 생존 확인으로 `Ready`·`Interrupted`·`NeedsReconciliation`을 가르고, 확정 종료·주인 없는 잠금만 풂. 대조 판단(`reconcile_task`)으로 남은 Run·의도·잠금 정리.
- **시각:** RFC 3339 UTC 초 단위의 생성·해석(`time.rs`). 기한 비교에 씁니다.

## 3. Stub·미지원·미검증

| 항목 | 상태 | 내용 |
|---|---|---|
| Daemon 연결 | 미연결 | `TaskScheduler`는 라이브러리로만 있습니다. `telos-weaver serve`는 아직 Phase 0의 메모리 상태로 동작합니다. HOST가 `RunExecutor`와 Local API를 연결할 때 통합 담당이 전환합니다. 그 전에는 Task 관리·재시작 복구를 지원한다고 표시하지 않습니다 |
| 실제 의존 모듈 | 대역 | 시험은 실제 `SqliteStore`와 대역(`RunExecutor`·`ContextBuilder`·`WorkspaceManager`·`SchedulerPolicy`)으로 실행했습니다. 실제 Provider·Workspace·문맥과의 결합은 H1에서 확인합니다 |
| Task MCP 연결 발급 | 미검증 | `task_mcp`가 있으면 `issue_binding`·`revoke`를 부르지만 시험은 `None`으로만 했습니다. `remainingChildren`은 Plan Task 상한을 넣는 임시 계산입니다 |
| 대기열 | 제한 | Core 메모리에만 있습니다. 재시작 뒤 Task는 `Ready`로 남고 다시 요청해야 합니다 |
| 검증 취소 | 범위 밖 | `Validating` Task를 취소하면 상태만 바꿉니다. 돌고 있는 검증 Job의 정지는 VALIDATION의 일입니다 |
| `DelegateTaskInput.deadline` | 제한 | 부모의 대기 기한으로만 씁니다. 자식 Task 자체의 기한을 저장할 필드가 없습니다 |
| 승인 대상 일치 | 범위 밖 | 완료 때 결과 검토 승인의 Task·Revision·상태만 봅니다. 대상 Artifact Version 일치(`ApprovalDecision::permits`)는 VALIDATION·MASTER가 확인합니다 |
| 토큰·비용 예산 | 범위 밖 | Task 수 상한만 집행합니다. 사용량 기록은 CONTEXT, 집행은 정책입니다 |
| 저장소 호출 | 제한 | 동기 `Store`를 비동기 함수 안에서 직접 부릅니다. 부하가 문제가 되면 `spawn_blocking`으로 옮깁니다 |
| Linux | 미검증 | `psyki-nas` 공용 Clone이 없어 실행하지 않았습니다. 재확인: 서버에서 `cargo test -p telos-core --test scheduler_tasks --test scheduler_runs --test scheduler_recovery` |

## 4. 시험

실행 환경: Windows 11 Pro 10.0.26300(Host `Psyki-HOME`, Windows 로컬), rustc 1.100.0, PowerShell에서 실행(Git Bash에서는 `link.exe`가 Git coreutils로 잡혀 링크 실패). 외부 CLI·모델 호출이 없는 시험입니다.

| 명령 | 날짜 | OS/Host | 기대 결과 | 실제 결과 | 종료 상태 | 종류 |
|---|---|---|---|---|---|---|
| `cargo test -p telos-core --test scheduler_tasks` | 2026-11-17 | Windows 로컬 | 9개 통과 | 9 passed | 0 | 통합(실제 SQLite, 의존 모듈 대역) |
| `cargo test -p telos-core --test scheduler_runs` | 2026-11-17 | Windows 로컬 | 15개 통과 | 15 passed | 0 | 통합(실제 SQLite, 의존 모듈 대역) |
| `cargo test -p telos-core --test scheduler_recovery` | 2026-11-17 | Windows 로컬 | 8개 통과 | 8 passed | 0 | 통합(실제 SQLite, Core 재시작 재현) |
| `cargo test -p telos-core --lib scheduler` | 2026-11-17 | Windows 로컬 | 2개 통과 | 2 passed | 0 | 단위(시각 변환) |
| `cargo test --workspace` | 2026-11-17 | Windows 로컬 | 전체 통과 | 283 passed, 0 failed, 42 ignored | 0 | 전체 회귀 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 2026-11-17 | Windows 로컬 | 경고 없음 | 경고 없음 | 0 | 정적 검사 |
| `cargo fmt --all --check` | 2026-11-17 | Windows 로컬 | 차이 없음 | 차이 없음 | 0 | 형식 |

시험별 확인 내용:

| 시험 | 확인 | 요구사항 |
|---|---|---|
| `registration_checks_dependencies_and_cycles` | Ready/Blocked 판정, 3개 순환·자기 의존·없는 선행·중복 ID 거부, 거부 뒤 흔적 없음 | TASK-01·05 |
| `registration_is_idempotent` | 같은 키·같은 요청 재생, 다른 요청 거부 | SAFE-13 |
| `depth_and_plan_task_limits_are_enforced` | 같은 요청 안의 부모 사슬 깊이, 깊이 상한, 없는 부모, Plan Task 수 상한 | DELEG-04, SAFE-02 |
| `stale_or_held_master_cannot_change_tasks` | 이전 Epoch·Worker 등록 거부, 보류된 Plan의 마스터 요청 거부, 사용자 취소는 허용 | MASTER-04 |
| `completion_is_explicit_and_releases_dependents` | Run 성공은 `Executed`, 완료 판정 뒤 후속 Task 해제, 예산 소비 | DELEG-09, §9.1 6번 |
| `required_validation_gates_completion` | 필수 검증 전 완료 거부, 일부 판정은 대기, 결과 검토 승인 요구 | VERIFY-02·03 |
| `failed_validation_leads_to_rework_within_the_limit` | 검증 실패 → 재작업, 판정 불가 → 마스터 대기, 상한 초과 → `Failed`, 늦은 판정 무시 | VAL-01, VERIFY-07 |
| `requested_rework_respects_the_limit` | 실행 전 재작업 거부, 상한 | §9.4 |
| `waiting_for_an_answer_records_where_to_resume` | 대기 복귀 상태와 Run `Waiting` 전환 | §9.4 |
| `a_run_is_recorded_before_and_after_dispatch` | Run·의도·Snapshot·Session·잠금 기록, 같은 키 재요청, 겹친 Run 거부, 종료 뒤 의도·잠금 정리 | §9.1, SESS-01, SAFE-13 |
| `blocked_or_unauthorized_tasks_do_not_run` | 선행 미완·정책 거부·없는 Task는 Run을 만들지 않음 | §9.1 2번 |
| `concurrency_limit_queues_requests_in_order` | 상한 1에서 대기열 순서, 재요청 중복 없음, 대기 중 취소 | TASK-03 |
| `busy_workspace_is_refused_without_side_effects` | Writer 잠금 충돌의 즉시 거부와 무흔적, 해제 뒤 실행 | GIT-02, DELEG-10 |
| `blocking_input_issue_holds_the_task` | 막는 입력은 `Blocked`·잠금 해제, 문맥 실패는 Task 유지 | CTX-02 |
| `session_strategy_is_checked_and_fallback_is_applied` | 다른 Task의 Session 거부, 원본 없는 재개 거부, Session 잠금, Capsule 대체 반영 | §8.1, FORK-03 |
| `failures_retry_only_within_the_limit` | 재시도 상한, 자동 재실행 없음, 재시도 무의미한 실패 | TASK-06 |
| `rejected_start_and_lost_start_response_differ` | 시작 거부(`Ready`, 무흔적)와 응답 유실(`NeedsReconciliation`, 의도 `dispatched`) | §14.3 |
| `cancel_wins_over_a_late_success` | 즉시 취소, `Cancelling`, 늦은 성공에도 `Cancelled`, 중복 보고 무시 | §9.5, E2E-06 |
| `late_report_of_an_older_run_does_not_touch_the_task` | 추적 유실, 대조 뒤 새 Run, 이전 Run 결과 미채택 | §9.5 |
| `cancel_propagates_to_descendants` | 자손 전파, `independent` 제외, 실행 중 자식 정지 | DELEG-10 |
| `delegation_runs_the_child_and_wakes_the_waiting_parent` | 위임 즉시 실행, 부모 `WaitingChild`, 멱등, 완료 뒤 복귀, Worker 취소 범위, 끝난 Run 자격 거부 | DELEG-01·04, SAFE-08 |
| `delegation_that_cannot_run_now_is_refused` | 동시성·Workspace 잠금·깊이·정책 거부, 시작 못 한 자식 정리 | §9.3, E2E-04 |
| `run_timeout_stops_the_run_and_counts_as_failure` | 실행 시간 상한, 정지 요청 1회, Timeout 실패로 기록 | TASK-06 |
| `expired_waits_do_not_hang_forever` | 사용자 응답 기한 → `Failed`, 자식 대기 기한 → 부모 복귀 | §9.3 |
| `intent_recorded_but_never_dispatched_returns_to_ready` | 전달 전 장애: `Ready`, 의도 `abandoned`, 잠금 해제, 생존 확인 안 함 | §14.3 |
| `live_process_after_restart_is_interrupted_not_rerun` | 살아 있는 프로세스: `Interrupted`, 증거 전체로 확인, 재실행 거부, 반복 대조 무변화 | §14.2, OPS-03 |
| `vanished_process_needs_reconciliation_before_any_rerun` | 사라진 프로세스: `NeedsReconciliation`, 결과 채택 뒤 `Executed`·정리·완료 | §14.3, SESS-05 |
| `unconfirmed_process_is_not_assumed_dead` | 확인 불가는 죽음으로 보지 않음, 대조 뒤 재실행, 이전 Run 결과 미채택 | §14.3 |
| `dispatched_without_start_result_is_held` | 증거 없는 `dispatched`는 대조 대기, 실패 판단 뒤 정리 | §14.3 |
| `only_locks_of_settled_or_missing_runs_are_released` | 주인 없는 잠금만 해제, 결과 불명 Run의 잠금 유지 | §14.3 |
| `cancel_in_flight_is_not_confirmed_by_a_restart` | 취소 중 장애는 `NeedsReconciliation`, 취소 판단 뒤 확정 | §9.5 |
| `settled_work_is_untouched_by_recovery` | 확정 결과 보존, 대조 무변화, 대기열은 재시작 뒤 비어 있음 | SESS-03 |

- 로그·Fixture 위치: 시험은 `%TEMP%\telos-persist\sched-*` 아래 일회용 DB를 쓰고 끝나면 지웁니다.

## 5. 반례 결과

§4 표의 거부·장애 시험이 반례입니다. 요약하면 다음 상황에서 Scheduler는 Run을 새로 만들지 않거나 Task를 확정하지 않습니다: 순환·상한 초과·이전 Epoch·보류 Plan, 정책 거부, 잠긴 Workspace, 막는 입력, 시작 응답 유실, 결과 불명·추적 유실, 취소 중 늦은 성공, 이전 Run의 늦은 보고, 재시작 뒤 생존 확인 불가.

## 6. 변경 요청과 인계

**REQ1-002 (채택·처리):** H0 `Scheduler`·`RunExecutor`를 구현에 맞게 바꿨습니다. 내용은 배정표 §4와 Spec §19.6.2 "Core 서비스 인터페이스" 문단에 있습니다. 바뀐 인터페이스를 쓰는 작업(HOST·MASTER·MCP·VALIDATION·CONTEXT)은 모두 착수 전이었습니다.

**인계:**

- **HOST:** `RunExecutor`(`start`·`cancel`·`probe`)를 구현하고, Provider 이벤트로 Run 종료를 판정해 `on_run_finished`로 보고합니다. `start`가 시작하지 않았음이 확실하면 `Rejected`, 결과를 모르면 `Unknown`을 돌려줍니다. `probe`는 PID 재사용을 증거(시작 시각·실행 토큰 Digest)로 걸러야 합니다. Core 시작 때 `recover`, 주기적으로 `enforce_deadlines`를 부릅니다.
- **MASTER:** `SchedulerPolicy`를 구현합니다. `authorize_run`이 Session 소유권을 확인합니다(`source`는 `resume`·`native-fork`의 원본 Session).
- **CONTEXT:** Snapshot ID 규칙(REQ1-002 6번)을 지킵니다. 전략을 대체하면 `strategy`와 `fallback`에 적고, Capsule이면 `capsule`에 Version을 넣습니다. Scheduler가 그 값으로 Adapter에 전달합니다.
- **WORKSPACE:** `prepare`가 쓰기 Task의 Writer 잠금을 저장소에 잡고 `holder_run_id`를 그 Run으로 둡니다. 해제는 Scheduler가 Run 종료 때 합니다.
- **MCP·VALIDATION:** 착수 지시 §3.2의 연결 지점을 따릅니다.
- **통합 담당:** Daemon 연결, Linux 시험, H1 Harness.

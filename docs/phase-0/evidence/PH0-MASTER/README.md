# PH0-MASTER 구현 및 검증 기록

| 항목 | 값 |
|---|---|
| 작업 ID·담당 | PH0-MASTER · `claude-opus-5.5/session-c9336d2c` |
| 작성·검증일 | 2026-10-24, Windows 로컬 |
| Branch / Worktree | `ph0/master` / `D:\worktree\TelosWeaver\ph0-master` |
| 기준 | `cc20ade`, 계약 `271e7fd` |
| 사용자 결정 (배정표 §6) | 마스터 Provider 임시 Claude(교체 가능), 시험 모델 `haiku`, 집행 kind 3종(`create_tasks`·`request_run`·`request_cancel`), 결과 왕복 시험 1개, 마스터 Run 도구 없음·읽기 전용 |

Spec §19.1의 Phase 0 조건 "마스터 후보가 구조화 결정을 반환하고 Core가 잘못된 결정·중복·권한 초과를 거부하는 최소 경로"와 §19.5.7의 PH0-MASTER 범위(결정 회수, kind별 검증, 권한·Epoch·멱등 거부, 허용 결정의 실행 결과 연결)를 구현했습니다.

## 1. 수정 파일

| 경로 | 소유 | 내용 |
|---|---|---|
| `crates/core/src/policy/mod.rs` | MASTER | 신뢰된 조회 `TrustedLookup`·`LookupDenial` (REQ-008) |
| `crates/core/src/policy/validator.rs` | MASTER | 결정 검증 `validate` |
| `crates/core/src/master/mod.rs`, `controller.rs`, `output.rs` | MASTER | `MasterController`, `RunExecutor`, 마스터 프롬프트·응답 해석 |
| `crates/core/tests/master_validation.rs`, `master_controller.rs` | MASTER | 검증기 8개, Controller 6개 |
| `crates/core/src/integration/{mod,policy}.rs` | INTEGRATION | 운영자 정책의 `TrustedLookup` 구현, 마스터 Run 읽기 전용 강제·Claude 도구 없음, `Phase0Execution::master` |
| `crates/core/tests/phase0_policy.rs`, `phase0_master.rs` | INTEGRATION | 정책 시험 1개 추가, 실제 마스터 실환경 2개 |

INTEGRATION 경로는 같은 담당자(통합 담당)가 수정했습니다. 공통 계약(Schema·Trait·Cargo)은 바꾸지 않았습니다.

## 2. 구현 내용

### 2.1 결정 검증 (`core/policy`)

Spec §4.2 집행 순서의 검사 단계를 이 순서로 수행하고 처음 실패한 사유로 거부합니다.

1. 호출자: 결정이 이 Plan의 **현재 활성 마스터 Run**에서 왔는지 → `unauthorized_caller`
2. Plan·Epoch·Revision: `plan_id` → `unauthorized_caller`, Epoch → `stale_controller_epoch`, Plan·Policy Revision → `stale_plan_revision`·`stale_policy_revision`
3. Payload: kind별 Schema(미정의 필드 거부) → `invalid_payload`
4. kind: Phase 0 집행 3종 외 → `unsupported_kind` (Payload 검사 뒤)
5. kind별:
   - `create_tasks`: Task 수 상한 → `limit_exceeded`, ID 중복 → `invalid_payload`, 다른 Project·Plan·`role: master` Task·허용 목록 밖의 Provider·Host·Workspace 모드·권한·예산 Profile → `permission_denied`, 부모·의존성 없음 → `unknown_task`, 위임 깊이 → `limit_exceeded`, 의존성 순환 → `dependency_cycle`
   - `request_run`: Task·Revision 없음 → `unknown_task`, `Ready` 아님 → `invalid_payload`, Capsule → `unsupported_capability`
   - `request_cancel`: Task 없음 → `unknown_task`
6. 신뢰된 조회(`TrustedLookup::check_task`): 거부 → `permission_denied`, 조회 실패 → `policy_unavailable`, 검사 불가 → `check_unsupported` (허용으로 간주하지 않음)

### 2.2 Master Controller (`core/master`)

- **결정 회수:** 마스터를 제어용 Task(`type: general`, `role: master`)로 실행합니다. 최종 응답의 ```json 블록 하나를 `{kind, payload, rationale}`로만 해석하며 다른 필드는 거부합니다. 결정 ID·Plan·Epoch·Policy Revision·마스터 Run·멱등 키는 Core가 채웁니다. Provider 전용 구조화 출력에 기대지 않아 마스터 Provider를 바꿔도 같은 형식입니다.
- **멱등:** 같은 키·같은 내용은 처음 결과를 `replayed: true`로 돌려주고 다시 집행하지 않습니다. 같은 키·다른 내용은 `idempotency_conflict`입니다.
- **집행:** `create_tasks` → Task 등록(`Ready`)과 Plan Revision 증가, `request_run` → 실행 감독으로 Worker Run 시작(Run 시작 시 정책 재검사가 다시 적용됨), `request_cancel` → Run 취소 요청(접수와 실제 종료 구분). 실행 실패는 결정 거부로 기록하며 부분 집행은 없습니다.
- **결과 왕복:** 다음 마스터 Turn은 Resume으로 같은 대화를 잇고, Plan 상태(Task 상태·Run 상태)를 프롬프트로 전달합니다. Run 성공은 Task 완료가 아니라 `Executed`입니다(§9.4).
- **Epoch:** `bump_epoch`로 제어권이 바뀌면 활성 마스터 Run이 비고, 이전 마스터의 늦은 결정은 거부됩니다.
- **기록:** 처리한 모든 결정 결과를 순서대로 남깁니다(`decision_log`). 메모리 보존입니다.

### 2.3 마스터 권한 (INTEGRATION 연결)

- 운영자 정책은 `role: master` Task를 읽기 전용 Workspace에서만 승인합니다.
- Claude 마스터 Run에는 도구를 주지 않습니다(`--tools ""`). Codex는 읽기 전용 Sandbox, Antigravity는 검토 모드로 실행됩니다.
- 신뢰된 조회는 Run 승인과 같은 규칙(권한 Profile·Provider·모델·Workspace)으로 검사하되 Snapshot을 남기지 않습니다.

## 3. 시험

| 명령 | 결과 | 종류 |
|---|---|---|
| `cargo test -p telos-core --test master_validation` | 8개 통과: 공통 실패 Fixture 전부 `invalid_payload`, 호출자·Plan·Epoch·Revision, 미집행 kind 5종, 권한·한도·ID 중복, 부모·의존성·순환·위임 깊이, 신뢰된 조회 3종 매핑, Run·취소 대상 검사 | Contract |
| `cargo test -p telos-core --test master_controller` | 6개 통과: 3 Turn 왕복(Fresh → Resume, 도구 없는 읽기 전용 마스터 Task, 상태 전달), 잘못된 마스터 출력, 멱등 재생·충돌, 이전 마스터 Run·Epoch 거부, 취소 집행, 응답 해석 엄격성 | Mock |
| `cargo test -p telos-core --test phase0_policy` | 11개 통과(마스터 읽기 전용·도구 없음·신뢰된 조회 포함) | Contract |
| `cargo test -p telos-core --test phase0_master -- --ignored --test-threads=1` | 2개 통과, 60.1초 | 실환경 |
| `cargo test --workspace --locked` | 142개 통과, 30개 무시(실환경) | 전체 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo fmt --all --check` | 통과 | 정적 검사 |

### 3.1 실환경 결과 (Windows 11, Claude Code 2.1.310, 마스터·Worker 모두 `claude-haiku-5-5`)

| 시험 | 결과 |
|---|---|
| `master_delegates_a_task_and_receives_the_result` | Turn 1: 마스터가 Worker Task `task-hello-txt` 생성 → **accepted**(`task_created`). Turn 2(Resume, 같은 세션): `request_run` → **accepted**(`run_requested`). Worker가 `hello.txt` = `hello` 생성, Run `Succeeded`, Task `Executed`. Turn 3(Resume): 마스터가 Plan 상태의 `Succeeded`를 보고 `propose_completion`(요약 "실행 상태는 Succeeded입니다") → Phase 0 범위대로 **rejected**(`unsupported_kind`) 기록 |
| `master_decision_outside_the_grant_is_rejected` | 마스터가 허용되지 않은 `codex` Provider로 Task 생성 → **rejected**(`permission_denied`, `/tasks/0/agent/provider`), Task 미생성 |

**관찰:** 첫 실행에서 마스터가 "Task 틀을 복사하고 `id`·`objective`만 바꾸라"는 지시를 틀 전체를 `objective` 안에 넣는 것으로 오해했고, Core가 `invalid_payload`로 정확히 거부했습니다. 프롬프트를 "바꿀 두 자리(`<NEW_TASK_ID>`, `<OBJECTIVE_AS_PLAIN_TEXT>`)를 표시한 완성형 Task 객체"로 바꾼 뒤 통과했습니다. 마스터 출력은 검증 없이 믿을 수 없다는 전제를 실제로 확인한 사례입니다.

## 4. 미구현·남은 일

| 항목 | 상태 |
|---|---|
| 미집행 kind 5종 | `answer_action`·`request_validation`·`request_rework`·`ask_user`·`propose_completion`은 Payload 검사 후 거부. 검증·재작업·사용자 개입·완료 판정은 P0 |
| Local API | 마스터 경로용 메서드 없음(공통 계약 변경 없음). Extension 연결 시 CONTRACT 변경 요청으로 처리 |
| 영속성 | Plan·결정 기록은 메모리. 재시작 복구·감사 DB는 P0 |
| 정식 정책 | 신뢰된 조회의 Phase 0 구현은 운영자 정책 파일. 경로·명령·Session 소유권·예산 잔액 조회는 미구현 |
| 예산 | Phase 0 정책에 예산 Profile이 없어 `default` 하나만 허용. 마스터 사용량의 Plan 예산 반영(MASTER-05)은 P0 |
| 마스터 교체 | Provider는 설정값이지만 Codex·Antigravity 마스터로는 실행해 보지 않음 |
| Linux | 미검증 |

## 5. 시험 세션 (Archive 대상)

| 세션 ID | 용도 |
|---|---|
| `5912c63a-2d97-4e77-a787-79bacc883dd8` | 1차 실행: 권한 밖 결정(프롬프트 수정 전) |
| `2d799df6-0e01-4dbf-8287-985c3eafc1b6` | 1차 실행: 위임(프롬프트 수정 전, `invalid_payload`) |
| `d0238afa-d17b-4edb-b407-ab1aee56c9d0` | 권한 밖 결정 |
| `e0510965-5d0f-468b-8b04-251a4e83fffa` | 위임 시험 마스터(Turn 1~3) |
| `9fa9bd66-d464-461a-82bc-6151c06eaa87` | 위임 시험 Worker |

# PH0-INTEGRATION 검증 기록

| 항목 | 값 |
|---|---|
| 작업 ID | PH0-INTEGRATION |
| 담당 | `claude-opus-5.5/session-c9336d2c` |
| Branch / Worktree | `main` / `D:\Downloads\Vibe_dev\TelosWeaver` |
| 기준 Commit | `fca1669` |
| G0 기준 Commit | `ae31eb2` |
| 상태 | G0 통과. G1~G3 진행 전 |

## 1. G0 판정

**판정: 통과 (2026-10-24).** Spec §19.5.8의 G0 조건인 배정·공통 인터페이스·시험 경계·최소 Stub 등록을 확정하고 기준 Commit을 기록했습니다.

| 조건 | 근거 |
|---|---|
| 배정 | 배정표 §2. INTEGRATION·CONTRACT 담당 기록, 나머지 7개 작업은 `unassigned` |
| 공통 인터페이스 | 배정표 §5의 7개 계약. 정의 내용은 [PH0-CONTRACT 기록](../PH0-CONTRACT/README.md) §2 |
| 시험 경계 | 공통 시험은 `UnsupportedAdapter` 시험 대역과 Schema·Wire 계약만 검사. Provider별 시험은 각 작업의 `tests/<provider>_*.rs` |
| 최소 Stub 등록 | `crates/adapters/src/runtime/mod.rs`, `crates/core/src/{api,execution,master,policy}/mod.rs`, `crates/rn-client/src/lib.rs`의 계약 재노출 |
| 기준 Commit | `ae31eb2` |

G0 통과는 계약이 정해졌다는 뜻이며 Provider·RN·Core API의 실행 지원을 뜻하지 않습니다.

## 2. 통합 이력

| Commit | 내용 |
|---|---|
| `556e62f` | G0 준비: `[workspace.dependencies]` 일원화, `fixtures` feature, Stub 경로 등록 |
| `cc142cf` | `ph0/contract`(`fd0b206`) 통합 |
| `ae31eb2` | `telos-rn-client`가 `telos_schema::rn`을 재노출하도록 등록 |

통합 시 `ph0/contract`의 변경 파일이 모두 PH0-CONTRACT 독점 경로(및 자기 증거 경로) 안에 있음을 확인했습니다.

## 3. 통합 기준 재검증 (`ae31eb2`)

| 명령 | 결과 |
|---|---|
| `cargo test --workspace --locked` | 통과 (18개) |
| `cargo clippy --workspace --all-targets --locked` | 경고 없음 |
| `cargo fmt --all --check` | 통과 |
| `cd extension && npm ci && npm run build` | TypeScript 42개 생성, `tsc` 통과 |

실행 환경: Windows 10.0.26300 (MINGW64 셸), rustc 1.94.0, cargo 1.94.0, Node 24.14.0, npm 11.9.0. 모두 Contract 수준이며 실환경 시험은 없습니다.

## 4. 한계와 남은 문제

- **자기 검토:** INTEGRATION과 CONTRACT를 같은 담당자가 맡아 G0 판정 주체(INTEGRATION, CONTRACT 확인)가 사실상 한 명입니다. 다른 작업 착수 전에 사용자 또는 다른 에이전트의 계약 검토를 권장합니다.
- **CI 없음:** 원격 저장소가 없어 `.github/workflows`를 두지 않았습니다. 통합 검사는 수동으로 실행합니다.
- **Linux 미검증:** 모든 검사는 Windows에서만 실행했습니다.
- **줄바꿈:** `core.autocrlf=true` 환경에서 새 Worktree가 CRLF로 체크아웃되어, Schema 재생성 시 내용 변경 없이 수정된 파일로 표시됩니다. `.gitattributes`는 Spec §19.5.3의 어느 작업 경로에도 없어 변경 요청 REQ-001로 등록했습니다.
- **미사용 의존성:** `telos-rn-client`의 `serde`·`serde_json`은 재노출 후 현재 쓰이지 않습니다. PH0-RN의 전송 구현에서 쓸 예정이라 유지합니다.

## 5. 다음 단계

- G1: 공용 런타임(HOST)과 대표 Provider Codex의 실제 경로 연결.
- 나머지 7개 작업은 배정 요청을 받아 기록한 뒤 `ae31eb2`를 계약 기준으로 착수합니다.

## 6. G0 개정 1

독립 검토(REQ-002~009)를 반영하여 계약 기준을 `ae31eb2`에서 **`ba97e7b`**로 개정했습니다. G0 통과 판정은 유지합니다. 판정·처리·재검증 내용은 [G0_REVISION_1.md](G0_REVISION_1.md)에 있습니다.

- §4의 "자기 검토" 한계는 독립 검토로 일부 보완되었습니다. 개정 처리 자체는 같은 담당자가 했으므로 검토자의 재확인을 권장합니다.
- 검토 제출 Branch `review/g0-contract-requests-20261024`와 그 Worktree(`D:\worktree\TelosWeaver\g0-review-requests-20261024`)는 제출자 소유라 정리하지 않았습니다.

## 7. G1 대표 실행

> 2026-10-24 · `claude-opus-5.5/session-c9336d2c` · `ph0/integration-g1` · 기준 `75ba56e` · 대표 Provider Claude(배정표 §6)

### 7.1 판정

**통과 (INTEGRATION·대표 Provider CLAUDE 판정, HOST 확인 대기).** Spec §19.5.8 G1 조건 "공용 런타임 + 대표 Provider가 시작→이벤트→완료/실패→취소→결과를 실제 경로로 재현"을 Windows 로컬에서 실제 Claude Code CLI로 확인했습니다. 경로는 Local API(Named Pipe, 토큰 인증) → `ExecutionSupervisor` → 운영자 정책 승인(`LocalRunPolicy`) → 확장 CLI 탐지(`NativeCliDiscoverer`) → `ClaudeCodeAdapter` → `NativeRuntime`(Job Object)입니다. 판정 주체 중 HOST의 확인을 받으면 확정합니다.

### 7.2 연결한 것

| 경로 | 내용 |
|---|---|
| `crates/core/src/integration/mod.rs` | `Phase0Execution`(Daemon 실행 구성), `discover_claude`(§7.5 탐지 요청 구성: 플랫폼·확장 폴더·PATH·명시 경로), `claude_adapter` |
| `crates/core/src/integration/policy.rs` | `LocalPolicyConfig`(운영자 설정, 미정의 필드 거부·경로 정규화·`bypassPermissions` 거부), `LocalRunPolicy`(`RunAuthorizer`: 권한 Profile·Provider·모델·Directory Workspace·읽기 전용·Capsule 검사, Snapshot 기록), `ClaudeLauncher`(승인 Snapshot으로만 실행 조건 구성, 최소 문맥 조립) |
| `crates/core/src/integration/tracked.rs` | `TrackedAdapter`: Daemon 종료 시 활성 Run 취소와 종료 이벤트 확인. 확인 실패는 `unconfirmed`로 보고 |
| `crates/core/src/main.rs` | `serve … [--policy <file>]`. 정책 파일이 없으면 기존처럼 `DenyRuns`. 종료 시 연결을 먼저 닫고 활성 Run 정리, 미확인 Run이 있으면 실패 종료 |
| `crates/core/Cargo.toml` | `sha2`(Policy Revision 해시) |
| `crates/core/src/lib.rs` | `pub mod integration` |
| `crates/core/tests/phase0_policy.rs` | 정책·실행 조건·종료 정리 시험 6개 |
| `crates/core/tests/phase0_g1.rs`, `tests/phase0/client.rs` | G1 실환경 시험 5개, Local API 클라이언트 |

Policy Revision은 `<revision>#<설정 파일 SHA-256 앞 12자>`입니다. Snapshot은 Task·Workspace 경로·읽기 전용·모델·Policy Revision·CLI 탐지 결과를 메모리에 기록하며, 영속 Snapshot 서비스는 P0 범위입니다. 정식 정책은 MASTER(`core/policy`)가 대체합니다.

### 7.3 실환경 결과 (Windows 11, Claude Code 2.1.310, `claude-haiku-5-5`)

`cargo test -p telos-core --test phase0_g1 -- --ignored --test-threads=1` → 5개 통과, 71.4초.

| 시험 | G1 항목 | 결과 |
|---|---|---|
| `g1_completed_run_produces_result_file` | 시작·이벤트·완료·결과 | 같은 멱등 키 재요청은 같은 Run. `session_started` → `tool_started`/`tool_completed`(`Write`, `succeeded`) → `usage` → `turn_finished`(`completed`) → `process_exited`(0, 자식 회수). Workspace에 `result.txt` = `pong`. `runs.inspect`: `Succeeded`, `lastSequence` 7 |
| `g1_provider_failure_is_reported` | 실패 | `error`(`configuration`) → `turn_finished`(`failed`) → `process_exited`(1). `runs.inspect`: `Failed` |
| `g1_unauthorized_request_is_rejected_before_start` | 승인 | 승인되지 않은 Workspace → `forbidden`, 프로세스 미생성 |
| `g1_cancel_reports_actual_termination` | 취소 | `runs.cancel` 접수(`accepted: true`) → `turn_finished`(`cancelled`) → `process_exited`(`killed`, 자식 회수) |
| `g1_daemon_shutdown_cancels_active_runs` | 종료 | 클라이언트 연결을 끊은 2초 뒤에도 Run은 `Running`(연결 종료 ≠ 취소). 서버 종료 후 `Phase0Execution::shutdown` → 취소 1, 미확인 0, 상태 `Cancelled` |

탐지 결과: 출처 `extension`, 위치 `relative-path`, 폴더 결정 `metadata`, CLI `2.1.310 (Claude Code)`.

| 검사 | 결과 |
|---|---|
| `cargo test --workspace --locked` | 91개 통과, 11개 무시(실환경) |
| `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo fmt --all --check` | 통과 |

### 7.4 한계와 남은 일

- **HOST 확인:** G1 판정 주체에 HOST가 포함됩니다(§19.5.8).
- **정상 종료 유예:** 취소는 두 경로 모두 유예(Adapter 5초·시험 2초) 안에 정상 종료하지 않아 강제 종료(`killed`)됐습니다. 자식 회수는 확인했습니다. Windows의 정상 종료 요청 효과는 HOST와 확인합니다.
- **상태 표기:** `runs.inspect`의 `status`가 `Succeeded`·`Failed`·`Cancelled`처럼 PascalCase로 나옵니다. 다른 열거 값(snake_case)과 표기가 다르므로 Spec 원문과 대조가 필요합니다(CONTRACT 확인 항목).
- **결과 판정:** G1은 실행 결과(파일 생성)까지입니다. 출력 계약 검증·Artifact 등록·Task 완료 판정은 P0 범위입니다.
- **Linux·Remote SSH:** WSL 미구성으로 미검증(G3).
- **영속성:** 실행 감독·Snapshot은 메모리입니다.

### 7.5 시험 세션 (Archive 대상)

PH0-CLAUDE 증거 §5와 같은 방식으로 처리합니다. 작업 폴더는 `%TEMP%\telos-phase0-g1\*\workspace`입니다.

| 세션 ID | 시험 |
|---|---|
| `67355b47-e307-4de0-8e0e-9d0e75b33791` | `g1_completed_run_produces_result_file` |
| `34413139-43ee-434f-8df0-aebd54c45c1d` | `g1_provider_failure_is_reported` |
| `1479a52f-205b-4f62-8396-ba8199901b0e` | `g1_cancel_reports_actual_termination` |
| `ebfa12d6-d49e-43c1-a703-9ed8e7d3c416` | `g1_daemon_shutdown_cancels_active_runs` |

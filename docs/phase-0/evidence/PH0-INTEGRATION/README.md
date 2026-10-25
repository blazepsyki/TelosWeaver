# PH0-INTEGRATION 검증 기록

| 항목 | 값 |
|---|---|
| 작업 ID | PH0-INTEGRATION |
| 담당 | `claude-opus-5.5/session-c9336d2c` |
| Branch / Worktree | `main` / `D:\Downloads\Vibe_dev\TelosWeaver` |
| 기준 Commit | `3f050e9` |
| G0 기준 Commit | `9710402` |
| 상태 | G0 통과. G1~G3 진행 전 |

## 1. G0 판정

**판정: 통과 (2026-10-24).** Spec §19.5.8의 G0 조건인 배정·공통 인터페이스·시험 경계·최소 Stub 등록을 확정하고 기준 Commit을 기록했습니다.

| 조건 | 근거 |
|---|---|
| 배정 | 배정표 §2. INTEGRATION·CONTRACT 담당 기록, 나머지 7개 작업은 `unassigned` |
| 공통 인터페이스 | 배정표 §5의 7개 계약. 정의 내용은 [PH0-CONTRACT 기록](../PH0-CONTRACT/README.md) §2 |
| 시험 경계 | 공통 시험은 `UnsupportedAdapter` 시험 대역과 Schema·Wire 계약만 검사. Provider별 시험은 각 작업의 `tests/<provider>_*.rs` |
| 최소 Stub 등록 | `crates/adapters/src/runtime/mod.rs`, `crates/core/src/{api,execution,master,policy}/mod.rs`, `crates/rn-client/src/lib.rs`의 계약 재노출 |
| 기준 Commit | `9710402` |

G0 통과는 계약이 정해졌다는 뜻이며 Provider·RN·Core API의 실행 지원을 뜻하지 않습니다.

## 2. 통합 이력

| Commit | 내용 |
|---|---|
| `2d0179a` | G0 준비: `[workspace.dependencies]` 일원화, `fixtures` feature, Stub 경로 등록 |
| `f1ee458` | `ph0/contract`(`ade440e`) 통합 |
| `9710402` | `telos-rn-client`가 `telos_schema::rn`을 재노출하도록 등록 |

통합 시 `ph0/contract`의 변경 파일이 모두 PH0-CONTRACT 독점 경로(및 자기 증거 경로) 안에 있음을 확인했습니다.

## 3. 통합 기준 재검증 (`9710402`)

| 명령 | 결과 |
|---|---|
| `cargo test --workspace --locked` | 통과 (18개) |
| `cargo clippy --workspace --all-targets --locked` | 경고 없음 |
| `cargo fmt --all --check` | 통과 |
| `cd extension && npm ci && npm run build` | TypeScript 42개 생성, `tsc` 통과 |

실행 환경: Windows 10.0.26300 (MINGW64 셸), rustc 1.94.0, cargo 1.94.0, Node 24.14.0, npm 11.9.0. 모두 Contract 수준이며 실환경 시험은 없습니다.

## 4. 한계와 남은 문제

- **자기 검토:** INTEGRATION과 CONTRACT를 같은 담당자가 맡아 G0 판정 주체(INTEGRATION, CONTRACT 확인)가 사실상 한 명입니다. 다른 작업 착수 전에 사용자 또는 다른 에이전트의 계약 검토를 권장합니다.
- **CI 없음:** G0 당시 원격 저장소가 없어 `.github/workflows`를 두지 않았습니다. 2026-10-24 공개 원격을 만든 뒤에도 사용자 결정으로 CI는 보류 중이며 통합 검사는 수동으로 실행합니다.
- **Linux 미검증:** 모든 검사는 Windows에서만 실행했습니다.
- **줄바꿈:** `core.autocrlf=true` 환경에서 새 Worktree가 CRLF로 체크아웃되어, Schema 재생성 시 내용 변경 없이 수정된 파일로 표시됩니다. `.gitattributes`는 Spec §19.5.3의 어느 작업 경로에도 없어 변경 요청 REQ-001로 등록했습니다.
- **미사용 의존성:** `telos-rn-client`의 `serde`·`serde_json`은 재노출 후 현재 쓰이지 않습니다. PH0-RN의 전송 구현에서 쓸 예정이라 유지합니다.

## 5. 다음 단계 (G0 시점 기록)

> 이 절은 G0 판정 당시의 계획입니다. 이후 대표 Provider는 Claude로 바뀌었고(배정표 §6) G1은 §7에서 판정했습니다. 현재 다음 단계는 §8을 따릅니다.

- G1: 공용 런타임(HOST)과 대표 Provider Codex의 실제 경로 연결.
- 나머지 7개 작업은 배정 요청을 받아 기록한 뒤 `9710402`를 계약 기준으로 착수합니다.

## 6. G0 개정 1

독립 검토(REQ-002~009)를 반영하여 계약 기준을 `9710402`에서 **`077bd78`**로 개정했습니다. G0 통과 판정은 유지합니다. 판정·처리·재검증 내용은 [G0_REVISION_1.md](G0_REVISION_1.md)에 있습니다.

- §4의 "자기 검토" 한계는 독립 검토로 일부 보완되었습니다. 개정 처리 자체는 같은 담당자가 했으므로 검토자의 재확인을 권장합니다.
- 검토 제출 Branch `review/g0-contract-requests-20261024`와 그 Worktree(`D:\worktree\TelosWeaver\g0-review-requests-20261024`)는 제출자 소유라 정리하지 않았습니다.

## 7. G1 대표 실행

> 2026-10-24 · `claude-opus-5.5/session-c9336d2c` · `ph0/integration-g1` · 기준 `0e46f49` · 대표 Provider Claude(배정표 §6)

### 7.1 판정

**통과 확정 (2026-10-24, INTEGRATION·CLAUDE 판정, HOST 확인 완료).** HOST 확인은 [G1_HOST_REVIEW.md](../PH0-HOST/G1_HOST_REVIEW.md)(코드 `4791c5d`, 통합 `b561da2`)이며, 확인 중 발견된 실행 감독의 취소 경합 판정을 보정했습니다. REQ-015에 따라 시험의 자동 판정을 강화하고 실환경 5개를 다시 통과시켰습니다(§7.6). Spec §19.5.8 G1 조건 "공용 런타임 + 대표 Provider가 시작→이벤트→완료/실패→취소→결과를 실제 경로로 재현"을 Windows 로컬에서 실제 Claude Code CLI로 확인했습니다. 경로는 Local API(Named Pipe, 토큰 인증) → `ExecutionSupervisor` → 운영자 정책 승인(`LocalRunPolicy`) → 확장 CLI 탐지(`NativeCliDiscoverer`) → `ClaudeCodeAdapter` → `NativeRuntime`(Job Object)입니다. 판정 주체 중 HOST의 확인을 받으면 확정합니다.

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

- **HOST 확인:** 완료(§7.1).
- **정상 종료 유예:** 취소는 유예 안에 정상 종료하지 않아 강제 종료(`killed`)됐고 자식 회수는 확인했습니다. HOST 확인 결과, Windows 런타임은 별도 정상 종료 신호 없이 stdin을 닫고 유예를 기다린 뒤 Job을 종료하므로 관찰과 일치합니다. 이 결과는 Windows 정상 종료 신호의 증거가 아닙니다.
- **상태 표기:** 확인 완료. `RunStatus`는 계약에서 PascalCase(`Succeeded` 등) 그대로가 Wire 값입니다(`crates/schema/src/task.rs`). 결함이 아니며 시험은 이 계약 타입으로 비교합니다(REQ-015).
- **결과 판정:** G1은 실행 결과(파일 생성)까지입니다. 출력 계약 검증·Artifact 등록·Task 완료 판정은 P0 범위입니다.
- **Linux·Remote SSH:** WSL 미구성으로 미검증(G3).
- **영속성:** 실행 감독·Snapshot은 메모리입니다.

### 7.5 시험 세션 (삭제 완료)

> 2026-10-24: 사용자 지시로 이 목록의 시험 세션을 모두 삭제했습니다. 확장 대화 목록에는 처음부터 보이지 않았습니다(작업 폴더가 임시 폴더라 Workspace별 목록에 나타나지 않음). 앞으로는 병합 시점에 삭제합니다(배정표 §6 "시험 세션 정리").

PH0-CLAUDE 증거 §5와 같은 방식으로 처리합니다. 작업 폴더는 `%TEMP%\telos-phase0-g1\*\workspace`입니다.

| 세션 ID | 시험 |
|---|---|
| `67355b47-e307-4de0-8e0e-9d0e75b33791` | `g1_completed_run_produces_result_file` |
| `34413139-43ee-434f-8df0-aebd54c45c1d` | `g1_provider_failure_is_reported` |
| `1479a52f-205b-4f62-8396-ba8199901b0e` | `g1_cancel_reports_actual_termination` |
| `ebfa12d6-d49e-43c1-a703-9ed8e7d3c416` | `g1_daemon_shutdown_cancels_active_runs` |
| `2028fae9-9c6c-4490-8bfa-8528a4aab0bd` | 재검증: `g1_completed_run_produces_result_file` |
| `8873e3b0-f9cc-42d4-aa89-68f0b54195f1` | 재검증: `g1_provider_failure_is_reported` |
| `541b0fcb-04ba-4d02-9227-ff4697665a43` | 재검증: `g1_cancel_reports_actual_termination` |
| `9541daf9-7084-477b-be47-bf1215d057d0` | 재검증: `g1_daemon_shutdown_cancels_active_runs` |

### 7.6 REQ-015 시험 강화와 재검증

> 2026-10-24 · 기준 `b561da2`(HOST 취소 경합 보정 포함)

HOST가 지적한 대로 기존 시험은 미종료·취소 미재현도 통과시킬 수 있었습니다. `crates/core/tests/phase0_g1.rs`를 다음처럼 고쳤습니다.

- 최종 상태를 계약 타입 `RunStatus`의 Wire 값(`json!(RunStatus::Succeeded)` 등)과 정확히 비교합니다. 마지막 이벤트와 상태 갱신의 경합은 10초 제한 안에서 `runs.inspect`를 다시 조회해 처리합니다.
- 완료 `Succeeded`, 실패 `Failed`(둘 다 `lastSequence` = 받은 이벤트 수).
- 취소: `accepted: true`, `turn_finished`가 반드시 `cancelled`, 종료 원인 `terminated_on_request`/`killed`, `childrenReaped: true`, 최종 `Cancelled`.
- Daemon 종료: 연결 종료 뒤 `Running`, `shutdown` 취소 1·미확인 0, 최종 `Cancelled`.
- 취소·종료 시험이 진행 중인 Run을 확실히 잡도록 출력 작업을 1~1500으로 늘렸습니다.

| 검사 | 결과 |
|---|---|
| `cargo test -p telos-core --test phase0_g1 -- --ignored --test-threads=1` (haiku) | 5개 통과, 65.7초. 완료 `Succeeded`(7), 실패 `Failed`(6), 취소 `accepted` → `cancelled` → `killed`·회수 → `Cancelled`, 종료 `Running` → 취소 1·미확인 0 → `Cancelled` |
| `cargo test --workspace --locked` | 123개 통과, 24개 무시(실환경) |
| `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo fmt --all --check` | 통과 |

## 8. 현재 다음 단계: G2 구성 통합

> 2026-10-24 갱신 · 기준 `main` `e55c0b2`

### 8.1 현황

| 구성 | Adapter 구현 | Core 연결 (`telos_core::integration`) |
|---|---|---|
| Claude | integrated (`3bdb808`) | **연결됨** (`cfe1b19`, G1 판정) |
| Codex | integrated (`64f9674`) | **연결됨** (§8.4) |
| Antigravity | integrated (`286844a`) | **연결됨** (§8.4) |
| MASTER·RN·EXT | 미배정 | — |

### 8.2 Codex·Agy Core 연결 계획 (INTEGRATION)

Claude 연결(§7.2)과 같은 구조로 넓힙니다. 정책·탐지·종료 정리의 공통 경로를 Provider별로 나누어 씁니다.

1. **운영자 정책:** `LocalPolicyConfig`에 `codex`·`agy` 절을 추가하고, `LocalRunPolicy`가 Task의 Provider에 맞는 절로 모델·권한을 검사합니다. Snapshot에는 그 Provider의 탐지 결과를 기록합니다.
   - Codex: 허용 모델·`effort`, Sandbox는 Workspace 권한에 맞춤(`writable: false` → `read-only`), 승인은 기본 `Deny`, 실행 제한 시간.
   - Agy: 허용 모델·`--effort`·`--mode`, 데이터 디렉터리는 기본 확장 디렉터리(Spec §7.5 확정 규칙).
2. **실행 조건 Port:** `CodexLauncher`(`CodexLaunchResolver`, `authorize_approval`은 Phase 0에서 거부 유지), `AgyLauncher`(`AgyLaunchResolver`)를 `ClaudeLauncher`처럼 승인 Snapshot으로만 구성합니다.
3. **탐지와 구성:** `codex::cli_rule`·`agy::cli_rule`로 탐지하고 결과로 `CodexConfig`·`AgyConfig`를 만듭니다. Agy는 등록 전에 `verify_extension_data_dir`로 `--app_data_dir` 수용을 확인하고, 거부되면 다른 디렉터리로 바꾸지 않고 `unsupported`로 둡니다.
4. **종료 정리:** 세 Adapter 모두 `TrackedAdapter`로 감싸 Daemon 종료 시 활성 Run을 정리합니다.
5. **G2 실환경 시험** (`crates/core/tests/phase0_g2.rs`, `#[ignore]`): 한 Daemon에 세 Provider를 등록하고 Local API로 Provider별 완료·실패·취소를 실행합니다. 경량 모델(배정표 §6)과 시험 세션 정리 규칙(Claude Archive, Codex 소유 Thread Archive, Agy 시험 세션 정리)을 따릅니다.

### 8.3 G2의 나머지 조건

- MASTER(마스터 결정 검증)·RN(RPC)·EXT(Extension 연결)는 미배정입니다. G2는 이 연결 증거까지 필요합니다(Spec §19.5.8).
- HOST: G1 확인과 REQ-013 ③(`targetPlatform: "undefined"`)을 완료했습니다(통합 `b561da2`). 남은 HOST 일은 Linux·Remote SSH 검증입니다.

### 8.4 Codex·Agy Core 연결 결과

> 2026-10-24 · `ph0/integration-g2` · 기준 `1695844`

§8.2 계획대로 연결했습니다.

| 경로 | 변경 |
|---|---|
| `integration/policy.rs` | `CodexPolicy`(모델·`effort`·`runTimeoutSecs`), `AgyPolicy`(모델·`effort`·`mode`·`dataDir`), Provider별 모델 검사, Provider ID별 탐지 결과를 Snapshot에 기록, 승인 Snapshot의 Provider 일치 검사(`authorized`). `CodexLauncher`(Sandbox: 읽기 전용 Run → `read-only`, 쓰기 → `workspace-write`, 승인 `Deny`), `AgyLauncher`(읽기 전용 Run은 CLI 기본 검토 모드, `--add-dir <Workspace>`, `--disable-slash-commands`). Workspace 경로를 `\?\` 없는 일반 형식으로 정규화(`plain_path`) |
| `integration/mod.rs` | 공통 `discover(provider, cli_rule, 명시 경로)`, `Phase0Execution`이 정책에 있는 세 Provider를 탐지·등록. Agy는 등록 전 `verify_extension_data_dir`로 `--app_data_dir` 수용 확인(거부 시 `unsupported`로 등록하고 `notes`에 기록). 등록하지 못한 Provider 사유를 `notes`로 보고. Codex 소유 Thread Archive용 핸들 보관 |
| `main.rs` | Provider별 탐지 결과와 `notes` 출력 |
| `tests/phase0_policy.rs` | Codex·Agy 정책·실행 조건·Provider 교차 사용 거부·잘못된 설정 시험 4개 추가(총 10개) |
| `tests/phase0_g2.rs` | G2 실환경 시험 4개 |
| `tests/phase0/client.rs` | 구독 ID로 알림 거르기, Run 도중 구독이 닫히면 `lastSequence`부터 재구독 |

**실환경 결과** (Windows 11, Claude 2.1.310 `haiku`, Codex 0.164.0-alpha.11.1 `gpt-6-luna`/low, Antigravity 1.4.7 `gemini-3.8-flash-low`/low, 한 Daemon 구성):

| 시험 | 결과 |
|---|---|
| `g2_each_provider_completes_and_writes_a_result_file` | 세 Provider 모두 탐지(`notes` 없음) → 각자 Workspace에 `result-<provider>.txt` = `pong` 생성 → `turn_finished`(`completed`) → `process_exited`(자식 회수) → `Succeeded` |
| `g2_codex_and_agy_provider_failures_are_reported` | 없는 모델: Codex `error`(`configuration`) → `failed` → `Failed`(프로세스 exit 0이어도 실패), Agy `error`(`configuration`) → `failed` → `Failed` |
| `g2_codex_and_agy_cancel_reports_actual_termination` | 두 Provider 모두 `accepted` → `turn_finished`(`cancelled`) → `process_exited`(`killed`, 자식 회수) → `Cancelled` |
| `g2_daemon_shutdown_cancels_runs_of_every_provider` | 연결 종료 뒤 Codex·Agy 모두 `Running` → Daemon 종료 시 취소 2·미확인 0 → 모두 `Cancelled` |

같은 최종 코드로 G1 5개도 다시 통과했습니다(경로 정규화가 Claude 실행에도 적용되므로). Workspace 시험 127개 통과, 28개 무시(실환경), clippy `-D warnings`·fmt 통과.

**발견과 조치:**

1. **Antigravity가 Workspace 밖(`~/.gemini/antigravity/scratch/`)에 썼습니다.** "쓰기 성공"으로 보고했지만 파일은 Workspace에 없었습니다. 원인은 Windows `canonicalize`가 만든 확장 길이 경로(`\?\C:\...`)를 Antigravity가 작업 영역으로 인식하지 못한 것입니다. Workspace 경로를 일반 형식으로 정규화하고 `--add-dir`로 작업 영역을 명시해 해결했고, 이후 첫 시도에 Workspace 안에 썼습니다. 시험이 `scratch`에 만든 파일 2개는 지웠습니다. **AGY 담당 참고:** AGY 시험은 파일 쓰기 위치를 검사하지 않아 이 문제를 잡지 못했습니다. HOST 참고: 탐지 결과 경로도 같은 `\?\` 형식입니다.
2. **느린 구독자 종료:** Codex는 토큰 단위로 이벤트를 매우 빠르게 보내, 한 실행에서 Run이 끝나기 전 구독이 닫혔습니다(연결별 송신 대기열 8개). 계약대로 `lastSequence`부터 재구독하도록 시험 클라이언트를 고쳤습니다. **EXT 담당 참고:** Extension도 같은 재구독 처리가 필요합니다. HOST 참고: Codex 스트림에 비해 대기열이 작을 수 있습니다.
3. **Antigravity 대화 파일 정리:** agy 1.4.7은 대화를 `conversations/<id>.db`(강제 종료 시 `-wal`·`-shm` 잔존)로 저장합니다. 정리 코드가 `<id>.pb`만 지우고 있어 고쳤습니다. **AGY 담당 참고:** `agy_environment.rs`의 `cleanup_session`도 같은 문제가 있습니다.

### 8.5 시험 세션 정리 현황

> 2026-10-24 최종: Claude 세션 폴더 18개(33개)를 삭제했고, Antigravity `e386a213`은 요약 DB에서 CLI 전용 프로젝트(`default-cli-project`)로 확인되어 삭제했습니다. Codex 시험 Thread 22개(G2 14개, PH0-CODEX 8개)는 모두 Archive 상태이며, 아래에서 Archive되지 않았다고 적은 Thread도 확인 결과 2개(`01a16d89-0038…`, `01a16d8a-bd44…`)만 남아 있어 App Server `thread/archive`로 처리했습니다. Codex는 Thread 목록을 내부 SQLite에도 두므로 파일 삭제 대신 공식 Archive로 정리합니다.

- **Antigravity:** 시험 세션 6개(`898cc499`·`0d96778a`·`ba7bdfbf`·`69c76b8d`·`79303ad7`·`8a99e97b`)의 `brain/<id>`와 `conversations/<id>.db(-wal/-shm)`를 지웠습니다. `e386a213-d36d-4557-b06c-4be82ad0029f`(19:51 생성)는 첫 실행 시간대지만 내용으로 시험 세션임을 확인하지 못해 남겼습니다. `conversation_summaries.db`의 요약 기록은 남습니다.
- **Codex:** 정상 종료한 시험의 Thread는 Adapter가 Archive했습니다. 어설션 실패로 정리 전에 끝난 실행의 Thread는 Archive되지 않았습니다. Adapter는 자기가 만든 Thread만 Archive하므로 새 프로세스에서 처리할 수 없습니다: `01a16d89-0038-76f0-9808-394029b8b859`, `01a16d89-a725-7fa2-87a5-db7aa1e0b16d`, `01a16d8d-d8dd-7cd2-a989-fca583d11bb3`, `01a16d93-e562-7fa0-9027-567f49234a68`, 첫 실행 완료 시험의 Thread 1개(ID 미수집). 이후 완료 시험은 판정 전에 정리하도록 고쳤습니다.
- **Claude (삭제 완료):** G2 `ec6e79e0-ede3-43b7-8ca3-7054cb1efd8a`, `3ff7f760-79e6-4353-b913-4d6f936387ce`, `741d5dc4-852d-4224-8247-7e35864fb167`, `6f011265-1c46-4e1d-996a-d78874a978d1`, `d1e1169c-db12-485f-a358-2b43970cc55f`. G1 재검증 `cac0203e-5074-4aea-abe0-dd6bc4502940`, `2c8a9a43-6840-4edf-a063-3449a4aaba83`, `88c6b64a-9c99-4a87-827c-5d96c21d85a4`, `194594cb-364c-4498-9e70-3f4e715c5e13`.

### 8.6 G2 남은 조건

세 Provider의 Core 연결은 끝났습니다. G2 판정에는 MASTER(마스터 결정 검증)·RN(RPC)·EXT(Extension 연결) 증거가 더 필요하며 세 작업은 미배정입니다. (G2 진행 중 기록)

현황(2026-10-25): MASTER는 `e14de42`, EXT는 `43ce639`로 통합했습니다. G2에 남은 조건은 RN(PH0-RN-05~08, Local RPC·Client·Core 연결)입니다.

### 8.7 PH0-EXT 검토·통합 (`43ce639`)

`ph0/ext`(`a5c5397`, 코드 `4ad8dcf`, 인계 `7118a69`)를 검토해 통합했습니다. 변경은 소유 경로인 `extension/**`와 `docs/phase-0/evidence/PH0-EXT/`뿐이며 Rust·Cargo·공통 Schema·RN은 바뀌지 않았습니다. 배정표 커밋 `c3f246d`·`ef25b1d`는 담당 에이전트가 `main`에 직접 기록했습니다(§1의 담당자 갱신 규칙 범위).

**계약 대조**

| 항목 | 결과 |
|---|---|
| Project Key·경로 정규화 | `locations.ts`가 `telos_schema::api::normalize_project_root`·`project_key`와 같은 규칙(`\\?\UNC\` → `//`, `\\?\` 제거, 소문자, 끝 `/` 제거, SHA-256 앞 16자) |
| Pipe·Socket·토큰 위치 | Core `server.rs`·Schema 문서와 같음(`\\.\pipe\telos-weaver-<key>`, `XDG_RUNTIME_DIR`/`/tmp/telos-weaver-<uid>`, `%LOCALAPPDATA%\telos-weaver\<key>\api-token`). Unix에서 Core는 `geteuid`·`HOME`, Extension은 `getuid`·`os.homedir()`을 써서 setuid 같은 특수 환경에서만 다를 수 있음 |
| 토큰 | Core가 쓰는 64자 소문자 hex와 같은 형식·길이 검사, 내용은 오류에 노출하지 않음 |
| Core 실행 | `serve --project-root --project-id --host-id`만 사용(정책·shell 없음), `--version` 출력 `telos-weaver x.y.z` 검사, `detached`·`unref`로 UI와 독립 |
| 프로토콜 | API `0.2`, 프레임 1 MiB, UTF-8 엄격 디코딩, 요청 ID 상관·시간 초과·대기 32개 상한 |
| 설정 보안 | `corePath`·`hostId`는 `machine` 범위라 Workspace 설정이 덮어쓸 수 없음. Untrusted Workspace에서는 세 설정 모두 제한되고 연결 명령이 비활성 |

**재검증 (Windows 11, `ph0/ext`와 병합본 `main` 각각)**

| 명령 | 결과 |
|---|---|
| `npm ci --ignore-scripts` | 통과, 취약점 0 |
| `npm test` | 18개 통과 |
| `npm run test:core` (Worktree·`main`에서 각각 빌드한 Core) | 1개 통과(인증·Degraded·DenyRuns 거부·연결 해제 후 생존·PID 재사용) |
| `npm run test:host` (VS Code 1.143.1, 격리 프로필) | 통과, 확인 항목 5개, `coreSurvivedHostExit=true`, 결과 `%TEMP%\telos-ext-host-GUnhJj` |
| Rust Workspace `cargo test --workspace` (`main`) | 142개 통과 |
| 시험 후 프로세스 | `telos-weaver` 프로세스 없음. Provider·모델 호출 없음 |

**비차단 발견 → REQ-016** (배정표 §4)

1. `CoreConnection.failures`가 재연결 성공 후에도 0으로 돌아가지 않아, 한 세션에서 Core가 다섯 번 재시작되면 이후 자동 재연결이 멈춥니다(`connect()`에서만 초기화).
2. Heartbeat(5초)마다 `connected` 상태가 다시 발생해 Output에 "연결됨"이 반복 기록됩니다(256 KiB 상한으로 잘리지만 Run 출력이 밀려남).
3. `RunFeed`가 구독 응답을 기다리는 동안 다른 구독의 알림까지 대기열(64개 상한)에 넣어, 여러 Run을 동시에 관찰하게 되면 다른 구독 때문에 `limit`으로 끊길 수 있습니다(현재 UI는 한 번에 한 Run).

연결·재연결 중복 코드(`connect`와 `scheduleReconnect`의 client 연결 처리)도 1번을 고칠 때 함께 정리하기를 권합니다. 판정: **통합**. Linux·Remote SSH 실행 검증과 전체 UI는 기존대로 후속입니다.

### 8.8 REQ-016 검토·통합 (`2331c7c`)

`ph0/ext-req016`(`44a4df3`, Codex `PH0-EXT-REQ016-01a17034`)를 검토했습니다. 기준 `e899789` 이후 `main`의 PH0-RN-05 변경과 경로가 겹치지 않고, 변경은 EXT 소유 경로(`extension/**`, `evidence/PH0-EXT/REQ-016.md`)뿐입니다. Rust·공통 계약·Schema 변경은 없습니다.

| 요청 항목 | 확인 |
|---|---|
| ① 재연결 횟수 | `connect`·`scheduleReconnect`의 연결 처리를 `useClient`로 합쳤습니다. 인증과 첫 Health(Degraded 포함)가 성공한 뒤에만 `failures = 0`입니다. Health가 실패하면 초기화하지 않아 연속 5회 상한이 유지됩니다. `closed` 처리는 generation과 client 소유권을 함께 보므로, 이전 연결이 늦게 닫혀도 새 연결을 지우지 않습니다. Health 시간 초과는 클라이언트가 `timeout`으로 닫혀 재연결을 예약하고, 같은 시도의 `catch`가 다시 예약하려 해도 `this.retry` 검사가 막습니다 |
| ② Heartbeat 출력 | `phaseName`이 바뀔 때만 Output에 기록하고, 화면은 Health 응답마다 갱신합니다 |
| ③ 구독 분배 | `request`의 응답 처리 함수가 `pending.resolve` 안에서 동기로 실행됩니다. `receive`는 한 데이터 묶음의 줄을 순서대로 처리하므로, 다음 알림을 해석하기 전에 구독 ID가 연결됩니다. 처리 함수의 예외는 그 요청만 거부합니다(전송 계층으로 번지지 않음). 64개 임시 대기열은 없앴습니다. 이 방식은 Core가 구독 응답을 송신 대기열에 먼저 넣고 재생 작업을 시작하는 순서(`server.rs`의 `RunsSubscribe`)에 기대며, 그 순서는 HOST 시험 `api_replays_finished_run_and_unsubscribes_live_run`이 이미 검사합니다 |

**재검증 (Windows 11, Node v24.14.0, VS Code 1.143.1)**

| 명령 | 결과 |
|---|---|
| 수정 전 소스(`e899789`) + 새 시험 3개(`six separate outages`·`foreign traffic while opening`·`repeated Health refreshes`) | 3개 모두 실패(제출 기록과 같음). 임시 Worktree는 삭제 |
| `npm ci --ignore-scripts`, `npm test` (Worktree·병합본 `main` 각각) | 취약점 0, 28개 통과 |
| `node --test test/live-core.cjs` (`main` 빌드 Core) | 1개 통과 (Worktree·`main` 각각) |
| `node scripts/test-host.cjs` (격리 프로필) | 통과, 5항목, `coreSurvivedHostExit=true` (Worktree `%TEMP%\telos-ext-host-5ubupF`, `main` `%TEMP%\telos-ext-host-WfYiAh`) |
| 시험 후 프로세스 | `telos-weaver` 없음. Provider·모델 호출 없음 |

**비차단 발견 → REQ-017**: 공통 계약 `telos_schema::api` 모듈 문서에는 "구독 이벤트는 응답과 섞여 도착할 수 있습니다"라고만 적혀 있습니다. **한 구독의 알림은 그 구독의 응답 뒤에만 온다**는 보장은 Core 구현과 HOST 시험에만 있습니다. Extension이 이제 이 순서에 기대므로 계약 문서에 이 보장을 명시해야 합니다(문서만 바뀌고 JSON Schema 변화 없음). 판정: **통합**, REQ-016 완료.

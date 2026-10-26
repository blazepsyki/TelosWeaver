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

### 8.9 RN Core 연결과 G2 판정

PH0-RN-07([기록](../PH0-RN/RN-07/README.md))로 G2의 남은 조건인 RN 연결 증거가 갖춰졌습니다. 이 Branch에는 INTEGRATION 몫(Core 연결 `integration/rn.rs`·`start_rn`·`main.rs`, REQ-019)이 함께 들어 있습니다.

| G2 조건 (Spec §19.5.6) | 증거 (Windows 11 실환경) |
|---|---|
| 세 Provider | §8.4: 한 Daemon에서 Claude·Codex·Antigravity 완료·결과 파일·실패·취소·Daemon 종료 정리 (`aef3297`) |
| 마스터 검증 | `evidence/PH0-MASTER/README.md`: 실제 Claude 마스터의 결정 회수·검증·거부·집행·결과 왕복 (`e14de42`) |
| RN RPC | RN-06: 실제 RN의 RPC(인증·탐색·전송·구독·재동기화)와 실제 Claude 세션 왕복 (`cfba833`). RN-07: Core가 RN을 띄워 Client로 메시지 왕복, 강제 종료 뒤 재시작, RN 장애 중 실제 Claude Managed Run의 추적·취소 독립성, `core.health`의 `rn` |
| Extension 연결 | §8.7·§8.8: 실제 Core와 격리 Extension Host, REQ-016 (`43ce639`, `2331c7c`) |
| 공통 Contract·생성 시험 | 계약 기준 `7497877`(REQ-017·018 포함), `json_schemas` 포함 Schema 시험, Workspace 152개, Extension 28개 |

판정: **통과 판정 (각 담당 확인 대기)**. Spec은 G2를 "INTEGRATION, 각 담당 확인"으로 정합니다. CODEX·HOST·EXT(Codex), AGY(Gemini)의 확인을 요청합니다. HOST에는 REQ-019 사후 확인도 함께 요청합니다. Linux·Remote SSH는 G3 조건이며 이 판정에 포함하지 않습니다.

**담당 확인 (2026-10-25):** HOST·CODEX·EXT([HOST](../PH0-HOST/G2_HOST_REVIEW.md)·[CODEX](../PH0-CODEX/G2_REVIEW.md)·[EXT](../PH0-EXT/G2_REVIEW.md), `9f552b8`, 통합 `e8380ae`)와 AGY(배정표 PH0-AGY 행, `2fff559`)가 모두 동의했습니다. HOST는 REQ-019 사후 확인을 마치고 시험 3개를 더했으며, 실제 RN 왕복·재시작을 재현했습니다. INTEGRATION은 병합본에서 Workspace 155개·엄격 Clippy·fmt·Extension 29개를 재확인했습니다. G2 **통과 확정**. 남은 조건(Linux·Remote SSH, RN-08 수용)은 G3에 속합니다.

## 9. G3 준비: Linux Host 공통 검사

### 9.1 REQ-020 통합 (`836fb11`)

Rust 1.99.1(사용자가 Windows·Linux를 맞춤)에서 deprecated된 `AtomicUsize::fetch_update` 때문에 엄격 Clippy가 실패하던 것을 PH0-HOST(Codex)가 `try_update`로 바꿨습니다(`85eadb9`, [HOST 기록](../PH0-HOST/REQ-020.md)). 한 줄이고 인자·반환·동작이 같다는 근거를 현재 Toolchain의 표준 라이브러리 소스로 확인한 기록을 검토했습니다. 병합본에서 Windows 재검사: 엄격 Clippy·fmt 통과, 시험 155개, RN Fork 빌드, Extension 29개.

### 9.2 `psyki-nas` 공통 검사 ([절차](../../LINUX_REMOTE_HOST.md) §4.1·§4.2)

| 항목 | 값 |
|---|---|
| Host | `psyki-nas` (Ubuntu 26.04.1 LTS, 커널 7.0.0-38-generic, x86_64) |
| Toolchain | rustc·cargo 1.99.1, clippy 0.1.99, rustfmt 1.10.0, Node v24.22.1, npm 11.19.1 (Windows와 동일) |
| 저장소 | `~/TelosWeaver`, GitHub에서 Clone, `836fb11`, `core.autocrlf=false`, Git 이메일은 전역 noreply |

| 명령 | 결과 |
|---|---|
| `cargo test --workspace --locked` | 152개 통과, 실환경 32개 기본 제외 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 통과 |
| `cargo fmt --all --check` | 통과 |
| `rn-fork`: `cargo build --locked`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo fmt --check` | 모두 통과. **RN Fork의 첫 Linux 빌드**이며, RN-06에서 고친 Unix 분기(`ipc.rs`)·`private.rs`의 Unix 경로가 컴파일됩니다. `pkg-config` 크레이트는 빌드됐지만 시스템 `pkg-config` 실행 파일은 필요하지 않았습니다 |
| `rn-fork`: 격리 `cargo test --locked`(`HOME`·`XDG_*`·`TELOS_RN_HOME`을 임시 폴더로) | 212개 통과 |
| `extension`: `npm ci`, `npm test`(타입 생성·빌드·시험) | 29개 통과 |
| `git status` | 깨끗함 |

시험 개수 차이(Windows 155 / Linux 152)는 플랫폼 전용 시험입니다. Windows 전용 4개(`token_acl_contains_only_current_user`, `pipe_acl_contains_only_current_user`, `non_utf8_windows_path_is_rejected_before_execution`, `windows_eof_exit_and_job_kill_report_actual_child_termination`), Linux 전용 1개(`external_signal_is_not_reported_as_a_natural_exit`). RN Fork(Windows 204 / Linux 212)도 플랫폼별 시험 구성이 다릅니다.

Provider·모델 호출은 없었습니다. 다음은 작업별 실환경 시험(절차 §4.3: HOST 런타임·확장 탐지·세 Provider·RN Core 연결·Extension Remote-SSH)과 Remote SSH 시나리오(§4.4)입니다.

### 9.3 G3 Linux 배정 작업 통합 (`b99ff29`)

사용자 지시로 INTEGRATION이 각 에이전트의 CLI를 직접 실행해 [작업 지시](G3_LINUX_ASSIGNMENTS.md) §2~§5를 맡겼습니다(Codex CLI 0.164.0-alpha.11.1 `codex exec` ×3, Antigravity CLI 1.4.7 `agy -p` ×1, 전체 권한 + 지시문의 범위 제한). 네 Branch는 모두 `94e1369` 기준이며 자기 경로만 바꿨습니다.

| 작업 | Branch·커밋 | 결과 | 증거 |
|---|---|---|---|
| PH0-HOST | `ph0/host-linux` `fe1b49d`·`ba8df90`·`4df2769` | H1 Process Group SIGKILL 시험 추가·`runtime_native` 통과, H2 실제 `telos-weaver serve`의 XDG Socket·0700/0600·인증 거부·`rn` degraded·DenyRuns·Socket 정리(`host_linux.rs`), H3 두 벌 설치에서 활성 Claude 2.1.310·Codex 26.1021.22027·`agy` 탐지(`discovery_environment.rs`). 다른 사용자 접근·`XDG_RUNTIME_DIR` 대체 경로는 미실행 | [HOST](../PH0-HOST/LINUX.md) |
| PH0-CODEX | `ph0/codex-linux` `d393275`·`d2056b1`·`ac43487` | 실환경 7개 통과(`gpt-6-luna`, effort low), 활성 CLI 0.164.0-alpha.11.1, Thread 7개 Archive 확인, 과거 Windows 누락 Thread 5개도 Archive 상태. 시험 코드만 수정(Host ID·플랫폼 문자열을 환경에 맞춤, Windows 기본값 유지) | [CODEX](../PH0-CODEX/LINUX.md) |
| PH0-EXT | `ph0/ext-linux` `86da91d`·`e371c29` | E1 Linux Core 실제 연결(`live-core.cjs`, Socket·토큰 0600, 디렉터리 0700, 잘못된 토큰 거부) 통과. **E2 Remote-SSH Extension Host는 사용자 VS Code 조작이 필요해 미실행**(단계별 절차 기록) | [EXT](../PH0-EXT/LINUX.md) |
| PH0-AGY | `ph0/agy-linux` `aa347cc`·`5374729` | 실환경 7개 통과(`gemini-3.8-flash-low`, effort low). Adapter 수정 2건: SSH 미로그인 메시지의 `Authentication` 분류, Linux SIGTERM 직전 `result(ERROR)`를 취소로 정규화. 시험 대화 7개 즉시 삭제 | [AGY](../PH0-AGY/LINUX.md) |

**검토:** AGY의 취소 정규화는 result 줄에서 `TurnFinished(Cancelled)`를 내고 종료 경로가 `mapper.turn_finished()`로 중복을 막는 것을 확인했습니다. 인증 분류는 시간 초과 분류보다 먼저 검사됩니다. 새 Linux 시험 파일 두 개는 `#![cfg(target_os = "linux")]`라 Windows 빌드에서 빠집니다.

| 병합본 검사 | Windows | `psyki-nas` |
|---|---|---|
| `cargo test --workspace --locked` | 155개 통과, 33개 제외 | 153개 통과, 35개 제외 |
| 엄격 Clippy·fmt | 통과 | 통과 |
| Extension `npm test` | 29개 | 29개 |
| `live-core.cjs`(그 Host의 Core) | 1개 | 1개 |
| `host_linux`(실제 Daemon, 모델 호출 없음) | — | 통과 |
| `discovery_environment` | — | **전제 조건 실패**(아래) |

**환경 변화:** 2026-10-25 22:18:55에 서버 VS Code Server가 이전 확장(Claude 2.1.309·Codex 26.1007.21434)을 지워 확장이 한 벌씩만 남았습니다(`.obsolete` 갱신 시각). 그래서 `discovery_environment`의 "두 벌 설치" 단언이 실패합니다. HOST의 통과 기록은 두 벌이 있던 실행 시점의 것이며 설치 목록이 증거에 남아 있습니다. 그 단언만 임시로 뺀 재실행은 통과했습니다(활성 버전·실행 파일·`agy` 위치 모두 일치, 커밋하지 않음).

**후속 요청:**
1. PH0-HOST: `discovery_environment`의 두 벌 설치를 실패가 아닌 전제 조건 보고로 바꿈(한 벌이면 활성 선택 규칙의 다중 버전 경로가 검증되지 않았음을 출력).
2. PH0-AGY: 취소 정규화(`run.rs`)의 Mock 시험 추가(현재 실환경으로만 확인).
3. PH0-AGY 인계: Linux Remote SSH에서 `agy`는 `<app_data_dir>/antigravity-oauth-token` 파일 토큰을 쓰는데 확장 공유 폴더(`~/.gemini/antigravity`)에는 이 파일이 없습니다. `phase0_g2`의 Linux 재실행은 기본 CLI 폴더(`~/.gemini/antigravity-cli`)로 하거나 사용자가 `--app_data_dir=antigravity`로 한 번 로그인해야 합니다(AGY-DATA-02).

**시험 세션:** Codex Thread 7개는 Archive 완료([CODEX](../PH0-CODEX/LINUX.md)에 ID), Antigravity 대화 7개는 AGY가 즉시 삭제했고 빈 폴더를 확인([AGY](../PH0-AGY/LINUX.md)에 ID), HOST·EXT는 Provider 세션 없음. 병합 시 서버 임시 폴더(`/tmp/telos-codex-live-*`, `/tmp/telos-codex-linux-audit-*`)를 지웁니다.

### 9.4 INTEGRATION 몫 Linux 재실행 (`psyki-nas`, 2026-10-26)

[작업 지시](G3_LINUX_ASSIGNMENTS.md) §6의 실환경 재실행(사용자가 대기를 해제). Branch `ph0/integration-linux`, 서버 Worktree `~/TelosWeaver-wt/integration-linux`(`91f0789`).

| 항목 | 값 |
|---|---|
| Host | `psyki-nas` (Ubuntu 26.04.1 LTS, 커널 7.0.0-38-generic), rustc 1.99.1, Node v24.22.1 |
| CLI | Claude 2.1.310(`~/.vscode-server/extensions/anthropic.claude-code-2.1.310-linux-x64/.../claude`, `metadata` 탐지), Codex 0.164.0-alpha.11.1(확장 26.1021.22027), Antigravity `~/.gemini/bin/agy` 1.4.7(확장 1.9.0) |
| 모델 | Windows와 같음: Claude `haiku`, Codex `gpt-6-luna`, Antigravity `gemini-3.8-flash-low`, effort low |
| 환경 변수 | `TELOS_HOST_ID=psyki-nas`, `TELOS_CLAUDE_CLI`, `TELOS_RN_EXECUTABLE`(서버에서 빌드한 `rn-fork/target/debug/telos-rn`), `TELOS_AGY_VSCODE_LISTING=hidden` |
| Antigravity | 사용자 결정(2026-10-26): 이미 로그인된 CLI 기본 디렉터리(`~/.gemini/antigravity-cli`) 사용 → Spec v1.5.5 §7.5 "Host별 차이". G2 정책에 `agy.vscodeListing: hidden`을 명시 |

**시험 코드 보정(`91f0789`):** `phase0_g2`가 Antigravity 표시 모드를 `TELOS_AGY_VSCODE_LISTING`(기본 `listed`)으로 받고 목록 기록 검사도 그 값으로 판정(`hidden`이면 대화가 CLI 디렉터리에만 있어야 함). `claude_environment`의 탐지 시험 Host ID를 `TELOS_HOST_ID`로 받음(기본 `windows-local`). Windows 기본 동작은 그대로입니다.

| 묶음 | 명령(`-- --ignored --test-threads=1 --nocapture`) | 결과 | 확인 |
|---|---|---|---|
| PH0-CLAUDE | `cargo test -p telos-adapters --test claude_environment` | **7개 통과**, 60.2초 | 탐지(Remote SSH 확장, `metadata`)·완료·도구 거부·잘못된 모델·취소(취소→종료 817ms, Process Group)·Resume/Fork·목록 표시(`listed`·`hidden`) |
| G1 | `cargo test -p telos-core --test phase0_g1` | **5개 통과**, 136.4초 | 완료·결과 파일, Provider 실패, 실제 종료 보고 취소, 시작 전 미승인 거부, Daemon 종료 시 Run 취소 |
| 마스터 | `cargo test -p telos-core --test phase0_master` | **2개 통과**, 72.4초 | 위임·결과 수신, 허가 밖 결정(Codex 작업) 거부 |
| PH0-RN | `cargo test -p telos-core --test phase0_rn` | **2개 통과**, 33.9초 | Core가 시작한 실제 `telos-rn`(RPC 0.1)과 Client 왕복·강제 종료 뒤 재시작·정지, RN 정지 중 `core.health`의 `rn` degraded와 실제 Claude Run 진행·취소 |
| G2 | `cargo test -p telos-core --test phase0_g2` | **4개 통과**, 332.6초 | 세 Provider 완료·결과 파일, Codex·Antigravity 실패·실제 종료 취소, Daemon 종료 시 모든 Provider Run 취소. 목록 기록: Claude `Some(true)`, Codex `Some(true)`, Antigravity `Some(false)`(정책 `hidden`과 일치) |

Windows 병합 전 확인: Workspace 166개 통과·36개 제외, fmt 통과(엄격 Clippy는 보정 커밋 때 통과).

**시험 세션:**

- Claude(서버 `~/.claude/projects/`의 시험 폴더 16개, 모두 `/tmp/telos-*` cwd): `-tmp-telos-claude-env-{cancel,completed,denied,invalid-model,listing,listing-hidden,resume}`, `-tmp-telos-phase0-g1-{cancel,completed,failure,shutdown}-workspace`, `-tmp-telos-phase0-g2-completed-workspace`, `-tmp-telos-phase0-master-{delegate-master,delegate-work,forbidden-master}`, `-tmp-telos-phase0-rn-independent-workspace`. 병합 시 폴더째 삭제
- Codex: G2 시험이 Archive(`01a173df-2607-73b2-9f29-82dc192eec1f` cancel, `01a173e0-6a02-7790-ae99-dd951769c5e6` failure, `01a173e1-9e97-7692-835c-b75c54980e74` shutdown, `01a173e2-dd31-7593-a41a-788e3c06e7f2` completed). 독립 App Server 조회에서 활성 0개·Archive 4개
- Antigravity: G2 시험이 `antigravity-cli`의 `brain`·`conversations`를 삭제(`63e1a7fe-110f-4c50-a338-eaab1134c914`, `6baf4644-255a-476b-a3f3-f7d5d4311bb8` 등). 남은 대화 0개

**남은 G3 항목:** PH0-EXT E2(Remote-SSH Extension Host), Remote SSH 시나리오(절차 §4.4), DEC-08 Linux 화면 확인(Claude·Codex), HOST `discovery_environment` 두 벌 단언(§9.3 후속 1), PH0-RN-08, G3 대조표·판정.

### 9.5 Remote SSH: PH0-EXT E2와 시나리오 (`psyki-nas`, 2026-10-26, 사용자 VS Code)

사용자가 Windows VS Code로 조작하고 INTEGRATION이 서버에서 프로세스·권한·로그를 확인했습니다. 서버 Worktree `~/TelosWeaver-wt/g3-remote`(`1733bf3`, 시험 뒤 삭제), 시험 상태·Socket은 `.telos-weaver/remote-e2/state`(`XDG_STATE_HOME`·`XDG_RUNTIME_DIR`), Remote Settings `telosWeaver.corePath`·`telosWeaver.hostId=psyki-nas`(시험 뒤 사용자가 복구). Run 시작·조회는 빌드된 Extension 모듈(`dist/core`)을 쓰는 서버 보조 스크립트로 했습니다(토큰 미출력).

**E2: Remote-SSH Extension Host**(절차 [PH0-EXT Linux](../PH0-EXT/LINUX.md) §4를 이 Worktree 경로로 수행)

| 단계 | 결과 |
|---|---|
| 원격 창(`SSH: Psyki-NAS`) → F5 개발 Host | Running Extensions에서 TelosWeaver가 `SSH: Psyki-NAS`(원격 Extension Host)로 표시. 활성화만으로 Core 미시작(`미연결`) |
| Core 시작 / 연결 | `연결됨 (degraded)`, Host `psyki-nas`, `rn` degraded. Core PID 107682(01:44:18), 부모 = 원격 Extension Host(105081), 자기 세션(SID=PID). Core 환경에 시험 XDG 경로 전달. Socket `0600`·토큰 `0600`·디렉터리 `0700`, 소유 `gth1919` |
| 연결 해제(Core 유지) → Core 연결 | 같은 Core에 재연결(이 프로젝트의 Core 1개 유지) |
| 개발 Host 창 종료 | Core 107682가 같은 시작 시각으로 생존, 부모가 1로 바뀜(UI 수명과 분리). **E2 통과** |

**시나리오 1: Run 중 SSH 단절**(절차 §4.4 1~3). 정책 Core(PID 109370, Claude `haiku`, Write 허용)를 서버에서 시작하고 개발 Host가 연결. Run `run-f8cb7ba356a0375ed0cb471b4da72255`(파일 5개를 차례로 쓰기)

| 시각 | 사건(근거) |
|---|---|
| 01:51:47 | Run 시작, Claude PID 115177 |
| 01:52:00~01:52:16 | `part1~3.txt` 작성 |
| **01:52:22.7** | 사용자가 Windows에서 `Get-Process ssh \| Stop-Process -Force` → sshd `Connection closed by 192.168.1.2`(VS Code 연결 모두) |
| 01:52:24·01:52:37 | **단절 중** `part4·5.txt` 작성 |
| 01:52:43~46 | SSH 재접속, VS Code Server `The client has reconnected` |
| 이후 | Run `Succeeded`, attempt 1, 이벤트 19개(`process_exited` 포함), Core PID 109370 유지, 파일 5개. 개발 Host에서 `Run 출력 구독`으로 1~19 전체 수신 |

판정: UI 연결과 원격 실제 실행이 분리되고 중복 실행이 없음(§14.2 "Remote SSH 단절" P0) **통과**.

**시나리오 2: Host 유실(Core 강제 종료)**(절차 §4.4 4, SSH 단절과 별도)

| 시각 | 사건 |
|---|---|
| 01:54:06 | Run `run-04a295fb27bece7f7aea1a6bb37dd901` 시작, Claude PID 117764 |
| **01:54:16.4** | `kill -9 109370`(Core) |
| 01:54:16~01:54:54 | **Core 없이 Claude가 계속 실행**해 `part1~5.txt`를 모두 쓰고 스스로 종료 |
| 사용자 화면 | 패널 `연결 오류`, 진단 "실행 중인 Core에 연결할 수 없습니다" |
| 01:56:53 | 사용자 `Core 시작 / 연결` → 새 Core 119715(정책 없음) `연결됨 (degraded)`. 남은 Socket 파일이 시작을 막지 않음. 새 Core는 이전 Run 두 개를 `not_found`로 거부(Phase 0 메모리 기록, Health `no crash recovery`와 일치) |

**발견 사항(G3 판정 전 처리 필요):**

1. **Linux에서 Core 비정상 종료 시 Provider 프로세스가 감독 없이 계속 실행됨**(HOST). Core가 죽어도 자식 Claude가 40초 가까이 작업을 마치고 파일을 썼고, 그 부작용은 어떤 Core 기록에도 남지 않습니다. Spec §14.2 "Core 비정상 종료" P0(불명확 상태 차단)와 §3 프로세스 감독 관점에서 검토가 필요합니다. Windows Job Object 경로의 같은 상황 결과는 아직 확인하지 않았습니다.
2. **Claude가 긴 `sleep`을 백그라운드로 돌리고 Turn을 끝낸 Run이 `Interrupted`로 끝남**(CLAUDE). 첫 시도 Run `run-eaeaed12408ec00ac1994ec808187035`: 이벤트 1~9(`turn_finished`까지)만 있고 `process_exited` 없이 `Interrupted`, 백그라운드 `sleep`·결과 파일 없음. 원인(백그라운드 작업이 남은 상태의 `-p` 종료 처리) 조사 필요.
3. **연결 오류 중 패널에 마지막 Health 값이 남아 표시됨**(EXT, 비차단). `연결 오류`와 함께 `execution ok` 등이 현재 상태처럼 보입니다.

**시험 세션 정리:** 서버 Claude 시험 폴더 `-home-gth1919-TelosWeaver-wt-g3-remote--telos-weaver-remote-e2-workspace`(세션 `041d7056-a6f6-4a3b-b2a0-af4ac5166fa6`, `64afc33c-471b-4c17-98ca-a05c6cd73b7f`, `a2cd2986-655d-4146-b179-172bfc40c566`) 삭제, 시험 Core 정지, Worktree 삭제. Codex·Antigravity 세션은 만들지 않았습니다.

### 9.6 발견 2 처리: Claude가 이어간 Turn으로 Run이 `Interrupted` (2026-10-26, CLAUDE)

**원인(재현).** Windows에서 확장 동봉 CLI 2.1.310을 Adapter와 같은 인자(`-p`, Stream JSON, `haiku`, `acceptEdits`, `Bash(sleep:*)`·`Write`)로 직접 실행하고 "Bash `run_in_background`로 `sleep 20` → 'started' → 끝나면 `done.txt` 작성"을 요청했습니다. CLI는 첫 `result`(`started`)를 낸 뒤 끝나지 않고 `task_updated`·`task_notification`·`background_tasks_changed`를 거쳐 **같은 프로세스에서 두 번째 `system/init`·도구 호출·`result`(`done`)**를 출력했습니다. Adapter가 이 출력을 `turn_finished` 뒤의 이벤트로 넘기자 Core가 Run 이벤트 규칙 위반으로 Run을 끊었습니다(§9.5 첫 시도: 이벤트 9개, `Interrupted`, 결과 파일·프로세스 없음).

**처리(`ph0/claude-bg`).**

| 변경 | 내용 |
|---|---|
| `crates/adapters/src/claude.rs` | 자식 환경에 `CLAUDE_CODE_DISABLE_BACKGROUND_TASKS=1`. 같은 요청을 이 값으로 직접 실행하면 CLI가 `run_in_background`를 입력 검증 오류로 거부하고, 모델이 포그라운드로 처리해 `result` 하나로 끝남(재현 확인) |
| `crates/adapters/src/claude/run.rs` | 방어: `turn_finished` 뒤 새 Turn 출력(이벤트로 바뀌는 줄 또는 해석 실패)이 오면 기록하지 않고 프로세스를 끝냄. 요청 종료(종료 코드 없음)로 보고되어 Core는 성공으로 판정하지 않음 |
| `crates/adapters/tests/claude_adapter.rs` | 기본 환경에 이 변수가 있는지 검사. 새 Mock 시험 `continuation_after_result_is_not_recorded_and_stops_the_process`(fixture `background-continuation.jsonl`: 첫 `result` 뒤 작업 알림·두 번째 `init`·응답·`result`). 방어 코드를 빼면 이 시험이 시간 초과로 실패함을 확인 |
| `crates/adapters/tests/claude_environment.rs` | 실환경 `env_background_tasks_do_not_continue_the_run`: 같은 요청에서 Turn 1개, 종료 코드 0·자연 종료(이어 갔다면 요청 종료로 실패) |
| Spec v1.5.6 §7.3 | Provider가 스스로 이어가는 Turn의 처리 규칙 |

| 검사 | Windows | `psyki-nas` |
|---|---|---|
| `claude_adapter`(Mock) | 16개 통과 | — |
| `env_background_tasks_do_not_continue_the_run`(실환경) | 통과, 17.7초(세션 `294704fe-fc8d-4835-b051-b940baea29a2`) | 통과, 21.6초(세션 `8fb8d993-4512-4f96-a0da-069c562619be`) |
| Workspace·엄격 Clippy·fmt | 167개 통과·37개 제외, 통과 | — |

**시험 세션 정리:** 직접 재현 세션 `87ec58ea-5124-43d1-ac6a-6bc1ea8b09fb`·`e63a155f-3751-4df2-8917-e883e506aef5`(Windows 임시 폴더 `/tmp/telos-bg-*`), 실환경 시험 세션 2개(각 Host의 `telos-claude-env/background`). 병합 시 Claude 시험 폴더를 지웁니다.

### 9.7 PH0-RN-08 검토·통합 (2026-10-26)

Antigravity(`gemini-3.8-flash/session-f1b89d3a`, 독립 검토자, Spec §19.5.10)가 `ph0/rn-08`에 제출(`dbbdc79`·`332ebab`, [RN-08 증거](../PH0-RN/RN-08/README.md)). 변경: 원본 Python E2E 9개 삭제와 사유(`rn-fork/tests/e2e/README.md`, `TELOSWEAVER.md` #30), RN-08 증거, PH0-RN 요약. 자기 경로만 수정.

**INTEGRATION 재확인**

| 항목 | 방법 | 결과 |
|---|---|---|
| 제거 대상 부재 | `rn-fork/src`에서 `reason` CLI 실행(`find_bin("reason")`·`Command::new("reason")`·`reason session`), `auto_update`·`install.sh/ps1`·`Request::Proof`·`verified_people`·`upgrade` 검색 | 0건 |
| `ring` 사용 범위 | 단어 경계 `ring::` 검색 | `src/signed.rs`만(중계 목록 서명 검증·시험 서명) |
| HTTP·업데이트 의존성 | `Cargo.toml`의 `reqwest`·`ureq`·`hyper`·`self_update` | 없음 |
| 삭제한 E2E의 참조 | `rn-fork` 전체 | `README.md` 한 문장이 남아 있어 통합 때 수정 |
| Windows 격리 시험 | 임시 `USERPROFILE`·`APPDATA`·`LOCALAPPDATA`·`TELOS_RN_HOME`, `cargo test --locked` | 204개 통과, 엄격 Clippy 0건, fmt 통과 |
| `psyki-nas` 격리 시험 | 임시 `HOME`·`XDG_*`·`TELOS_RN_HOME`(Branch를 서버로 보내 별도 Worktree) | 212개 통과, 엄격 Clippy 0건, fmt 통과 |

**검증 한계(증거의 "완벽히 입증"을 이 범위로 읽음)**

- RN-POC-13은 소스 정적 검색과 Windows 바이너리 문자열 검사에 근거합니다. 간접 호출을 완전히 배제하는 증명은 아니며, Linux 바이너리 문자열은 확인하지 않았습니다.
- RN-POC-14의 Daemon 관찰은 팀에 가입하지 않은 격리 상태에서 2~3초입니다. 팀 가입·중계 사용 중의 외부 통신은 관찰 범위 밖입니다(유지 대상 네트워크 기능, DEC-07).
- RN-POC-15는 RN-05~07의 시험과 실환경(메일함 왕복·재시작·정지, RN 장애 중 Claude Run 독립)과 이번 Windows `phase0_rn` 실환경 1개에 근거합니다. Linux 실환경은 §9.4의 `phase0_rn` 2개(`psyki-nas`)입니다.
- Remote SSH에서 RN을 따로 시험하지 않았습니다. RN은 Core가 같은 Host에서 시작하므로 §9.4·§9.5의 Linux Core 결과에 기댑니다.

판정: RN-08 완료 조건(제거 검증과 기능 시험의 근거 존재, 대상 OS 증거와 남은 조건 인계)을 위 한계와 함께 충족한다고 보고 통합합니다.

### 9.8 비차단 후속 두 건 검토·통합 (2026-10-26)

**PH0-AGY `Hidden` 주석 보정**(`gemini-3.8-flash/session-c2e931ec`, `502a3c8`, [지시서](AGY_LISTING_FOLLOWUP.md)). 병합 `c28c801`.

| 항목 | 결과 |
|---|---|
| 범위 | `agy.rs` `AgyVsCodeListing::Hidden` 문서 주석, `agy_environment.rs` `env_vscode_listing` 관찰 주석, AGY 증거 §8. 주석만 변경(동작·단언·Capability 사유·공개 API 불변) |
| 내용 | 화면 확인(`07b7210`)의 범위를 Windows 로컬·확장 1.9.0·`agy` 1.4.7로 한정하고 Linux·Remote SSH는 미판정으로 적음. 자동 시험이 UI 숨김을 보장하지 않는다는 구분 유지 |
| 검사(병합본, Windows) | `cargo fmt --all --check`·엄격 Clippy 통과, `agy_adapter` 16개 통과. 담당 제출: Workspace 167개 통과·37개 제외 |

**REQ-022 Codex 외부 호출 절차 보정**(`Codex / PH0-CODEX-REQ022-01a17586`, 이전 세션 `…-01a17034` 인수, `80bd5c0`, [요청서](REQ-022.md), [CODEX 기록](../PH0-CODEX/REQ-022.md)). 병합 `6990c32`.

| 항목 | 결과 |
|---|---|
| 범위 | `docs/AGENT_CLI_VSCODE_SESSIONS.md` 요약·A1~A6·D·E·F, CODEX 증거, 참고 클라이언트 `req022_app_server.py`와 오프라인 시험. Spec·공통 계약·Adapter 불변 |
| 결론 | `exec --thread-source cli`·`exec fork … --thread-source cli`는 `thread_source`만 바꾸고 `source`는 `exec`로 남아 기본 목록에서 제외. 표시할 작업은 App Server `thread/start`·`turn/start`, 기존 `exec` 대화는 App Server `thread/fork`로 새 `vscode` Thread |
| 참고 클라이언트 검토 | 승인·클라이언트 도구 요청을 자동 허용하지 않음, 응답 정책(Sandbox·승인·네트워크·쓰기 범위)이 요청과 다르면 Turn 전 거부, `source`가 `cli`·`vscode`가 아니면 거부, `ephemeral: false`, 설정 값·자격 증명 미출력. Turn `completed`·서버 종료 코드·클라이언트 종료 코드를 구분 |
| 화면 확인 | 새 App Server 대화(`01a173dd-3257-…`)의 목록 표시·열기는 사용자 확인 통과. App Server Fork 분기의 화면 열기·실시간 반영·Linux·Remote SSH는 미검증으로 문서에 기록 |
| 검사(INTEGRATION) | 오프라인 시험 6개 Worktree·병합본 각각 통과(`python -I -B …/test_req022_app_server.py`), `git diff --check` 통과. 실계정 재실행 없음(담당의 실환경 결과는 이전 세션 기록에 근거, CODEX 기록 §6) |
| 시험 Thread | CODEX 기록 §5의 9개 모두 `~/.codex` 활성 세션 0개·보관 1개씩(INTEGRATION 재확인). 요청서의 시험 Thread 2개는 접수 때 Archive |

두 건 모두 Worktree·Branch를 삭제했습니다.

## 10. DEC-08 VS Code 확장 목록 표시 통합 (2026-10-26)

| 항목 | 값 |
|---|---|
| 수행 | PH0-INTEGRATION 몫을 사용자 지시로 `claude-opus-5.5/session-be1c1774`가 수행 |
| 기준 | `main` `f91108a`, Spec v1.5.3 → v1.5.4, 계약 `7497877`(변경 없음) |
| Branch | `ph0/listing-int` / `D:\worktree\TelosWeaver\ph0-listing-int` |
| 병합 | `ph0/claude-listing`(`21da117`), `ph0/codex-listing`(`5449185`), `ph0/agy-listing`(`07b7210`). 충돌 없음 |
| 지시 | [VSCODE_LISTING_ASSIGNMENTS.md](VSCODE_LISTING_ASSIGNMENTS.md) §5 I1~I4, REQ-021 |

### 10.1 담당 결과 검토

| 작업 | 범위 | 검토 |
|---|---|---|
| PH0-CLAUDE | `CLAUDE_CODE_ENTRYPOINT` 명시·부모 세션 변수 제거, `with_vscode_listing` 빌더, Capability. [증거](../PH0-CLAUDE/VSCODE_LISTING.md) | 독점 경로만 수정. Mock 15개·실환경 7개, 화면 확인 통과 |
| PH0-CODEX | `listed`: 영속 생성·Fork에 `threadSource: "vscode"`, 응답 출처 검사·비대화형 출처 거부, `hidden` 프로세스 생성 전 거부, `with_vscode_listing` 생성자. [증거](../PH0-CODEX/VSCODE_LISTING.md) | 독점 경로만 수정. 출처 실측으로 숨김 수단 없음 확인(REQ-021), 사용자 결정(Hidden 거부) 반영. 화면 확인 통과 |
| PH0-AGY | `AgyVsCodeListing`↔`AgyDataDirMode` 대응, Capability, 취소 정규화 Mock 시험(§9.3 후속 2). [증거](../PH0-AGY/VSCODE_LISTING.md) | 독점 경로만 수정. `listed`·`hidden` 화면 확인 통과. 비차단: `AgyVsCodeListing::Hidden` 문서 주석이 "UI에서 숨겨지는지 미검증"으로 남아 있어 증거(§3.3·§4 화면 확인 통과)와 다름 → 다음 AGY 작업에서 갱신 |

### 10.2 통합 변경 (I1·I2, REQ-021)

| 경로 | 내용 |
|---|---|
| `crates/core/src/integration/policy.rs` | `VsCodeListing`(`listed` 기본/`hidden`), `claude`·`codex`·`agy` 절의 `vscodeListing`. Codex `hidden`은 정책 로드에서 거부. Agy `dataDir`은 이전 표현으로 유지하고 `vscodeListing`과 다르면 거부. 승인 Snapshot(`RunSnapshot`)에 `vscode_listing`·`vscode_listing_method` 기록 |
| `crates/core/src/integration/mod.rs` | 정책 값을 Claude `with_vscode_listing`, Codex `CodexAdapter::with_vscode_listing(…, Listed)`, Agy `AgyVsCodeListing::data_dir_mode()`로 전달. Agy `--app_data_dir` 사전 검증은 `listed`일 때만 |
| `crates/core/tests/phase0_policy.rs` | 시험 3개: 기본 `listed`와 Provider별 지정 방식 기록, Claude·Agy `hidden` 기록, Codex `hidden`·Agy 충돌·알 수 없는 값 거부 |
| `crates/core/tests/phase0_g2.rs` | `g2_each_provider_completes_and_writes_a_result_file`에 세션별 목록 대상 기록 검사 추가(정리 전 확인) |
| `docs/DEVELOPMENT_v1.5.md` | v1.5.4: §7.6 판정 결과·Codex `hidden` 미지원·Agy `dataDir` 관계·요약 인덱스 잔여 행 처리 |
| `docs/AGENT_CLI_VSCODE_SESSIONS.md` | E절을 통합 결과로 갱신 |

### 10.3 검사 (Windows `windows-local`)

| 검사 | 결과 |
|---|---|
| `cargo test --workspace --locked` | 166개 통과(기존 155 + CLAUDE 2 + CODEX 4 + AGY 2 + 정책 3), 실환경 36개 기본 제외 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 통과 |
| `cargo fmt --all --check`, `git diff --check` | 통과 |
| `phase0_g2::g2_each_provider_completes_and_writes_a_result_file` (실환경, 정책 기본값) | 통과(66초). 한 Daemon에서 세 Provider 완료·결과 파일 작성, 목록 대상 기록: Claude 모든 줄 `claude-vscode`, Codex 출처 검사 통과 후 완료, Agy 대화가 확장 데이터 디렉터리에만 생성. 세션 `214bc038-9e58-4aca-a59f-a95774e87c4d`(Claude), `01a17393-4cc9-7773-b953-b765bfaa714c`(Codex, Archive), `df215f30-17a7-4a11-b5ba-6b8b942a4083`(Agy, 파일 삭제) |
| Extension | 변경 없음. 통합 Worktree에 `node_modules`가 없어 `npm test` 미실행 |

### 10.4 판정

- 세 Provider `listed`: Adapter 자동 판정·사용자 화면 확인·Core 통합 경로 모두 통과. §19.1 DEC-08 항목은 Windows 로컬 기준 충족.
- `hidden`: Claude·Antigravity 통과, Codex 미지원(정책 거부, 사용자 결정).
- 미판정: Linux·Remote SSH, 실시간 화면 반영, 실행 중 확장 쪽 입력과의 경합(§7.6은 안내만 규정).

### 10.5 시험 세션 정리 (사용자 지시 2026-10-26)

| Provider | 처리 |
|---|---|
| Claude | `~/.claude/projects/`의 시험 폴더 8개 삭제(`C--Users-Psyki-HOME-AppData-Local-Temp-telos-claude-env-{cancel,completed,denied,invalid-model,listing,listing-hidden,resume}`, `…-telos-phase0-g2-completed-workspace`). 사용자 Workspace 폴더 `d--Downloads-Vibe-dev`에서는 화면 확인용 파일 2개(`46ad660e…`, `d1e69e03…`)만 삭제하고 다른 대화는 건드리지 않음 |
| Codex | DEC-08 시험 Thread는 CODEX가 모두 공식 Archive(증거 §6). G2 통합 시험 Thread `01a17393…`도 Archive |
| Antigravity | 대화 파일은 AGY·G2 정리로 삭제됨. 남은 `33339a4c-298f-4432-ab36-f22d4d30c013`(Codex 화면 확인 폴더의 시험 대화)의 `brain`·`conversations/*.db`를 삭제. 요약 인덱스(`conversation_summaries.db`)의 시험 행(확장 쪽 12개, CLI 쪽 12개)은 확장 내부 DB라 수정하지 않음(§7.6). 확장 쪽 `listed` 행은 확장 목록에 빈 항목으로 보일 수 있어 사용자 결정 필요 |

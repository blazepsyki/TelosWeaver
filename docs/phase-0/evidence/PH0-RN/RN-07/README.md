# PH0-RN-07 Client·Core 연결

| 항목 | 값 |
|---|---|
| 서브 태스크·담당 | PH0-RN-07 · `claude-opus-5.5/session-c9336d2c` (RN Client는 PH0-RN, Core 연결은 PH0-INTEGRATION — 같은 담당) |
| 작성일 | 2026-10-25 |
| Branch / Worktree | `ph0/rn-07` / `D:\worktree\TelosWeaver\ph0-rn` |
| 기준 | Spec v1.5.1 §12.3·§12.5(`4b08d54`), §19.5.10, 배정표 §6 "RN Core 연결", 계약 기준 `7497877` |
| 사용자 결정 (2026-10-25) | ① 실행 파일은 정책 `rn.executable` 절대 경로만 ② 상태 폴더는 Core 프로젝트별 상태 폴더 아래 `rn` ③ Core가 자식 프로세스로 시작·정지, 이미 있으면 연결만, 예기치 않은 종료 시 최대 3회 재시작 ④ Core 내부 Rust API로만, 밖에는 `core.health`의 `rn`만 ⑤ 독립성은 실제 Claude Run 중 RN 강제 종료로 확인. 재검토: P1·P2 착수 전 사용자 단위 공유 RN(Spec v1.5.1) |
| 공통 변경 | REQ-019(사용자 승인): HOST 소유 `execution/supervisor.rs`에 Health 구성 요소 등록 연결점. HOST 사후 확인 요청 |

## 1. `telos-rn-client` (`crates/rn-client`)

| 항목 | 내용 |
|---|---|
| 접속 | RN 상태 폴더의 `rpc.json`·`rpc-token`(64자 소문자 hex 확인)을 읽고 Pipe/Socket에 접속합니다. Windows는 Pipe 서버 PID가 `rpc.json`의 PID인지 확인합니다. `rn.hello` 결과의 RPC 버전이 다르면 계약 위반입니다 |
| 요청 | 기본 제한 시간 5초. 시간이 지나도 연결은 유지하고 늦은 응답은 버립니다. 대기 요청 32개, 프레임 1 MiB |
| 오류 | `Unavailable`(RN 없음·Pipe 위조), `Timeout`, `Protocol`(계약 위반·프레임 상한, 연결 닫음), `Closed`, `Rejected(RnError)`, `Busy` |
| 재시도 | `connect_retrying`: 250 ms부터 두 배(최대 4 s)로 정한 횟수만. 인증 거부·계약 위반은 다시 시도하지 않습니다 |
| 구독 | 응답을 해석하는 그 자리에서 구독 ID를 연결합니다(REQ-018 순서 보장). 소비자가 1,024개 넘게 밀리면 그 구독만 닫고, 호출자가 마지막 커서로 다시 구독합니다 |
| 메서드 | `health`·`providers`·`discover`·`resolve`·`capabilities`·`send`·`status`·`subscribe`, 일반 `request` |

## 2. Core 연결 (`crates/core/src/integration`)

| 항목 | 내용 |
|---|---|
| 정책 | `LocalPolicyConfig.rn: { executable }`(절대 경로만, 미정의 필드 거부) |
| 시작 (`RnService::start`) | 공용 프로세스 런타임(Windows Job Object·`CREATE_NO_WINDOW`)으로 `telos-rn daemon`을 띄웁니다. `TELOS_RN_HOME` = Core 프로젝트별 상태 폴더(Local API 토큰이 있는 폴더) 아래 `rn`, `TELOS_RN_HOST_ID` = Core `--host-id`, `TELOS_RN_CODEX_BIN` = Core가 확장에서 찾은 Codex CLI(없으면 넘기지 않음). 출력은 RN 폴더의 `daemon.log`로 계속 읽어 냅니다. 같은 폴더에 이미 RN이 돌면 연결만 하고, 10초 안에 RPC가 열리지 않으면 실패로 둡니다 |
| 감시 | 2초마다 `health.get`. 끊기거나 Core가 띄운 RN이 끝나면 Degraded로 두고 1·2·4초 뒤 최대 3회 다시 띄웁니다. 60초 이상 정상이면 횟수를 되돌리고, 넘으면 "gave up"으로 Degraded에 남습니다 |
| Health | `core.health`에 `rn` 구성 요소를 Core 기본 구성 요소 뒤에 더합니다(REQ-019). 연결: `ok` + RPC·Fork Commit·PID·"started by Core/already running". 설정 없음·시작 실패·재시작 중: `degraded` + 사유. 전체 상태 판정은 바꾸지 않습니다 |
| 정지 | Core 종료 순서: Local API 닫기 → 활성 Run 취소·확인 → RN 정지(Core가 띄운 경우만, 3초 유예 뒤 강제). 이미 돌던 RN은 두고 나옵니다 |
| 노출 | `Phase0Execution::start_rn`이 `RnService`를 돌려주고 `client()`로 내부에서 씁니다. Local Core API·마스터 결정에는 메시지 기능을 더하지 않았습니다(결정 ④, 계약 변경 없음) |
| 폴더 결정 위치 | `start_rn` 한 곳(Spec v1.5.1 재검토 시 이 함수만 바꿈) |

**REQ-019 (`execution/supervisor.rs`, HOST 소유):** `HealthComponent`(`Arc<dyn Fn() -> ComponentHealth>`) 목록과 `add_health_component`를 더하고, `health()`가 기본 구성 요소 뒤에 그 결과를 붙입니다. 기존 구성 요소·전체 상태는 그대로입니다.

## 3. 검증 (Windows 11, rustc 1.94.0)

| 명령 | 결과 |
|---|---|
| `cargo test --workspace` | 152개 통과, 32개 실환경 제외 (기존 143개 + RN Client 6개 + Core RN 3개) |
| `cargo clippy --workspace --all-targets -- -D warnings` | 경고 0건 |
| `cargo fmt --all -- --check` | 통과 |
| `rn-fork`: `cargo build --locked` | 성공(RN-06 코드 그대로, 변경 없음) |

**추가한 시험**

| 시험 | 확인 |
|---|---|
| `rn-client/tests/client.rs` 6개 (가짜 RN, 실제 Pipe) | hello·토큰·거부 변환 / 시간 초과 뒤 연결 유지·늦은 응답 버림 / 같은 데이터 묶음의 구독 알림과 다른 구독 알림 분리, 연결 끊김 시 구독 종료 / 프레임 상한은 대기 요청에 이유를 주고 닫음 / `rpc.json` 없음·토큰 아님·PID 다른 Pipe는 `Unavailable`, 재시도 간격 / 인증 거부는 재시도 안 함 |
| `phase0_rn::the_policy_takes_an_rn_executable_only_as_an_absolute_path` | 상대 경로·미정의 필드 거부 |
| `phase0_rn::without_rn_in_the_policy_core_reports_it_not_configured_after_its_own_components` | 정책 없음 → `rn` Degraded "not configured", 구성 요소 순서, Local API `core.health`에도 보임 |
| `phase0_rn::an_rn_that_cannot_start_leaves_core_running_and_says_why` | 없는 실행 파일 → Degraded "not started", RN 폴더는 Core 상태 폴더 아래, 실행 감독 정상, 정지 후 "stopped with Core" |

### 3.1 실환경 1: 실제 RN 왕복·재시작·정지 (`rn_round_trip_restart_and_stop_with_core`, 모델 사용 없음)

`TELOS_RN_EXECUTABLE`=이 Worktree의 `rn-fork/target/debug/telos-rn.exe`. Core 상태 폴더는 시험 임시 폴더입니다.

| 단계 | 결과 |
|---|---|
| 시작 | `rn`: `rpc 0.1, fork a1748d2…, pid 2768 (started by Core)`. `rpc.json` PID = `owned_pid` |
| 메일함 `review`(원본 CLI `listen`) | `discover`에 `attached: true`. Client의 hello Endpoint는 `telosweaver`, Host는 Core의 `host-local` |
| 구독 중 Envelope 전송 → review 읽음 → review가 `telosweaver`로 답장 | 이벤트: `receipt queued` → `presence` → `receipt delivered` → `presence busy` → `message`(`replyTo: rn07-m1`, 발신자 Endpoint). `status` = `delivered` |
| RN 강제 종료(`taskkill /F`) | `rn` Degraded `stopped; restarting (1/3)` → `ok` `pid 37052 (started by Core)`(새 프로세스). Receipt 유지, 이전 커서 → `resync_required` |
| Core 쪽 정지 | RN 접속 불가(프로세스 종료). 남은 `telos-rn` 프로세스 없음 |

### 3.2 실환경 2: RN 장애 중 Managed Run (`a_managed_run_goes_on_and_cancels_while_rn_is_down`, Claude `haiku`)

| 단계 | 결과 |
|---|---|
| 구성 | 정책 `claude`(haiku, `acceptEdits`, Read·Write만) + `rn`. Claude CLI 2.1.310(확장 탐지) |
| Run 시작(1~1500 출력 과제) → `session_started` 뒤 RN 강제 종료 | `core.health`: `rn` degraded `stopped; restarting (1/3)`, `execution` ok, 전체 degraded |
| RN이 꺼진 동안 | `runs.inspect` `Running`, 2초 뒤에도 `Running` |
| `runs.cancel` | 접수 `accepted`, 이벤트 `turn_finished cancelled` → `process_exited`(`childrenReaped: true`, `killed`), `runs.inspect` `Cancelled` |
| 그사이 RN | 스스로 복구 `pid 37912 (started by Core)` |
| Core 종료 정리 | 확인되지 않은 Run 없음 |

이 2초 동안 Claude의 출력 이벤트는 없었습니다(이벤트는 `session_started`·`turn_finished`·`process_exited` 3개). 그래서 "RN이 꺼진 동안 출력 이벤트가 계속 수집됐다"는 판정은 하지 않습니다. 확인한 것은 추적(`Running`)·취소 접수·실제 종료·자식 회수·최종 상태가 RN과 무관하게 동작한다는 점입니다.

### 3.3 검증 한계

- Linux·Remote SSH는 실행하지 않았습니다(WSL 미구성, RN-06의 Linux 빌드 확인 불가 한계와 같음).
- 같은 폴더에 이미 돌던 RN에 "연결만" 하는 경로는 단위 시험 없이 코드 검토만 했습니다(실환경 1은 Core가 띄운 경우).
- RN 재시작 상한("gave up") 경로는 실제로 3회 연속 실패시키지 않았습니다(없는 실행 파일 시험은 첫 시작 실패만 확인).
- RN 정지가 강제 종료(3초 유예)라 RN DB는 WAL로 남습니다. RN-05의 재시작 시험에서 Receipt 보존은 확인했습니다.
- 자기 검토만 했습니다. REQ-019는 HOST 사후 확인을 요청했습니다.

## 4. 시험 세션 (병합 시점 삭제 대상)

| Provider | 세션 | 위치 |
|---|---|---|
| Claude | `93051c83-fd40-4b90-a904-9ecfaef77a03` | `~/.claude/projects/C--Users-Psyki-HOME-AppData-Local-Temp-telos-phase0-rn-independent-workspace` |

## 5. 후속

| 항목 | 넘긴 곳 |
|---|---|
| 제거 검증·원본 E2E 정리·외부 접속 관찰·Linux 빌드와 실행 | PH0-RN-08 (WSL 필요) |
| 메시지 기능의 Local Core API·마스터 결정 노출, Core Managed Run의 RN 주소 노출 | P0 이후(결정 ④, Spec §12.5) |
| 사용자 단위 공유 RN 재검토 | P1 기존 세션 연결 또는 P2 원격·팀 착수 전(Spec v1.5.1 §12.5) |
| REQ-019 사후 확인 | PH0-HOST |

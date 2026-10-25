# PH0-RN-06 Local RPC 서버

| 항목 | 값 |
|---|---|
| 서브 태스크·담당 | PH0-RN-06 · `claude-opus-5.5/session-c9336d2c` |
| 작성일 | 2026-10-25 |
| Branch / Worktree | `ph0/rn-06` / `D:\worktree\TelosWeaver\ph0-rn` |
| 기준 | Spec v1.5 §12.1~12.5·§19.5.10, 배정표 §6 "RN RPC", 계약 `telos_schema::rn`(REQ-018, 계약 기준 `7497877`) |
| 사용자 결정 (2026-10-25) | ① RPC 전용 Pipe/Socket과 시작마다 새 토큰, 첫 요청 `rn.hello` ② 원본·RPC Pipe 모두 현재 사용자만 ③ 연결되지 않은 세션은 목록에만, 전송은 `refused` ④ 이벤트는 Core 메시지의 Receipt·Core 메일함 답장·접속 상태 ⑤ 커서는 메모리 1,024개, 재시작 후 `resync_required` |

## 1. 계약 (REQ-018, `7497877`)

RN-06 착수 시 `telos_schema::rn`에는 요청·응답만 있고, 인증·알림 프레임·구독 결과가 없었습니다. 그래서 REQ-018로 먼저 다음을 추가했습니다(CONTRACT, 통합 `7497877`).

- `RnRpcInfo`(`rpc.json`), `RnHelloParams`(토큰은 `SecretString`)·`RnHelloResult`(Core 메일함 Endpoint)
- `RnNotification`(`messages.event`)·`RnEventParams`, `SubscribeResult`, `InboundMessage`
- `SessionEndpoint.attached`, 오류 `unauthenticated`·`frame_too_large`, `RnRequest` Debug 토큰 가림
- 모듈 문서: 접속·프레임·구독·연결된 세션만 전송

Schema 7개를 추가하고 5개를 갱신했습니다. Workspace 143개·Extension 28개가 통과했습니다.

## 2. 구현 (`rn-fork/`)

| 항목 | 내용 |
|---|---|
| 접속 | 원본 Pipe 옆에 전용 Pipe `\\.\pipe\telos-rn-rpc-<hash>`(Unix는 `rpc-<원본 이름>`)를 둡니다. 시작할 때 32바이트 난수 토큰을 `rpc-token`에 씁니다. 빈 파일을 만들어 사용자 전용 권한을 적용한 뒤에 내용을 씁니다. 그다음 `rpc.json`(Endpoint·RPC 버전·PID)을 씁니다. 토큰 권한을 적용하지 못하면 RPC를 열지 않습니다(에이전트용 원본 API는 그대로 동작) |
| 접근 권한 (결정 ②) | `private.rs`가 Core Local API(`crates/core/src/api/security.rs`)와 같은 방식을 씁니다. 원본·RPC Pipe 모두 현재 사용자 SID만 허용하는 보호 DACL `D:P(A;;GA;;;<SID>)`이고, 원격 클라이언트는 거부합니다. 토큰 파일은 소유자를 확인한 뒤 같은 DACL을 적용합니다. Unix는 Socket·토큰 0600 |
| 프레임 | NDJSON 1 MiB(`api::MAX_FRAME_BYTES`). 넘으면 `frame_too_large`(`requestId: ""`)를 보낸 뒤 닫습니다. UTF-8·JSON이 아니면 `invalid_params`, 버전이 다르면 `unsupported_protocol_version`, 모르는 메서드는 `unknown_method`. 이 세 경우는 연결을 유지합니다 |
| 인증 | `rn.hello` 전의 요청과 잘못된 토큰은 `unauthenticated`입니다. 토큰은 비교 시간이 위치에 따라 달라지지 않게 비교합니다. 결과에 RPC 버전·Host·연결 ID·Core 메일함 Endpoint를 줍니다 |
| `health.get` | `ok`, RPC `0.1`, 원본 `b3557ff…`, Fork Commit(`build.rs`가 빌드 때 `git rev-parse HEAD`, 미커밋 변경이 있으면 `-dirty`), Host |
| `providers.list` | `claude`(세션 목록 위치가 있으면 사용 가능, 연결된 세션만 전송한다고 표시), `codex`(`TELOS_RN_CODEX_BIN`이 있을 때만), `agy`(P1, 불가). 확인하지 않은 버전은 적지 않습니다 |
| `sessions.discover` | 다음 세 종류를 모읍니다. 살아 있는 세션, 기억된 세션(`offline`), RN이 모르는 Claude 세션 목록의 세션(`attached: false`). Core 메일함은 넣지 않습니다. Provider·Host·Workspace로 거릅니다(Windows는 대소문자·구분자 무시) |
| `sessions.resolve` | Fingerprint와 Host를 확인한 뒤 해당 세션 |
| `sessions.capabilities` | 연결되지 않은 세션은 Tier 없음. 연결된 세션은 `pull`이고, Hook이 있으면 `turn`도 넣습니다. `push`는 실제 실행에서 검증하지 않았으므로 보고하지 않습니다(RN-05 한계). `canReply` = 연결됨, `canResume` = false |
| `messages.send` / `status` | RN-05의 Envelope 규칙에 더해, 연결되지 않은 세션은 `refused`입니다(결정 ③). RN이 아는 세션이면 `wired`로, 모르는 세션이면 Claude 세션 목록으로 판정합니다 |
| `messages.subscribe` (결정 ④·⑤) | 응답을 먼저 보내고 재생 이벤트, 그다음 새 이벤트를 보냅니다. 이벤트는 셋입니다. ① Core 발신자(Core 메일함 Fingerprint)로 보낸 Envelope의 Receipt 변경 ② Core 메일함으로 온 메시지(구독자에게 넘긴 시점에 `delivered`) ③ 연결 세션의 접속·변경·해제. 이벤트 생산은 0.5초 주기 또는 새 메시지 도착 시입니다. 최근 1,024개를 메모리에 두고 커서는 `<RN 시작 ms>:<순번>`입니다. 다른 시작의 커서나 보존 범위 밖 커서는 `resync_required`를 먼저 보내고 이어서 받습니다. 송신 대기열(1,024)을 넘기는 느린 연결은 닫습니다(커서로 재개) |
| `providers.agy.probe` | `refused`(P1) |
| 연결 여부 (`wired`) | RN MCP 서버로 등록됐거나 자기 Hook을 보낸 세션만 `true`입니다. 저장소에 남겨(스키마 2, Migration 전 백업) 오프라인이어도 유지합니다. `setup` 후 Daemon이 Claude 세션 목록에서 등록하는 세션(`claude_seen`)은 `false`입니다 |
| Core 메일함 | `telosweaver`(키 `telosweaver:core`, provider `telosweaver`)를 Daemon 시작 때 기억된 메일함으로 둡니다. 영구 보존이고, 다른 기계의 presence에는 알리지 않습니다. Core Envelope의 발신자 주소이며 에이전트 답장이 여기로 옵니다(RN-05 후속의 "답장 경로") |
| 임시 요청 정리 | RN-05의 원본 API 요청 `send_envelope`·`receipt`·`endpoint`와 숨은 `telos-rn call`을 지웠습니다. 시험용 숨은 명령 `telos-rn rpc <method> [params] [--listen 초]`가 `rpc.json`·토큰으로 RPC에 접속합니다. Windows는 Pipe 서버 PID가 `rpc.json`의 PID인지 확인합니다 |
| 기존 결함 수정 | PH0-RN-04에서 지운 `paths::link_legacy_socket`을 Unix 전용 `ipc.rs` 분기가 아직 불러 **Linux 빌드가 깨져 있었습니다**. 호출을 지웠습니다(Patch #29). Windows에서만 빌드해 와서 드러나지 않았습니다 |

## 3. 검증 (Windows 11, rustc 1.94.0, `rn-fork/`)

| 명령 | 결과 |
|---|---|
| `cargo build --locked` | 성공, 경고 0건. `Cargo.lock` 변화 없음(`windows-sys` 기능만 추가) |
| `cargo test` (격리 사용자 폴더) | 204개 통과 (RN-05의 196개 + 8개) |
| `cargo clippy --locked --all-targets -- -D warnings` | 경고 0건 |
| `cargo fmt --check` | 통과 |
| `cargo check --target x86_64-unknown-linux-gnu` | **실행 불가**: `ring` 빌드에 Linux용 C 컴파일러(`x86_64-linux-gnu-gcc`)가 없음. Unix 분기는 컴파일로 확인하지 못했습니다(WSL 구성 후) |

**추가한 시험 (8개)**

| 시험 | 확인 |
|---|---|
| `private::…::a_pipe_and_a_protected_file_allow_this_user_alone` | Pipe와 보호한 파일의 DACL 항목이 현재 사용자 Allow 하나뿐, 같은 이름의 첫 인스턴스 재생성 거부 |
| `rpc::…::the_rpc_takes_nothing_before_hello_and_only_this_engines_token` | `rpc.json`·토큰 형식, hello 전·틀린 토큰 `unauthenticated`, JSON 아님·버전·모르는 메서드 오류 뒤 연결 유지, hello 결과의 Core Endpoint, health·providers, agy `refused`, 새 연결은 다시 hello |
| `rpc::…::a_frame_over_the_limit_is_answered_then_the_connection_closes` | 1 MiB+1 → `frame_too_large`, 연결 닫힘 |
| `rpc::…::core_reaches_attached_sessions_only_and_hears_their_receipts_and_replies` | 연결된 메일함과 세션 목록에서만 등록된 Claude 세션의 `attached`, Core 메일함 제외, 필터, 위조 Fingerprint, Tier, 연결 안 된 세션 `refused`(Receipt 없음), Receipt 이벤트 `queued`→`delivered`, 답장 `message` 이벤트(발신자 Endpoint·`replyTo`)와 답장 `delivered`, 해제 presence |
| `rpc::…::a_cursor_resumes_without_repeats_and_one_it_cant_reach_gets_resync_first` | 커서 뒤부터만 재생하고 응답이 먼저, 다른 시작의 커서·보존 범위 밖 커서는 `resync_required`, 보존 1,024개 |
| `rpc::…::a_session_its_own_hooks_report_stays_attached_once_offline_and_one_from_the_registry_never_is` | Hook 세션은 오프라인이 돼도 `attached`, 세션 목록에서만 등록된 세션은 끝까지 `false` |
| `rpc::…::same_tokens_and_folders` | 토큰 비교, 폴더 비교 |
| `store::receipt_tests::schema_1_gains_whether_a_session_came_in_through_telos_rn_after_a_backup` | 스키마 1→2 백업(`network-v1-…`), 이전 세션은 `wired = false`, 한번 켜지면 유지 |

**바꾼 기존 시험 2개:** `presence_leaves_out_sessions_in_a_folder_that_is_off`는 주소 후보에 Core 메일함이 들어가도록 기대값을 바꿨습니다(에이전트가 답장할 수 있어야 함). `a_session_has_one_name_…`는 세션 칸이 12개가 되어 직접 넣는 값 수를 맞췄습니다.

시험 중 고친 결함: 같은 밀리초에 바뀐 `queued`·`delivered` 중 두 번째를 이벤트 생산기가 건너뛰었습니다(메시지 ID로만 중복 판단). 같은 시각 안에서는 (메시지, 상태, 사유)로 비교하게 고쳤고, RPC 시험을 3회 반복해 확인했습니다.

### 3.1 실환경 1: 격리 Daemon (모델 사용 없음)

`USERPROFILE`·`APPDATA`·`LOCALAPPDATA`·`TELOS_RN_HOME`을 임시 폴더로 두고, 메일함 `review`와 `telos-rn rpc`로 시험했습니다.

| 단계 | 결과 |
|---|---|
| 시작 | `rpc.json`: `\\.\pipe\telos-rn-rpc-7aa1…`, RPC `0.1`, PID. 토큰 64바이트, `icacls`: `PSYKI-HOME\Psyki-HOME:(F)`만 |
| `health.get` | `ok`, 원본 `b3557ff…`, Fork `38d095c…-dirty`(작업 중 빌드), Host `rn06-host`. hello 결과에 Core Endpoint(`telosweaver`/`core`) |
| `providers.list` | 격리 폴더라 Claude 세션 목록 없음 → `claude` 불가(사유 표시), `codex` 불가(`TELOS_RN_CODEX_BIN` 없음), `agy` P1 |
| `sessions.discover` / `capabilities` | `review` `attached: true`, Tier `turn`·`pull` |
| 구독 중 `messages.send` → 재전송 → review 읽음 → review가 원본 CLI로 `telosweaver`에 답장 | 이벤트 순서: `receipt queued` → `receipt delivered` → `presence busy` → `message`(답장, `replyTo: rn06-m1`, 발신자 Endpoint). 재전송은 같은 Receipt |
| 마지막 커서로 재구독(5초) | 응답만 오고 반복 이벤트 없음 |
| 재시작 | 토큰 바뀜. 이전 커서로 구독 → `resync_required`가 먼저 오고 새 이벤트가 이어짐. Receipt `delivered` 유지 |
| `providers.agy.probe` | `refused` |
| Daemon 로그 | Pipe 두 개 대기와 세션 등록만. 외부 접속 없음 |

### 3.2 실환경 2: 실제 Claude Code 세션 (haiku)

| 항목 | 값 |
|---|---|
| CLI·조건 | 확장 동봉 `claude.exe` 2.1.310, `haiku`, 저장소 밖 임시 폴더, `--mcp-config --strict-mcp-config`와 `--settings`(UserPromptSubmit Hook 하나)를 이번 실행에만 넘김, 허용 도구 `list_team`·`send_message`. 사용자 설정 변경 없음 |

| 단계 | 결과 |
|---|---|
| Pipe 권한(.NET `GetAccessControl`) | 원본 `telos-rn-0a22…`, RPC `telos-rn-rpc-0a22…` 둘 다 `Allow PSYKI-HOME\Psyki-HOME` 하나 |
| Turn 1 (`--session-id 88882f4f-…`) | 세션 `work-claude` 등록 |
| `sessions.discover {"provider":"claude"}` | 시험 세션 `attached: true`. **RN을 연결하지 않은 사용자의 실제 Claude 세션 2개**(이 작업 세션 `f20f1c8c…`, 이전 세션 `c9336d2c…`)가 `attached: false`로 보임 |
| `sessions.capabilities` | 시험 세션 `turn`·`pull`, 연결 안 된 세션 Tier 없음·`canReply: false` |
| 연결 안 된 세션(이 작업 세션)에 `messages.send` | `refused`, Receipt 없음(`messages.status` → `not_found`). 이 세션에 아무것도 오지 않았음 |
| 시험 세션(오프라인)에 `messages.send` | `queued` |
| Turn 2 (`--resume`) | Hook이 메시지를 보여 줌 → `delivered`(`shown to the recipient`). Claude가 `send_message(to="psyki-home/telosweaver", message="ack: RN-06 hello from TelosWeaver Core", reply_to="rn06-claude-m1")` |
| 구독 이벤트 | `receipt queued` → `receipt delivered` → `message`(발신자 = 시험 세션 Endpoint, `replyTo: rn06-claude-m1`) |
| 사용량 | Turn 1 $0.0039, Turn 2 $0.0057 |

### 3.3 검증 한계

- Linux·Remote SSH 빌드·실행은 확인하지 못했습니다. Unix 분기(Socket 0600, 토큰 0600)는 컴파일도 하지 못했습니다.
- `presence`의 busy/idle은 Hook·Claude 세션 목록이 알려 준 값입니다. `-p` 실행이 끝난 세션이 잠시 `busy`로 남는 것을 봤습니다(원본 수명 규칙: Hook만 있는 세션은 6시간 또는 프로세스 종료까지).
- 같은 OS 사용자의 다른 프로세스는 토큰 파일과 원본 API에 접근할 수 있습니다(SAFE-01 한계, 계약 문서에 명시).
- 원격 전송·Agy·push 전달은 범위 밖입니다. 자기 검토만 했습니다(PH0-RN-08에서 다른 검토자).

## 4. 시험 세션 (병합 시점 삭제 대상)

| Provider | 세션 | 위치 |
|---|---|---|
| Claude | `88882f4f-49ea-4ca8-a4e0-bb09a211fd6d` (Turn 1·2) | `~/.claude/projects/C--Users-Psyki-HOME-AppData-Local-Temp-claude-rn06-claude-tSkD-work` |

임시 폴더 `%TEMP%\claude\rn06-*`, `rniso.*`도 함께 지웁니다.

## 5. 후속

| 항목 | 넘긴 곳 |
|---|---|
| `telos-rn-client`(Rust) — `rpc.json`·토큰 읽기, hello, 시간 제한·제한된 재시도, 구독·커서·재동기화 처리 | PH0-RN-07 |
| Core가 RN을 시작·정지하고 `TELOS_RN_HOME`·`TELOS_RN_HOST_ID`·`TELOS_RN_CODEX_BIN`을 넘기는 연결, RN 장애 중 Degraded·Managed Run 독립성 | PH0-RN-07 · INTEGRATION |
| Linux 빌드·실행, 원본 E2E 정리, 외부 접속 관찰 | PH0-RN-08 (WSL 구성 필요) |
| 공통 계약 | REQ-018로 완료. 추가 요청 없음 |

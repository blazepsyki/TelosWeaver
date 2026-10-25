# PH0-RN-05 메시지·정체성·저장소

| 항목 | 값 |
|---|---|
| 서브 태스크·담당 | PH0-RN-05 · `claude-opus-5.5/session-c9336d2c` |
| 작성일 | 2026-10-25 |
| Branch / Worktree | `ph0/rn-05` / `D:\worktree\TelosWeaver\ph0-rn` |
| 기준 | Spec v1.5 §12.2·§12.4·§12.5·§19.5.10, 배정표 §6 "RN 메시지 의미", 계약 `telos_schema::rn`(`3a37919`, 변경 없음) |
| 사용자 결정 (2026-10-25) | ① `delivered`는 실제로 읽었을 때만 ② Fingerprint는 Workspace 제외 ③ TTL 상한·보존 7일 ④ Migration 때만 백업, 3개 유지 ⑤ 메시지 왕복은 메일함과 실제 Claude 세션 1개 ⑥ Codex CLI는 Core가 경로 전달. RN 주소 노출은 미룸 |

## 1. 정체성 (Endpoint)

| 필드 | 값 |
|---|---|
| `hostId` | `TELOS_RN_HOST_ID`(Core가 RN을 시작할 때 넘김, PH0-RN-07), 없으면 컴퓨터 이름 |
| `provider` | Core의 Provider ID. Claude Code는 `claude`, 나머지는 RN 에이전트 이름(`codex`, 메일함은 `cli`) |
| `nativeSessionId` | 에이전트가 붙인 세션 ID(RN 세션 키 `<agent>:<id>`의 뒷부분) |
| `fingerprint` | `blake3` 파생 키 해시(`telos-rn endpoint fingerprint v1`)로 `hostId`·`provider`·`nativeSessionId`를 길이와 함께 묶음. Workspace는 넣지 않음(결정 ②) |

- 프로세스로만 알려진 세션(`proc:<pid>`)은 Endpoint가 없습니다. 같은 PID를 다음 프로세스가 쓸 수 있기 때문입니다.
- Fingerprint는 세 필드에서 정해지므로, 막는 대상은 **주소·이름 재바인딩과 다른 Host로의 혼동**입니다. 같은 대화가 다른 폴더로 옮겨 가는 것은 막지 않습니다(결정 ②의 의미).
- Envelope 수신자의 Fingerprint가 세 필드와 맞지 않거나 `hostId`가 이 Host가 아니면 `identity_mismatch`로 거부합니다(RN-POC-11). 발신자 Fingerprint는 검사하지 않고 멱등 키의 범위로만 씁니다(발신자가 붙인 자기 이름).

## 2. Receipt 상태 전이

모든 메시지는 저장될 때 Receipt를 하나 가집니다(`receipts` 테이블). 최종 상태(`delivered`·`refused`·`expired`·`failed`)는 다시 바뀌지 않습니다. 메시지는 Receipt가 최종 상태가 아닐 때만 보여 줍니다.

| 상태 | 들어가는 경우 |
|---|---|
| `accepted` | 저장 직후(곧바로 다음 상태로 바뀜) |
| `queued` | 수신 세션이 오프라인, Hook·Inbox를 기다림, 또는 에이전트 자체 Inbox·Queue에 넘김(`handed off to …; not yet confirmed read`). 사유에 경로를 적습니다(결정 ①) |
| `held` | 사람 승인 대기: 정책 Hold, 또는 **다른 세션이 주소를 가져감**(`identity_mismatch: …`) |
| `delivered` | 수신자가 실제로 봄: Inbox가 돌려줌(`listen`, `inbox`, MCP `check_inbox` 후 `Delivered`), Hook이 출력한 뒤 `Delivered`, 또는 넘긴 메시지가 그 세션 Transcript에 나타남(`seen in the recipient session's transcript`) |
| `refused` | 루프 방지·정책 거부·파일 미지원, 사람이 Hold 메시지를 버림 |
| `expired` | 만료 시각까지 읽히지 않음(넘긴 메시지는 `expired before its reading was confirmed`) |
| `failed` | 수신 세션이 하루 넘게 사라져 잊힘(메시지는 지우고 Receipt는 남김) |

- **넘김과 읽음의 구분(결정 ①):** 원본은 Claude Code Inbox 소켓에 쓰거나 `codex queue`가 성공하면 읽음으로 표시했습니다. 이제 이 경우는 다시 보여 주지 않되 `queued`로 둡니다. MCP 서버는 `Delivered { handed_off: true }`로 알립니다. Daemon은 세션 Transcript(Hook이 알려 준 경로)를 2초마다 최대 15분 동안 읽고, 메시지 ID가 나타나면 `delivered`로 바꿉니다. Transcript를 모르면 `queued`로 남고, 만료되면 `expired`가 됩니다.
- **재바인딩(§12.2):** 메시지를 저장할 때 대상 세션 키를 기록합니다. 보낼 때 살아 있는 세션 키, 오프라인이면 그 이름을 가졌던 세션 키를 씁니다. 그 주소를 지금 가진 세션이 다른 키라면 보여 주지 않고 `held`로 돌립니다. 사람이 승인하면(`telos-rn approve`) 지금 그 주소를 가진 세션에게 갑니다. 원본은 기억되지 않는 세션(`proc:`, Hook으로만 본 일회성 실행)의 메일을 같은 이름을 다음에 가져간 세션에게 자동으로 주었는데, 이 동작을 바꾼 것입니다(Patch #21). 이름이 없던 주소로 간 메일은 처음 그 이름을 가진 세션에 묶입니다.
- **재시작 후 불명 상태:** 넘긴 뒤 확인되지 않은 Receipt는 Engine이 다시 열릴 때 `handed off before the engine restarted: whether its recipient read it is not confirmed`로 바뀝니다. `delivered`로 올리지 않고, 다시 보여 주지도 않습니다(중복 표시 방지). 넘기지 않은 메시지는 그대로 기다립니다.
- **원격 전송:** 다른 컴퓨터로 보낸 메시지는 상대가 답한 단계(`queued`·`held`·`refused`)까지만 기록합니다. 상대 세션이 읽었는지는 돌아오지 않으므로 `delivered`가 되지 않습니다(P2 원격 범위, Phase 0 미시험).

## 3. Envelope (`messages.send`·`messages.status` 전 단계)

정식 RPC(버전·요청 상관·구독)는 PH0-RN-06이 만듭니다. RN-05는 같은 의미를 기존 로컬 API에 임시 요청으로 붙였습니다.

| 요청 | 결과 |
|---|---|
| `{"op":"send_envelope","envelope":MessageEnvelope}` | `DeliveryReceipt` |
| `{"op":"receipt","message_id":…}` | `DeliveryReceipt` (`messages.status`) |
| `{"op":"endpoint","address":…}` | `EndpointRef` (`sessions.resolve` 전 단계) |

거부는 응답의 `code`에 `RnErrorCode` 문자열을 넣고, 오류 문구 앞에도 붙입니다. 시험용 숨은 명령 `telos-rn call '<json>'`이 요청 하나를 보내고 응답을 출력합니다.

| 검사 | 결과 |
|---|---|
| 같은 발신자 Fingerprint + 같은 멱등 키 + 같은 Envelope(필드 순서 무관, 정렬한 JSON의 해시) | 처음 Receipt를 그대로 돌려줌(메시지 하나) |
| 같은 키, 다른 내용 또는 다른 `messageId` | `idempotency_conflict` |
| 다른 키, 이미 쓰인 `messageId` | `invalid_params` |
| `expiresAt` 지남 / 7일 초과 / RFC 3339 아님 | `expired` / `invalid_params` / `invalid_params` (결정 ③) |
| 수신자 Fingerprint 불일치·다른 Host | `identity_mismatch` |
| 이 Host에 없는 세션 | `not_found` |
| Artifact 참조 | `refused`(미지원) |
| ID·키 형식(1~128자, 영숫자·`-_.:`), 빈 본문·64 KB 초과, 문자열이 아닌 `text` 본문 | `invalid_params` |

- 메시지·Receipt·멱등 키는 한 Transaction으로 저장합니다(일부만 남지 않음).
- Tier `pull`은 깨우지 않는 메시지(FYI)이고, `push`·`turn`은 깨우는 메시지입니다. 실제 전달 경로는 세션이 가진 경로를 따릅니다(`sessions.capabilities`는 RN-06).
- 발신자 Endpoint가 이 Host의 세션이면 그 주소로, 아니면 `<person>/telosweaver`로 표시합니다.

## 4. 저장·만료·보존·백업

| 항목 | 기준 |
|---|---|
| 만료 | Envelope는 `expiresAt`(7일 이내). 원본 경로 메시지는 **이 Engine에 저장된 때부터** 7일. 발신 측 시계는 믿지 않음 |
| 보존 | 최종 상태가 된 지 7일 지난 메시지·Receipt·멱등 키를 10분 주기 정리 때 삭제. 파일은 이어서 `sweep_files`가 정리 |
| 스키마 | `PRAGMA user_version` 1. 더 높은 버전의 DB는 열지 않음(내려 쓰지 않음) |
| 백업 (결정 ④) | 기존 DB를 Migration하기 직전에만 `VACUUM INTO`로 `~/.telos-rn/backups/network-v<이전>-<ms>.db`. 최근 3개 유지. 백업이 실패하면 Migration하지 않고 시작을 멈춤. 새 DB는 백업하지 않음 |
| 이전 메시지 | Migration 때 Receipt 생성. 원본에서 읽음 표시였던 메시지는 실제로 봤는지 기록이 없으므로 `queued`(`whether its recipient read it is not known`)로 두고 만료 후 정리 |

## 5. Codex CLI (결정 ⑥)

`codex queue`는 `TELOS_RN_CODEX_BIN`으로 받은 실행 파일만 씁니다(`agent::codex_bin`). `PATH`나 일반 설치 위치를 찾지 않습니다. 경로가 없으면 Codex 세션은 push 대상이 아니고(`hook_tier`가 `turn`), Hook으로만 받습니다. Core가 확장 동봉 Codex CLI(DEC-06)를 찾아 이 변수로 넘기는 연결은 PH0-RN-07·INTEGRATION이 합니다. 이번에 실제 Codex 전달은 시험하지 않았습니다.

## 6. 검증 (Windows 11, rustc 1.94.0)

| 명령 (`rn-fork/`) | 결과 |
|---|---|
| `cargo build --locked` | 성공, 경고 0건. `Cargo.lock`에는 `telos-schema`(경로 의존)만 추가 |
| `cargo test` (격리: `USERPROFILE`·`HOME`·`APPDATA`·`LOCALAPPDATA`·`TELOS_RN_HOME`을 임시 폴더로) | 196개 통과 (RN-04의 179개 + 17개) |
| `cargo clippy --locked --all-targets` | 경고 0건 |
| `cargo fmt --check` | 통과 |

**추가한 회귀 시험 (17개)**

| 시험 | 확인 |
|---|---|
| `telos::tests` 3개 | Endpoint 구성·Fingerprint 필드 분리·`proc:` 제외·Claude 별칭 거부, RFC 3339 왕복·잘못된 시각 거부, 상태 문자열이 계약과 같음 |
| `store::receipt_tests::a_message_is_delivered_only_once_its_recipient_was_shown_it` | 넘김은 `queued`·다시 안 보임, 읽음·Transcript 확인만 `delivered`, 최종 상태 불변, 같은 ID 재삽입 무시 |
| `…mail_past_its_expiry_is_expired_and_no_longer_shown` | 만료 메시지 미표시·승인 목록 제외, 넘긴 메시지의 만료 사유, 만료 후 읽음 표시 무시, 기본 만료는 저장 시각 기준 |
| `…a_final_receipt_goes_with_its_message_and_key_a_week_on` | 7일 전에는 유지, 지나면 메시지·Receipt·멱등 키 삭제, 열린 메시지는 유지 |
| `…an_envelope_is_stored_whole_or_not_at_all` | 키 충돌 시 아무것도 남지 않음, 같은 ID 재저장 없음 |
| `…after_a_restart_mail_handed_off_is_unconfirmed_and_the_rest_waits_as_it_was` | 파일 DB 재시작: 넘긴 메시지는 불명 사유의 `queued`, 대기 메시지는 그대로, 새 DB는 백업 없음 |
| `…a_migration_backs_the_database_up_first_and_keeps_the_last_three_copies` | 스키마 0 DB 4회 Migration: 백업 3개 유지, 백업이 Migration 전 상태임, 이전 읽음 메시지는 `delivered`가 아님, 새 버전 DB 거부 |
| `…mail_a_session_never_read_fails_when_it_is_forgotten_and_settled_mail_says_how` | 잊힌 세션의 메일 `failed`, 버림 `refused`, 승인 `queued` |
| `daemon::envelope::tests` 6개 | Endpoint로 전달·읽은 뒤 `delivered` / 재시도는 같은 Receipt·충돌 거부·발신자별 키 범위 / 정체성·Host·만료·형식·Artifact 거부와 아무것도 남지 않음 / 주소를 다른 세션이 가져가면 `held`, 승인 후 전달 / 오프라인 세션 Envelope는 그 세션이 돌아오면 전달 / 넘긴 메시지는 Transcript에 나타나면 `delivered`, 안 나타난 것은 `queued` |
| `agent::tests::the_codex_cli_is_only_the_one_core_passed_never_one_on_the_path` | 경로를 받지 않으면 없음, 없는 파일·폴더 거부 |

**바꾼 기존 시험 2개:** `queued_mail_says_when_it_may_be_dropped_and_who_gets_it`(오프라인 안내 문구가 "새 세션이 이름을 가져가면 승인 대기"로 바뀜), `a_claude_session_is_told_delivered_only_through_a_server_that_knows_its_inbox`의 `pending` 호출 형식(세션 키 인자 추가). 동작 기대값은 바꾸지 않았습니다.

### 6.1 실환경 1: 메일함 왕복 (모델 사용 없음)

격리 폴더(`USERPROFILE`·`APPDATA`·`LOCALAPPDATA`·`TELOS_RN_HOME`)에서 `telos-rn` 메일함 `alpha`·`beta`를 만들고 `telos-rn call`로 시험했습니다.

| 단계 | 결과 |
|---|---|
| Endpoint | `{"hostId":"rn05-host","provider":"cli","nativeSessionId":"beta","fingerprint":"624bb96e…"}` |
| Envelope alpha→beta (`rn05-m1`) | `queued` (FYI) |
| 같은 Envelope 재전송 | 같은 Receipt(`updatedAt` 동일) |
| 같은 키·다른 본문 | `idempotency_conflict: this idempotency key was used for another message` |
| beta가 `listen`으로 읽음 | `delivered`, `shown to the recipient` |
| beta가 원본 CLI 경로로 답장 | `queued` → alpha가 읽은 뒤 `delivered` |
| 읽지 않은 `rn05-m2`를 두고 `telos-rn stop` 뒤 재시작 | `rn05-m1` `delivered`, `rn05-m2` `queued` 그대로 |
| 다른 Host 주소 / 위조 Fingerprint | 둘 다 `identity_mismatch` |
| 상태 폴더 | `~/.telos-rn`에 `network.db`·`machine.key`·`daemon.log`·`daemon.lock`만. Migration이 없어 `backups/` 없음 |
| Daemon 로그 | Pipe 대기·세션 등록 줄만. 외부 접속 없음(팀 없음) |

시험 중 처음 두 번은 시험 쪽 문제로 다시 실행했습니다. ① Receipt 요청의 `id` 필드가 로컬 API 프레임의 요청 `id`와 겹쳐 Daemon이 요청을 해석하지 못했습니다. 필드를 `message_id`로 바꾸어 수정했습니다(코드 수정). ② 셸 스크립트가 여러 줄 JSON에서 ID를 뽑지 못했습니다(스크립트 수정). 중단한 첫 실행이 남긴 `telos-rn` Daemon 프로세스는 이 Worktree의 실행 파일인지 확인한 뒤 종료했습니다.

### 6.2 실환경 2: 실제 Claude Code 세션 (haiku)

| 항목 | 값 |
|---|---|
| CLI | VS Code 확장 `anthropic.claude-code-2.1.310-win32-x64`의 `claude.exe` 2.1.310 (DEC-06) |
| 조건 | 모델 `haiku`, 저장소 밖 임시 폴더, `--strict-mcp-config --mcp-config`(이번 실행만 `telos-rn` MCP 서버), `--settings`(이번 실행만 UserPromptSubmit Hook 하나), `--allowedTools mcp__telos-rn__list_team,mcp__telos-rn__send_message`, `--permission-mode default`. `telos-rn`의 상태는 임시 `TELOS_RN_HOME` |
| 사용자 설정 | **바꾸지 않음.** `setup`을 실행하지 않고 실행 인자로만 MCP·Hook을 넘겼으므로 Backup·Diff·복원할 대상이 없습니다(결정 ⑤에서 정한 "setup 적용 직전 확인"이 필요한 단계가 생기지 않음) |

| 단계 | 결과 |
|---|---|
| Turn 1 (`--session-id a645faf1-…`): `list_team` 한 번 | 세션 `psyki-home/work-claude` 등록·기억. Endpoint `{"provider":"claude","nativeSessionId":"a645faf1-0d44-4fad-b87f-e8cafa14f84d","fingerprint":"ec61057c…"}` |
| 오프라인 세션에 Envelope (`rn05-claude-m1`, tier `pull`) | `queued`, `it reaches that session when it's back` |
| Turn 2 (`--resume`) | UserPromptSubmit Hook이 메시지를 맥락에 넣음(Transcript에 `hookEvent":"UserPromptSubmit","content":"<reason-message … id=\"rn05-claude-m1\">` 기록). Receipt `delivered`, `shown to the recipient`(00:09:01.837Z, Turn 2 시작 직후) |
| Claude의 답장 | `send_message(to="psyki-home/alpha", message="ack: RN-05 hello from alpha", reply_to="rn05-claude-m1")`. alpha가 읽은 뒤 `delivered` |
| 사용량 | Turn 1 $0.0039, Turn 2 $0.0057 (`claude-haiku-5-5`) |

**처음 시도(실패, 기록만):** 세션 `87775e0e-df1d-4fae-ae63-c8b13487eee5`. `check_inbox`로 기다리게 하려 했습니다. 그러나 Claude Code가 세션 메시지 소켓을 열어 두어서 MCP 서버가 이 세션을 push 대상으로 보고 `check_inbox`를 숨겼습니다(원본 동작). 모델은 도구를 찾지 못하고 끝났고, 메시지는 보내지 않았습니다. 그래서 두 Turn 방식으로 바꿨습니다.

**관찰:** RN Daemon의 `who`에 시험과 무관한 실행 중인 Claude Code 세션(이 작업 세션)도 보였습니다. 원본의 Claude 세션 탐지가 `~/.claude/sessions`를 읽기 때문입니다(DEC-07에서 유지하기로 한 기능). 읽기만 했고 그 세션에는 아무것도 보내지 않았습니다.

### 6.3 검증 한계

- **push + Transcript 확인 경로**(MCP가 Claude Inbox에 넣고 Transcript로 `delivered`)는 단위 시험(가짜 Transcript)으로만 확인했습니다. 실제 Claude 실행에서는 Hook 경로를 썼습니다. 대화형 Claude Code 세션의 Inbox 전달은 P1(COMM-03)입니다.
- 실제 Codex 전달(`codex queue`)과 원격 전송은 시험하지 않았습니다.
- 외부 접속은 Daemon 로그와 팀이 없다는 조건으로만 확인했습니다. 네트워크 관찰(RN-POC-14)은 PH0-RN-08입니다.
- Linux·Remote SSH는 시험하지 않았습니다(대기 지시 유지).
- 자기 검토만 했습니다. 다른 검토자 확인은 PH0-RN-08에서 받습니다.

## 7. 시험 세션 (병합 시점 삭제 대상)

| Provider | 세션 | 위치 |
|---|---|---|
| Claude | `87775e0e-df1d-4fae-ae63-c8b13487eee5` (처음 시도) | `~/.claude/projects/C--Users-Psyki-HOME-AppData-Local-Temp-claude-rn05-claude-MDbU-work` |
| Claude | `a645faf1-0d44-4fad-b87f-e8cafa14f84d` (Turn 1·2) | `~/.claude/projects/C--Users-Psyki-HOME-AppData-Local-Temp-claude-rn05-claude-P2wc-work` |

임시 `TELOS_RN_HOME` 폴더(`%TEMP%\claude\rn05-*`, `rniso.*`)도 함께 지웁니다.

## 8. 후속

| 항목 | 넘긴 곳 |
|---|---|
| 임시 요청(`send_envelope`·`receipt`·`endpoint`)을 버전 RPC(`messages.send`·`messages.status`·`sessions.resolve`·`sessions.discover`)로 감싸기, `sessions.capabilities`가 실제 경로만 보고, 구독(`Receipt` 이벤트) | PH0-RN-06 |
| Core가 RN 시작 시 `TELOS_RN_HOST_ID`(Core의 `--host-id`)와 `TELOS_RN_CODEX_BIN`(확장 동봉 Codex CLI) 전달 | PH0-RN-07 · INTEGRATION |
| Core 발신자 Endpoint 표기(현재 `<person>/telosweaver`로 표시, 답장 경로 없음) | PH0-RN-07 |
| 대화형 세션의 push·Transcript 확인 실환경, 원본 E2E 정리, 외부 접속 관찰 | PH0-RN-08 (대화형 전달은 P1) |
| 공통 계약 변경 | 없음. 전달 경로는 `DeliveryReceipt.reason`으로 충분했습니다 |

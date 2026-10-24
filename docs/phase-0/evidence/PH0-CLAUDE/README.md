# PH0-CLAUDE 구현 및 검증 기록

| 항목 | 값 |
|---|---|
| 작업 ID·담당 | PH0-CLAUDE · `claude-opus-5.5/session-c9336d2c` |
| 작성·검증일 | 2026-10-24, Windows 로컬 (`windows-local`) |
| Branch / Worktree | `ph0/claude` / `D:\worktree\TelosWeaver\ph0-claude` |
| 기준 | `0b5b34f` (Spec v1.4, 계약 `271e7fd`) |
| 대상 CLI | `anthropic.claude-code` 2.1.310 (`win32-x64`) 확장의 `resources/native-binary/claude.exe`, `2.1.310 (Claude Code)` |
| 시험 조건 | 배정표 §6: 모델 `haiku`, 기본 `~/.claude`, 저장소 밖 일회용 폴더, 허용 도구 명시, `--dangerously-skip-permissions` 미사용 |

G1 대표 Provider 작업입니다(배정표 §6). 이 기록은 Adapter 단위의 실환경 결과이며 G1 판정이 아닙니다. G1은 Core 실행 감독·실행 승인과 연결한 경로에서 판정합니다.

## 1. 수정 파일

| 경로 | 내용 |
|---|---|
| `crates/adapters/src/claude.rs` | Adapter, `cli_rule`, `ClaudeLaunch`·`ClaudeLaunchResolver`, `ClaudeCodeConfig`, Capability 보고 |
| `crates/adapters/src/claude/protocol.rs` | Stream JSON 한 줄 → 공통 이벤트 Payload 변환 |
| `crates/adapters/src/claude/run.rs` | Run별 이벤트 보존·재생, 프로세스 감시·취소·종료 보고 |
| `crates/adapters/tests/claude_adapter.rs` | Mock 시험 13개 (실제 출력 Fixture 재생) |
| `crates/adapters/tests/claude_environment.rs` | 실환경 시험 6개 (`#[ignore]`, 명시 실행. 탐지 1개 포함) |
| `crates/adapters/tests/fixtures/claude/*.jsonl` | 실제 CLI 출력에서 경로·불필요 필드를 지운 Fixture 3개 |
| `crates/adapters/src/lib.rs` | `mod claude` → `pub mod claude` (CONTRACT 소유 파일, 같은 담당자가 처리. REQ-010의 Provider별 `cli_rule` 공개 규칙을 따름) |

## 2. 실행 방식과 결정

- **Run = 프로세스 하나 = Turn 하나.** `claude -p --input-format stream-json --output-format stream-json --verbose --strict-mcp-config --permission-mode <mode>`로 시작하고 프롬프트를 stdin의 Stream JSON 사용자 메시지로 보낸 뒤 바로 닫습니다. 프롬프트를 명령줄 인자로 넘기지 않습니다.
  - 근거: 공통 이벤트 규칙(REQ-006)상 전용 프로세스 Run에서 `turn_finished` 뒤에는 `usage`·`error`·`process_exited`만 올 수 있습니다. 실험에서 `--input-format stream-json`은 한 프로세스에서 여러 Turn을 처리했지만(`init`·`result`가 Turn마다 반복), 이를 한 Run에 담으면 규칙을 어깁니다.
  - 결과: `send_input`은 `unsupported`이며 이어지는 대화는 다음 Run의 `resume`으로 처리합니다.
- **권한은 시작 전에만 정합니다.** Print 모드에는 실행 중 승인 응답 경로가 없으므로 `answer_approval`은 `unsupported`입니다. `bypassPermissions`는 프로세스 시작 전에 `configuration` 오류로 거부합니다.
- **실행 조건은 Core가 정합니다.** `ClaudeLaunchResolver`가 승인된 Workspace 경로·프롬프트·모델·권한·도구 목록을 돌려줍니다. Adapter는 경로를 직접 정하지 않습니다(§7.2). 이 Port는 Claude 모듈 안의 타입이며 공통 계약이 아닙니다. 세 Provider가 같은 형태가 필요하면 CONTRACT 변경 요청으로 공통화합니다.
- **세션:** `fresh`·`capsule`은 새 세션, `resume`은 `--resume <id>`, `native_fork`는 `--resume <id> --fork-session`입니다.
- **사용량:** `result.usage`를 `delta`·`provider_reported`로 보냅니다. `total_cost_usd`는 구독 사용량이므로 비용으로 기록하지 않습니다(§15.2).
- **stderr:** 내용을 이벤트·로그에 복사하지 않습니다.

### 2.1 Stream JSON 대응 (2.1.310 관찰)

| 원본 | 공통 이벤트 |
|---|---|
| `system`/`init` (첫 번째) | `session_started` (`providerVersion`, `model`), 이후 이벤트의 `nativeSessionId` |
| `assistant` `content[].text` | `text_delta` (`assistant`) |
| `assistant` `content[].tool_use` | `tool_started` |
| `system`/`permission_denied` + `user` `tool_result` | `tool_completed` (`denied`) |
| `user` `tool_result` (`is_error`) | `tool_completed` (`failed`) |
| `result` | `usage`, (`is_error`이면 `error`), `turn_finished` |
| 프로세스 종료 | `process_exited` (종료 원인·자식 회수) |
| `rate_limit_event`, `system`/`thinking_tokens`, `thinking` 등 | 무시 |
| JSON이 아닌 줄, 상한 초과 줄, stdout 읽기 오류 | `error` (`protocol_parse`) 후 프로세스 종료. 정상 메시지로 해석하지 않음 |
| `result` 없이 종료 | `error` (`provider`) + `turn_finished` (`failed`) |
| 취소 | `turn_finished` (`cancelled`) + `process_exited` (`terminated_on_request`/`killed`) |

오류 분류: HTTP 401/403·`*auth*` → `authentication`, 404·`model_not_found`·`invalid_request` → `configuration`, 429·`rate_limit`·`max_turns` → `resource_limit`, 그 밖 → `provider`.

관찰 사항: `--disallowedTools`로 막은 도구는 권한 거부가 아니라 도구 목록에서 빠집니다(호출 시 "No such tool available" `tool_result` 오류 → `failed`). 허용 목록에 없는 도구는 `permission_denied`와 결과의 `permission_denials`로 보고됩니다(→ `denied`).

## 3. 시험

환경: Windows 11, rustc 1.94.0, Claude Code 2.1.310, 모델 `claude-haiku-5-5`.

| 명령·시험 | 기대 결과 | 실제 결과 | 종류 |
|---|---|---|---|
| `cargo test -p telos-adapters --test claude_adapter` | 13개 통과 | 13개 통과 | Mock (실제 출력 Fixture 재생) |
| `TELOS_CLAUDE_CLI=… cargo test -p telos-adapters --test claude_environment -- --ignored --test-threads=1` | 5개 통과 | 5개 통과, 35.9초 | 실환경 (`environment-tested`) |
| `cargo test --workspace --locked` | 전체 통과 | 75개 통과, 5개 무시(실환경) | Contract·Mock |
| `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo fmt --all --check` | 경고 없음 | 통과 | 정적 검사 |

### 3.1 실환경 결과 (PROV 대응)

| 시험 | PROV | 결과 |
|---|---|---|
| `env_completed_run` | 02·03·04·06 | `session_started` → `text_delta`("pong") → `usage` → `turn_finished`(`completed`) → `process_exited`(0, `exited`, 자식 회수 `true`). 이벤트 규칙 검사 통과 |
| `env_tool_denied` | 07 | `Read`만 허용 상태에서 `Write` 요청 → `tool_completed`(`Write`, `denied`), 파일 미생성 |
| `env_invalid_model_fails` | 06 | `error`(`configuration`) → `turn_finished`(`failed`, `api_error`) → `process_exited`(1) |
| `env_cancel_terminates_process_tree` | 08 | `session_started` 뒤 취소 → `turn_finished`(`cancelled`) → `process_exited`(`killed`, 1, 자식 회수 `true`). 취소 요청→종료 2.04초. 2초 유예 안에 정상 종료하지 않아 강제 종료됨 |
| `env_resume_and_fork` | 05·FORK-POC-01 | Resume: 같은 세션 ID(`c9c865af…`)로 이전 대화의 코드워드를 답함. Fork: 새 세션 ID(`45558394…`)로 같은 코드워드를 답함 |

| `env_discovery_finds_extension_cli` | 01 | HOST `NativeCliDiscoverer`(통합 `b0a58b4`) + `cli_rule("win32-x64")` → `found`: 출처 `extension`, 위치 `relative-path`, 폴더 결정 `metadata`, 확장 2.1.310(`win32-x64`), CLI `2.1.310 (Claude Code)`, SHA-256 기록. 버전 명령만 실행 |

위 다섯 실행 시험은 실행 파일을 환경 변수로 지정했고, PROV-01은 탐지 결과로 같은 파일을 찾는지 따로 확인했습니다. 확장이 2.1.309에서 2.1.310으로 업데이트된 상태였으며 버전 폴더 이름이 바뀌어도 상대 경로 규칙은 그대로였습니다. 탐지 결과 경로가 Windows 확장 길이 경로(`\\?\C:\...`) 형식으로 나오므로 표시·비교 시 정규화가 필요한지 HOST에 전달합니다.

### 3.2 Resume·Fork 판정

- PROV-05(Native Resume): 이 환경·버전에서 동작을 확인했습니다. 단 Resume은 원본 세션에 Turn을 추가하므로, 원본을 보존해야 하면 Fork를 씁니다.
- FORK-POC-01(Native Fork): 새 Native ID와 부모 대화 문맥 계승을 확인했습니다. 부모 기준 Turn 지정(`source_turn_id`)은 쓰지 않았고 미검증입니다.
- Capability는 아직 `unverified`로 보고합니다. 반복 시험·실패 경로(없는 세션 ID 등) 확인 후 `supported`로 올립니다.

## 4. 미구현·미검증·남은 일

| 항목 | 상태 |
|---|---|
| Core 연결 (G1) | `ExecutionSupervisor`에 Adapter 등록, 신뢰된 `RunAuthorizer`와 `ClaudeLaunchResolver` 구현이 필요합니다(INTEGRATION). |
| 탐지 결과 기록 | 탐지는 확인했습니다(§3.1). 탐지 결과를 Adapter 구성·Capability·Context Snapshot에 연결하는 일은 Core 연결(G1)에서 합니다. |
| 정상 종료 유예 | Windows에서 2초 유예 안에 정상 종료하지 않았습니다. 유예 시간·정상 종료 신호의 효과는 HOST 런타임과 함께 확인합니다. |
| Linux·Remote SSH | WSL 미설치로 미검증입니다. |
| PROV-09 모델·권한 Capability | 모델 지정(`haiku`)·권한 모드·도구 목록은 동작을 확인했습니다. 모델 목록 탐지는 하지 않습니다. |
| PROV-10 확장과 동시 사용 | P1. 이번 시험은 확장이 실행 중인 상태에서 같은 `~/.claude`를 썼고 충돌은 관찰하지 못했지만 체계적으로 시험하지 않았습니다. |
| 이벤트 보존 상한 | 메모리 보존이며 Run별 상한·`EventsUnavailable`을 아직 두지 않았습니다. |
| 시험 세션 Archive | §5 목록. 미처리 |

## 5. 시험 세션 (삭제 완료)

> 2026-10-24: 사용자 지시로 이 목록의 시험 세션을 모두 삭제했습니다. 확장 대화 목록에는 처음부터 보이지 않았습니다(작업 폴더가 임시 폴더라 Workspace별 목록에 나타나지 않음). 앞으로는 병합 시점에 삭제합니다(배정표 §6 "시험 세션 정리").

사용자 결정(배정표 §6)에 따라 시험 세션은 끝난 뒤 Claude Code 확장의 대화 목록에서 보이지 않도록 Archive합니다. 확장은 Archive 목록을 VS Code `globalState`의 `hiddenSessionIds`에 둡니다. VS Code 실행 중 외부에서 이 값을 바꾸면 덮어써질 수 있으므로, 확장 화면에서 처리하거나 VS Code를 닫은 상태에서 처리합니다.

시험은 모두 저장소 밖 임시 폴더(`%TEMP%\telos-claude-env\*`, 세션 scratchpad `claude-poc\w1~w5`)를 작업 폴더로 썼습니다. 세션 기록은 `~/.claude/projects/` 아래 해당 폴더 이름으로 저장됩니다.

| 세션 ID | 용도 |
|---|---|
| `77659984-9222-4a46-af0c-fe11f6978f20` | 형식 확인: 기본 응답 |
| `81f05110-1cc1-44e7-8fcd-1478190dafc7` | 형식 확인: 도구 사용·비활성 도구 |
| `84b71ca3-b957-4a16-b04b-7f73bab63937` | 형식 확인: 권한 거부 |
| `06b3177d-83c6-4eea-90d2-97a60db1099f` | 형식 확인: 잘못된 모델 |
| `b2f9a0f6-1978-42b1-a3ce-ab512219a24f` | 형식 확인: Stream JSON 입력 두 Turn |
| `71ef1c72-9de8-4ee8-841b-ec4aaa86f71d` | `env_completed_run` |
| `7b58a94b-b344-4147-bcfe-749d31da3e7f` | `env_tool_denied` |
| `66d53244-88fe-4d1a-a6d8-6b4ea00bda9a` | `env_invalid_model_fails` |
| `b513a09c-b022-48e8-85e8-77c16a55ca36` | `env_cancel_terminates_process_tree` |
| `c9c865af-c9de-42f4-9ee3-f0232809b788` | `env_resume_and_fork` 부모·Resume |
| `45558394-394f-4b3c-9b4a-c1b0561395b6` | `env_resume_and_fork` Fork |

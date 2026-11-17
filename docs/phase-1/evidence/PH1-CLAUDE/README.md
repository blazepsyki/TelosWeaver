# PH1-CLAUDE 검증 기록

> Spec §19.5.8 형식입니다. 지시는 [H0 뒤 병렬 구현 착수 지시](../PH1-INTEGRATION/H0_KICKOFF.md) §1·§3.8(공통 P1~P7, Claude L1~L4)입니다.
> 실행한 시험만 통과로 적었고 Mock 통과와 실환경 통과를 구분했습니다.

| 항목 | 값 |
|---|---|
| 작업 ID | PH1-CLAUDE (Claude Code Provider Adapter) |
| 담당 | 통합 담당(PH1-INTEGRATION)이 위임한 Claude 서브 에이전트(`claude-fable-5.2`) |
| Branch / Worktree | `ph1/claude` / `D:\worktree\TelosWeaver\ph1-claude` |
| 기준 Commit | `c5963c5` |
| 결과 Commit | 코드·시험 `a24ffed`, `c7ebf8a`. 이 문서는 그 다음 Commit |
| 계약 기준 | `bac970e` (H0 계약, Spec v1.5.11) |
| 작성·검증일 | 2026-11-16 ~ 2026-11-17, Windows 로컬(`windows-local`, Windows 11 Pro 10.0.26300) |
| 대상 CLI | `anthropic.claude-code` 2.1.325(`win32-x64`) 확장의 `resources/native-binary/claude.exe`, `2.1.325 (Claude Code)` |

착수 지시는 2.1.318을 기준으로 적혀 있지만 시험 시점에 설치된 확장은 2.1.325였습니다. 아래 실환경 결과는 모두 2.1.325의 결과입니다.

## 0. 계약 확인 (P1)

`crates/schema/src/{provider.rs, mcp.rs, judgment.rs, policy.rs, context.rs, events.rs}`와 Contract 시험(`mcp_contract.rs`·`judgment_contract.rs`·`policy_contract.rs`)을 읽었습니다. 계약 타입은 바꾸지 않고 구현할 수 있었습니다. 구현하면서 계약이 정하지 않은 점이 세 가지 있어 Adapter 안에서 정하고 §6에 변경 요청 초안으로 적었습니다.

| 계약 | 판단 | 비고 |
|---|---|---|
| `RunContext.taskMcp`, `TaskMcpBinding` | 그대로 구현 | 자격 파일 **경로를 MCP 서버 프로세스에 넘기는 수단**이 정해져 있지 않습니다. 환경 변수로 넘겼습니다(초안 A) |
| `ApprovalRequiredPayload.target`·`argsDigest`, `ProviderApprovalRequest` | 그대로 구현 | `action`에 쓸 낱말과 Digest 계산 규칙이 정해져 있지 않습니다. Codex Adapter의 기존 낱말과 `sha256:<hex>` 표기를 따랐습니다(초안 B) |
| `judge`, `CapabilityEvidence`, `PreflightCheck`·`PreflightReport` | 그대로 구현 | `judge`는 보고서 하나를 받으므로 기능마다 필요한 점검만 모은 보고서를 만들어 넘깁니다. 증거는 플랫폼으로만 고릅니다(§9) |
| `SessionRequest.capsuleVersionId`, `CapsuleContent` | 그대로 구현 | Capsule 본문은 `ClaudeLaunchResolver`가 돌려주는 프롬프트로 전달되므로 `TaskSpec`·`RunContext`를 바꿀 필요가 없습니다. Capsule 모드에서 `capsuleVersionId`가 필수인지는 계약에 없습니다(초안 C) |
| `AgentAdapter` Trait, `AdapterError` | 그대로 구현 | 모르는 승인 요청 ID에 쓸 전용 오류가 없어 Codex와 같이 `Failure`(`configuration`)를 씁니다 |

## 1. 수정 파일

`git diff --name-only c5963c5..HEAD` 기준이며 모두 독점 경로입니다.

| 경로 | 내용 |
|---|---|
| `crates/adapters/src/claude.rs` | 판정 기준 적용(`detect`), `ClaudeApprovals`, `with_approvals`·`with_evidence`·`preflight`, Task MCP 인자, Capsule 검사, `answer_approval` 연결 |
| `crates/adapters/src/claude/preflight.rs` (새 파일) | 실환경 증거 표(`builtin_evidence`), 사전 점검, 기능별 점검 보고서 |
| `crates/adapters/src/claude/mcp.rs` (새 파일) | Task MCP 주입 파일 작성·검사·정리, `TASK_MCP_CREDENTIAL_FILE_ENV` |
| `crates/adapters/src/claude/protocol.rs` | 승인 요청·철회·제어 요청 변환, 대상·Digest 계산, Task MCP 연결 상태 확인, 끊긴 Turn 분류 |
| `crates/adapters/src/claude/run.rs` | 승인 요청 보관과 응답, stdin 수명, 취소 때 `interrupt`, 주입 파일 정리 |
| `crates/adapters/tests/claude_adapter.rs` | Mock 시험 16 → 32개. 가짜 런타임이 사전 점검과 stdin 대기를 재현 |
| `crates/adapters/tests/claude_environment.rs` | 실환경 시험 9 → 17개 |
| `crates/adapters/tests/fixtures/claude/approval-{allow,deny,interrupt}.jsonl` | 2.1.325 실제 출력(경로 정리) |
| `crates/adapters/tests/fixtures/claude/task-mcp-call.jsonl` | 2.1.325에서 관찰한 Task MCP 도구 호출 출력을 옮겨 적은 것(사용량 값 없음) |
| `crates/adapters/tests/fixtures/claude/help-2.1.325.txt` | `claude --help` 출력 |
| `crates/adapters/tests/fixtures/claude/task_mcp_server.rs` | 시험용 stdio MCP 서버(도구 `telos_probe` 하나, 표준 라이브러리만 사용) |
| `docs/phase-1/evidence/PH1-CLAUDE/README.md` | 이 문서 |

## 2. 구현한 기능

### 2.1 Capability 판정 기준 (P2, L4)

- `VERIFIED_CLI_VERSIONS` 등 고정 버전 비교를 없애고 `telos_schema::judge(통과한 버전 목록, 현재 버전, 사전 점검)`로 정합니다. 대상 키는 `managed_run`·`event_stream`·`cancel`·`native_resume`·`native_fork`·`answer_approval`·`vscode_listing`입니다.
- 통과한 버전 목록은 `CapabilityEvidence` 형태의 증거(`preflight::builtin_evidence`)에서 같은 플랫폼·같은 키의 통과 기록만 고릅니다. Core가 기록한 증거는 `ClaudeCodeAdapter::with_evidence`로 더합니다.

| 플랫폼 | CLI | 날짜 | 근거 | 키 |
|---|---|---|---|---|
| `win32-x64` | 2.1.310 | 2026-10-24·25 | Phase 0 `PH0-CLAUDE` | 기본 3개, Resume·Fork, 목록 표시 |
| `linux-x64` | 2.1.310 | 2026-10-26 | Phase 0 통합 증거 §9.4 | 기본 3개, Resume·Fork, 목록 표시 |
| `win32-x64` | 2.1.318 | 2026-11-07 | H0 결정 문서 A1(통합 담당 재시험 기록) | 기본 3개, Resume·Fork, 목록 표시 |
| `win32-x64` | 2.1.325 | 2026-11-17 | 이 문서 §4.2 | 기본 3개, Resume·Fork, 목록 표시, `answer_approval` |

- 현재 버전은 탐지 때의 문자열이 아니라 `detect`가 `--version`을 다시 실행해 읽은 값입니다. 읽지 못하면 어떤 증거와도 맞지 않아 `unverified`입니다.
- `native_resume`·`native_fork`는 통과 증거가 있으면 `supported`이고(H0 D11), 특정 Turn 지정 불가 같은 제한은 `reason`에 적고 실행 전에 거부합니다. `supported`의 근거 수준은 `environment-tested`, `unverified`는 `contract-tested`입니다. 같은 major.minor의 다른 버전으로 인정한 경우에는 `reason`에 시험한 버전과 "사전 점검 통과"를 적습니다.
- `answer_approval`의 증거는 Adapter 경로 시험(`env_live_approval_allow_and_deny`)만 씁니다. CLI만 직접 확인한 Phase 0 시험은 근거로 쓰지 않습니다.
- `send_input`은 수단이 계약과 맞지 않아 버전과 관계없이 `unsupported`입니다(§2.6).

### 2.2 사전 점검 (P3)

`detect`가 모델을 호출하지 않는 실행 세 번으로 `PreflightReport`를 만듭니다. `ClaudeCodeAdapter::preflight`로 보고서만 받을 수도 있습니다.

| 실행 | 점검 |
|---|---|
| `claude --version` | `VersionOutput`: `<숫자.숫자.숫자…> (Claude Code)` 형식 |
| `claude --help` | `HelpContains` 14개: 옵션 정의 줄(두 칸 들여쓰기 뒤 `-`)에 있는 이름만 인정. 설명 속 언급은 세지 않음 |
| `claude -p --input-format stream-json … --resume 00000000-0000-4000-8000-000000000000 --fork-session` (stdin 없음) | `OptionAccepted` 15개: Adapter가 실제로 쓰는 옵션 전부 |

- 세 번째 실행은 없는 세션을 가리키므로 CLI가 옵션 해석 뒤 `No conversation found`로 끝납니다. 2.1.325에서 결과 줄이 `num_turns: 0`·사용량 0이고 `~/.claude/projects` 폴더 수가 그대로임을 확인했습니다(§4.2).
- CLI는 모르는 옵션(`error: unknown option '--x'`)과 받지 않는 값(`error: option '…' argument '…' is invalid`)을 `error:` 줄로 거부합니다. 그 줄이 가리키는 옵션을 실패로 적고, 한 번에 하나만 보고되므로 그 옵션을 빼고 다시 실행해 나머지를 확인합니다. 어느 옵션인지 알 수 없는 오류나 20초 시간 초과는 남은 옵션 전체를 실패로 둡니다.
- `--version`·`--help`를 붙인 실행으로는 모르는 옵션을 잡을 수 없었습니다(`claude --no-such-option --version`이 버전을 출력하고 0으로 끝남). 그래서 실제 실행 경로로 확인합니다.
- `--permission-prompt-tool`은 2.1.325 도움말에 정의 줄이 없습니다(`--permission-prompts` 설명에만 나옴). 도움말 점검에서 빼고 실행 점검으로만 확인합니다.
- 점검마다 필요한 기능을 정해 두고, `judge`에는 그 기능에 필요한 점검만 모은 보고서를 넘깁니다. `--fork-session`이 없어지면 `native_fork`만, `--resume`이 없어지면 `native_resume`·`native_fork`만, `--permission-prompt-tool`이 없어지면 `answer_approval`만 `unverified`가 됩니다. 모든 Run이 쓰는 옵션이 없어지면 전부 `unverified`입니다.
- 같은 버전의 증거가 있으면 `judge`는 사전 점검과 관계없이 `supported`를 돌려줍니다. 계약 그대로 두었습니다.

### 2.3 Task MCP Run 단위 주입 (P4, L1)

- `RunContext.taskMcp`가 `Some`이면 그 Run에만 `--mcp-config <파일>`을 붙입니다. `None`이면 인자가 이전과 같습니다.
- 파일은 자격 파일과 같은 디렉터리(Core 상태 디렉터리)에 `claude-mcp-<Run ID>-<번호>.json`으로 만듭니다. Workspace에는 쓰지 않습니다. Unix에서는 `0600`으로 만듭니다.
- 파일 내용은 `{"mcpServers": {"<서버 이름>": {"type": "stdio", "command", "args", "env": {"TELOS_TASK_MCP_CREDENTIAL_FILE": "<자격 파일 경로>"}}}}`입니다. Adapter는 자격 파일을 열지 않고 존재만 확인합니다. 자격 원문은 파일·인자·환경·stdin에 들어가지 않습니다.
- 주입한 서버의 도구만 `--allowedTools`에 `mcp__<서버 이름>` 규칙으로 더합니다. Core가 정한 허용 목록은 그대로입니다.
- 실행 전 거부(`configuration`): 서버 이름이 영숫자·`-`·`_`가 아니거나 `__`를 포함, 명령이 비어 있음, 자격 파일 경로가 상대 경로, 자격 파일 없음.
- 정리: 프로세스가 끝나면 `process_exited`를 내기 전에 파일을 지웁니다. 프로세스 시작 실패, stdin 쓰기 실패, 같은 키의 동시 시작 경합에서도 남지 않습니다.
- `system`/`init`의 `mcp_servers`에서 주입한 서버가 `connected`가 아니면 `error`(`configuration`)를 내고 Run을 끝냅니다. Task MCP를 요청한 Run이 서버 없이 진행하면 결과를 보고할 수 없기 때문입니다. CLI는 `-p`와 `--mcp-config`에서 첫 Turn 전에 서버 연결을 기다립니다(공식 문서 "Run Claude Code programmatically").

**`--strict-mcp-config` 판단: 계속 씁니다.** 2.1.325 실험(`env_strict_mcp_config_connects_only_the_injected_server`, 모델 호출 없음)에서 `--mcp-config`만 주면 주입한 서버 외에 사용자 설정에서 온 서버 1개가 함께 연결됐고, `--strict-mcp-config`를 함께 주면 주입한 서버만 연결됐습니다. Worker Run에 사용자·프로젝트의 MCP 서버가 섞이면 Run마다 도구 구성이 달라지고 Core가 허용하지 않은 도구가 보입니다. Adapter는 Phase 0부터 모든 Run에 `--strict-mcp-config`를 붙이고 있으며 그대로 둡니다.

### 2.4 Capsule 입력 (P5, SB-11)

- `SessionMode::Capsule`은 새 세션입니다. `sourceSessionId`가 있어도 `--resume`·`--fork-session`을 붙이지 않습니다.
- 본문은 Core의 `ClaudeLaunchResolver`가 돌려준 프롬프트를 stdin 메시지로 그대로 보냅니다. Adapter는 Capsule을 조립하지 않습니다.
- `fresh`와 다른 점: `capsuleVersionId`가 없으면 프로세스를 띄우기 전에 `configuration` 오류로 거부합니다. Capsule 참조 없는 Capsule 요청을 `fresh`처럼 조용히 실행하지 않기 위해서입니다(초안 C).

### 2.5 실시간 승인 (P6, L2, 이월 P1-2)

- `ClaudeCodeAdapter::with_approvals(ClaudeApprovals::Prompt)`로 만든 Adapter의 Run은 `--permission-prompt-tool stdio`로 시작하고 stdin을 열어 둡니다. 기본값(`PolicyOnly`)은 이전과 같습니다: 프롬프트를 쓰고 stdin을 닫으며, 승인이 필요한 도구는 CLI가 거부하고, `answer_approval`은 `UnsupportedCapability`(`answer_approval`, "not enabled")입니다.
- CLI의 `control_request`(`can_use_tool`)를 `approval_required`로 냅니다.

| 필드 | 값 |
|---|---|
| `requestId` | CLI의 `request_id` 그대로 |
| `action` | `Bash`·`PowerShell` → `command_execution`, `Write`·`Edit`·`MultiEdit`·`NotebookEdit` → `file_change`, `mcp__*` → `mcp_tool_call`, 그 밖 → `tool_use` |
| `summary` | `Claude Code requested permission to use <도구 이름>` |
| `answerable` | `true` |
| `target` | 인자의 `command`·`file_path`·`notebook_path`·`path`·`url` 중 처음 있는 값, 없으면 도구 이름 |
| `argsDigest` | `sha256:<hex>`. 인자 JSON을 키 순서로 정렬한 압축 표기의 SHA-256 |

- `answer_approval`: Allow는 `{"behavior":"allow","updatedInput":<요청 때의 인자>}`, Deny는 `{"behavior":"deny","message":…}`를 `control_response`로 씁니다. Allow가 실행시키는 인자는 이벤트의 `argsDigest`가 가리키는 인자입니다.
- 같은 요청에 같은 답을 다시 보내면 CLI에 다시 쓰지 않고 `Ok`, 다른 답이면 `IdempotencyConflict`, 모르는 요청 ID면 `Failure`(`configuration`, "unknown approval request"), CLI가 요청을 거두었거나 Run이 끝났으면 `Failure`(`configuration`, "no longer pending")입니다.
- Deny로 답한 도구와 CLI가 거둔 요청의 도구는 `tool_completed`(`denied`)로 보고합니다. CLI는 이 경로의 거부에 `permission_denied`를 보내지 않고 `is_error` 결과만 보내기 때문입니다.
- `result`를 받으면 stdin을 닫아 CLI가 끝나게 합니다. 답을 기다리는 요청은 64개까지이며 넘으면 `resource_limit`로 Run을 끝냅니다. Adapter는 기한을 두지 않습니다. Core가 기한 안에 답하거나 Run을 취소해야 합니다.
- **응답 전 취소:** stdin만 닫으면 CLI가 대기 중인 요청을 버린 뒤 모델을 한 번 더 불러 Turn을 `completed`로 마쳤습니다(2.1.325 실환경, 취소→종료 1.96초). 그래서 취소 때 먼저 `interrupt` 제어 요청을 보냅니다. CLI는 요청을 거두고 `result`(`terminal_reason: aborted_tools`)로 Turn을 끝내며 모델을 다시 부르지 않습니다(취소→종료 0.82초). 이 Turn은 `turn_finished`(`cancelled`)로 보고하고 Provider 오류 이벤트를 붙이지 않습니다.
- Adapter가 처리하지 않는 제어 요청에는 오류 응답을 써서 CLI가 기다리지 않게 합니다. 승인 경로가 없는 Run에 승인 요청이 오면 답할 수 없으므로 `protocol_parse` 오류로 Run을 끝냅니다.

### 2.6 진행 중 조정 검토 (L3, 이월 P1-3, SB-06)

**판정: `send_input`은 계속 `UnsupportedCapability`이고 후속 입력은 Resume Run입니다.**

- 문서: Agent SDK "Streaming Input"은 "Queued messages: send multiple messages that process sequentially, with ability to interrupt"라고만 적습니다. 같은 Turn에 반영한다는 보장이나 그런 옵션은 없습니다.
- 실험(2.1.325, `--input-format stream-json --replay-user-messages`): 첫 메시지의 Turn이 진행되는 동안 두 번째 메시지를 보냈습니다.

| 상황 | 결과 |
|---|---|
| 뒤에 도구 호출이 남아 있음(`sleep 3` 두 번 중 첫 호출 직후 전송) | 첫 도구 결과 바로 뒤에 두 번째 메시지가 처리 시작(`command_lifecycle` `started`)되어 **같은 Turn**에 반영. `result` 1개, 마지막 응답에 지시가 반영됨 |
| 뒤에 도구 호출이 없음(도구 없이 1~120 출력 중 전송) | 첫 `result` 뒤에 두 번째 메시지가 **새 Turn**으로 시작. `init`·`result`가 한 번 더 나옴. `result` 2개 |

- 같은 Turn에 들어가는 수단은 있지만 어느 쪽이 될지는 도착 시점과 남은 도구 호출에 달려 있고 보내는 쪽이 정할 수 없습니다. 새 Turn이 되면 Run당 Turn 하나(§7.3)에 어긋나 Adapter가 프로세스를 끝내야 하고, 끝난 Turn의 Run이 요청 종료로 보고됩니다. Phase 0(2.1.310)의 "메시지마다 새 Turn"은 두 번째 상황에 해당합니다.
- 지원하려면 계약에 "입력이 같은 Turn에 반영되지 않았을 때의 보고"가 필요합니다. 이 작업에서는 요청하지 않았습니다.

## 3. Stub·미지원·미검증

| 항목 | 상태 | 사유·재확인 |
|---|---|---|
| `send_input` | 설계상 미지원 | §2.6. `env_mid_turn_message_joins_the_turn_only_at_a_tool_boundary`로 재확인 |
| 특정 Turn 기준 Resume·Fork(`sourceTurnId`) | 미지원 | 2.1.325 `--help`에 Turn 지정 옵션 없음(`--resume`, `--fork-session`, `--session-id`뿐). 실행 전 거부 유지 |
| Linux의 새 기능 전부(사전 점검, Task MCP, Capsule, 실시간 승인) | 미검증 | `psyki-nas` 공용 Clone이 없어 실행하지 못함. Linux에서 `answer_approval`은 증거가 없어 `unverified`로 보고됨 |
| Linux·2.1.325의 기존 기능 | 미검증(판정은 `supported`) | 실환경 시험은 2.1.310뿐. 같은 major.minor + 사전 점검 통과 규칙으로 `supported`가 됨. Linux에서 사전 점검 자체도 실행하지 못함 |
| 실제 Task MCP 서버(`telos-weaver` stdio 하위 명령) 연결 | 미검증 | 서버가 아직 없어 시험용 서버로 확인 |
| 연결에 실패하는 MCP 서버의 실환경 동작 | Mock만 | `init` 출력의 상태를 `failed`로 바꾼 Fixture로 확인 |
| 승인 요청 여러 개 동시 대기, 처리하지 않는 제어 요청 응답 | Mock만 | 실환경에서는 요청 1개만 확인 |
| 승인 방식의 Run 단위 전환 | 미구현 | 지금은 Adapter 단위(`with_approvals`). `ClaudeLaunch`에 필드를 더하면 `core/integration/policy.rs`의 구조체 리터럴이 깨지므로 하지 않음(§6 인계) |
| 승인 요청 철회를 Core에 알리는 이벤트 | 없음 | 공통 이벤트에 해당 종류가 없음. Core는 `answer_approval` 실패나 Run 종료로 알게 됨 |

## 4. 시험

- 버전: Claude Code CLI·확장 2.1.325(`win32-x64`), 모델 `haiku`(`claude-haiku-5-5`), rustc·cargo 1.100.0
- 설정 범위: 기본 `~/.claude`(읽기만), `--setting-sources user`, 저장소 밖 임시 폴더, `--dangerously-skip-permissions`·`bypassPermissions` 미사용
- 실행 셸: PowerShell. 이 Host의 Git Bash에서는 `link.exe`가 Git의 coreutils로 잡혀 링크가 실패합니다
- 기록: `%TEMP%\telos-claude-env-run1.log`(1차), `%TEMP%\telos-claude-env-run2.log`(최종), `%TEMP%\telos-claude-ws-test.log`

### 4.1 공통 검사와 Mock

| 명령 | 날짜 | OS/Host | 기대 결과 | 실제 결과 | 종료 상태 | 종류 |
|---|---|---|---|---|---|---|
| `cargo test --workspace` | 2026-11-17 | Windows 11 / `windows-local` | 전체 통과 | 46개 묶음, 245개 통과, 0개 실패, 50개 제외(실환경 `#[ignore]`) | 0 | Contract·Mock |
| `cargo clippy --workspace --all-targets -- -D warnings` | 2026-11-17 | 같음 | 경고 없음 | 경고 없음 | 0 | 정적 검사 |
| `cargo fmt --all --check` | 2026-11-17 | 같음 | 차이 없음 | 차이 없음 | 0 | 정적 검사 |
| `cargo test -p telos-adapters --test claude_adapter` | 2026-11-17 | 같음 | 32개 통과 | 32개 통과 | 0 | Mock(실제 출력 Fixture 재생, 가짜 CLI의 사전 점검 응답) |
| `cargo test -p telos-adapters --lib claude` | 2026-11-17 | 같음 | 5개 통과 | 5개 통과(사전 점검 해석 3, 증거 표 1, 시각 1) | 0 | 단위 |

Mock 시험이 고정하는 것:

| 항목 | 시험 |
|---|---|
| P2 | `detect_reports_tested_versions_as_supported`, `detect_accepts_the_same_minor_line_only_with_a_passing_preflight`, `detect_uses_the_version_the_cli_reports_now`, `recorded_evidence_extends_the_builtin_evidence`, `detect_on_another_host_or_without_a_cli` |
| P3 | `preflight_report_lists_every_check_without_calling_the_model`, `removed_option_makes_dependent_capabilities_unverified`(옵션이 없어진 CLI, `default` 권한 값을 받지 않는 CLI) |
| P4·L1 | `task_mcp_is_injected_for_that_run_only_and_cleaned_up`, `task_mcp_file_is_removed_when_the_run_ends_or_fails_to_start`, `task_mcp_server_that_is_not_connected_fails_the_run` |
| P5 | `capsule_starts_a_new_session_with_the_capsule_body` |
| P6·L2 | `live_approval_reports_target_and_digest_and_returns_allow`, `live_approval_deny_is_returned_and_marks_the_tool_denied`, `cancel_before_the_answer_closes_the_approval_request`, `cancel_interrupts_the_turn_that_waits_for_an_approval`, `withdrawn_and_unsupported_control_requests`, `approval_request_without_live_approval_ends_the_run`, `unsupported_controls_and_unknown_runs_are_explicit` |

### 4.2 실환경

명령(2026-11-17, `windows-local`, CLI 2.1.325):

```text
$env:TELOS_CLAUDE_CLI = "<확장>\resources\native-binary\claude.exe"
cargo test -p telos-adapters --test claude_environment -- --ignored --test-threads=1 --nocapture
```

최종 실행: **17개 통과, 0개 실패, 112.77초, 종료 상태 0.** 세션 ID는 최종 실행의 것입니다.

| 시험 | 항목 | 기대 결과 | 실제 결과 | 세션 ID |
|---|---|---|---|---|
| `env_preflight_passes_and_capabilities_follow_the_evidence` | P2·P3 | 점검 전부 통과, 모델 호출 없음, 판정 기준대로 상태 | 점검 30개 통과(1.1초, 첫 실행 4.1초). 프로젝트 폴더 수 26 → 26. 7개 키 `supported`, `send_input` `unsupported` | 없음 |
| `env_task_mcp_tool_is_called_only_in_the_injected_run` | P4·L1 | 주입한 Run에서만 도구 호출, 사용자 설정 불변, 파일 정리 | `mcp__telosweaver-ph1-probe__telos_probe` 성공, 응답 문구 반환. 서버 기록 `call tool="telos_probe" credential_env=set credential_readable=true credential_bytes=24` 1건. 주입 파일 없음, 자격 파일 그대로. 자격 원문이 이벤트·세션 기록에 없음. Workspace에 `.mcp.json` 없음. `taskMcp` 없는 Run은 `NO-TOOL`, 서버 미시작. 사용자 설정의 MCP 관련 내용 전후 동일(446바이트 비교), 서버 이름이 사용자 설정에 없음 | `34abfde7-a46f-4aa5-ba4e-145579ce5009`, `67e22e0e-80fb-4e9b-a46e-c1a61ab1f5d8` |
| `env_strict_mcp_config_connects_only_the_injected_server` | L1 | strict에서 주입한 서버만 연결 | strict: 주입 서버 `connected`, 그 밖 0개. strict 없음: 주입 서버 `connected`, 그 밖 1개. 모델 호출 없음. 사용자 설정 전후 동일 | 없음 |
| `env_capsule_is_a_new_session_without_the_previous_conversation` | P5 | 새 세션, 본문 전달, 이전 대화 미사용 | 새 세션 ID. 응답에 Capsule의 값(`teal`)과 `NO-CODEWORD`가 있고 이전 세션의 코드워드 없음 | 이전 `d5214b9b-2124-418b-b7c5-30e0222dda83`, Capsule `7aa60392-9d80-417e-a05a-edce89a0c0f2` |
| `env_live_approval_allow_and_deny` | P6·L2 | 승인 요청 이벤트(대상·Digest), Allow 실행, Deny 차단, 모르는 ID·중복 응답 | 요청: `file_change`, 대상 `…\live-approval-allow\probe.txt`, `argsDigest` `sha256:` + 64자. 응답 전 파일 없음. 모르는 ID 거부, Allow 뒤 파일 내용 `OK`·`tool_completed`(`succeeded`)·종료 코드 0 자연 종료. 같은 답 반복 `Ok`, 다른 답 `IdempotencyConflict`. Deny: 파일 없음, `tool_completed`(`denied`) | `e5dc0632-c843-4623-bf95-97c63693963c`, `ed498285-b84e-4dc8-a8c9-937d4b4e6b74` |
| `env_live_approval_cancel_before_answer` | L2 | 도구 미실행, 프로세스 종료, 이후 응답 거부 | `tool_completed`(`denied`) → `turn_finished`(`cancelled`, `aborted_tools`) → `process_exited`(자식 회수 `true`). 파일 없음. 취소→종료 0.82초. 이후 응답은 `configuration` 오류 | `ffd3fbed-2ec2-436a-8f18-8ce0e2f169c4` |
| `env_mid_turn_message_joins_the_turn_only_at_a_tool_boundary` | L3 | §2.6의 두 상황 | 도구 있음: Turn 1개, 같은 Turn에 반영. 도구 없음: Turn 2개 | `92cf414f-c861-4318-bff6-30b482f4fdf9`, `f0e49ef8-a24d-4551-afd9-1fe473f4d9d7` |
| `env_send_input_is_rejected_while_the_run_continues` | L3 | 거부되고 Run은 그대로 완료 | `UnsupportedCapability`(`send_input`), Run `completed` | `fb3b273e-0913-4a71-9479-ee5933769c76` |
| `env_completed_run` | L4 | Phase 0과 같음 | 통과 | `4d835888-481c-4a47-8c27-57137b9b389c` |
| `env_background_tasks_do_not_continue_the_run` | L4 | Turn 1개, 자연 종료 | 통과 | `2eb17372-06b4-48d0-991d-d11c5daab64e` |
| `env_tool_denied` | L4 | `Write` 거부, 파일 없음 | 통과 | `30f23f1a-17b0-4cdd-b40a-02415eaecdba` |
| `env_invalid_model_fails` | L4 | `configuration` 오류, 실패 Turn | 통과 | `00346d90-2ec8-43af-9df4-9a555ea18cfc` |
| `env_cancel_terminates_process_tree` | L4 | 취소 뒤 종료·자식 회수 | 통과. 취소→종료 2.07초 | `7c583170-abc5-47f4-babc-4b88a3445edb` |
| `env_resume_and_fork` | L4 | Resume은 같은 세션, Fork는 새 세션 | 통과 | 부모·Resume `127cf1b4-f9b1-4d8c-b272-783a88b3c5f4`, Fork `9973a348-0a7f-4298-ab0b-1c0f9ee8e96d` |
| `env_vscode_listing` | L4 | `listed`·`hidden` 기록 | 통과 | `3b4f55c7-1ddd-4956-b24f-b99c92371281`, `62b4c12a-9b99-4544-a999-1a76c39abda5`, `b9f41683-0a6c-43f1-b144-bd7e2f66ad1d` |
| `env_permission_prompt_tool_offers_live_approval` | L4 | CLI 직접 실행의 Allow·Deny | 통과 | `82282965-5a88-4dcc-a5ac-f603c95df2cd`, `ce49bbb0-ac37-4cc6-898b-312a2f96d9f9` |
| `env_discovery_finds_extension_cli` | L4 | 확장에서 CLI 탐지 | 통과(버전 명령만 실행) | 없음 |

실환경 시험은 CLI 버전 문자열을 고정하지 않고 `--version` 값을 Adapter에 넘깁니다. 작업 폴더는 `%TEMP%`의 짧은 이름(`PSYKI~1`)을 풀어 씁니다(§9의 관찰 4).

### 4.3 실험 (시험 코드 밖, 2026-11-16, 2.1.325)

Adapter를 고치기 전에 CLI를 직접 실행해 Protocol을 확인했습니다. 결과는 Fixture와 위 시험에 반영했습니다.

| 실험 | 결과 | 세션 ID |
|---|---|---|
| 옵션 수용 확인 방법(`--version`·`--help`·프롬프트 없는 실행), 모델 호출 없음 | §2.2 | 없음 |
| 실시간 승인 Allow·Deny(`initialize` 없이) | `initialize` 제어 요청 없이 동작. 요청 필드 확인 | `8df4a250-3c2f-4862-a792-813617d55954`, `682d64f0-2055-434d-bb7e-d3af28a11250` |
| 승인 대기 중 `interrupt` | `control_cancel_request` → `result`(`aborted_tools`), 11ms, 모델 재호출 없음 | `e3afc6af-cb09-4a22-a112-f66ac630decb` |
| 진행 중 메시지 3회 | §2.6 | `49ca397a-aa05-466b-8683-300b7c004d6d`, `4028575f-7ca5-45c6-b3a9-e9134c78d99b`, `1d3fef98-73c6-4777-8631-70d19e77e677` |
| `--mcp-config` + strict 도구 호출 | `--tools ""`에서도 `mcp__<서버>` 규칙으로 MCP 도구 호출 | `dfa979a0-75e3-469c-8b52-2e23fc9342a9` |
| strict 유무의 `mcp_status`, 모델 호출 없음 | §2.3 | 없음 |

## 5. 반례 결과

| 반례 | 결과 | 종류 |
|---|---|---|
| 옵션이 없어진 CLI(`--fork-session`, `--resume`, `--permission-prompt-tool`, `--strict-mcp-config`) | 그 옵션이 필요한 기능만 `unverified`, 사유에 옵션 이름 | Mock |
| `--permission-mode default`를 받지 않는 CLI | 7개 키 `unverified`, 사유 `--permission-mode default not accepted` | Mock |
| major.minor가 다른 버전(2.2.0, 9.9.9) | `unverified`, 근거 수준 `contract-tested` | Mock |
| `--version` 출력이 형식과 다름, 탐지 뒤 다른 버전을 보고 | `unverified`("could not be read"), 보고된 버전으로 판정 | Mock |
| 다른 플랫폼의 증거, 실패한 시험의 증거 | 근거로 쓰지 않음 | Mock |
| 잘못된 `taskMcp`(서버 이름, 상대 경로, 없는 자격 파일) | 프로세스를 띄우지 않고 `configuration`, 파일 없음 | Mock |
| Task MCP Run의 프로세스 시작 실패 | `start_failed`, 주입 파일 없음 | Mock |
| 주입한 서버가 `failed`이거나 목록에 없음 | `error`(`configuration`) → 실패 Turn → 프로세스 종료, 도구 이벤트 없음 | Mock |
| `capsuleVersionId` 없는 Capsule | 프로세스를 띄우지 않고 `configuration` | Mock |
| 모르는 승인 요청 ID | `configuration`, CLI에 쓰지 않음 | Mock·실환경 |
| 같은 요청 중복 응답(같은 답 / 다른 답) | `Ok`(다시 쓰지 않음) / `IdempotencyConflict` | Mock·실환경 |
| 응답 전 Run 취소 | 도구 미실행, `cancelled`, 이후 응답 `configuration` | Mock·실환경 |
| CLI가 거둔 요청에 응답 | `configuration`("no longer pending") | Mock |
| 승인 경로가 없는 Run에 승인 요청 | `error`(`protocol_parse`) 뒤 종료, 승인 이벤트 없음 | Mock |
| 승인을 켜지 않은 Run의 `answer_approval`, 모르는 Run | `UnsupportedCapability`(`answer_approval`) / `UnknownRun` | Mock |
| `send_input` | `UnsupportedCapability`(`send_input`) | Mock·실환경 |
| 1차 실환경 실행에서 `env_live_approval_cancel_before_answer` 실패 | 취소 뒤 Turn이 `completed`로 끝남 → `interrupt` 전송으로 고친 뒤 통과(§2.5) | 실환경 |

## 6. 변경 요청과 인계

독점 경로 밖의 파일은 수정하지 않았습니다. 아래는 요청 ID를 받지 않은 초안이며 통합 담당이 판정합니다. 세 항목 모두 지금 구현을 막지는 않습니다.

### 초안 A — Task MCP 자격 파일 경로의 전달 수단

- **필요한 것:** `telos-schema`(`mcp.rs`)에 상수 `TASK_MCP_CREDENTIAL_FILE_ENV = "TELOS_TASK_MCP_CREDENTIAL_FILE"`와, `TaskMcpBinding.credentialFile` 문서에 "Adapter는 MCP 서버 프로세스의 이 환경 변수로 경로를 넘긴다"는 규칙.
- **이유:** 계약은 경로를 Adapter에 넘기는 것까지만 정합니다. 서버 프로세스가 경로를 받는 수단(환경 변수·인자)이 없으면 세 Provider와 PH1-MCP의 stdio 하위 명령이 서로 다르게 구현할 수 있습니다. 환경 변수는 Claude의 MCP 설정, Codex의 `mcp_servers`, Antigravity의 `mcp_config.json`이 모두 받습니다.
- **영향 작업:** CONTRACT, MCP, CODEX, CLAUDE, AGY.
- **시험 계획:** `mcp_contract.rs`에 상수 값 고정. 각 Adapter Mock에서 주입 설정의 환경 변수 확인. MCP 하위 명령이 그 변수로 자격을 읽는 시험.
- **지금 상태:** Claude Adapter는 같은 값을 `telos_adapters::claude::mcp::TASK_MCP_CREDENTIAL_FILE_ENV`로 씁니다. 승인되면 계약 상수로 바꿉니다.

### 초안 B — 승인 요청의 `action` 낱말과 `argsDigest` 계산 규칙

- **필요한 것:** `ApprovalRequiredPayload.action`에 쓰는 낱말 목록(지금 쓰이는 값: Codex `command_execution`·`file_change`·`permissions`, Claude가 더한 `mcp_tool_call`·`tool_use`)과, 이 값이 `PermissionSet.masterApprovableActions`·`ApprovalAction.action`(`validation.rs` 예시 `provider-tool`·`workspace-write`)과 어떻게 대응하는지. `argsDigest`의 표기(`sha256:<hex>`)와 계산 대상(키 정렬 압축 JSON).
- **이유:** `ProviderApprovalRequest::route_for`는 `action` 문자열로 경로를 정합니다. Provider마다 낱말이 다르면 같은 행위가 다른 경로로 갑니다. Core가 실행 직전에 Digest를 다시 비교하려면 계산 규칙이 같아야 합니다.
- **영향 작업:** CONTRACT, CODEX(C2), CLAUDE, MASTER, VALIDATION, HOST.
- **시험 계획:** `policy_contract.rs`에 낱말과 Digest 예시 고정. 각 Adapter Mock에서 같은 인자의 Digest 일치 확인.

### 초안 C — Capsule 모드에서 `capsuleVersionId`의 필수 여부

- **필요한 것:** `SessionRequest` 문서와 Contract 시험에 "`capsule` 모드는 `capsuleVersionId`가 있어야 한다"를 명시하거나, 필수가 아니라는 결정.
- **이유:** Claude Adapter는 없으면 실행 전에 거부합니다. 다른 Provider가 받아들이면 같은 요청의 결과가 Provider마다 달라집니다. 필수가 아니라고 정하면 `command_args`의 검사 한 곳을 지웁니다.
- **영향 작업:** CONTRACT, CONTEXT, SCHED, 세 Provider.
- **시험 계획:** `context_contract.rs` 또는 `judgment_contract.rs`에 규칙 고정.

### 인계

1. **실시간 승인 켜기(INTEGRATION·HOST):** `core/integration/mod.rs`에서 `ClaudeCodeAdapter::new(…).with_approvals(ClaudeApprovals::Prompt)`. 켠 뒤에는 `approval_required`마다 Core가 기한 안에 답하거나 Run을 취소해야 합니다. CLI는 답을 무기한 기다립니다. Run 단위로 바꾸려면 `ClaudeLaunch`에 필드를 더하고 `core/integration/policy.rs`의 리터럴을 함께 고쳐야 합니다.
2. **Task MCP(MCP·HOST):** `start_run` 시점에 `credentialFile`이 절대 경로의 존재하는 파일이어야 합니다. 주입 파일은 그 디렉터리에 만들므로 쓸 수 있어야 합니다.
3. **판정(SCHED·HOST):** `detect`는 CLI를 세 번 실행합니다(1~4초). Core 시작과 버전 변화 때 부르는 것을 전제로 했습니다. `preflight()`로 보고서를, `with_evidence()`로 Core가 기록한 증거를 넘길 수 있습니다.
4. **권한 모드 값:** 2.1.325 도움말의 `--permission-mode` 선택지는 `acceptEdits, auto, bypassPermissions, manual, dontAsk, plan`이고 `default`는 목록에 없지만 받아들여집니다. `default`가 없어지면 사전 점검이 전부 실패하므로 그때 운영 설정 값을 `manual`로 바꿔야 합니다.
5. **문서의 예고:** 공식 문서가 `--bare`를 "`-p`의 기본값이 될 예정"이라고 적습니다. `--bare`는 구독 로그인을 읽지 않습니다. 기본값이 바뀌면 Managed Run의 인증이 실패할 수 있고 사전 점검으로는 잡히지 않습니다.

## 7. 지원 백로그 판정 변화 제안

| ID | 지금 | 제안 | 근거 |
|---|---|---|---|
| SB-04 (Claude 부분) | 미지원 | 유지 | 2.1.325 `--help`에 Turn 지정 재개·분기 옵션 없음(`help-2.1.325.txt`) |
| SB-06 (Claude 부분) | 설계상 미지원 | 유지. 이유를 "실행 중 메시지는 뒤에 도구 호출 경계가 있을 때만 같은 Turn에 반영되고 아니면 새 Turn(2.1.325)"으로, 재확인 방법을 `env_mid_turn_message_joins_the_turn_only_at_a_tool_boundary`로 갱신 | §2.6, §4.2 |
| SB-07 | 미구현 | Adapter 부분 해소(§4로 이동). Windows 2.1.325 실환경 통과, `answer_approval` `supported`(Windows). Core 측(판단·라우팅·UI)은 P1-2의 다른 작업 | `env_live_approval_allow_and_deny`, `env_live_approval_cancel_before_answer` |
| SB-10 (Claude 부분) | 미검증(`unverified`) | 해소. H0 기준으로 판정해 Windows·Linux에서 `supported`(같은 버전 증거 또는 같은 major.minor + 사전 점검) | `detect_reports_tested_versions_as_supported`, `env_preflight_passes_and_capabilities_follow_the_evidence`, `env_resume_and_fork` |
| SB-11 | 미구현 | 유지(Core의 Capsule 조립이 남음). "Adapter는 `fresh`와 같이 처리"를 "Claude Adapter는 새 세션으로 시작하고 `capsuleVersionId` 없는 요청을 거부, 실환경 통과"로 갱신 | `capsule_starts_a_new_session_with_the_capsule_body`, `env_capsule_is_a_new_session_without_the_previous_conversation` |
| 새 항목 | — | Claude `answer_approval`(Linux): 미검증. `psyki-nas`에서 `env_live_approval_*` 실행 | Linux 증거 없음 |
| 새 항목 | — | Claude Task MCP 주입·Capsule·사전 점검(Linux): 미검증. `psyki-nas`에서 해당 실환경 시험 실행 | Linux 미실행 |
| 새 항목 | — | Claude Task MCP의 실제 서버 연결: 미검증. PH1-MCP 통합 뒤 확인 | 시험용 서버로만 확인 |

## 8. 시험 세션

지우지 않았습니다. 병합 때 통합 담당이 정리합니다.

- 작업 폴더: `C:\Users\Psyki-HOME\AppData\Local\Temp\telos-claude-env\<이름>`, 상태 폴더 `…\Temp\telos-claude-env-state\{task-mcp, strict-mcp}`
- 세션 폴더: `~/.claude/projects/` 아래 `telos-claude-env`가 들어간 25개. 시험 코드가 만든 것은 `C--Users-Psyki-HOME-AppData-Local-Temp-telos-claude-env-<이름>` 18개, 실험이 만든 것은 `C--Users-PSYKI--1-AppData-Local-Temp-telos-claude-env-exp-<이름>` 7개입니다(실험은 짧은 경로 이름으로 실행).
- `~/.claude.json`에는 이 폴더들의 프로젝트 항목이 없습니다(확인: 0건).

| 폴더 이름(접두어 뒤) | 세션 ID |
|---|---|
| `approval-allow` | `00c3d097-a64b-48ea-b198-c517cbcbc112`, `82282965-5a88-4dcc-a5ac-f603c95df2cd` |
| `approval-deny` | `bbeae663-a2ca-4338-957b-2d6eaed565bc`, `ce49bbb0-ac37-4cc6-898b-312a2f96d9f9` |
| `background` | `a437a8fd-d1e8-4079-869a-3b5d374c2b13`, `2eb17372-06b4-48d0-991d-d11c5daab64e` |
| `cancel` | `8b3cf287-1ee7-49d6-8d3a-5a22deb5943a`, `7c583170-abc5-47f4-babc-4b88a3445edb` |
| `capsule` | `0fb2d710-4273-41da-8e13-ea46120eadb3`, `29ee3236-8810-449f-b118-6d9eff8d324a`, `d5214b9b-2124-418b-b7c5-30e0222dda83`, `7aa60392-9d80-417e-a05a-edce89a0c0f2` |
| `completed` | `c40145da-b1d5-486b-bd2b-362d07c86e33`, `4d835888-481c-4a47-8c27-57137b9b389c` |
| `denied` | `8bc8dded-ed6a-4df7-9f2f-3db838f3c7e2`, `30f23f1a-17b0-4cdd-b40a-02415eaecdba` |
| `invalid-model` | `d12daf36-e56a-4e84-8593-35bd62647ccc`, `00346d90-2ec8-43af-9df4-9a555ea18cfc` |
| `listing` | `7a470757-01cc-443f-a59c-57acf52dff6d`, `732ed595-f1b3-4959-8c92-0629909b912a`, `3b4f55c7-1ddd-4956-b24f-b99c92371281`, `62b4c12a-9b99-4544-a999-1a76c39abda5` |
| `listing-hidden` | `1a9f8992-c74d-45dd-8597-755564e90f39`, `b9f41683-0a6c-43f1-b144-bd7e2f66ad1d` |
| `live-approval-allow` | `ac514037-c357-4d7b-87ef-a9485405e949`, `e5dc0632-c843-4623-bf95-97c63693963c` |
| `live-approval-deny` | `de7b96be-cb19-45e3-83eb-f895921d6cef`, `ed498285-b84e-4dc8-a8c9-937d4b4e6b74` |
| `live-approval-cancel` | `e50b9488-038d-436e-bdbf-6f1e85704ff5`(1차, 실패한 실행), `47e64783-671a-4d58-a0e6-dd2261522f40`(수정 뒤 단독 실행), `ffd3fbed-2ec2-436a-8f18-8ce0e2f169c4` |
| `mid-turn-tools` | `4b43de2c-3ecd-413a-928d-099cb585bbe8`, `92cf414f-c861-4318-bff6-30b482f4fdf9` |
| `mid-turn-text` | `001b814b-effe-4955-8974-cfd770d043c8`, `f0e49ef8-a24d-4551-afd9-1fe473f4d9d7` |
| `resume` | `89744689-4a45-4cfa-babd-ebf468e8eb2d`, `ad66e8ba-8502-46d1-b289-d507ea3685c0`, `127cf1b4-f9b1-4d8c-b272-783a88b3c5f4`, `9973a348-0a7f-4298-ab0b-1c0f9ee8e96d` |
| `send-input` | `f58d4b2f-1ed4-4ea2-a6c6-537430025b0f`, `fb3b273e-0913-4a71-9479-ee5933769c76` |
| `task-mcp` | `aa83ef43-5704-415c-8d90-6cd3bcd89f72`, `bcfdb334-f2d8-4224-9537-e2051e65d721`, `34abfde7-a46f-4aa5-ba4e-145579ce5009`, `67e22e0e-80fb-4e9b-a46e-c1a61ab1f5d8` |
| `exp-approval-allow` | `8df4a250-3c2f-4862-a792-813617d55954` |
| `exp-approval-deny` | `682d64f0-2055-434d-bb7e-d3af28a11250` |
| `exp-approval-interrupt` | `e3afc6af-cb09-4a22-a112-f66ac630decb` |
| `exp-mcp-strict` | `dfa979a0-75e3-469c-8b52-2e23fc9342a9` |
| `exp-steer-plain` | `49ca397a-aa05-466b-8683-300b7c004d6d` |
| `exp-steer2-notool` | `4028575f-7ca5-45c6-b3a9-e9134c78d99b` |
| `exp-steer2-tool` | `1d3fef98-73c6-4777-8631-70d19e77e677` |

합계 52개 세션입니다(세션 파일 수와 대조). 실환경 시험 전체를 두 번(1차 16개 통과·1개 실패, 최종 17개 통과) 실행했고 취소 시험을 한 번 따로 실행했습니다.

## 9. 확인하지 못했거나 가정한 것

**가정**

1. 증거는 플랫폼(`win32-x64`·`linux-x64`)으로만 고릅니다. 계약 문구는 "같은 Host·플랫폼"이지만 Host ID가 다른 같은 플랫폼의 Host에서도 내장 증거를 인정합니다. Host까지 맞추려면 Core가 Host별 증거를 `with_evidence`로 넘기고 내장 증거를 빼야 합니다.
2. 자격 파일 경로는 환경 변수 `TELOS_TASK_MCP_CREDENTIAL_FILE`로 넘깁니다(초안 A).
3. 승인 `action` 낱말과 Digest 계산 규칙(초안 B).
4. Capsule 모드는 `capsuleVersionId`가 있어야 합니다(초안 C).
5. 승인 방식은 Adapter 단위 설정입니다.
6. 주입한 Task MCP 서버가 연결되지 않으면 Run을 실패로 끝냅니다. 지시에 없던 판단입니다.
7. 2.1.318의 증거는 H0 결정 문서 A1의 기록을 옮긴 것입니다. 이 작업에서는 2.1.318을 실행하지 못했습니다(설치본이 2.1.325).

**확인하지 못한 것**

1. Linux(`psyki-nas`) 전체. Mock 시험도 Linux에서 실행하지 않았습니다.
2. 사전 점검의 세 번째 실행이 세션을 만들지 않는다는 것은 2.1.325에서만 확인했습니다. 사용자 수준 Hook(`SessionStart` 등)이 그 실행에서 도는지는 확인하지 않았습니다.
3. 실제 `telos-weaver` MCP 서버, 연결에 실패하는 서버의 실환경 동작, 승인 요청 여러 개 동시 대기.
4. 실환경의 승인 대상은 `Write` 하나입니다. `Bash`·MCP 도구의 승인 요청은 Mock으로만 확인했습니다.
5. 확장과 같은 세션을 동시에 쓰는 경우(PROV-10)는 이번에도 시험하지 않았습니다.

**관찰**

1. `claude --no-such-option --version`과 `claude -p --no-such-option --help`는 오류 없이 끝납니다. 옵션 수용은 실제 실행으로만 확인할 수 있습니다.
2. 승인 대기 중 stdin이 닫히면 CLI는 요청을 버리고 모델을 다시 불러 Turn을 마칩니다. 승인을 쓰지 않는 Run의 취소는 이전과 같습니다(stdin이 이미 닫혀 있고, 유예 2초 뒤 종료).
3. `--tools ""`는 내장 도구만 끕니다. `--mcp-config`의 MCP 도구는 그대로 쓸 수 있습니다.
4. 작업 폴더 경로에 Windows 짧은 이름(`PSYKI~1`)이 있으면 쓰기 승인 요청의 사유가 "suspicious Windows path pattern that requires manual approval"(`safetyCheck`)로 나왔습니다(`default` 모드에서 관찰). `acceptEdits`에서도 승인을 요구하는지는 확인하지 않았습니다. Workspace 경로는 긴 이름으로 넘기는 편이 안전합니다.

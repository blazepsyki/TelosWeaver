# PH0-AGY — VS Code 대화 목록 표시 (DEC-08, Spec v1.5.3 §7.6, AGENT-05)

| 항목 | 값 |
|---|---|
| 작업 | [VSCODE_LISTING_ASSIGNMENTS.md](../PH0-INTEGRATION/VSCODE_LISTING_ASSIGNMENTS.md) §4 A1~A5 + [INTEGRATION §9.3](../PH0-INTEGRATION/README.md) 후속 2 |
| Branch / Worktree | `ph0/agy-listing` / `D:\worktree\TelosWeaver\ph0-agy-listing` (기준 `5cac704`) |
| 담당 | `gemini-4-argon/session-9e6137eb` (같은 세션에서 Claude Opus 5.5가 이어 마무리) |
| 환경 | Windows 11 (`windows-local`), 확장 `google.google-antigravity` 1.9.0, `agy.exe` 1.4.7, 모델 `gemini-3.8-flash-low` / `--effort low` / `--mode plan` |
| 수정 경로 | `crates/adapters/src/agy.rs`, `crates/adapters/tests/agy_adapter.rs`, `crates/adapters/tests/agy_environment.rs`, 이 문서 (모두 PH0-AGY 독점 경로) |

## 1. A1 — 설정 표현

- `AgyVsCodeListing { Listed (기본), Hidden }`을 `crates/adapters/src/agy.rs`에 추가했습니다. `telos-schema`·공통 파일은 바꾸지 않았습니다.
- 대응: `Listed ↔ AgyDataDirMode::ExtensionShared`(`--app_data_dir=antigravity`), `Hidden ↔ AgyDataDirMode::DefaultCli`(옵션 생략). `Custom`은 표준 모드가 아니므로 `vscode_listing() == None`.
- API: `AgyVsCodeListing::data_dir_mode`, `From<AgyVsCodeListing> for AgyDataDirMode`, `AgyDataDirMode::{from_vscode_listing, vscode_listing}`, `AgyConfig::{with_vscode_listing, vscode_listing}`, `AgyAdapter::{with_vscode_listing, vscode_listing}`. `with_default_cli_data_dir`는 `with_vscode_listing(Hidden)`과 같습니다.
- `AgyConfig` 필드는 바꾸지 않았습니다. `crates/core/src/integration/mod.rs`(INTEGRATION 소유)의 구조체 리터럴이 그대로 컴파일되며, 기존 `data_dir_mode` 기본값(`ExtensionShared`)이 곧 `listed` 기본입니다(공통 규칙 §1.2). INTEGRATION I1은 `vscodeListing` 정책 값을 `AgyVsCodeListing::data_dir_mode()`로 바꿔 넣으면 됩니다. 기존 `dataDir`와 함께 쓸 때의 우선순위는 INTEGRATION이 정합니다.

## 2. A2 — `vscode_listing` Capability

| 조건 | 상태 | 근거 수준 |
|---|---|---|
| 검증 버전(1.4.7)·같은 Host, `--app_data_dir` 수용 | `supported` | `environment-tested` |
| 미검증 CLI 버전 | `unverified` | `contract-tested` |
| `--app_data_dir` 옵션 거부(`data_dir_option_supported == false`) | `unsupported` | `environment-tested` |

- 운영자 설정이 `hidden`이어도 표시 **수단**의 지원 여부를 보고합니다(§7.6). `hidden` + 옵션 거부도 `unsupported`입니다.
- `reason`에 확장 1.9.0 / `agy` 1.4.7 기준, `hidden`은 디렉터리별 로그인(`AGY-DATA-02`)·Linux Remote SSH 파일 토큰(`<app_data_dir>/antigravity-oauth-token`) 주의를 적었습니다.

## 3. A3 — 자동 판정 (실환경)

시험: `cargo test -p telos-adapters --test agy_environment env_vscode_listing_summaries_and_workspace_filter -- --ignored --test-threads=1 --nocapture` → **1개 통과 (29.85s)**. 두 인덱스(`~/.gemini/antigravity/conversation_summaries.db`, `~/.gemini/antigravity-cli/conversation_summaries.db`)는 Python `sqlite3`의 `mode=ro`로만 읽었습니다.

### 3.1 `listed` (`--app_data_dir=antigravity`, `--add-dir <cwd>`, `--project` 생략)

| 확인 | 결과 |
|---|---|
| 확장 인덱스 행 | 있음: `app_data_dir="antigravity"`, `source=""`(확장에서 시작한 대화와 같은 값), `step_count=2` |
| `workspace_uris` | `["file:///C:/Users/Psyki-HOME/AppData/Local/Temp/telos-agy-env/listing-listed"]` |
| `project_id` | `"default-cli-project"` (`--project`를 주면 그 프로젝트 UUID가 됨, 사전 관찰) |
| CLI 인덱스 행 | 없음 |
| Resume(`--conversation <id>`) | 같은 ID 유지, `step_count` 2 → 5 |

### 3.2 `hidden` (옵션 생략, `--add-dir <cwd>`)

| 확인 | 결과 |
|---|---|
| 파일 위치 | `antigravity-cli/brain/<id>`·`antigravity-cli/conversations/<id>.db`에만 생김 |
| CLI 인덱스 행 | 있음: `app_data_dir="antigravity-cli"`, `source=""`, `workspace_uris`에 작업 폴더 |
| 확장 인덱스 행 | **있음**: `source="CORTEX_TRAJECTORY_SOURCE_CLI"`, 제목 없음 (§3.3) |

### 3.3 발견: `hidden` 대화가 확장 인덱스에 나타남

- 두 번 실행(`4da24486-…`, `11848e67-…`) 모두 `hidden` 대화가 확장 인덱스에 `source="CORTEX_TRAJECTORY_SOURCE_CLI"`로 기록됐습니다. 당시 확장 허브 `agy --hub --app_data_dir=antigravity …`(PID 36324)가 실행 중이었습니다.
- 대화 DB(`antigravity-cli/conversations/<id>.db`)를 지운 뒤에는 확장 인덱스의 그 행이 사라졌습니다. 반면 이전 DefaultCli 대화 `4fa80c33-…`는 DB가 `antigravity-cli`에만 남아 있고, 확장 인덱스에 같은 `source`로 남아 있습니다.
- 추정: 확장 쪽 허브가 CLI 기본 폴더의 대화도 감시해 CLI 출처로 색인합니다(`agy.exe` 기호에 `AdditionalConvoDir`·`isCLIAppDataDir`·`untagOwnCLISummary`가 있음). **원인은 확인하지 않았습니다.**
- 따라서 **"`hidden`이면 확장 인덱스에서 빠진다"는 자동 판정은 성립하지 않습니다.** 확장 UI가 `CORTEX_TRAJECTORY_SOURCE_CLI` 행을 목록에 보여 주는지는 화면 확인(A4)으로만 알 수 있습니다. 보여 준다면 Agy의 `hidden`은 §7.6이 뜻하는 숨김을 제공하지 못하므로 Spec 개정이 필요합니다(사용자 합의 사항, 코드로 먼저 바꾸지 않음).
- 시험은 이 관찰을 고정합니다: `hidden`의 파일은 `antigravity-cli`에만 생기고, 확장 인덱스에 행이 있으면 반드시 `CORTEX_TRAJECTORY_SOURCE_CLI`여야 합니다(`listed`의 `source=""`와 구별).

## 4. A4 — 화면 확인 (미검증)

| 항목 | 상태 |
|---|---|
| 자동 판정(§3) | 실행·통과 (§3.3의 `hidden` 제한 포함) |
| 화면 확인 | **미검증** — 사용자 VS Code 조작 필요 |

절차(사용자):
1. 저장소 밖 일회용 폴더를 VS Code로 열고 Antigravity 대화 목록을 엽니다.
2. 그 폴더에서 `agy --app_data_dir=antigravity --add-dir <폴더> -p "Reply LISTED_UI"` 실행 → 목록에 새 대화가 보이는지, Workspace 필터에서 보이는지 확인합니다.
3. `agy --add-dir <폴더> -p "Reply HIDDEN_UI"`(옵션 생략) 실행 → 목록에 **보이지 않는지** 확인합니다(§3.3의 핵심 질문).
4. 확인 후 두 대화의 `brain/<id>`·`conversations/<id>.db*`를 지웁니다.

## 5. INTEGRATION §9.3 후속 — 취소 정규화 Mock 시험

`cancel_normalizes_trailing_error_result_to_cancelled`: 취소 요청 뒤 종료 신호를 받은 프로세스가 닫히기 직전 `result(status:"ERROR", "failed to send message: trajectory not found")`를 내는 상황을 가짜 런타임(`Script::lines_on_exit`)으로 재현합니다. 결과 순서 `session_started → usage → turn_finished(cancelled, reason "cancelled") → process_exited(terminated_on_request)`, `error` 이벤트 없음, `turn_finished` 중복 없음을 확인했습니다.

## 6. 시험 결과

| 명령 | 결과 | 종류 |
|---|---|---|
| `cargo test -p telos-adapters --test agy_adapter` | 16개 통과 (신규 2: `vscode_listing_modes_and_capability_follow_spec_7_6`, `cancel_normalizes_trailing_error_result_to_cancelled`) | Mock·Contract |
| `env_vscode_listing_summaries_and_workspace_filter` | 1개 통과 | 실환경 |
| `cargo test --workspace --locked` | 157개 통과, 0 실패, 34개 무시(실환경) | Workspace |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 통과 | 정적 검사 |
| `cargo fmt --all --check` | 통과 | 포맷 |

기존 실환경 시험 7개는 이번 변경(시험 보조 코드 외 실행 경로 불변)으로 다시 돌리지 않았습니다.

## 7. 시험 세션과 정리

| 세션 | 모드 | 정리한 파일 | 남은 인덱스 행 |
|---|---|---|---|
| `bea8b20e-41ae-4b48-a39e-4ef33d35eeb3` (1차, 시험 실패 회차) | listed + resume | `antigravity/brain/<id>`, `antigravity/conversations/<id>.db` | 확장 인덱스 1행 (`source=""`) |
| `4da24486-7057-45cf-bdc2-22ebbe1dbb6f` (1차) | hidden | `antigravity-cli/brain/<id>`, `antigravity-cli/conversations/<id>.db` | CLI 인덱스 1행. 확장 인덱스 미러 행은 사라짐 |
| `d80be8ac-d7c0-41fd-8897-bc45498acdc5` (2차) | listed + resume | `antigravity/brain/<id>`, `antigravity/conversations/<id>.db` | 확장 인덱스 1행 (`source=""`) |
| `11848e67-e74a-456e-9d44-ecafdc0ede92` (2차) | hidden | `antigravity-cli/brain/<id>`, `antigravity-cli/conversations/<id>.db` | CLI 인덱스 1행. 확장 인덱스 미러 행은 사라짐 |

`-wal`·`-shm`·`.pb`는 남지 않았습니다. 공통 규칙 §1.4에 따라 `conversation_summaries.db`는 수정하지 않았으므로 요약 행 4개가 남아 있습니다. `listed` 2행은 확장 목록에 내용 없는 항목으로 보일 수 있습니다. 지우려면 사용자가 확장 UI에서 삭제해야 합니다. 1차 회차는 시험 단언 실패(§3.3 발견)로 끝났지만 정리는 단언 전에 실행되었습니다.

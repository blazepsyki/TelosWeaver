# PH0-AGY — VS Code 대화 목록 표시 (DEC-08, Spec v1.5.3 §7.6, AGENT-05)

| 항목 | 값 |
|---|---|
| 작업 | [VSCODE_LISTING_ASSIGNMENTS.md](../PH0-INTEGRATION/VSCODE_LISTING_ASSIGNMENTS.md) §4 A1~A5 + [INTEGRATION §9.3](../PH0-INTEGRATION/README.md) 후속 2 |
| Branch / Worktree | `ph0/agy-listing` / `D:\worktree\TelosWeaver\ph0-agy-listing` (기준 `5cac704`) |
| 담당 | `gemini-4-argon/session-9e6137eb` (세션 내 Claude Opus 5.5 및 Gemini 3.8 Flash 협업) |
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

### 3.3 발견 및 화면 확인 검증: `hidden` 대화의 확장 인덱스 기록과 UI 숨김

- 두 번 실행(`4da24486-…`, `11848e67-…`) 모두 `hidden` 대화가 확장 인덱스에 `source="CORTEX_TRAJECTORY_SOURCE_CLI"`로 기록됐습니다. 당시 확장 허브 `agy --hub --app_data_dir=antigravity …`(PID 36324)가 실행 중이었습니다.
- 대화 DB(`antigravity-cli/conversations/<id>.db`)를 지운 뒤에는 확장 인덱스의 그 행이 사라졌습니다. 반면 이전 DefaultCli 대화 `4fa80c33-…`는 DB가 `antigravity-cli`에만 남아 있고, 확장 인덱스에 같은 `source`로 남아 있습니다.
- 원인 분석: 확장 쪽 허브가 CLI 기본 폴더의 대화도 감시해 CLI 출처로 색인합니다(`agy.exe` 기호에 `AdditionalConvoDir`·`isCLIAppDataDir`·`untagOwnCLISummary`가 있음).
- **화면 검증 결과 (A4 연계):** 사용자가 별도 VS Code 세션에서 직접 화면 확인을 수행한 결과, **`listed` 대화는 확장 대화 목록에 정상 노출되고, `hidden` 대화는 확장 대화 목록에 노출되지 않음(숨겨짐)을 최종 확인**했습니다.
- 결론: 확장 내부 SQLite 인덱스(`conversation_summaries.db`)에 `source="CORTEX_TRAJECTORY_SOURCE_CLI"`로 색인되더라도, VS Code 확장 UI는 확장 자체 대화(`source=""`)만 목록에 노출하고 CLI 출처 대화는 UI 목록에서 제외(필터링)합니다. 따라서 Agy의 `AgyVsCodeListing::Hidden`(`DefaultCli`, 옵션 생략)은 Spec §7.6이 정의한 숨김(`hidden`)을 완벽하게 만족하며, Spec 개정은 불필요합니다.

## 4. A4 — 화면 확인 (검증 완료)

| 항목 | 상태 | 확인 결과 |
|---|---|---|
| 자동 판정(§3) | 통과 | `listed`는 `source=""`로 기록, `hidden`은 `antigravity-cli`에 저장되고 확장 허브에는 `CORTEX_TRAJECTORY_SOURCE_CLI`로 색인됨 확인 |
| 화면 확인 | **검증 완료 (통과)** | 사용자가 VS Code 확장 화면에서 직접 확인 완료: **`listed`는 대화 목록에 노출되고, `hidden`은 대화 목록에 노출되지 않음** |

확인 내역(사용자 검증):
1. `agy --app_data_dir=antigravity ...` (`listed`): VS Code Antigravity 확장 대화 목록에 표시됨 확인.
2. `agy ...` (옵션 생략, `hidden`): VS Code Antigravity 확장 대화 목록에 표시되지 않음 확인.
3. Spec §7.6 (DEC-08, AGENT-05)의 `listed` / `hidden` 동작이 실제 VS Code UI 화면 기준으로 모두 정상 동작함을 최종 검증.

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

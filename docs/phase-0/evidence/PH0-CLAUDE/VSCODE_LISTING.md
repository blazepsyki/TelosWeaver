# PH0-CLAUDE VS Code 확장 대화 목록 표시 (DEC-08) 구현 및 검증 기록

| 항목 | 값 |
|---|---|
| 작업 ID·수행 | PH0-CLAUDE (담당 `claude-opus-5.5/session-c9336d2c`) · 사용자 지시로 `claude-opus-5.5/session-be1c1774`가 수행 |
| 작성·검증일 | 2026-10-25, Windows 로컬 (`windows-local`) |
| Branch / Worktree | `ph0/claude-listing` / `D:\worktree\TelosWeaver\ph0-claude-listing` |
| 기준 | `269a771` (Spec v1.5.3, 계약 `7497877`) |
| 근거 | Spec DEC-08·§7.6·AGENT-05, [작업 지시](../PH0-INTEGRATION/VSCODE_LISTING_ASSIGNMENTS.md) §2 |
| 대상 | `anthropic.claude-code` 2.1.310 (`win32-x64`) 확장과 동봉 CLI `2.1.310 (Claude Code)` |
| 시험 조건 | 배정표 §6: 모델 `haiku`, 기본 `~/.claude`, 저장소 밖 일회용 폴더, 도구 없음(`--tools ""`), `--dangerously-skip-permissions` 미사용 |

## 1. 수정 파일

| 경로 | 내용 |
|---|---|
| `crates/adapters/src/claude.rs` | `VsCodeListing`(`Listed` 기본/`Hidden`), `ClaudeCodeAdapter::with_vscode_listing`·`vscode_listing`, 자식 환경 `command_env`, Capability `vscode_listing` |
| `crates/adapters/tests/claude_adapter.rs` | Mock 시험 2개 추가, `detect` 시험에 `vscode_listing` 확인 추가 (13 → 15개) |
| `crates/adapters/tests/claude_environment.rs` | 실환경 시험 `env_vscode_listing` 추가 (6 → 7개) |

공통 계약·Cargo·다른 작업 경로는 바꾸지 않았습니다.

## 2. 구현 결정

- **출처 지정:** 자식 프로세스 환경에 `CLAUDE_CODE_ENTRYPOINT`를 항상 명시합니다. `Listed` → `claude-vscode`, `Hidden` → `sdk-cli`. Core가 상속한 값에 기대지 않습니다.
- **부모 세션 변수 제거:** `CLAUDECODE`, `CLAUDE_CODE_SESSION_ID`, `CLAUDE_CODE_CHILD_SESSION`, `CLAUDE_CODE_SESSION_ATTENDED`, `CLAUDE_CODE_MESSAGING_SOCKET`, `CLAUDE_CODE_MESSAGING_TOKEN`, `CLAUDE_PID`를 공용 런타임의 `CommandSpec.env` `None` 항목으로 지웁니다(Windows·Unix 런타임 모두 제거를 지원함을 코드로 확인).
- **설정 전달:** `ClaudeCodeConfig`·`ClaudeLaunch`에 필드를 더하면 INTEGRATION 소유 `crates/core/src/integration/**`의 생성 코드가 깨지므로, 기본값 `Listed`를 가진 빌더 메서드 `with_vscode_listing`으로 추가했습니다. INTEGRATION이 운영자 정책을 연결하기 전에도 기본으로 표시됩니다.
- **Capability `vscode_listing`:** 실환경 판정한 2.1.310만 `supported`/`environment-tested`, 다른 버전은 `unverified`/`contract-tested`. 운영자 설정이 `Hidden`이어도 수단의 지원 여부를 보고합니다(§7.6).
- **Resume·Fork:** 모든 Run에 같은 방식으로 지정하므로, 이어 쓴 세션에 다른 출처 값이 섞이지 않습니다.

## 3. 판정 규칙 (확장 2.1.310 코드 기준)

확장 `extension.js`에서 확인한 규칙을 실환경 시험이 그대로 검사합니다.

| 규칙 | 확장 동작 | 시험 검사 |
|---|---|---|
| programmatic 판정 | 세션 JSONL 첫 줄 부근 `entrypoint`가 `sdk-cli`·`sdk-ts`·`sdk-py`이거나 `sessionKind`가 `daemon*`이면 제외(`claudeCode.claudeProcessWrapper` 미설정 시) | 파일의 **모든** `entrypoint`가 기대 값인지 (더 엄격) |
| 목록 범위 | VS Code 첫 Workspace 폴더의 `projects/<인코딩>` 폴더만 읽음, `includeWorktrees: false` | 세션 파일 폴더 이름 = 기록된 `cwd`의 인코딩(영숫자 외 `-`), 기록된 `cwd` = 실행 Workspace(실제 경로 비교) |

## 4. 시험 결과

| 구분 | 결과 | Host |
|---|---|---|
| Mock (`claude_adapter`) | 15개 통과. 신규: `runs_are_listed_in_the_extension_by_default`(기본 `claude-vscode`·부모 변수 7개 제거), `hidden_runs_are_recorded_as_automation_for_every_session_mode`(Fresh·Resume·NativeFork 모두 `sdk-cli` 한 번만), `detect`의 `vscode_listing`(2.1.310 `supported`, 9.9.9 `unverified`, `Hidden`이어도 `supported`) | windows-local |
| 실환경 `env_vscode_listing` | 통과. Fresh → Resume(같은 세션 ID 유지, 문맥 이어짐) → NativeFork(새 ID) 모두 `claude-vscode`, 위치 `projects/C--Users-Psyki-HOME-AppData-Local-Temp-telos-claude-env-listing/`. `Hidden` Run은 모든 줄 `sdk-cli` | windows-local |
| 실환경 전체(`claude_environment`) | 7개 통과(66초). 기존 6개(완료·도구 거부·잘못된 모델·취소·Resume/Fork·탐지)가 환경 변경 뒤에도 그대로 통과 | windows-local |
| Workspace | `cargo test --workspace --locked` 157개 통과(실환경 34개 기본 제외), `cargo clippy --workspace --all-targets --locked -- -D warnings` 통과, `cargo fmt --all --check` 통과 | windows-local |
| 확장 화면 확인 (L4) | **통과 (사용자 확인, 2026-10-25).** Adapter로 사용자 VS Code 창의 첫 Workspace 폴더 `D:\Downloads\Vibe_dev`에서 Run 2개 실행(임시 시험 파일, 커밋 안 함). `Listed` 세션 `46ad660e…`은 Claude Code 확장의 과거 대화 목록에 나타났고, 열어서 대화가 보였으며, 사용자가 확장에서 후속 입력("ok")을 보내 정상 응답을 받음(파일 12 → 23줄, 추가된 줄도 모두 `claude-vscode`). `Hidden` 세션 `d1e69e03…`(모든 줄 `sdk-cli`)은 목록에 나타나지 않음 | windows-local |
| Linux (`psyki-nas`) | 미실행 (이번 필수 범위 아님) | — |

**상속 환경에서의 확인:** 시험은 VS Code의 Claude Code 세션 안에서 실행해 셸에 `CLAUDE_CODE_ENTRYPOINT=claude-vscode`, `CLAUDECODE=1`, `CLAUDE_CODE_SESSION_ID` 등 부모 세션 변수가 있는 상태였습니다. 그런데도 `Hidden` Run이 `sdk-cli`로 기록되어 명시 값이 상속 값을 이긴다는 것을 확인했고, 부모 변수를 제거한 상태로 기존 7개 시험이 모두 정상 동작했습니다.

## 5. 시험 세션 (병합 시점 정리 대상)

배정표 §6 "시험 세션 정리"에 따라 병합 시 `~/.claude/projects/<시험 작업 폴더>`를 지웁니다. 이번 시험 폴더는 모두 `C:\Users\Psyki-HOME\AppData\Local\Temp\telos-claude-env\` 아래입니다.

| 시험 | 세션 ID |
|---|---|
| `env_vscode_listing` 1회차 (단독 실행) | `efea2951-4c9a-46b7-9da4-13386f025cbb`(Fresh·Resume), `904fef97-8969-432e-a75a-1d926ccc4d52`(Fork), `76c97035-5179-4fd9-8a9f-baa806c9fed3`(Hidden) |
| `env_vscode_listing` 2회차 (전체 실행) | `a0da35a6-15d7-4a4a-a25e-487d9b18aaf6`(Fresh·Resume), `bc8c2092-60fa-41fb-8fb7-fae33d5c5d84`(Fork), `4c62e93a-ed2e-4edc-b64a-e813ad2d5910`(Hidden) |
| 기존 시험 재실행 | `c776a121-9e90-419e-9015-ec6a6ba0123e`(취소), `25d2ac28-96b5-4a29-8609-7995009cd876`(완료), `cb51889c-d75a-4581-8306-1861df2a7f48`(잘못된 모델), `40d57f1d-2446-4e48-9702-9e25d632d8c2`·`a22cead8-0c54-44bb-9b0a-63e04987e17d`(Resume·Fork), `a7656f4c-26b5-4466-aed0-6e193f10e5e3`(도구 거부) |

해당 폴더: `C--Users-Psyki-HOME-AppData-Local-Temp-telos-claude-env-{listing,listing-hidden,completed,denied,invalid-model,cancel,resume}`.

화면 확인 세션은 사용자 Workspace 폴더(`~/.claude/projects/d--Downloads-Vibe-dev/`)에 있으므로 **폴더째 지우지 않고 두 파일만** 지웁니다: `46ad660e-c20d-457a-983c-c32ad60f6ef9.jsonl`(표시, 사용자 후속 입력 포함), `d1e69e03-f84e-41ea-b575-d247b965409a.jsonl`(숨김). 같은 이름의 하위 폴더가 생겼다면 함께 지웁니다. 그 폴더의 다른 세션은 사용자 대화이므로 건드리지 않습니다.

## 6. 남은 일과 인계

1. **확장 화면 확인:** 완료(§4). 실행 후 확장에서 이어 쓴 세션도 `claude-vscode`로 유지됨을 확인했습니다. 이어 쓴 세션을 Core가 다시 `resume`할 때의 소유권 확인(§7.6, §8.1)은 Core 범위입니다.
2. **INTEGRATION 연결:** 운영자 정책의 `vscodeListing`을 `ClaudeCodeAdapter::with_vscode_listing`으로 전달하고, 적용 값(`claude-vscode`/`sdk-cli`)을 Run 승인 Snapshot에 기록 (지시서 §5 I1·I2).
3. **확장 버전 변경 시 재판정:** 판정 규칙은 확장의 비공개 코드입니다. `VSCODE_LISTING_VERIFIED_VERSIONS`에 없는 버전은 `unverified`로 보고되므로, 새 버전에서 `env_vscode_listing`을 다시 실행하고 확장 코드의 판정 규칙을 확인한 뒤 목록에 추가합니다.

# PH0-CODEX: Capability 판정 보정과 제어 실환경 검증

| 항목 | 값 |
|---|---|
| 담당 | `Codex / PH0-CODEX-CAPABILITY-01a17586` |
| 날짜 | 2026-10-26 (KST) |
| 기준 | main `9da1d40`, 착수 배정 `249fe60`, Spec v1.5.7, 계약 `6a4f58a` |
| Branch / Worktree | `ph0/codex-capability` / `D:\worktree\TelosWeaver\ph0-codex-capability` |
| Linux Worktree | `psyki-nas`: `~/TelosWeaver-wt/codex-capability` (detached HEAD) |
| 작업 원본 | [C3 지시서](../PH0-INTEGRATION/CODEX_CAPABILITY_FOLLOWUP.md) X1~X5 |
| 코드 | `9fd9a4e` (판정·실환경 시험), `a8e05bd` (승인 시험의 Task·Run 권한 범위 검사) |
| 판정 | 구현·검증 완료 제출. INTEGRATION 검토·통합 대기 |

## 1. 변경과 판정 결과

공통 키는 REQ-024의 상수를 그대로 사용했습니다. 새 `codex/capabilities.rs`가 검증된 `(플랫폼, CLI 버전)` 목록과 설치 확장의 ID·버전·플랫폼을 바탕으로 보고합니다. 목록 밖 조합은 지원으로 추정하지 않습니다.

CLI `codex-cli 0.164.0-alpha.11.1`, 확장 `openai.chatgpt` 26.1021.22027의 결과:

| Capability | Windows x64 | Linux x64 (`psyki-nas`) | 근거·제한 |
|---|---|---|---|
| `managed_run`·`event_stream`·`cancel` | `supported` / `environment-tested` | 같은 판정으로 보정 | 기존 Phase 0 양 OS 실환경 증거. 이번에 같은 기본 시험을 모델로 반복하지 않음 |
| `native_resume`·`native_fork` | `supported` / `environment-tested` | 같은 판정으로 보정 | 기존 Resume·Fork 실환경 증거. Turn 선택·세션 소유권 등 세부 제한은 별도 |
| `vscode_listing` (`listed`) | `supported` / `environment-tested` | 같은 판정으로 보정 | Windows는 기존 자동 목록·사용자 화면 확인, Linux는 기존 기본 DB 목록 확인. Linux 화면·실시간 갱신은 미검증으로 명시 |
| `send_input` | **`supported` / `environment-tested`** | **`unverified` / `mock-tested` 유지** | 이번 Windows 같은 Turn steering 통과. Linux 제어 실계정 시험은 미실행 |
| `answer_approval` | **`supported` / `environment-tested`** | **`unverified` / `mock-tested` 유지** | 이번 Windows 명령 실행 승인 Allow/Deny 통과. 다른 승인 종류 전체를 검증한 것은 아님 |
| `vscode_listing` (`hidden`) | `unsupported` 유지 | `unsupported` 유지 | 검증된 Hidden 생성 수단이 없어 실행 전에 거부 |

CLI 버전·플랫폼·확장 조합이 증거 목록 밖이면 `unverified`로 보고합니다. 실행 규칙 자체가 없는 플랫폼의 기존 시작 거부도 유지합니다. 새 기능이 공통 키 목록에 추가돼도 기본 실행 증거를 자동 적용하지 않도록 기본 기능 다섯 개를 명시적으로 구분했습니다.

`reason`도 Host와 기능별로 분리했습니다. Linux에 Windows 화면 확인을 적용하거나, 검증되지 않은 버전에 검증 사유를 붙이지 않습니다. Windows 제어 사유에는 `gpt-6-luna`·`low`와 확인한 경로의 범위를 기록합니다.

## 2. 설치 확장 식별 (X2)

Windows 폴더 이름을 문자열로 비교하던 방식을 제거했습니다. `CodexConfig`에는 탐지 결과의 확장 DTO가 없으므로, 공개 구성 구조와 Core 호출부를 바꾸지 않고 **선택된 실행 파일의 실제 설치 manifest**에서 다음 값을 읽습니다.

- `publisher`·`name`: `openai.chatgpt` 식별.
- `version`: 검증된 확장 버전과 비교.
- `__metadata.targetPlatform`: 실행 Host 플랫폼과 비교.

실행 파일을 canonicalize하고 상위 `package.json`을 크기 상한 1 MiB로 읽습니다. ID·버전·플랫폼 중 하나가 맞지 않거나 확인되지 않으면 목록 표시를 검증됐다고 보고하지 않습니다. 이름을 바꾼 확장 폴더에서도 올바른 manifest를 식별하고, 다른 ID·버전·플랫폼·누락 metadata를 거부하는 Mock 시험을 넣었습니다.

실제 Windows와 Linux 설치 manifest 모두 `openai.chatgpt` 26.1021.22027과 각 Host의 플랫폼을 확인했습니다. 별도 `env_capability_metadata_has_no_model_calls`는 실제 파일을 읽는 `detect()`를 검사하고 런타임 프로세스 생성이 0회임을 확인합니다.

## 3. Windows 실환경 제어 시험

실행 파일: `C:\Users\Psyki-HOME\.vscode\extensions\openai.chatgpt-26.1021.22027-win32-x64\bin\windows-x86_64\codex.exe`.

`--version`으로 CLI 버전을, App Server `model/list`로 `gpt-6-luna`의 `low` 지원을 확인했습니다. 성공한 세 Thread의 실제 JSONL `turn_context`에서도 모델 `gpt-6-luna`·effort `low`를 재확인했습니다. 모든 작업은 새 일회용 폴더·Thread에서 수행했고 사용자 대화를 재개하지 않았습니다.

### 3.1 같은 Turn steering (X3)

`env_steering_updates_the_same_turn`은 전경 `Start-Sleep -Seconds 10` 실행을 요청하고 도구 시작 뒤 `send_input`으로 새 최종 응답 지시를 보냅니다.

| 항목 | 결과 |
|---|---|
| Thread | `01a1773d-c372-7c93-9888-996c2c1c3243` |
| Native Turn | `01a1773d-c384-7033-86a9-4f04929eef4e` |
| 최초 최종 응답 지시 | `ORIGINAL` |
| 추가 입력 뒤 실제 응답 | **`C3_STEERED`** |
| 공통 이벤트 | **14개**, 동일 Native Turn ID 유지, `turn_finished` 1개, `check_run_events` 통과 |
| 프로세스 종료 | 종료 코드 0·자식 회수 확인. 완료 뒤 전용 App Server를 정리 |

추가 입력의 응답 Turn ID가 활성 Turn과 같은지는 기존 `RunEntry::send_input`이 검사합니다. 수집 때 같은 멱등 키로 `start_run`을 다시 호출한 결과도 최초 `RunStarted`와 같아야 하며, 새 작업·Turn 생성으로 성공을 대체하지 않습니다.

Protocol 해석은 [공식 App Server 문서](https://learn.chatgpt.com/docs/app-server#steer-an-active-turn)와 설치 CLI에서 생성한 Schema를 함께 확인했습니다. 공식 설명만으로 실환경 통과를 판정하지 않았습니다.

### 3.2 승인 Allow/Deny (X4)

`env_approval_allow_and_deny_control_the_write`는 쓰기가 허용된 시험 Task·Workspace와 일치하는 Run 문맥을 만듭니다. Native sandbox는 **read-only**에서 시작하며 다음 한 명령을 실행할 때 승인 요청을 기다립니다.

```powershell
Set-Content -LiteralPath approval.txt -Value C3_APPROVED
```

시험용 `CodexLaunchResolver`만 `Prompt`와 승인 검사를 제공합니다. Task 권한 프로필·읽기 전용 여부·Task ID/Revision·Workspace/Host 문맥, 실제 cwd, 정확한 명령을 검사합니다. 다른 명령·다른 위치·읽기 전용 Task·다른 Task·네트워크 승인·다른 승인 종류는 거부합니다. Native 요청의 Thread·Turn 일치는 기존 `RunEntry`가 먼저 확인합니다. 영구 승인이나 전역 정책 변경은 없습니다.

| 항목 | Deny | Allow |
|---|---|---|
| Thread | `01a1774f-c504-75f3-80c2-f898f4f84dc6` | `01a1774f-e159-7f42-969a-86674299fe63` |
| Native Turn | `01a1774f-c516-7c30-b612-0e5eb3a1dfc8` | `01a1774f-e16c-7880-85ec-a5908d54ff69` |
| 승인 요청·응답 | 상관된 요청 1개에 `decline` | 상관된 요청 1개에 `accept` |
| 파일 | **미생성** | **생성**, 내용 `C3_APPROVED` 확인 |
| 공통 이벤트 | 10개·유효한 순서·거부 결과 확인 | 9개·유효한 순서·Turn 완료 확인 |

최종 Task·Run 권한 범위 검사까지 포함한 시험이 통과했습니다(17.50초). 이 결과는 **명령 실행 승인 경로**의 증거입니다. Native `fileChange` 승인 전체·권한 확장 요청·MCP 승인의 실환경 통과를 주장하지 않습니다. 실제 Core의 `CodexApprovals::Deny` 고정 정책과 Phase 1 승인 UI는 바꾸지 않았습니다.

## 4. 품질·플랫폼 검사

코드 `a8e05bd` 기준입니다. Rust 1.99.1로 실행했습니다.

| 검사 | Windows | `psyki-nas` Linux |
|---|---|---|
| `cargo test --workspace --locked` | **177개 통과·실패 0개·42개 기본 제외** | **179개 통과·실패 0개·42개 기본 제외** |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 통과 | 통과 |
| `cargo fmt --all -- --check` | 통과 | 통과 |
| 실제 설치 metadata 기반 `detect()` (모델 호출 없음) | 1개 통과 | 1개 통과 |
| 같은 Turn steering 실계정 시험 | 통과 | 미실행·미검증 유지 |
| 승인 Allow/Deny 실계정 시험 | 통과 | 미실행·미검증 유지 |

Mock 행렬은 두 검증 플랫폼·다른 플랫폼·다른 CLI 버전과 확장 ID/버전/플랫폼/metadata 누락을 대조합니다. 이름을 바꾼 폴더도 manifest로 판정하고, fixture 버전을 지원으로 올리지 않는 것도 확인합니다. 시험 승인 정책의 범위 거부는 계정 없이 별도 검사합니다.

Windows 원본 검사 로그: `%TEMP%\telos-codex-capability-workspace.log`. Linux 로그: `/tmp/telos-codex-capability-workspace.log`. Windows OpenSSH가 원격 명령 완료 뒤 `close - IO is still pending on closed socket`과 종료 코드 1을 내는 현상이 있으므로, 원격 `REMOTE_WORKSPACE_EXIT=0`·`REMOTE_CHECKS_EXIT=0`과 Cargo 결과를 구분해 확인했습니다.

주요 명령:

```powershell
$env:TELOS_CODEX_CLI = 'C:\Users\Psyki-HOME\.vscode\extensions\openai.chatgpt-26.1021.22027-win32-x64\bin\windows-x86_64\codex.exe'
$env:TELOS_CODEX_VERSION = 'codex-cli 0.164.0-alpha.11.1'
$env:TELOS_CODEX_HOST_ID = 'windows-local'
$env:TELOS_CODEX_MODEL = 'gpt-6-luna'
cargo test -p telos-adapters --lib env_steering_updates_the_same_turn --locked -- --ignored --nocapture --test-threads=1
cargo test -p telos-adapters --lib env_approval_allow_and_deny_control_the_write --locked -- --ignored --nocapture --test-threads=1
cargo test -p telos-adapters --lib env_capability_metadata_has_no_model_calls --locked -- --ignored --nocapture
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
```

## 5. 실패 시도와 정리

실패한 시험을 지원 근거로 계산하지 않았습니다.

1. 첫 steering 시험의 `.ps1` 파일 실행이 Windows 실행 정책으로 거부돼 전경 대기 조건이 성립하지 않았고 응답도 `ORIGINAL`이었습니다. OS 설정을 바꾸지 않고 인라인 `Start-Sleep`으로 시험 조건을 고친 뒤 §3.1을 통과했습니다.
2. 첫 승인 시험은 결과 검사에서 `outcome`이 없는 이벤트에 직접 인덱싱해 panic했습니다. 안전한 필드 조회로 고쳤고 해당 시험 Thread는 정확한 일회용 cwd를 확인한 뒤 공식 Archive했습니다.
3. 초기 Allow는 Native 명령 미리보기의 이스케이프된 Windows 경로가 시험 정책의 예상 표현과 달라 거부됐습니다. 표현만 정규화하고 cwd·명령 전체의 엄격한 비교는 유지해 통과했습니다. 임의 명령을 허용하는 대체 경로는 넣지 않았습니다.
4. 최종 검토에서 승인 시험의 Task 쓰기 범위를 명시하고 읽기 전용 Task·다른 Task/Workspace 문맥을 거부하도록 보강했습니다. §3.2의 최종 성공 Thread는 이 검사까지 적용한 재시험입니다.

사용한 Native Thread는 총 **9개**, 모두 공식 Archive 성공을 확인했습니다. 각 ID의 활성 세션 파일 0개·보관 파일 1개도 재확인했습니다. 세션 JSONL·SQLite를 직접 수정하지 않았습니다.

| 용도 | Thread ID | 임시 폴더 이름 (`%TEMP%` 아래) |
|---|---|---|
| 첫 steering 실패 | `01a17739-9cf4-7011-898e-afe8264da01d` | `telos-codex-live-22636-1793001626679106400` |
| 승인 시험 검사 panic | `01a1773b-a98f-73c0-803d-0566cea6f78a` | `telos-codex-live-5048-1793001760997565500` |
| 중간 Deny 확인 | `01a1773d-86c5-7853-8335-61664a2298d5` | `telos-codex-live-34616-1793001883127060200` |
| 초기 Allow 정책 거부 | `01a1773d-ac6d-77e2-8466-71dd07c48134` | `telos-codex-live-34616-1793001892813054300` |
| steering 성공 | `01a1773d-c372-7c93-9888-996c2c1c3243` | `telos-codex-live-34616-1793001898675065100` |
| 중간 Deny 확인 | `01a17740-efef-7382-b271-094d20863a67` | `telos-codex-live-13572-1793002106685030900` |
| 중간 Allow 확인 | `01a17741-180f-73f3-b50d-86d672238876` | `telos-codex-live-13572-1793002116972349500` |
| 최종 Deny 성공 | `01a1774f-c504-75f3-80c2-f898f4f84dc6` | `telos-codex-live-42936-1793003078700139500` |
| 최종 Allow 성공 | `01a1774f-e159-7f42-969a-86674299fe63` | `telos-codex-live-42936-1793003086012945000` |

Native Schema 조사 자료는 `%TEMP%\telos-codex-capability-1yto35y_`에 보존했습니다. 모델 목록 확인은 Thread를 만들지 않았으므로 위 9개 집계에 포함하지 않았습니다.

## 6. 범위와 인계

- 수정 경로는 `crates/adapters/src/codex.rs`·`codex/**`·이 기록뿐입니다. Core·공통 계약·Capability 키·다른 Adapter·Cargo/Lockfile은 변경하지 않았습니다.
- Linux 기본 실행·목록 표시는 기존 Phase 0 증거를 사용했고 이번 추가 모델 시험은 Windows에서만 수행했습니다. Linux 화면·실시간 표시, Windows 이외의 제어 기능, 다른 승인 종류는 미검증으로 남깁니다.
- Branch와 Windows/Linux Worktree는 검토용으로 보존합니다. Native Thread는 Archive했고, 시험 폴더는 INTEGRATION의 통합 시 정리 대상으로 남깁니다.
- PH0-CODEX를 `review`로 제출하고 INTEGRATION에 증거 범위·Capability 상태/사유·시험 정책 검토와 통합을 요청합니다. G3 판정과 다른 담당의 진행 상태는 변경하지 않습니다.

# PH0-AGY 구현 및 검증 기록

> DEC-08(VS Code 대화 목록 표시, Spec §7.6) 작업 기록은 [VSCODE_LISTING.md](VSCODE_LISTING.md)에 있습니다.

| 항목 | 값 |
|---|---|
| 작업 ID·담당 | PH0-AGY · `gemini-4-argon/session-9e6137eb` |
| 작성·검증일 | 2026-10-24, Windows 로컬 (`windows-local`) |
| Branch / Worktree | `ph0/agy` / `D:\worktree\TelosWeaver\ph0-agy` |
| 기준 | `611032a` (Spec v1.4, `PH0-HOST` `685bd96`·`PH0-CLAUDE` `3bdb808` 통합 본) |
| 대상 CLI | `google.google-antigravity` 1.9.0 확장이 관리하는 `C:\Users\Psyki-HOME\.gemini\bin\agy.exe`, 버전 `1.4.7` |
| 시험 조건 | 배정표 §6: 모델 `gemini-3.8-flash-low`, `--effort low`, 저장소 밖 일회용 폴더(`%TEMP%\telos-agy-env\*`), `--dangerously-skip-permissions` 미사용, 생성된 시험 세션(`brain/<id>`) 즉시 삭제 |

## 1. 수정 파일

| 경로 | 내용 |
|---|---|
| `crates/adapters/src/agy.rs` | `AgyAdapter`, `cli_rule`, `AgyDataDirMode`, `AgyLaunch`·`AgyLaunchResolver`, `AgyConfig`, `--app_data_dir` 수용 검증 및 Capability 보고 |
| `crates/adapters/src/agy/protocol.rs` | `agy --output-format stream-json` NDJSON 한 줄 → 공통 `ProviderEventPayload` 변환 (`init`, `step_update`, `result`, `denied_actions`) |
| `crates/adapters/src/agy/run.rs` | Run별 이벤트 보존·재구독(`after_sequence`), 프로세스 감시·취소·자식 회수·종료 보고 |
| `crates/adapters/tests/agy_adapter.rs` | Mock·Contract 시험 14개 (실제 `agy` 1.4.7 출력 Fixture 재생) |
| `crates/adapters/tests/agy_environment.rs` | 실환경 시험 6개 (`#[ignore]`, 탐지·데이터 디렉터리 비교·도구 소프트 거부·잘못된 모델·취소·Resume/Fork 검증 및 시험 세션 자동 정리) |
| `crates/adapters/tests/fixtures/agy/*.jsonl` | 실제 `agy` 1.4.7 출력에서 경로를 정규화한 Fixture 3개 (`success.jsonl`, `permission-denied.jsonl`, `model-not-found.jsonl`) |
| `docs/phase-0/evidence/PH0-AGY/README.md` | 본 구현 및 실환경 검증 증거 문서 |

## 2. 실행 방식과 설계 결정

- **Run = 전용 프로세스 하나 = Turn 하나.** `agy [--app_data_dir=antigravity] --input-format stream-json --output-format stream-json`으로 시작하고, 프롬프트를 stdin의 Stream JSON 사용자 메시지(`{"event":"user","message":{"role":"user","content":...}}\n`)로 쓴 뒤 즉시 stdin을 닫습니다.
  - 근거 1 (Spec §7.4): `stdin`을 열어 둔 채 프로세스 종료만 기다리지 않습니다. `agy`의 `--input-format stream-json` 모드는 stdin EOF를 보기 전까지 다음 Turn 입력을 기다리므로, 해당 Run의 Turn 입력을 보낸 직후 stdin을 닫아 `result` 출력 뒤 프로세스가 스스로 종료되게 합니다.
  - 근거 2 (`-p` 인자 파싱 모호성 방지): `agy` 1.4.7에서 `-p` 뒤에 다른 플래그(`--input-format` 등)가 오면 그 플래그 문자열을 프롬프트로 오인해 실패(`Error: -p took "--input-format" as its prompt`)합니다. `--input-format stream-json`은 `-p` 없이도 단독으로 Headless Print 모드로 진입하며 명령줄 길이·인용 문제를 피합니다.
  - 근거 3 (공통 이벤트 규칙 REQ-006): 전용 프로세스 Run에서 `turn_finished` 뒤에는 `usage`·`error`·`process_exited`만 올 수 있습니다. 따라서 `send_input`은 `UnsupportedCapability`로 거부하고, 후속 Turn은 이전 Run의 `result` 소비 후 다음 Run의 `SessionMode::Resume`(`--conversation <id>`)으로 직렬화해 실행합니다 (§7.4, §8.1).
- **실행 중 승인 응답 경로 없음 (§7.4).** Headless Print 모드에는 실시간 `control_request`/`control_response` 승인 응답 경로가 없으므로 `answer_approval`은 `UnsupportedCapability`입니다. 권한 확인이 필요한 도구 호출은 Headless 모드에서 자동 소프트 거부되고 `step_update`(`state: "ERROR"`) 및 `result.denied_actions`에 보고됩니다.
- **종료 코드 0 소프트 거부 판정 (§7.3, §7.4).** `agy` 1.4.7은 도구가 소프트 거부되어 응답을 전혀 만들지 못한 경우에도 `result.status: "SUCCESS"`, `result.response: ""`, 종료 코드 `0`으로 끝납니다(stderr에 `jetski: no output produced — a tool required the "..." permission that headless mode cannot prompt for, so it was auto-denied` 출력). `AgyAdapter`는 종료 코드와 `status`만 보지 않고 `denied_actions`와 `response`를 함께 확인해 `tool_completed`(`denied`), `error`(`tool_denied`), `turn_finished`(`failed`, `reason: "tool_denied"`)로 기록합니다.
- **권한 우회 차단 (§7.4).** `dangerously-skip-permissions`, `bypassPermissions`, `--hub`, `--remote-control`, `-i`/`--prompt-interactive`는 프로세스 시작 전에 `FailureKind::Configuration`으로 거부합니다.
- **세션 전략 (§8.1, PROV-05, FORK-POC-01).**
  - `Fresh`·`Capsule`: 새 대화를 시작합니다.
  - `Resume`: `--conversation <source_session_id>`를 전달해 기존 대화를 이어받습니다.
  - `NativeFork`: `agy` 1.4.7은 `--fork` 플래그가 없고 Headless 모드에서 `/fork`를 명시적으로 거부합니다(`/fork is not available in print mode`). 미지원 기능을 지원하는 것처럼 표시하지 않는 원칙(§2.1, §8.2-6)에 따라 `detect()`와 `start_run()` 모두에서 `native_fork`를 `Unsupported`로 거부합니다.

### 2.1 Stream JSON 프로토콜 매핑 (`agy` 1.4.7 관찰)

| 원본 (`--output-format stream-json`) | 공통 이벤트 (`ProviderEventPayload`) |
|---|---|
| `{"event":"init","conversation_id":"...","init":{"model":"..."}}` (첫 번째) | `session_started` (`providerVersion`, `model`), 모든 이벤트의 `nativeSessionId` |
| `{"event":"step_update","step_update":{"step_type":"agent_response","text_delta":"..."}}` | `text_delta` (`assistant`) |
| `{"event":"step_update","step_update":{"step_type":"tool","state":"ACTIVE","tool_name":"..."}}` | `tool_started` (`step_index` 기준 중복 방지) |
| `{"event":"step_update","step_update":{"step_type":"tool","state":"DONE","tool_info":{"output":"..."}}}` | `tool_completed` (`succeeded`, 요약 포함) |
| `{"event":"step_update","step_update":{"step_type":"tool","state":"ERROR","tool_info":{"error":{"message":"permission check failed..."}}}}` | `tool_completed` (`denied`) |
| `{"event":"step_update","step_update":{"step_type":"tool","state":"ERROR",...}}` (일반 도구 오류) | `tool_completed` (`failed`) |
| `{"event":"result","result":{"status":"SUCCESS","usage":{...},"denied_actions":[...]}}` | `usage` (`delta`, `provider_reported`, `cacheIncludedInInput: false`), 소프트 거부 무응답 시 `error`(`tool_denied`) + `turn_finished`(`failed`), 정상 시 `turn_finished`(`completed`) |
| `{"event":"result","result":{"status":"ERROR","error":"..."}}` | `error` (§7.3 분류: `authentication`·`configuration`·`tool_denied`·`timeout`·`resource_limit`·`provider`) + `turn_finished` (`failed`) |
| 프로세스 종료 | `process_exited` (종료 코드·원인·자식 회수 `childrenReaped`) |
| JSON 파싱 실패, 줄 길이 상한(4 MiB) 초과, stdout 읽기 오류 | `error` (`protocol_parse`) 후 즉시 프로세스 종료 요청 (REQ-003) |
| `result` 없이 프로세스 종료 | `error` (stderr 힌트 또는 `provider`) + `turn_finished` (`failed` / 취소 시 `cancelled`) |

## 3. Antigravity 데이터 디렉터리 검증 (Spec §7.5, AGY-DATA-01~05)

사용자 합의(배정표 §6: 실제 TelosWeaver 운용 시 세 확장이 모두 활성 상태이므로 확장을 끈 비현실적 가정을 제외하고 확장이 켜진 실환경에서 `--app_data_dir=antigravity`와 기본 `antigravity-cli`를 직접 비교)에 따라 검증했습니다.

| ID | 확인 내용 | 실환경 관찰 및 판정 |
|---|---|---|
| **AGY-DATA-01** | `--hub` 없이 `-p`/`--output-format stream-json`과 함께 `--app_data_dir=antigravity` 수용 여부 | **통과 (`supported`)**. `agy --app_data_dir=antigravity --version`(`1.4.7`), `models`, `--input-format stream-json --output-format stream-json` 모두 종료 코드 `0`으로 수용됨. 정의되지 않은 플래그(`--unknown-flag-test`)는 즉시 종료 코드 `1`/`2`(`flags provided but not defined`)로 거부되며, `--app_data_dir=antigravity`는 조용히 무시되지 않고 실제 활성 데이터 디렉터리를 `~/.gemini/antigravity`로 전환함 (`AgyAdapter::verify_extension_data_dir`로 코드화). |
| **AGY-DATA-02** | 확장 로그인 상태 공유 및 인증 저장 위치 | **통과**. 데이터 디렉터리 내부에는 상태 파일(`~/.gemini/antigravity/antigravity_state.pbtxt` vs `~/.gemini/antigravity-cli/jetski_state.pbtxt`)과 `installation_id`가 별도로 존재함(파일 이름·존재만 확인, 내용 미열람). 빈 임시 디렉터리를 `--app_data_dir`로 지정하면 새 `jetski_state.pbtxt`와 `installation_id`를 만들고 `Authentication required. Please visit the URL to log in:`으로 실패함. 즉 인증 상태는 데이터 디렉터리별로 귀속되며, `--app_data_dir=antigravity`를 쓰면 별도 CLI 로그인 없이 VS Code 확장의 로그인 상태를 그대로 공유함. |
| **AGY-DATA-03** | 대화·로그·상태 파일 생성 위치 및 확장 UI 노출 | **확인 완료**. `--app_data_dir=antigravity` 실행 시 `~/.gemini/antigravity/brain/<conversation_id>`가 생성되고 확장과 공유하는 `conversation_summaries.db`에 기록됨. 옵션을 생략(기본값)하면 `~/.gemini/antigravity-cli/brain/<conversation_id>`에만 생성되어 확장 UI 대화 목록과 분리됨 (`env_completed_run_and_data_dir_comparison`에서 자동 검증). |
| **AGY-DATA-04** | 확장 허브 프로세스(`agy --hub`) 실행 중 동시 실행 충돌 여부 | **충돌 없음**. 확장 허브(`PID 21648`, `agy.exe --hub --hub-port=56727 --app_data_dir=antigravity ...`)가 떠 있는 상태에서 `--app_data_dir=antigravity`로 복수의 Managed Run(완료·도구 거부·취소·Resume)을 실행한 결과 SQLite WAL(`conversation_summaries.db-wal`)로 동작하여 잠금 오류·허브 중단·확장 오류 없이 모두 정상 종료됨. |
| **AGY-DATA-05** | 디렉터리별 `settings.json` 차이에 따른 권한·모드·모델 적용 | **확인 완료**. `-p "/permissions" --output-format stream-json` 실행 결과 두 디렉터리 모두 상위 `~/.gemini/settings.json`의 `"scope": "shared"` 권한 규칙을 동일하게 읽음. `-p "/config"`에서는 `antigravity-cli/settings.json`에 있는 `colorScheme`·`trustedWorkspaces` 차이가 있으나, Managed 실행은 `--model`, `--effort`, `--mode`, `--add-dir`를 명령줄 인자로 명시하므로 디렉터리 기본값 차이에 영향받지 않음. |

**확정 규칙 (Spec §7.5 반영 요청 사항):**
1. `AgyAdapter`는 기본값으로 `AgyDataDirMode::ExtensionShared`(`--app_data_dir=antigravity`)를 사용하여 DEC-06(VS Code 확장의 CLI·인증 활용)에 따라 별도 터미널 로그인 없이 즉시 동작하게 합니다. 확장 대화 목록과의 완전 분리가 필요한 경우 `AgyDataDirMode::DefaultCli`로 전환할 수 있습니다.
2. `AgyDataDirMode::ExtensionShared`(또는 `Custom`) 사용 시 실행 전 버전 확인(`AgyAdapter::verify_extension_data_dir`)에서 `--app_data_dir=antigravity --version` 수용 여부를 확인하며, CLI 업데이트로 옵션이 거부되면 다른 디렉터리로 조용히 대체하지 않고 `CapabilityState::Unsupported`로 거부합니다 (§7.5 규칙 5).

## 4. 시험 결과

환경: Windows 11 (`win32-x64`), `rustc` 1.94.0, Antigravity 확장 `1.9.0`, `agy.exe` `1.4.7`, 모델 `gemini-3.8-flash-low` (`--effort low`).

| 명령·시험 | 기대 결과 | 실제 결과 | 종류 |
|---|---|---|---|
| `cargo test -p telos-adapters --test agy_adapter` | 14개 통과 | 14개 통과 (0.00s) | Contract·Mock (`contract-tested`, 실제 출력 Fixture 재생) |
| `cargo test -p telos-adapters --test agy_environment -- --ignored --test-threads=1 --nocapture` | 6개 통과 | 6개 통과 | 실환경 (`environment-tested`) |
| `cargo test --workspace --locked` | 전체 통과 | 92개 통과, 12개 무시(실환경) | Workspace 전체 Contract·Mock |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 경고 0 | 통과 (경고 0) | 정적 검사 |
| `cargo fmt --all --check` | 차이 0 | 통과 | 포맷 검사 |

### 4.1 실환경 시험 상세 (`PROV-01~09`, `FORK-POC-01`, `AGY-DATA-01~05`)

| 시험 함수 (`agy_environment.rs`) | 검증 항목 | 실환경 결과 |
|---|---|---|
| `env_discovery_and_data_dir_option` | `PROV-01`, `AGY-DATA-01` | `NativeCliDiscoverer` + `AgyAdapter::cli_rule("win32-x64")`로 `google.google-antigravity` `1.9.0` 및 `C:\Users\Psyki-HOME\.gemini\bin\agy.exe`(`1.4.7`, `source: "extension"`, `location: "managed-path"`, SHA-256 `22588b6d5a6f6d2d66091c73dfb9255edc7bdafe9aff9dd20d958ce116496695`) 탐지 성공. 이어서 `AgyAdapter::verify_extension_data_dir`(`--app_data_dir=antigravity --version`) 호출로 `1.4.7` 반환 확인. |
| `env_completed_run_and_data_dir_comparison` | `PROV-02·03·04·06`, `AGY-DATA-02·03·04·05` | ① `ExtensionShared`: `session_started` → `text_delta`("AGY_PONG\n") → `usage`(입력 8106, 출력 5) → `turn_finished`(`completed`) → `process_exited`(0, `exited`, `childrenReaped: true`). `~/.gemini/antigravity/brain/56b9e7a0-...`에만 폴더가 생성됨을 확인 후 즉시 삭제. ② `DefaultCli`: `text_delta`("CLI_PONG\n") → `turn_finished`(`completed`) → `process_exited`(0). `~/.gemini/antigravity-cli/brain/e95b0514-...`에만 폴더가 생성됨을 확인 후 즉시 삭제. |
| `env_tool_denied` | `PROV-07` | 기본 `request-review` 모드에서 `whoami` 실행 요청 → `tool_started`(`run_command`) → `tool_completed`(`run_command`, `outcome: "denied"`) → `usage` → `error`(`kind: "tool_denied"`, `denied_actions: [RunCommand]`) → `turn_finished`(`failed`, `reason: "tool_denied"`) → `process_exited`(0). 종료 코드가 0이어도 소프트 거부 무응답을 실패로 판정함을 실환경에서 증명. |
| `env_invalid_model_fails` | `PROV-06` | `--model no-such-model-xyz` → `error`(`kind: "configuration"`) → `turn_finished`(`failed`) → `process_exited`(1, `exited`). 0 토큰 가짜 `usage` 이벤트를 만들지 않음. |
| `env_cancel_terminates_process_tree` | `PROV-08` | `session_started` 수신 직후 `cancel_run` 호출 → 취소 요청부터 프로세스 종료까지 `2.04s` 소요 → `turn_finished`(`cancelled`) → `process_exited`(`exitCode: 1`, `termination: "killed"`, `childrenReaped: true`). |
| `env_resume_and_fork_rejection` | `PROV-05`, `FORK-POC-01` | ① 첫 Run(`c93cab31-9381-479d-b509-1f33e6151108`)에서 코드워드 `birch-42` 저장 후 종료. ② `SessionMode::Resume`(`--conversation c93cab31-...`)으로 재개하여 같은 `nativeSessionId`에서 `birch-42` 응답 수신 확인. ③ `SessionMode::NativeFork` 요청은 프로세스 생성 전에 `UnsupportedCapability { capability: "native_fork" }`로 즉시 거부됨을 확인. 또한 CLI 직접 프로브(`agy -p "/fork" --output-format stream-json`)에서 종료 코드 2 및 `/fork is not available in print mode` 오류를 확인함. |

### 4.2 시험 세션 정리 확인

배정표 §6의 격리·정리 원칙에 따라 프로브 및 `agy_environment` 실행 중 생성된 모든 임시 대화 폴더(`~/.gemini/antigravity/brain/<id>` 및 `~/.gemini/antigravity-cli/brain/<id>`)는 `cleanup_session`으로 즉시 삭제했으며, 사용자의 기존 대화 파일은 일절 열람·수정·삭제하지 않았습니다.

## 5. 통합 인계 및 변경 요청 (`PH0-INTEGRATION` / `PH0-CONTRACT` / `PH0-HOST`)

1. **`crates/adapters/src/lib.rs`의 `pub mod agy;` 공개 전환 (`PH0-INTEGRATION`)**:
   - `crates/adapters/src/lib.rs`는 `PH0-CONTRACT` 소유이므로 독점 경로 규칙(Spec §19.5.3)을 준수해 본 브랜치(`ph0/agy`)에서는 수정하지 않고 `AgyAdapter`의 공개 메서드로 모든 계약·실환경 시험을 구성했습니다.
   - `PH0-CLAUDE` 통합(`b8e3fa9`)과 동일하게 통합 시 `crates/adapters/src/lib.rs`의 `mod agy;`를 `pub mod agy;`로 변경하면 `telos_adapters::agy::{AgyConfig, AgyLaunch, AgyLaunchResolver, AgyDataDirMode, cli_rule}`를 모듈 경로로도 바로 쓸 수 있습니다.
2. **Spec §7.5 Antigravity 데이터 디렉터리 검증 결과 반영 (`PH0-INTEGRATION`)**:
   - 본 문서 §3의 `AGY-DATA-01~05` 실환경 검증 결과와 확정 규칙(기본 `--app_data_dir=antigravity` 사용 + 실행 전 버전 명령으로 옵션 수용 확인, 미수용 시 조용한 대체 없이 `unsupported` 처리)을 `docs/DEVELOPMENT_v1.4.md` §7.5에 반영하고, 실제 운용 환경과 맞지 않는 "① 확장을 끈 상태" 전제를 정리해 줄 것을 요청합니다.
3. **VS Code `extensions.json`의 `"targetPlatform": "undefined"` 처리 참고 (`PH0-HOST` / `PH0-INTEGRATION`)**:
   - `google.google-antigravity`처럼 플랫폼 비의존(universal) 확장은 VS Code가 `~/.vscode/extensions/extensions.json`에 `"targetPlatform": "undefined"` 문자열을 기록합니다. 현재 `crates/adapters/src/discovery/mod.rs`는 `"universal"`만 허용하고 `"undefined"`를 건너뛰어 `ExtensionResolution::FolderScan`으로 대체 탐지되므로, `platform == "universal" || platform == "undefined"`를 함께 허용하면 `ExtensionResolution::Metadata`로 즉시 해석할 수 있습니다.

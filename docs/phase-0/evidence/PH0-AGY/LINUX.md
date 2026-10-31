# PH0-AGY Linux (`psyki-nas`) 실환경 검증 기록 (G3)

| 항목 | 값 |
|---|---|
| 작업 ID·담당 | PH0-AGY (Linux G3) · `Antigravity (Gemini) / PH0-AGY-LINUX-cli` |
| 작성·검증일 | 2026-10-25, Linux Remote SSH 호스트 (`psyki-nas`) |
| Branch / Worktree | Windows: `ph0/agy-linux` / `D:\worktree\TelosWeaver\ph0-agy-linux`<br>Linux (`psyki-nas`): `ph0/agy-linux` / `/home/gth1919/TelosWeaver-wt/agy-linux` |
| 기준 커밋 | `94e1369` (`master`: Phase 0 배정표 G3 Linux 실환경 배정 요청) |
| 대상 호스트 | `psyki-nas` (`ssh Psyki-NAS`, Ubuntu 26.04.1 LTS, Kernel `7.0.0-38-generic`, `x86_64`) |
| 대상 CLI | 원격 VS Code 서버 확장 `google.google-antigravity` `1.9.0` (`/home/gth1919/.vscode-server/extensions/google.google-antigravity-1.9.0`)이 관리하는 `/home/gth1919/.gemini/bin/agy`, 버전 `1.4.7` (SHA-256 `40349135371a8ed1f1af1e9f8b741de99174ae9fce6380b96c2d1f543480af7a`) |
| 시험 조건 | [G3_LINUX_ASSIGNMENTS.md](../PH0-INTEGRATION/G3_LINUX_ASSIGNMENTS.md) §1·§5: 모델 `gemini-3.8-flash-low`, `--effort low`, 저장소 밖 일회용 폴더(`/tmp/telos-agy-env/*`), `--dangerously-skip-permissions` 미사용, 생성된 모든 시험 세션(`brain/<id>` 및 `conversations/<id>.db*`) 즉시 삭제 및 ID 기록 |

## 1. Linux 호스트(`psyki-nas`) 환경 기록

[LINUX_REMOTE_HOST.md](../../LINUX_REMOTE_HOST.md) §4.1에 따른 실행 환경 기록:

```text
host_id=psyki-nas
kernel=7.0.0-38-generic
os=Ubuntu 26.04.1 LTS
rustc=rustc 1.99.1 (58cf8f9c3 2026-10-13)
cargo=cargo 1.99.1 (5f94df478 2026-08-27)
node=v24.22.1
npm=11.19.1
commit=94e1369
```

## 2. 수정 파일

| 경로 | 내용 |
|---|---|
| [crates/adapters/src/agy/protocol.rs](../../../../crates/adapters/src/agy/protocol.rs) | `classify_error_message`에서 Linux/SSH Headless 미로그인 시 반환되는 `"authentication failed or timed out"` 문자열이 `"timed out"`보다 먼저 `"authentication failed"`(`FailureKind::Authentication`)로 분류되도록 순서 및 패턴(`"not logged in"` 포함) 보완 (Spec §7.3) |
| [crates/adapters/src/agy/run.rs](../../../../crates/adapters/src/agy/run.rs) | `RunEntry::supervise`에서 `cancel_requested == true`인 상태에서 Linux `SIGTERM` 수신 직후 내부 서버가 닫히며 출력하는 종료 직전 `result`(`status: "ERROR"`, `"failed to send message: trajectory not found..."`)를 가짜 `Error` 이벤트 없이 `TurnFinished { status: TurnStatus::Cancelled, reason: Some("cancelled") }`로 귀결하도록 보완 (`PROV-08`) |
| [crates/adapters/tests/agy_adapter.rs](../../../../crates/adapters/tests/agy_adapter.rs) | `"authentication failed or timed out"` 오류 메시지가 `FailureKind::Authentication`으로 분류되는지 확인하는 계약 시험 단언 추가 (14개 전체 통과) |
| [crates/adapters/tests/agy_environment.rs](../../../../crates/adapters/tests/agy_environment.rs) | ① Linux(`psyki-nas`) 탐지 호스트(`host_id = "psyki-nas"`, `ExtensionResolution::Metadata`) 지원, ② `cleanup_session`에서 `~/.gemini/{antigravity,antigravity-cli}/brain/<id>`뿐 아니라 `conversations/<id>.{db,db-wal,db-shm,pb}`까지 함께 삭제하고 로그 출력, ③ Linux Remote SSH의 파일 기반 토큰(`antigravity-oauth-token`) 유무에 따른 `ExtensionShared` vs `DefaultCli` 비교 검증, ④ 7번째 실환경 시험 `env_workspace_file_write`(`accept-edits` + 정규화 경로 + `--add-dir`에서 Workspace 내부 파일 생성 확인) 추가 |
| [docs/phase-0/evidence/PH0-AGY/LINUX.md](LINUX.md) | 본 Linux (`psyki-nas`) 실환경 검증 증거 문서 |

## 3. A1: Antigravity Linux 실환경 시험 7개 실행 결과 (`Host = psyki-nas`)

실행 명령 (`psyki-nas`, `/home/gth1919/TelosWeaver-wt/agy-linux`):

```bash
cargo test -p telos-adapters --test agy_adapter --locked
cargo test -p telos-adapters --test agy_environment --locked -- --ignored --test-threads=1 --nocapture
```

| 구분 | 명령 / 시험 함수 | 검증 항목 | Host | 결과 | 비고 |
|---|---|---|---|---|---|
| Contract·Mock | `cargo test -p telos-adapters --test agy_adapter --locked` | `PROV-01~09`, `FORK-POC-01`, `AGY-DATA-01~05` | `psyki-nas` / `windows-local` | **14 / 14 통과** (0.00s) | 실제 `agy` 1.4.7 출력 Fixture 재생 + SSH 인증 오류 분류 계약 검증 |
| 실환경 1 | `env_discovery_and_data_dir_option` | `PROV-01`, `AGY-DATA-01` | `psyki-nas` | **통과 (`pass`)** | `/home/gth1919/.vscode-server/extensions/google.google-antigravity-1.9.0`(`resolution: "metadata"`) 및 `/home/gth1919/.gemini/bin/agy`(`1.4.7`, `source: "extension"`, `location: "managed-path"`, SHA-256 `40349135371a8ed1f1af1e9f8b741de99174ae9fce6380b96c2d1f543480af7a`) 탐지 성공. `AgyAdapter::verify_extension_data_dir`(`--app_data_dir=antigravity --version`) → `1.4.7` 확인 |
| 실환경 2 | `env_completed_run_and_data_dir_comparison` | `PROV-02·03·04·06`, `AGY-DATA-02~05` | `psyki-nas` | **통과 (`pass`)** | ① `ExtensionShared`(`--app_data_dir=antigravity`): `psyki-nas`의 `~/.gemini/antigravity`에는 SSH 파일 토큰(`antigravity-oauth-token`)이 없어 다른 디렉터리로 조용히 대체하지 않고 `Error(Authentication)` + `TurnFinished(Failed)` + `ProcessExited(exitCode: 1)`로 정확히 보고됨을 확인. ② `DefaultCli`(`~/.gemini/antigravity-cli`): `SessionStarted` → `TextDelta("CLI_PONG\n")` → `Usage(in: 7607, out: 4)` → `TurnFinished(Completed)` → `ProcessExited(0, Exited, childrenReaped: true)` 및 `antigravity-cli/{brain,conversations}` 생성 확인 |
| 실환경 3 | `env_workspace_file_write` | `PROV-02·07`, `AGY-DATA-03` | `psyki-nas` | **통과 (`pass`)** | `accept-edits` 모드 + `canonicalize()`된 `/tmp/telos-agy-env/workspace-write` + `--add-dir` 전달 시 `ToolStarted(write_file)` → `ToolCompleted(write_file, Succeeded)` → `TurnFinished(Completed)` → `ProcessExited(0)`. 파일이 `scratch/`가 아닌 실제 Workspace(`/tmp/telos-agy-env/workspace-write/result-agy.txt`, 내용 `"pong"`)에 생성됨을 확인 |
| 실환경 4 | `env_tool_denied` | `PROV-07` | `psyki-nas` | **통과 (`pass`)** | 기본 `request-review` 모드에서 `run_command`(`whoami`) 요청 → `ToolStarted(run_command)` → `ToolCompleted(run_command, Denied)` → `Usage(in: 6809, out: 36)` → `Error(ToolDenied)` → `TurnFinished(Failed, reason: "tool_denied")` → `ProcessExited(exitCode: 0, Exited)`. 종료 코드 0이어도 소프트 거부를 실패로 판정 |
| 실환경 5 | `env_invalid_model_fails` | `PROV-06` | `psyki-nas` | **통과 (`pass`)** | `--model no-such-model-xyz` → `Error(Configuration)` → `TurnFinished(Failed)` → `ProcessExited(exitCode: 1, Exited)`. 0 토큰 가짜 `Usage` 미발행 확인 |
| 실환경 6 | `env_cancel_terminates_process_tree` | `PROV-08` | `psyki-nas` | **통과 (`pass`)** | `SessionStarted` 직후 `cancel_run` 호출 → `SIGTERM` 전송 후 `454.4ms` 만에 종료 → `TurnFinished(Cancelled, reason: "cancelled")` → `ProcessExited(exitCode: 1, termination: "terminated_on_request", childrenReaped: true)` |
| 실환경 7 | `env_resume_and_fork_rejection` | `PROV-05`, `FORK-POC-01` | `psyki-nas` | **통과 (`pass`)** | ① 첫 Run(`8e09bbd2-fd8e-4a6f-8043-ffca9197e9c5`)에서 `birch-42` 기억 후 종료. ② `SessionMode::Resume`(`--conversation 8e09bbd2-...`)으로 재개해 같은 세션 ID에서 `birch-42` 응답 확인. ③ `SessionMode::NativeFork`는 프로세스 시작 전에 `UnsupportedCapability { capability: "native_fork" }`로 거부 |

## 4. A2: Linux (`psyki-nas`)에서의 `AGY-DATA` (Spec §7.5) 검증

원격 서버 확장 `/home/gth1919/.vscode-server/extensions/google.google-antigravity-1.9.0` 및 `~/.gemini/bin/agy` `1.4.7` 조합에서 `AGY-DATA-01~05`를 검증했습니다.

| ID | 확인 항목 | Linux (`psyki-nas`) 실환경 관찰 및 판정 |
|---|---|---|
| **AGY-DATA-01** | `--app_data_dir=antigravity` 옵션 수용 여부 | **통과 (`supported`)**. `/home/gth1919/.gemini/bin/agy --app_data_dir=antigravity --version` 실행 시 종료 코드 `0`과 버전 `1.4.7`을 반환하며, 활성 데이터 디렉터리를 `/home/gth1919/.gemini/antigravity`로 전환함 (`~/.gemini/antigravity/cli.log` 기록 확인). |
| **AGY-DATA-02** | 확장 로그인 상태 공유 및 Linux Remote SSH 토큰 저장 방식 차이 | **핵심 플랫폼 차이 발견 및 규명 완료**. 아래 §4.1에 상세 기록. |
| **AGY-DATA-03** | 대화(`brain/<id>`, `conversations/<id>.db`) 및 Workspace 파일 생성 위치 | **확인 완료**. ① 대화 상태는 지정된 `<app_data_dir>/brain/<id>`와 `<app_data_dir>/conversations/<id>.db`에만 생성되며 다른 데이터 디렉터리로 새지 않음. ② Linux에서는 `std::fs::canonicalize()`가 `/tmp/telos-agy-env/workspace-write`처럼 `\\?\` 접두사 없는 표준 POSIX 절대 경로를 반환하므로, `agy`의 내부 경로 비교에서 Workspace 밖 경로로 오인되지 않고 `result-agy.txt`(`pong`)를 `scratch/`가 아닌 실제 Workspace 안에 바로 기록함 (`env_workspace_file_write` 통과). |
| **AGY-DATA-04** | 원격 확장 프로세스와의 공존·충돌 여부 | **충돌 없음**. `~/.gemini/antigravity` 및 `~/.gemini/antigravity-cli` 모두 SQLite WAL 모드로 동작하며, 복수의 Headless Run 실행 및 세션 정리 중 잠금 오류나 손상 없음. |
| **AGY-DATA-05** | 디렉터리별 설정 차이 영향 | **영향 없음**. Managed 실행은 `--model`, `--effort`, `--mode`, `--add-dir`, `--disable-slash-commands`를 명시적으로 전달하므로 디렉터리별 기본 설정 차이에 영향받지 않음. |

### 4.1 Linux Remote SSH 환경의 `AGY-DATA-02` 인증 저장 구조 (Windows 로컬과의 차이)

`psyki-nas`에서 `~/.gemini/antigravity/cli.log` 및 디렉터리 구성을 조사한 결과(인증 파일 내용은 열람하지 않고 파일 이름·메타데이터·`cli.log`의 저장소 선택 로그만 확인), Windows 로컬과 Linux Remote SSH 사이에 다음과 같은 구조적 차이가 있음을 확인했습니다:

1. **Windows 로컬 (`windows-local`)**:
   - `agy.exe`가 OS 자격 증명 저장소(Windows Credential Manager)를 기본 토큰 저장소로 사용합니다.
   - `~/.gemini/antigravity`와 `~/.gemini/antigravity-cli` 양쪽 모두에 `jetski_state.pbtxt`가 존재하고 OS 키링의 OAuth 토큰을 공유하므로, `--app_data_dir=antigravity`로 Headless 실행 시 별도 CLI 로그인 없이 즉시 인증됩니다.
2. **Linux Remote SSH (`psyki-nas`)**:
   - `agy` 내부 서버(`language_server_linux_x64`)는 `SSH_CONNECTION` / `SSH_CLIENT` 환경 변수를 감지하면 OS 키링 대신 **데이터 디렉터리 내부의 파일 기반 토큰 저장소**(`<app_data_dir>/antigravity-oauth-token` + `<app_data_dir>/jetski_state.pbtxt`)를 사용합니다:
     ```text
     I1025 22:02:13.214174 873851 composite_token_storage.go:123] Using file-based token storage because SSH session detected
     ```
   - 한편 VS Code Remote-SSH에서 동작하는 `google.google-antigravity` 확장은 OAuth 토큰을 **로컬(클라이언트) VS Code `SecretStorage`**에 보관하고, 원격 서버의 `agy --hub --app_data_dir=antigravity` 프로세스에는 gRPC(`resolveOauthToken`)로 메모리에만 전달합니다. 따라서 원격 서버의 `~/.gemini/antigravity/`에는 `antigravity_state.pbtxt`(30B)만 있고 `antigravity-oauth-token` 및 `jetski_state.pbtxt` 파일이 생성되지 않습니다.
   - 반대로 사용자가 `psyki-nas` SSH 터미널에서 수행한 CLI 로그인(`agy login` / `agy`)은 CLI 기본 디렉터리인 `~/.gemini/antigravity-cli/`에 `antigravity-oauth-token`(622B)과 `jetski_state.pbtxt`(920B)를 저장했습니다.
   - 그 결과 `psyki-nas`에서 `--app_data_dir=antigravity`로 Headless Print 모드를 실행하면 `antigravity/` 안에 파일 토큰이 없어 `{"event":"result","result":{"status":"ERROR","error":"authentication failed or timed out"}}` (종료 코드 `1`)로 실패하고, `--app_data_dir`를 생략한 기본 CLI 디렉터리(`~/.gemini/antigravity-cli`, `AgyDataDirMode::DefaultCli`)에서는 정상 인증되어 성공합니다.

## 5. A3: Turn 직렬화·도구 거부·종료 코드 vs 결과 비교 (Linux vs Windows, Spec §19.1 G3)

| 비교 항목 | Windows 로컬 (`windows-local`, `win32-x64`) | Linux 원격 SSH (`psyki-nas`, `linux-x64`) | 호환성 판정 |
|---|---|---|---|
| **CLI 탐지 (`PROV-01`)** | `~/.vscode/extensions/google.google-antigravity-1.9.0` (`Metadata`) → `~/.gemini/bin/agy.exe` `1.4.7` | `~/.vscode-server/extensions/google.google-antigravity-1.9.0` (`Metadata`) → `~/.gemini/bin/agy` `1.4.7` | **동일 계약**. 두 OS 모두 `ExtensionResolution::Metadata` 및 `BinaryLocation::ManagedPath`로 탐지됨 |
| **Turn 직렬화 (`PROV-02·03·05`)** | 1 Run = 1 프로세스 = 1 Turn (`--input-format stream-json --output-format stream-json`, 프롬프트 1줄 전송 후 즉시 `stdin` EOF 닫기). 후속 Turn은 `result` 확인 후 `--conversation <id>`로 직렬화 실행 | **완전 동일**. `stdin` 닫기 직후 단일 Turn만 수행하고 `result` 출력 뒤 종료 코드 `0`으로 자체 종료됨. `--conversation <id>` Resume 시 동일 `conversation_id` 유지 | **완전 일치** |
| **Native Fork 거부 (`FORK-POC-01`)** | `SessionMode::NativeFork` → `UnsupportedCapability { capability: "native_fork" }` | **완전 동일** | **완전 일치** |
| **도구 소프트 거부 (`PROV-07`) 및 종료 코드 vs 결과** | 기본 `request-review` 모드에서 `run_command` 호출 시 `step_update`(`state: "ERROR"`, `"permission check failed..."`) + `result.status: "SUCCESS"`, `result.response: ""`, `denied_actions: [{"tool_type":"RunCommand"}]`, **종료 코드 `0`** → Adapter가 `ToolCompleted(Denied)` + `Error(ToolDenied)` + `TurnFinished(Failed, reason: "tool_denied")`로 변환 | **완전 동일**. Linux에서도 `agy` 1.4.7은 종료 코드 `0`과 `status: "SUCCESS"`를 반환하며, `AgyAdapter`가 동일하게 `ToolDenied` 및 `TurnStatus::Failed`로 판정함 | **완전 일치** (Spec §2.1·§7.3: 종료 코드 0 ≠ Turn 성공) |
| **잘못된 모델 오류 (`PROV-06`)** | `result.status: "ERROR"`, 종료 코드 `1` → `Error(Configuration)` + `TurnFinished(Failed)`, 가짜 `Usage` 없음 | **완전 동일** (`exitCode: 1`, `FailureKind::Configuration`) | **완전 일치** |
| **인증 실패 메시지 분류 (Spec §7.3)** | 미인증 시 stderr `"Authentication required. Please visit the URL to log in:"` | SSH Headless 미인증 시 stdout `result.error: "authentication failed or timed out"` + stderr `"Error: authentication required. Run 'agy' to log in, then retry."` (`protocol.rs` 수정으로 `FailureKind::Authentication` 분류 일치) | **일치 완료** (`aa347cc`) |
| **취소 및 프로세스 트리 정리 (`PROV-08`)** | Graceful 종료 신호 미응답으로 유예 시간(`2.04s`) 경과 후 Windows Job Object가 강제 종료(`termination: "killed"`, `exitCode: 1`, `childrenReaped: true`), 종료 전 `result` 미출력 → `TurnFinished(Cancelled)` | 프로세스 그룹 `SIGTERM` 수신 즉시(`454ms~516ms`) 내부 서버가 종료되며 `termination: "terminated_on_request"`, `exitCode: 1`, `childrenReaped: true`로 종료. 단, 내부 서버가 먼저 닫히며 종료 직전 `result(ERROR: "trajectory not found")`를 한 줄 출력하므로 `run.rs`에서 `cancel_requested` 시 이를 `TurnFinished(Cancelled)`로 정규화함 | **일치 완료** (`aa347cc`). Linux가 `SIGTERM`에 즉시 응답해 약 4배 빠르게 정리됨 |
| **Workspace 경로 정규화 (`AGY-DATA-03`)** | `std::fs::canonicalize()`가 `\\?\D:\...` 확장 경로 접두사를 붙여 그대로 전달하면 `agy`가 Workspace 밖 경로로 오인하고 `scratch/`에 파일을 씀 → `\\?\` 접두사 제거 필요 | `std::fs::canonicalize()`가 `/tmp/telos-agy-env/workspace-write` 표준 경로를 반환하므로 `\\?\` 문제 자체가 발생하지 않으며 `accept-edits` + `--add-dir`에서 Workspace 내부에 즉시 파일 생성 | **검증 완료** (`env_workspace_file_write`) |

## 6. A4: 생성·삭제된 시험 세션 ID 목록 및 정리 확인

시험 중 생성된 모든 세션은 각 시험 종료 시 `cleanup_session`을 통해 `<app_data_dir>/brain/<id>` 디렉터리와 `<app_data_dir>/conversations/<id>.{db,db-wal,db-shm,pb}` 파일을 즉시 삭제했습니다. 기존 사용자 대화는 건드리지 않았습니다.

| 단계 | 세션 ID (`conversation_id`) | 생성 위치 (`psyki-nas`) | 삭제된 항목 | 현재 상태 |
|---|---|---|---|---|
| 초기 CLI 인증 프로브 | `10a453e8-9aa6-4771-9ab7-1a8e2979b30f` | `~/.gemini/antigravity-cli/` | `brain/10a453e8-...`, `conversations/10a453e8-....db` | **삭제 완료** |
| `env_cancel_terminates_process_tree` (1차 실행) | `1bf1e63d-a524-4a8d-81dc-be2233a58069` | `~/.gemini/antigravity-cli/` | `brain/1bf1e63d-...`, `conversations/1bf1e63d-....db` | **삭제 완료** |
| `env_completed_run_and_data_dir_comparison` | `6e07d3f4-1c24-45eb-8adf-702047a877c7` | `~/.gemini/antigravity-cli/` | `brain/6e07d3f4-...`, `conversations/6e07d3f4-....db` | **삭제 완료** |
| `env_resume_and_fork_rejection` (Parent + Resume) | `8e09bbd2-fd8e-4a6f-8043-ffca9197e9c5` | `~/.gemini/antigravity-cli/` | `brain/8e09bbd2-...`, `conversations/8e09bbd2-....db` | **삭제 완료** |
| `env_tool_denied` | `bab7ab86-13a8-445a-a315-5038a9243492` | `~/.gemini/antigravity-cli/` | `brain/bab7ab86-...`, `conversations/bab7ab86-....db` | **삭제 완료** |
| `env_workspace_file_write` | `9321b2c3-b33f-463f-8c7e-9d84d377313f` | `~/.gemini/antigravity-cli/` | `brain/9321b2c3-...`, `conversations/9321b2c3-....db` | **삭제 완료** |
| `env_cancel_terminates_process_tree` (2차 검증) | `ba437e1b-aeeb-423d-b503-9f539dcf2ab0` | `~/.gemini/antigravity-cli/` | `brain/ba437e1b-...`, `conversations/ba437e1b-....db` | **삭제 완료** |

정리 후 `psyki-nas` 상태 확인 명령 및 결과:

```text
$ ssh Psyki-NAS 'ls -la ~/.gemini/antigravity/brain ~/.gemini/antigravity/conversations ~/.gemini/antigravity-cli/brain ~/.gemini/antigravity-cli/conversations'
/home/gth1919/.gemini/antigravity-cli/brain: (비어 있음, . 및 .. 만 존재)
/home/gth1919/.gemini/antigravity-cli/conversations: (비어 있음, . 및 .. 만 존재)
/home/gth1919/.gemini/antigravity/brain: (비어 있음, . 및 .. 만 존재)
/home/gth1919/.gemini/antigravity/conversations: (비어 있음, . 및 .. 만 존재)
```

일회용 작업 디렉터리 `/tmp/telos-agy-env` 또한 삭제 완료했습니다.

## 7. 통합 인계 사항 (`PH0-INTEGRATION` 참고)

1. **Linux Remote SSH에서 `phase0_g2` 실행 시 Antigravity 데이터 디렉터리 선택 (`crates/core/src/integration/`)**:
   - §4.1에서 확인한 바와 같이, Windows 로컬에서는 `--app_data_dir=antigravity`(`AgyDataDir::ExtensionShared`)가 OS 자격 증명 저장소를 통해 VS Code 확장 로그인을 그대로 공유하지만, Linux Remote SSH(`psyki-nas`)에서는 `agy`가 파일 기반 토큰(`<app_data_dir>/antigravity-oauth-token`)을 사용하며 VS Code 원격 확장은 토큰을 파일로 저장하지 않고 메모리(gRPC)로만 주입합니다.
   - 따라서 `psyki-nas`에서 Headless CLI 실행 시 `~/.gemini/antigravity/antigravity-oauth-token`이 없으면(예: 사용자가 `agy --app_data_dir=antigravity login`을 별도로 실행하지 않은 상태) `--app_data_dir=antigravity`는 `FailureKind::Authentication`으로 실패하고, `~/.gemini/antigravity-cli`(`AgyDataDir::Default`)에만 CLI 토큰이 존재합니다.
   - `PH0-INTEGRATION`에서 `psyki-nas`의 `phase0_g2` 실환경 시험을 돌릴 때는 ① `crates/core/tests/phase0_g2.rs`(또는 `AgyLaunchResolver`)에서 `~/.gemini/antigravity/antigravity-oauth-token` 부재 시 `AgyDataDir::Default`를 쓰도록 구성하거나, ② 사용자가 `agy --app_data_dir=antigravity`로도 1회 CLI 로그인을 수행한 상태에서 `ExtensionShared`를 쓸 수 있습니다 (규칙에 따라 본 작업에서는 사용자의 인증 토큰 파일을 복사하거나 수정하지 않았습니다).
2. **Linux `SIGTERM` 취소 시 종료 직전 `result(ERROR)` 처리 완료 (`crates/adapters/src/agy/run.rs`)**:
   - Linux에서 `cancel_run` 호출 시 `agy`가 `SIGTERM`으로 종료되면서 내부 서버가 먼저 닫혀 출력하는 `result(ERROR)`를 `TurnStatus::Cancelled`로 정규화했으므로, `phase0_g2`의 `g2_codex_and_agy_cancel_reports_actual_termination` 단언(`turn_finished.status == "cancelled"`)이 Linux에서도 그대로 통과합니다.

# PH0-HOST 구현 및 검증 기록

| 항목 | 값 |
|---|---|
| 작업 ID·담당 | PH0-HOST · Codex / `PH0-HOST-20261024` |
| 작성·검증일 | 2026-10-24, Windows 로컬 |
| 상태 | in-progress. 소유 경로 구현·독립 검증 완료, 공통 등록·의존성·진입점 통합과 Linux 실행 검증 대기 |
| Branch / Worktree | `ph0/host` / `D:\worktree\TelosWeaver\ph0-host` |
| Spec·계약 기준 | `23605b0` (Spec v1.3.7), `ba97e7b` (G0 개정 1) |
| 착수·구현 Commit | `9522d79` (착수 기록), `159617e` (구현), `53ad1de` (최종 경계 검사 보완) |
| 기준 문서 | [Spec](../../../DEVELOPMENT_v1.4.md), [배정표](../../WORK_ALLOCATION.md) |

사용자의 착수 지시에 따라 전용 Worktree에서 구현했습니다. 공통 Schema, Adapter Trait, Cargo.toml·Cargo.lock, Adapter 루트 모듈 등록, Core main.rs는 직접 수정하지 않았습니다. REQ-010·011의 확정·반영과 게이트 판정은 기존 담당자가 처리합니다. 이 기록은 G1·G3 통과 판정이 아닙니다.

## 1. 수정 파일

구현 파일 목록은 `git diff --name-only 9522d79..53ad1de`로 확인합니다.

| 소유 경로 | 파일·내용 |
|---|---|
| `crates/adapters/src/runtime/` | `mod.rs`, `native.rs`, `windows.rs`, `unix.rs`. G0 Trait을 유지하고 `NativeRuntime` 추가 |
| `crates/adapters/src/discovery/` | `mod.rs`. REQ-010 확정 전 내부 탐색 엔진 |
| `crates/adapters/tests/` | `runtime_native.rs`, `discovery_extensions.rs`, `discovery_resolution.rs`, `fixtures/runtime/fixture.rs` |
| `crates/core/src/api/` | `mod.rs`, `security.rs`, `server.rs` |
| `crates/core/src/execution/` | `mod.rs`, `supervisor.rs` |
| `crates/core/tests/` | `host_api.rs`, `host_execution.rs`, `fixtures/host/support.rs` |
| 이 증거 디렉터리 | 본 기록, [통합 인계](HANDOFF.md), [REQ-011 적용안](REQ-011-INTEGRATION.patch), [진입점 원문](daemon_entrypoint.rs), [검증 스크립트](Validate-Host.ps1), `VALIDATION-Cargo.lock` |

문서·적용안은 구현 Commit 뒤의 별도 인계 Commit에 포함합니다. 공통 파일의 실제 변경이 아니라 검토용 Artifact입니다.

## 2. 구현한 기능

### 2.1 공용 런타임

- Windows는 `CreateProcessW`의 Job List Attribute로 Job Object를 생성 시점에 연결하고 필요한 stdin/stdout/stderr Handle만 상속합니다. Unicode·공백·따옴표·끝의 역슬래시와 환경 변수 변경·삭제를 처리합니다. `.cmd`·`.bat`·확장자 없는 Shim을 실행 파일 대신 사용하지 않습니다.
- Unix는 새 Process Group에서 실행하고 SIGTERM 뒤 유예 시간이 지나면 SIGKILL을 보냅니다. 신호 종료를 자연 종료로 기록하지 않습니다. 이 경로는 Linux 대상 컴파일·Clippy까지만 확인했습니다.
- stdout/stderr 공통 sequence, 줄·Queue 상한, `LineTooLong`, `ReadError`를 구현했습니다. 긴 줄을 잘린 정상 Protocol 데이터로 전달하지 않습니다. 출력 Backpressure와 종료 제어를 분리했습니다.
- 취소된 `wait` 재호출, stdin·wait·취소 동시 사용, 반복·동시 `terminate`의 동일 결과를 지원합니다. 종료 Future를 버려도 시작한 종료 작업은 계속됩니다. 종료·자식 회수를 확인하지 못하면 성공으로 보고하지 않습니다.

### 2.2 CLI 탐색 엔진

- 명시 경로 → 확장이 포함·관리하는 실행 파일 → PATH 순서로 선택합니다. 명시 경로나 선택 후보의 버전 확인이 실패하면 조용히 다른 파일을 실행하지 않습니다.
- `extensions.json`, `.obsolete`, SemVer·플랫폼 접미사를 해석합니다. 활성 메타데이터 우선, 부재 시 가장 높은 적용 가능 버전 선택, 경로 탈출·여러 후보·잘못된 메타데이터 거부를 처리합니다.
- 예상 상대 경로가 없을 때만 확장 내부를 제한 검색합니다. 깊이 8·항목 10,000개, 메타데이터 4 MiB 상한을 둡니다. PATH의 상대 경로·현재 디렉터리는 제외합니다.
- 공용 런타임으로 버전을 확인하고 SHA-256을 계산합니다. 버전 확인은 5초, 한 줄 16 KiB·총 출력 64 KiB로 제한하며 확인 중 해시가 바뀌면 거부합니다.
- 선택 경로·출처·확장 버전·CLI 버전·해시를 내부 결과에 보관합니다. 공개 DTO·Host·Snapshot 연결과 `pub mod discovery` 등록은 REQ-010 확정이 필요합니다. 내부 타입은 공통 계약의 대체물이 아닙니다.

### 2.3 Local API와 실행 감독

- G0 projectKey·주소·사용자 상태 디렉터리·시작마다 바뀌는 32바이트 토큰 규칙을 구현했습니다. Windows Pipe·토큰 파일에 현재 사용자 SID만 허용하는 DACL을 적용하고 실패 시 시작하지 않습니다. 두 번째 Core가 기존 토큰을 바꾸기 전에 바인딩을 실패시킵니다.
- UTF-8 NDJSON의 1 MiB 상한을 LF 제외 바이트 수로 검사합니다. 초과 시 빈 requestId의 `frame_too_large` 뒤 연결을 닫습니다. 토큰·수신 JSON을 로그나 오류에 복사하지 않습니다.
- `core.hello`, `core.health`, `runs.start`, `runs.inspect`, `runs.cancel`, `runs.subscribe`, `runs.unsubscribe`를 연결했습니다. 인증 전 요청 거부, 구독 응답 후 이벤트 전달, sequence 기반 재생을 처리합니다. 연결 종료를 Run 취소로 취급하지 않습니다.
- `ExecutionSupervisor`가 Run ID·attempt·멱등 결과를 관리합니다. 같은 키·다른 내용은 conflict입니다. Provider 이벤트를 G0 검사기로 확인하며 순번 누락·잘못된 종료·수집 실패를 성공으로 기록하지 않습니다. 취소 접수와 실제 종료를 구분합니다.
- 신뢰된 `RunAuthorizer`가 승인된 Workspace·Snapshot·Policy 참조를 제공해야 합니다. 기본 `DenyRuns`와 제안 진입점은 실행을 거부합니다. 미구현 Scheduler·영속 저장소를 정상으로 보고하지 않으며 Health는 degraded입니다. 실행 성공과 Task 완료·검증 통과를 구분합니다.

내부 상한은 연결 32개, 연결별 구독 32개·송신 Queue 8개, 시작 요청 128개·직렬화 합계 8 MiB, Run별 이벤트 4,096개·8 MiB, 전체 이벤트 32 MiB입니다. Phase 0 메모리 보존 한계이며 운영 예산 계약을 대신하지 않습니다. 상한 초과 시 실패·취소 또는 구독 종료로 처리합니다. 같은 OS 사용자 Worker의 토큰 접근을 막는 Sandbox는 이 범위에 없습니다.

## 3. Stub·미지원·미검증

| 항목 | 현재 상태 |
|---|---|
| 공식 Workspace 빌드·시험 | REQ-011 미반영으로 통합 검증 미실행. 아래 독립 Manifest 결과와 구분합니다. |
| discovery 공개 연결 | REQ-010 대기. 엔진은 전용 시험에서 소스 경로로 포함하며 최종 공개 API는 미확정입니다. |
| Daemon 진입점 | 원래 main.rs를 유지했습니다. 컴파일·버전 실행을 확인한 적용안과 패치를 제공합니다. 기본 정책은 `DenyRuns`입니다. |
| 실제 Provider | Codex·Claude·Agy는 다른 작업입니다. 외부 AI CLI 실행·로그인·네트워크 전송을 수행하지 않았습니다. 탐색 버전 확인도 시험 대역입니다. |
| 정책·Snapshot·Workspace | 신뢰된 승인 Port만 제공합니다. 서비스 구현·연결은 별도 작업입니다. |
| 보존·재시작 | 메모리만 사용합니다. SQLite·재시작 복구·Writer Lock·Scheduler·Artifact 저장은 미구현입니다. |
| Daemon 종료·활성 Run | API 서버 종료는 연결·구독을 정리합니다. 실행 허용 정책을 붙이는 진입점은 활성 Run 취소·실제 종료 확인도 조합해야 합니다. 현재 적용안은 Run 시작을 허용하지 않습니다. |
| Linux·Remote SSH | WSL 배포판 미설치. 프로세스 그룹·Socket 권한·자식 회수·Remote SSH 실행은 미검증입니다. Rust Linux Target만 추가해 교차 검사했습니다. |
| 권한 분리 | 다른 OS 사용자 로그인 시험은 미실행입니다. Windows 실제 객체의 DACL ACE·SID는 확인했습니다. Unix Process Group의 의도적 탈출을 막는 것은 Sandbox 영역입니다. |

## 4. 시험

환경은 Windows, rustc/cargo 1.94.0입니다. 독립 Manifest는 저장소 밖 `C:\Users\Psyki-HOME\AppData\Local\Temp\telos-host-repro-20261024\Cargo.toml`에 만들었습니다. 테스트 실행 파일·런타임 Fixture도 임시 디렉터리에 생성합니다. 저장소의 공통 Cargo.toml·Cargo.lock은 변경하지 않습니다.

확인한 버전은 Tokio 1.54.1, windows-sys 0.61.2, getrandom 0.4.3, libc 0.2.191, semver 1.0.28, sha2 0.11.1입니다. 전이 의존성은 `VALIDATION-Cargo.lock`에 보관합니다. 이는 공식 Workspace Lock의 대체물이 아닙니다.

| 명령·검사 | 날짜·Host | 기대 결과 | 실제 결과·종료 코드 | 종류 |
|---|---|---|---|---|
| `Validate-Host.ps1 -CheckLinux -Offline` | 2026-10-24, Windows | 독립 시험·양쪽 Clippy 통과 | 30개 통과, Windows·Linux 대상 Clippy 통과, 0 | 아래 종류 혼합, 공식 통합 아님 |
| `cargo test --manifest-path <독립 Manifest> --workspace --offline -- --test-threads=1` | 동일 | HOST 전용 시험 통과 | 런타임 10 + I/O 오류 1 + 탐색 7 + API 단위 3 + API 연결 5 + 감독 4 = 30, 0 | 로컬 프로세스·Pipe 실환경 / Contract / Mock |
| `cargo clippy --manifest-path <독립 Manifest> --workspace --all-targets --offline -- -D warnings` | 동일 | 경고 없음 | 통과, 0 | 정적 검사 |
| 위 명령에 `--target x86_64-unknown-linux-gnu` 추가 | Windows 교차 검사 | Linux 코드·시험·제안 진입점 검사 | 통과, 0. Linux 실행 결과가 아님 | 조건부 컴파일·정적 검사 |
| 제안 실행 파일 `--version` | 동일 | 버전 출력 | `telos-weaver 0.0.0`, 0 | 실행 파일 실환경 |
| `git apply --check docs/phase-0/evidence/PH0-HOST/REQ-011-INTEGRATION.patch` | 동일 | 적용 가능 | 통과, 0. 실제 적용은 하지 않음 | 인계 Artifact 검사 |
| 소유 Rust 경로 `rustfmt --check`, `git diff --check` | 동일 | 형식·공백 오류 없음 | 통과, 0 | 정적 검사 |

재현 명령입니다. PowerShell 7·Rust가 필요합니다. `-CheckLinux`는 `x86_64-unknown-linux-gnu` Target 설치가 필요하며 스크립트가 WSL·OS 설정을 설치하거나 변경하지는 않습니다. Registry Cache가 없는 환경은 `-Offline`을 생략합니다.

```powershell
& ./docs/phase-0/evidence/PH0-HOST/Validate-Host.ps1 -CheckLinux -Offline
```

스크립트는 새 임시 디렉터리에 Manifest와 보관한 Lock을 복사하고 `--locked`로 실행합니다. 검증 디렉터리는 조사할 수 있도록 남깁니다. 공식 의존성 통합 후에는 공식 Workspace 명령으로 다시 판정해야 합니다.

## 5. 반례 결과

- 대기 Future 취소 후 재대기, 동시·반복 종료, 종료 Future를 버린 뒤의 프로세스 회수를 확인했습니다.
- 출력 미소비·막힌 1 MiB stdin 쓰기 중에도 프로세스 트리 종료가 완료됩니다. 부모가 스스로 먼저 끝나도 Job의 자식을 회수합니다. 긴 줄·주입 I/O 오류는 명시적 이벤트로 전달합니다.
- 잘못된 토큰, 인증 전 요청, 다른 프로토콜, 알 수 없는 메서드, 잘못된 UTF-8·요청 형식·중복 hello, 다른 프로젝트·Host, 정책 미연결, 같은 멱등 키의 내용 변경을 거부했습니다.
- 정확히 1 MiB인 프레임을 수신하고 초과 시 오류 뒤 EOF를 확인했습니다. 한글을 한 바이트씩 나누어 보내도 복원합니다.
- 두 번째 Core의 시작 실패는 기존 토큰을 유지하며 재시작하면 이전 토큰을 거부합니다. 프로젝트 내부 토큰 경로와 디렉터리인 api-token 경로를 거부합니다. 실제 Pipe·토큰 DACL에는 현재 사용자 ACE만 있습니다.
- 완료 전·후 재연결 시 Run을 유지하고 sequence 이후 이벤트를 회수했습니다. 순번 누락·종료 없는 Stream은 interrupted, 시작 실패 종료 이벤트는 failed로 처리합니다.
- 탐색 시험은 활성 메타데이터, 숫자 버전, 플랫폼·폐기 폴더, 경로 탈출·중복 후보·깨진 메타데이터, 후보 부재, 우선순위·출처·해시, 버전 확인 실패를 포함합니다.

## 6. 변경 요청과 인계

### REQ-010 CLI 탐색 계약과 모듈 등록

- **처리·상태:** CONTRACT가 공통 타입·Adapter 루트 등록, INTEGRATION이 기준 조정. 검토 대기입니다.
- **필요 계약:** 실행 Host·플랫폼·명시 경로·확장 루트, Provider 규칙의 확장 ID·상대/관리 경로·버전 명령, 결과의 Provider·Host·출처·실행 경로·확장/CLI 버전·해시·미탐지 사유.
- **구현 경계:** 내부 `DiscoveryRule`, `SelectedCli`는 `pub(crate)`이며 공통 DTO를 대신하지 않습니다. 확정된 DTO로 매핑하고 공개 Port를 연결해야 합니다. 모듈 등록만 먼저 하지 말고 DTO·소비 경로를 함께 처리하십시오.
- **시험 연결:** 탐색 시험의 소스 경로 포함을 정식 공개 API 호출로 전환하고 실제 CLI 버전 확인은 각 Provider PoC에서 추가합니다.

### REQ-011 실행 의존성과 Daemon 진입점 연결

- **처리·상태:** INTEGRATION. 공통 Cargo·Lock·main.rs를 실제 수정하지 않았으며 검토 대기입니다.
- **적용안:** [패치](REQ-011-INTEGRATION.patch)와 [진입점 원문](daemon_entrypoint.rs). 새 버전을 Root Workspace에 모으고 Crate별 의존성·Tokio 기능·플랫폼 의존성과 Core 시험 fixtures feature를 연결합니다. Cargo.lock 갱신은 통합 담당자가 수행해야 합니다.
- **공개 경계:** `NativeRuntime`, `ApiServer::{start, endpoint, token_path, shutdown}`, `ApiServerConfig`, `ExecutionSupervisor::{new, start, inspect, cancel, subscribe, health}`, `RunAuthorizer`, `RunPermit`, `DenyRuns`입니다. 승인 Port는 Core 내부 경계이며 G0 Schema 변경이 아닙니다. MASTER·Snapshot·Workspace 담당과 연결 의미를 맞춰야 합니다.
- **제안 진입점:** `serve --project-root <path> --project-id <id> --host-id <id>`. 토큰·임의 권한을 인자로 받지 않습니다. 현재는 인증·Health 확인용이며 `runs.start`는 정책 미연결로 unavailable입니다. 실행 허용·활성 Run 종료까지 연결하기 전 G1을 판정할 수 없습니다.
- **다음 검증:** 공식 Workspace 빌드·시험, 실제 Codex G1, WSL Linux·권한·취소·Remote SSH 검증. 순서는 [통합 인계](HANDOFF.md)를 따릅니다.

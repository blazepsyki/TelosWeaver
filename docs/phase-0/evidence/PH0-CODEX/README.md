# PH0-CODEX 구현·검증 기록

| 항목 | 값 |
|---|---|
| 작업 ID·담당 | PH0-CODEX · Codex / `PH0-CODEX-20261024` |
| 작성일 | 2026-10-24 |
| 상태 | main 통합 완료(`64f9674`), 배정표 integrated(`86f580e`). REQ-012·REQ-014 완료. Core 연결은 후속 통합 범위 |
| 구현 Branch / Worktree | `ph0/codex` / `D:\worktree\TelosWeaver\ph0-codex` — 통합 후 정리됨. 현황 문서 갱신을 위해 최신 main에서 같은 이름으로 새로 생성 |
| 기준 | 착수 `7a9d4b3`, 공통 계약 `3a37919`, 최신 main 반영 `83cf156`(main `04cc2c3`, Spec 개정 1.4.3 포함) |
| 구현 Commit | 초기 `a498976`, 실환경 시험·사용량 보정·승인 경계 `0dcc1ca`, 통합 `64f9674` |
| 배정·승인 | 대표 main `31369f6`(예약), `796e3ce`(사용자 권장안 승인 후 착수·REQ-012) |
| 기준 문서 | [Spec](../../../DEVELOPMENT_v1.5.md), [대표 배정표](../../WORK_ALLOCATION.md) |

사용자가 기존 Codex 인증, 저장소 밖 일회용 폴더, 사용 가능한 경량 모델·low, 신규 시험 세션의 Resume/Fork 및 Archive를 승인했습니다. 기존 대화·프로젝트를 시험 대상으로 삼지 않았습니다. G1 대표 Provider는 기존 결정대로 Claude입니다. Linux·Remote SSH 검증은 사용자 지시로 대기하며 이 제출로 G2·G3를 판정하지 않습니다.

## 1. 수정 파일과 구현 범위

| 파일 | 내용 |
|---|---|
| `crates/adapters/src/codex.rs` | 확장 CLI 탐색 규칙, 신뢰된 실행 구성 Port, Capability, 멱등 시작, Thread/Turn 생성·Resume·Fork·소유 세션 Archive |
| `crates/adapters/src/codex/rpc.rs` | 공용 ProcessRuntime 기반 stdio JSONL, initialize/initialized, 요청 상관 관계, 제한·시간 초과·종료 확인 |
| `crates/adapters/src/codex/protocol.rs` | Thread/Turn 필터, 텍스트·도구·사용량·실패·완료 이벤트 변환 |
| `crates/adapters/src/codex/run.rs` | sequence·이벤트 재구독·보존, 입력·승인 상관 관계, 취소와 실제 종료·자식 회수 결과 |
| `crates/adapters/src/codex/tests.rs` | Mock·Protocol 시험 12개 |
| `crates/adapters/src/codex/tests/environment.rs` | 명시 실행하는 Windows 실환경 시험 7개 |
| `crates/adapters/tests/fixtures/codex/usage-multi-step.jsonl` | 자체 시험의 실제 사용량을 익명 ID로 기록한 2단계 Fixture |
| `docs/phase-0/evidence/PH0-CODEX/**` | 이 검증 기록, 인계, 공통 변경 제안 패치 |

한 Run은 Adapter가 소유한 App Server 프로세스 하나와 Turn 하나에 대응합니다. Resume/Fork는 별도 프로세스에서 원본 Native Thread를 사용하는 방식입니다. `turn/completed`와 프로세스 종료를 구분하고, NativeRuntime의 실제 종료 보고 후 마지막 `process_exited`를 기록합니다. 종료 확인 실패 시 오류와 중단 상태를 남기며 종료 이벤트를 만들어 내지 않습니다.

시작은 Task·Run·Host 일치와 멱등 키를 검사합니다. 같은 키·같은 내용은 하나의 시작 결과를 공유하고, 다른 내용이나 Run ID 재사용 충돌은 거부합니다. 시작 실패는 이벤트 스트림을 만들지 않습니다. 이벤트는 Run별 sequence 1부터 보존하고 재구독 시 이후 이벤트만 반환합니다.

기본 Sandbox는 read-only이며 WorkspaceWrite도 승인된 cwd에만 구성합니다. 추가 MCP 서버·웹 검색·앱·플러그인·에이전트 위임·Hook을 Thread 설정에서 끄고 프롬프트는 stdin Protocol로 전달합니다. 토큰·인증 파일을 직접 읽거나 복사하지 않았습니다. Provider 오류 원문과 stderr는 공통 이벤트에 복사하지 않습니다.

## 2. 실행 환경과 Protocol 근거

| 항목 | 실제 확인 결과 |
|---|---|
| 날짜·OS | 2026-10-24, Microsoft Windows 11 Pro `10.0.26300`, x64 |
| VS Code 확장 | `openai.chatgpt` · `26.1021.22027` · `win32-x64` |
| 실제 실행 파일 | `C:\Users\Psyki-HOME\.vscode\extensions\openai.chatgpt-26.1021.22027-win32-x64\bin\windows-x86_64\codex.exe` |
| CLI 버전 | `codex-cli 0.164.0-alpha.11.1` |
| SHA-256 | `4fd2f057bad603bf53dc202457ffd0c3591de62e867a8b01e73816e3fbce5a73` |
| 연결 | `app-server --stdio`, JSONL 요청·응답·알림, initialize 후 initialized |
| 모델·강도 | 실제 model/list에서 확인한 `gpt-6-luna`, `low`. 다른 모델로 조용히 대체하지 않음 |
| 시험 Workspace | `%TEMP%/telos-codex-live-<pid>-<timestamp>`에 시험별 생성. 저장소 파일을 입력하지 않음 |
| 승인·도구 | `CodexApprovals::Deny`, read-only 또는 명시한 시험에만 WorkspaceWrite. apply_patch 허용·거부 확인 |

현재 설치된 실행 파일의 `app-server generate-json-schema --experimental --out <dir>`로 요청 Schema를 생성해 구현했습니다. 생성 결과는 `%TEMP%/telos-codex-schema-20261024`에 있으며 거대한 전체 Schema는 저장소에 복제하지 않았습니다. 공식 설명은 [OpenAI App Server 문서](https://learn.chatgpt.com/docs/app-server)를 참고했습니다. 버전별 필드 차이의 판단 근거는 이 설치 버전에서 생성한 Schema와 실제 응답입니다.

확장 탐지는 `NativeCliDiscoverer`와 `cli_rule()`을 사용합니다. Windows·Linux·macOS의 x64/arm64 경로 규칙은 Mock으로 확인했으며 Windows x64 외의 실행 성공을 주장하지 않습니다.

## 3. Mock·Protocol 검증

```powershell
cargo test -p telos-adapters --lib codex::tests --locked --offline
```

Codex 일반 시험 12개 통과, 실환경 7개는 기본 실행에서 제외합니다. 다음 반례를 포함합니다.

- 시작 멱등성·충돌, 시작 실패 시 이벤트 없음, Provider 진단 원문 비노출
- sequence 연속성·완료 후 재구독, Protocol 파손과 종료 불명을 성공으로 처리하지 않음
- 동시 취소와 강제 정리, 취소 요청과 정상 완료가 경합할 때 실제 완료 결과 유지
- 도구·권한 상승 자동 거부, Thread/Turn 범위 밖 승인 거부, 중복 응답 멱등성
- 신뢰된 `authorize_approval`이 없으면 Allow 거부. 사용자 답변만으로 권한을 부여하지 않음
- turn/steer의 대상 Turn·입력 키 상관 관계와 중복 입력 방지
- Resume·Fork의 Native ID와 Archive 소유권, 외부 세션 Archive 거부
- 실제 다단계 사용량·중복 알림·과거 Thread 사용량 제외·카운터 역행·다른 Turn 알림

사용량에서 `tokenUsage.last`는 Turn 전체가 아니라 모델 응답 하나였습니다. 실제 도구 허용 시험의 두 응답은 입력 `11965 + 12035`, 출력 `46 + 5`였고 최종 total은 입력 `24000`, 출력 `51`, cache read `11776`이었습니다. 이 관찰 후 첫 last와 이후 total 증가분으로 Run 누적 사용량을 계산하도록 보정했습니다. Fixture 재생은 과거 Thread 사용량 100000을 더한 경우에도 이번 Run의 24000/51만 계산하는지 확인합니다. cache와 입력의 포함 관계 및 금액은 임의 추정하지 않습니다.

## 4. Windows 실환경 검증

기존 인증을 CLI가 사용하게 두고 아래 환경 변수만 지정했습니다. 재실행은 실제 계정 사용량을 소모하므로 기본 Workspace 시험에는 포함하지 않습니다.

```powershell
$env:TELOS_CODEX_CLI = '<공통 탐색으로 확인한 확장 실행 파일 절대 경로>'
$env:TELOS_CODEX_VERSION = (& $env:TELOS_CODEX_CLI --version)
$env:TELOS_CODEX_MODEL = 'gpt-6-luna'
cargo test -p telos-adapters --lib codex::tests::environment --locked --offline -- --ignored --nocapture --test-threads=1
```

7개 모두 통과했습니다(실행 시간 53.11초). 사용량 보정 및 신뢰된 승인 Port 보강 이후에는 실제 사용량 Fixture·Mock으로 해당 변경을 검증했습니다. 마지막 코드 전체를 동일한 실계정 7종으로 재실행한 것으로 표시하지 않습니다.

| 시험 | 기대 결과 | 실제 결과 |
|---|---|---|
| env_discovery | 확장 CLI·버전·해시 탐지 | 위 확장 메타데이터·실제 경로·버전·해시 확인 |
| env_managed_smoke | 새 Thread/Turn, 텍스트 스트림과 정상 Turn 종료 | `TELOS_PONG`, 이벤트 8개, Turn completed, 실제 프로세스 exit 0·childrenReaped true |
| env_invalid_model | 잘못된 모델을 configuration 실패로 처리 | `telos-invalid-model` 거부, Turn failed. 프로세스 exit 0이어도 성공으로 판정하지 않음 |
| env_cancel | 중단 요청 뒤 실제 cancelled·종료 확인 | 이벤트 3개, cancelled, childrenReaped true |
| env_tool_allowed | 시험 cwd의 파일 쓰기 허용 | WorkspaceWrite에서 apply_patch 성공, allowed.txt의 `TELOS_ALLOWED` 확인 |
| env_tool_denied | 실제 도구 권한 거부, 파일 없음 | read-only에서 file_change denied, denied.txt 없음, 거부 결과와 Turn 완료 구분 |
| env_resume_fork | Resume의 맥락·ID 유지, Fork의 맥락·새 ID·부모 불변 | 단어 기억 확인, Resume는 같은 Thread, 첫 Turn 기준 Fork는 새 Thread, thread/read로 부모 Turn 2개 유지 확인 |

PROV-01~03·06·07의 기본 Managed 경로를 실제 CLI로 확인했습니다. PROV-04는 Run 결과·이벤트의 Native Session/Turn ID 연결까지 구현했으며 Core DB 저장은 통합 범위입니다. PROV-08은 NativeRuntime의 종료·자식 회수 결과와 취소를 확인했고 전역 Scheduler 동시성 정책은 구현하지 않았습니다. PROV-09는 버전·Host와 증거 수준을 분리합니다. PROV-05·FORK-POC-01은 실환경 확인, FORK-POC-02는 대화 분리까지 확인했으나 Git Worktree·파일 격리는 별도입니다.

### 4.1 시험 세션 정리 기록

모든 세션은 이 시험에서 새로 생성한 Thread입니다. 아래 각 Thread의 `thread/archive`가 성공했습니다. Resume는 부모 Thread를 재사용하며 별도 Thread를 만들지 않습니다. 기존 사용자 대화는 Archive하지 않았습니다.

| 용도 | Native Thread ID | 정리 |
|---|---|---|
| 초기 단독 smoke | `01a16d3c-496c-7253-876a-594f42e39a66` | Archive 성공 |
| cancel | `01a16d41-e690-7772-b056-8e61279dee3a` | Archive 성공 |
| invalid model | `01a16d42-1840-7a13-82c9-c912a704aedf` | Archive 성공 |
| smoke | `01a16d42-3070-7d01-b4f4-47444be61b5d` | Archive 성공 |
| parent·resume | `01a16d42-4564-7d23-b289-2890e3f3681e` | Fork 정리 후 Archive 성공 |
| fork | `01a16d42-69ba-7233-9d57-f9a507f7481a` | Archive 성공 |
| tool allowed | `01a16d42-79f6-7863-9614-1d987df6da48` | Archive 성공 |
| tool denied | `01a16d42-a022-7a83-975d-327960b8a574` | Archive 성공 |

부모 첫 Turn은 `01a16d42-4577-7722-994f-fb0280a30ad5`, Resume Turn은 `01a16d42-5ccf-7782-bcae-0444af1a9778`, Fork Turn은 `01a16d42-69e0-7841-b036-7bb4e26bb1f9`였습니다.Fork의 lastTurnId에는 부모 첫 Turn을 지정했습니다.

관찰된 부모·Resume·Fork의 입력/cache read는 각각 11681/9984, 11713/11520, 11713/0이었습니다. 이 한 번의 관찰로 Fork의 토큰 절감률·비용·품질 우위를 주장하지 않습니다. FORK-POC-04의 전체 사용량 비교는 미실시입니다.

## 5. Worktree 복구와 통합 전 로컬 검사

사용자의 Worktree 삭제 후 Git에 남아 있는 `ph0/codex`를 원래 경로로 다시 연결했습니다.

```powershell
git worktree add D:/worktree/TelosWeaver/ph0-codex ph0/codex
```

복구 직후 HEAD는 `83cf156`였고 `0dcc1ca`의 조상 관계와 깨끗한 작업 디렉터리를 확인했습니다. 커밋된 구현·Fixture·시험은 모두 복구했습니다. 삭제 전 미커밋 파일까지 복구했다는 주장은 하지 않습니다.

복구 후 통합 전에 다음 검사를 실행했습니다. 아래 Clippy 실패와 패치 적용 가능 결과는 당시 기록이며, 현재 통합 후 결과는 §7을 따릅니다.

| 명령 | 실제 결과 |
|---|---|
| `cargo test --workspace --locked --offline` | 118개 통과, 실패 0, 실환경 24개 제외 |
| `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | 실패. 공통 `unimplemented.rs:90`의 unused macro 및 `:169`의 unused import 2개 |
| `cargo clippy --workspace --all-targets --locked --offline -- -D warnings -A unused-macros -A unused-imports` | 위 두 lint만 예외로 둔 진단 검사 통과. 정식 엄격 Clippy 통과를 대신하지 않음 |
| `cargo fmt --all -- --check` | 통과 |
| `git apply --check docs/phase-0/evidence/PH0-CODEX/COMMON_CHANGES.patch` | 적용 가능 확인. 공통 파일에는 적용하지 않음 |

로컬 전체 로그는 `%TEMP%/telos-codex-restored-test.log`, `telos-codex-restored-clippy.log`, `telos-codex-restored-clippy-scoped.log`에 있습니다. 다른 장비에서는 시험 명령과 소유 Fixture로 재현합니다.

## 6. 공통 변경 처리 결과와 남은 범위

- **REQ-012 완료(`64f9674`):** CONTRACT·INTEGRATION이 `lib.rs`를 `pub mod codex;`로 변경했습니다. 소비자가 `CodexConfig`·`CodexLaunchResolver`·`cli_rule()`을 사용할 수 있습니다. 공통 DTO·Trait·Cargo 변경은 없습니다.
- **REQ-014 완료(`64f9674`):** 미사용 `unimplemented_adapter!` 매크로와 재수출을 제거했습니다. `UnsupportedAdapter`·공통 계약 시험은 유지됐고 엄격 Clippy의 차단 원인이 해소됐습니다.
- [COMMON_CHANGES.patch](COMMON_CHANGES.patch)는 위 두 요청의 **통합 전 제안 이력**입니다. 이미 반영된 현재 main에 다시 적용하는 패치가 아닙니다.
- Core의 신뢰된 실행 승인·Workspace/Snapshot 해석·Codex Resolver 연결, 공통 탐색 결과 주입, Daemon 종료 시 활성 Run 정리는 INTEGRATION·MASTER 범위에 남아 있습니다. 현재 `Phase0Execution`에는 Claude만 연결되어 있습니다. [인계](HANDOFF.md)를 참고하십시오.
- `send_input`과 대화형 승인 전달은 Mock 수준입니다. Allow에는 신뢰된 native action 검사 구현이 필요하며 실제 대화형 Allow 성공은 검증하지 않았습니다.
- Linux·Remote SSH·macOS·다른 CLI 버전, 공식 IDE Attached·동시 세션 사용, 파일 Worktree 격리, 전역 스케줄링·지속 DB·재시작 복구는 이번 검증에 포함하지 않았습니다.

## 7. 통합 후 현황 재확인

2026-10-24 사용자 현황 최신화 요청으로 대표 main `86f580e`를 확인했습니다. 통합 담당은 `64f9674`에서 Codex 구현과 REQ-012·REQ-014를 반영하고 `86f580e`에서 PH0-CODEX를 integrated로 기록했습니다. 통합 커밋에는 병합본 Codex 탐지 실환경 재확인 결과도 기록되어 있습니다. 이 탐지 재확인은 통합 담당의 실행 기록이며, 이번 현황 갱신에서 실계정 생성 시험을 다시 실행한 것은 아닙니다.

현재 main에서 다음을 직접 다시 실행했습니다.

| 명령 | 결과 |
|---|---|
| `cargo test --workspace --locked --offline` | 118개 통과, 실패 0, 실환경 24개 기본 제외 |
| `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | 경고 예외 없이 통과 |
| `cargo fmt --all -- --check` | 통과 |

테스트 로그는 `%TEMP%/telos-status-latest-test.log`에 있습니다. §3·§4의 Mock 12개와 Windows 실환경 7개는 기존 검증 증거이며, 위 기본 Workspace 실행이 제외된 실환경 24개를 실행했다는 의미는 아닙니다.

구현 Branch·Worktree 삭제는 Spec §19.5.5의 통합 후 정리 규칙에 따른 정상 처리입니다. 현황 문서 갱신은 배정표 `7619a17`을 기록한 뒤 최신 main에서 `ph0/codex`·`D:\worktree\TelosWeaver\ph0-codex`를 새로 만들어 진행했습니다. 구현의 integrated 상태를 유지하며 코드·공통 계약·게이트 판정은 변경하지 않습니다. 문서 갱신 결과는 대표 배정표의 PH0-CODEX 행에 기록합니다.

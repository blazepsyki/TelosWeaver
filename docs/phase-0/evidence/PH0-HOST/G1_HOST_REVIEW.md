# G1 HOST 확인 및 REQ-013 ③ 처리 결과

| 항목 | 값 |
|---|---|
| 작업·담당 | PH0-HOST · `Codex / PH0-HOST-20261024` |
| 날짜·환경 | 2026-10-24, Windows 11 x64 로컬 |
| 기준 | main `5b5e64f`, 착수 기록 `eef86ba`, Spec 개정 1.4.3, 공통 계약 `271e7fd` |
| 구현 Commit | `d13a1fc` |
| Branch / Worktree | `ph0/host` / `D:\worktree\TelosWeaver\ph0-host` |
| 확인 대상 | INTEGRATION의 [G1 증거 §7](../PH0-INTEGRATION/README.md#7-g1-대표-실행), 통합 `d3130c1` 및 현 main |

## 1. HOST 확인

**Windows G1의 기존 실행 관찰 결과를 HOST 관점에서 확인했습니다.** Local API → 승인 서비스 → 대표 Claude Adapter → NativeRuntime의 경로, 취소 접수와 실제 종료의 구분, 자식 회수, 연결 종료와 Daemon 종료의 구분이 확인 대상입니다. 이번 확인은 기존 실환경 증거와 코드 대조 및 아래 HOST 로컬 재검증을 근거로 합니다. 실제 Claude 모델 호출을 다시 실행한 결과는 아닙니다.

실행 감독의 취소 경합·종료 불명 판정 문제는 §3처럼 수정해 제출했습니다. INTEGRATION은 `d13a1fc`를 통합·재검증한 뒤 HOST 확인을 G1 기록에 반영해 주십시오. G1 실환경 시험의 자동 판정 강화도 §5의 REQ-015로 요청합니다. 게이트 확정과 PH0-HOST 전체 verified 전환은 이 문서에서 수행하지 않습니다. Linux·Remote SSH는 기존 사용자 지시로 대기합니다.

| 검토 대상 | 확인 결과 |
|---|---|
| 승인 전 실행 방지 | ExecutionSupervisor는 RunAuthorizer의 유효한 Permit을 받은 후 Adapter를 시작합니다. 정책 없는 진입점은 DenyRuns를 유지합니다. |
| 시작·완료·실패·결과 파일 | 기존 G1은 같은 키의 시작 멱등성, Write 산출물 pong, completed/failed Turn, 실제 종료·자식 회수 및 inspect 결과를 기록했습니다. Source 경로와 해당 실행 흐름이 일치합니다. |
| 취소 접수 | runs.cancel의 accepted는 요청 수락입니다. Cancelling은 진행 상태이며 실제 Turn/프로세스 결과로 최종 판정해야 합니다. 이번 경합 수정은 §3을 따릅니다. |
| 프로세스·자식 회수 | Windows 런타임은 생성 시 JOB_LIST로 Job에 할당하고 종료 후 ActiveProcesses=0을 확인합니다. 로컬 시험은 자식 프로세스 원본 핸들의 최종 종료도 별도로 확인했습니다. |
| API 연결 종료 | 클라이언트 연결 종료는 Run 취소를 호출하지 않습니다. 기존 G1은 연결 종료 2초 뒤에도 Running을 관찰했습니다. |
| Daemon 종료 | TrackedAdapter가 활성 Run의 취소를 요청하고 이벤트 스트림 종료·inspect_run Finished를 확인합니다. 제한 시간 초과 또는 종료 미확인은 unconfirmed로 보고합니다. 기존 G1은 취소 1·미확인 0·최종 Cancelled를 기록했습니다. |
| 상태 문자열 | 현재 계약의 RunStatus에는 serde rename_all이 없으므로 Running/Succeeded/Failed/Cancelled가 실제 Wire 값입니다. 이것을 소문자라고 가정하는 시험은 수정해야 합니다(REQ-015). Schema 문자열을 HOST에서 변경하지 않았습니다. |

## 2. 정상 종료 유예와 강제 종료

현재 Windows `Control::request_stop()`은 별도 stop 신호를 보내지 않습니다. `NativeHandle::terminate()`가 stdin을 가능한 경우 닫고 유예를 기다리며, 남아 있으면 `TerminateJobObject`로 Job을 종료합니다. 따라서 기존 G1에서 5초/2초 유예 뒤 killed가 관찰된 것은 이 경로와 일치합니다. 해당 결과를 Windows 정상 종료 신호를 보낸 증거로 해석하지 않습니다.

이번 Windows 실제 로컬 프로세스 시험의 결과는 다음과 같습니다.

- stdin을 EOF까지 읽는 echo Fixture: 종료 코드 0, TerminatedOnRequest, ChildrenReaped::All.
- stdin을 읽지 않고 자식을 유지하는 tree Fixture: 유예 후 Killed, ChildrenReaped::All.
- 종료 전에 자식의 PROCESS_SYNCHRONIZE 핸들을 보관해 생존 상태를 확인하고, 정리 후 같은 핸들에서 최대 3초 동안 기다려 WAIT_OBJECT_0을 확인했습니다. PID 재사용을 종료 증거로 사용하지 않습니다.

초기 시험에서 종료 직후의 대기 시간 0인 조회가 WAIT_TIMEOUT을 반환한 경우가 있었습니다. 최종 시험은 보관한 핸들로 제한 시간 안의 최종 종료까지 확인합니다. Windows의 강제 종료는 비동기이며 종료 확인에는 프로세스 핸들 대기가 필요하다는 [Microsoft TerminateProcess 설명](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-terminateprocess)과, Job의 각 프로세스에 같은 종료 효과를 적용한다는 [TerminateJobObject 설명](https://learn.microsoft.com/en-us/windows/win32/api/jobapi2/nf-jobapi2-terminatejobobject)을 참고했습니다.

## 3. 검토 중 발견·수정한 실행 감독 문제

기존 Supervisor는 취소 수락 뒤의 내부 cancelled 플래그로 최종 결과를 Cancelled로 덮어쓸 수 있었습니다. 취소를 수락하는 동안 실제 정상 완료가 먼저 도착한 경우도 Succeeded를 Cancelled로 바꿨습니다. 또한 TurnCancelled를 종료 불명·자식 미회수보다 먼저 판정했습니다.

이 문제는 새 회귀 시험으로 수정 전 실패를 확인한 뒤 HOST 소유 코드에서 수정했습니다.

| 실제 이벤트 | 취소 수락 여부 | 수정 후 최종 상태 |
|---|---|---|
| Turn completed, exit 0, 자식 회수 확인 | 수락됨. 완료가 수락 응답 전/후에 도착하는 경우 모두 시험 | Succeeded |
| Turn failed, 정상적으로 확인된 프로세스 정리 | 수락됨 | Failed |
| Turn cancelled, 프로세스 종료·자식 회수 확인 | 수락됨 | Cancelled |
| termination unknown 또는 자식 미회수/회수 미확인 | completed/cancelled Turn과 조합하여 시험 | Interrupted |

취소 수락은 미완료 Run을 Cancelling으로 표시하는 데만 사용하고, 완료 상태는 실제 이벤트로 계산합니다. 공통 DTO·Trait·Cargo·Provider 모듈은 변경하지 않았습니다.

## 4. REQ-013 ③ universal 메타데이터

기존 엔진은 targetPlatform 문자열 undefined를 활성 후보에서 제외해 folder-scan으로 넘어갔습니다. 회귀 Fixture에서 활성 1.0.0 대신 비활성 9.0.0을 고르는 현상과 중복 활성 항목의 모호성 검사를 우회하는 현상을 재현했습니다.

`discovery/mod.rs`에서 문자열 undefined를 universal과 동일하게 선택하고 결과의 target_platform은 None으로 정규화했습니다. 메타데이터 미존재 시 폴더 해석, 명시된 다른 플랫폼 제외, 폐기·경로 검사는 기존 동작을 유지합니다. 회귀 시험은 Windows/Linux/macOS target 요청에서 활성 메타데이터 선택 및 중복·폐기 거부를 확인하며 실제 Linux/macOS 실행 시험을 뜻하지 않습니다.

설치된 Antigravity의 읽기 전용 탐지 시험도 수행했습니다.

| 항목 | 결과 |
|---|---|
| 확장 | google.google-antigravity 1.9.0 |
| 활성 메타데이터 | relativeLocation=google.google-antigravity-1.9.0, targetPlatform=undefined |
| CLI | `C:\Users\Psyki-HOME\.gemini\bin\agy.exe`, 1.4.7 |
| SHA-256 | 22588b6d5a6f6d2d66091c73dfb9255edc7bdafe9aff9dd20d958ce116496695 |
| 탐지 | source=extension, location=managed-path, resolution=metadata, targetPlatform 생략 |
| 실행 범위 | 기존 AGY 탐지·데이터 디렉터리 옵션 시험의 --version 명령만 실행. 모델 생성·세션 변경 없음 |

REQ-013의 채택·완료 판정은 기존 통합 담당이 수행합니다. 이 제출은 ③ 구현·검증 결과이며 다른 요청자의 행을 대신 변경하지 않았습니다.

## 5. REQ-015: G1 실환경 시험의 자동 판정 강화

**요청 작업:** PH0-HOST. **처리 대상:** PH0-INTEGRATION 소유 `crates/core/tests/phase0_g1.rs`. 공통 계약 변경은 없습니다.

확인된 시험 문제는 다음과 같습니다.

1. 완료·실패 시험의 `assert_ne!(status, "running")`은 계약의 Running과 다르므로 Running 상태도 통과시킬 수 있습니다.
2. 취소 시험은 Turn이 cancelled일 때만 termination을 검사합니다. accepted와 cancelled 자체를 요구하지 않아 취소가 재현되지 않아도 통과할 수 있습니다.
3. Daemon 종료 시험은 unconfirmed가 비었는지만 요구합니다. 종료 전 Running, 취소 수 1, 종료 후 Cancelled를 검증하지 않아 이미 끝난 Run도 통과할 수 있습니다.

다음과 같이 실제 계약 타입을 사용해 검사를 강화해 주십시오.

- `telos_schema::RunStatus`와 `json!(RunStatus::Succeeded/Failed/Cancelled)`를 사용합니다. 소문자 상태 비교나 Schema의 임의 이름 변경으로 해결하지 않습니다.
- 마지막 이벤트 전달과 Supervisor의 최종 상태 갱신 사이의 경합을 허용하도록 제한 시간 안에서 runs.inspect를 재조회하고 정확한 기대 상태를 검사합니다. 끝없이 기다리거나 임의 고정 sleep 뒤 성공 처리하지 않습니다.
- 취소 시험은 accepted=true, turn_finished.status=cancelled, process_exited의 확인된 종료 원인과 childrenReaped=true, 최종 RunStatus::Cancelled를 확인합니다.
- Daemon 종료 시험은 연결 종료 후 RunStatus::Running, shutdown.cancelled=1·unconfirmed 빈 목록, 최종 RunStatus::Cancelled를 확인합니다.

호환 영향은 시험 판정 강화뿐이며 API·Schema·정책 동작 변경은 없습니다. 적용 후 기본 Workspace 검사와 승인된 기존 haiku·일회용 Workspace 조건의 G1 실환경 시험 5개를 재검증하고, 새 시험 세션은 기존 Archive 절차로 정리해 주십시오. 이 요청에서 실제 Claude 시험을 새로 실행하거나 다른 담당의 소스 파일을 수정하지 않았습니다.

## 6. 이번 검사와 인계

| 실행 명령 | 실제 결과 |
|---|---|
| `cargo test -p telos-adapters --test discovery_extensions --test discovery_resolution --locked --offline` | 19개 통과 |
| `cargo test -p telos-core --test host_execution --locked --offline` | 6개 통과 |
| `cargo test -p telos-adapters --test runtime_native --locked --offline` | Windows 11개 통과 |
| `cargo test -p telos-adapters --test agy_environment env_discovery_and_data_dir_option --locked --offline -- --ignored --nocapture` | 실제 설치 환경의 버전·탐지 시험 1개 통과 |
| `cargo test --workspace --locked --offline` | 123개 통과, 실패 0, 실환경 24개 기본 제외 |
| `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | 통과 |
| `cargo fmt --all -- --check` | 통과 |

추가된 일반 시험은 탐색 2개·실행 감독 2개·Windows 런타임 1개입니다. 로컬 전체 로그는 `%TEMP%/telos-host-g1-review-test.log`에 있습니다. 기존 G1 실계정 5개는 INTEGRATION의 증거이며 이번 실행 결과와 합산하지 않습니다.

INTEGRATION 인계: `d13a1fc`의 소유 경로를 확인해 통합·재검증하고 REQ-013 ③ 및 G1 HOST 확인을 배정표에 반영해 주십시오. REQ-015는 별도 처리·실환경 재검증이 필요합니다. G2·G3와 Linux·Remote SSH 대기는 유지합니다.

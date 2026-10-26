# PH0-EXT: 비연결 상태의 이전 Health 표시 보정

| 항목 | 값 |
|---|---|
| 담당 | `Codex / PH0-EXT-HEALTH-01a17586` |
| 날짜 | 2026-10-26 (KST) |
| 기준 | main `33c2995`, 착수 배정 `8613343`, Spec v1.5.6, 공통 계약 `7497877` |
| Branch / Worktree | `ph0/ext-health` / `D:\worktree\TelosWeaver\ph0-ext-health` |
| 작업 원본 | [INTEGRATION 지시서](../PH0-INTEGRATION/EXT_HEALTH_FOLLOWUP.md), G3 발견 3 (비차단) |
| 판정 | 구현·검증 완료 제출. INTEGRATION 검토·통합 대기 |

## 1. 확인한 문제와 선택

Core 연결 종료와 재연결 실패 처리에서 이전 `ConnectionState`를 펼쳐 쓰므로 `health`·`hello`가 `error` 상태에 남았습니다. `StatusProvider`도 연결 상태와 관계없이 이 값을 표시했습니다.

이전 값을 시각과 함께 보관하는 대신, **오류 전환 시 현재 Health·Core 정보를 제거**하는 방식을 선택했습니다. 이번 범위에는 상태 이력 조회 요구가 없고, 마지막 값과 현재 값을 구분하기 위한 새 시간 필드·표시 체계를 추가할 필요가 없기 때문입니다.

| 지시 | 구현 |
|---|---|
| X1: 비연결 Health 표시 금지 | 연결 종료 시 `phase`·프로젝트 경로·안전한 진단만 남김. View는 `connected`일 때만 Health 구성 요소를 표시 |
| X2: 오류 경로 일치·새 Health 복원 | 재연결 실패도 같은 방식으로 이전 값을 제거. 성공 시 실제 새 Health로 복원 |
| X3: 오류 중 `hello` 처리 | 오류에서는 `hello`도 제거. Core 버전·Project·Host와 상태 표시줄의 Host tooltip은 `connected`일 때만 표시 |
| X4: 상태 전환 로그 유지 | 기존 로그 처리에는 손대지 않음. 반복 Health·상태 전환 시험 통과 |

인증 직후 첫 Health를 기다리는 `connecting` 상태에는 기존 흐름대로 인증된 `hello`가 일시적으로 들어갈 수 있습니다. 이를 현재 연결 정보로 View·tooltip에 표시하지 않습니다. 공개 `state()` 타입은 바꾸지 않았고, `error`·명시적 `disconnected`에서는 `hello`·`health`가 없습니다. 프로젝트 경로·진단은 오류 상태에서 유지합니다.

수정 경로는 `extension/`과 이 증거뿐입니다. Core·공통 Schema·생성 타입 원본·RunFeed·Core 수명·재연결 횟수/간격·권한 정책은 바꾸지 않았습니다.

## 2. 수정 전 실패 재현

새 회귀 시험 3개를 먼저 추가하고, **기존 소스에서 빌드한 결과로 3개 모두 실패**함을 확인했습니다.

```powershell
cd extension
npm run build
node --test --test-name-pattern='socket loss|Heartbeat failure|non-connected views' test/core.test.cjs test/ui.test.cjs
```

| 시험 | 수정 전 결과 |
|---|---|
| Socket 단절·실패한 재연결·새 응답 복원 | `health`가 `undefined`여야 하는데 이전 `degraded`·`persistence unavailable` 객체가 남아 실패 |
| Heartbeat의 refresh 실패·재연결의 Health 실패·복원 | 같은 이전 Health 객체가 남아 실패 |
| 비연결 View·새 응답 복원 | `error shows stale Core`로 실패. `error`에서 이전 Core 행이 남음 |

회귀 시험은 연결 상태 모델과 표시 계층을 나눠 확인합니다. View 시험에는 일부러 이전 `hello`·`health`가 남은 Snapshot을 넣어 `error`·`connecting`·`disconnected` 모두에서 표시를 차단하는지 검사합니다. 재연결 뒤에는 다른 Host·새 구성 요소만 표시되는지도 확인합니다.

## 3. 수정 후 검사 (Windows 로컬)

환경: Node v24.22.1, npm 11.19.1, 설치된 VS Code 1.143.1. Worktree에서 `cargo build -p telos-core --bin telos-weaver --locked`로 준비한 `target/debug/telos-weaver.exe` (`0.0.0`)를 사용했습니다. Rust 소스 변경과 Workspace 시험 재실행은 없습니다.

| 명령·범위 | 결과 |
|---|---|
| `npm ci --ignore-scripts` | 의존성 준비 성공, Lockfile 변경 없음 |
| `npm test` | 타입 생성·TypeScript 빌드, **32개 통과·실패/제외 0개** (기존 29 + 신규 3) |
| `npm run test:core` | 실제 DenyRuns Core **1개 통과·제외 0개**. 인증·잘못된 토큰 거부·Run 거부·분리 후 생존·동일 PID 재사용 확인. 이후 이 시험의 Core만 강제 종료해 IPC 단절 시 이전 `health`·`hello` 제거와 프로젝트 경로 유지를 확인 |
| `npm run test:host` | 격리 Extension Host 종료 코드 0, **6개 확인 통과**. 연결 해제의 공개 상태에서 Health·Core 정보 부재, 재접속의 정보 복원, Host 종료 후 Core 생존 확인 |
| `git diff --check` | 통과 |

실제 Core 시험 PID는 `42288`, 격리 Host가 시작한 Core PID는 `42636`입니다. Host 시험의 `coreSurvivedHostExit: true`를 확인한 뒤 Runner가 시험 Core를 종료했습니다. 두 PID가 더 이상 존재하지 않는 것도 재확인했습니다. 실제 Provider·모델 호출은 없으며 사용자 설정·기존 Core·대화는 사용하지 않았습니다.

격리 Host 결과: `%TEMP%\telos-ext-host-8xEhzM\result.json`. 별도 프로필·확장 디렉터리를 사용했고 사용자 창·프로필을 재사용하지 않았습니다. 실제 Core 시험의 임시 폴더 `%TEMP%\telos-ext-0Kzzm6`은 시험 정리 코드로 삭제됐습니다.

재실행:

```powershell
cd extension
npm test
$env:TELOS_CORE_EXECUTABLE = 'D:\worktree\TelosWeaver\ph0-ext-health\target\debug\telos-weaver.exe'
npm run test:core
$env:TELOS_VSCODE_EXECUTABLE = 'C:\Users\Psyki-HOME\AppData\Local\Programs\Microsoft VS Code\Code.exe'
npm run test:host
```

## 4. 검증 범위와 인계

- View의 행 표시·복원은 Node UI 시험으로 확인했고, 실제 VS Code API·Core 수명·공개 상태는 격리 Host에서 확인했습니다. 사용자 화면의 육안 확인은 하지 않았습니다.
- Linux·Remote SSH는 이번 후속에서 재시험하지 않았습니다. 지시서상 필수 검사가 아니며 기존 G3 Remote SSH 성공 기록을 새 검사 결과로 계산하지 않습니다.
- G3 발견 3은 비차단 후속입니다. 이 작업만으로 G3 전체를 통과 처리하지 않습니다.
- `extension/README.md`에 오류·연결 중 표시 원칙과 현재 Node 시험 수를 반영했습니다.
- 착수 기록 직전 공유 main에서 REQ-023 병합이 진행 중이었고, 실패한 사전 검사 뒤 뒤따른 커밋 명령이 그 병합을 EXT 착수 제목으로 완료했습니다(`1799c37`). 병합 내용·부모·Git 사용자 정보는 보존하고 제가 만든 커밋의 제목만 `33c2995`로 바로잡았습니다. 실제 EXT 착수 기록은 별도 `8613343`이며, 이후 main 수정에는 병합 진행 여부와 실행 실패 중단 검사를 적용했습니다.

PH0-EXT를 `review`로 제출합니다. INTEGRATION에 소유 경로·오류 상태·UI 표시·회귀 결과 검토와 병합·배정표 최종 상태 반영을 요청합니다. Branch·Worktree는 검토용으로 보존하며, 격리 Host 자료는 사용자 설정을 포함하지 않는 로컬 시험 결과로 남깁니다.

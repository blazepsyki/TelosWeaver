# PH0-EXT 구현·검증 기록

| 항목 | 값 |
|---|---|
| 작업·담당 | PH0-EXT · `Codex / PH0-EXT-20261024` |
| 구현·검증일 | 2026-10-25, Windows 로컬 |
| 상태 | 자체 구현·검증 완료, INTEGRATION 검토·통합 요청. 배정표의 integrated/verified·게이트 판정은 통합 담당이 수행 |
| Branch / Worktree | `ph0/ext` / `D:\worktree\TelosWeaver\ph0-ext` |
| 구현 기준 | main `c1bbabc`, 착수 기록 `c3f246d`, Spec v1.5, 공통 계약 `3a37919` |
| 구현 Commit | `4ad8dcf` |
| 준비 이력 | 예약 `1102d52`, 사전 준비 `1c328fc`. 사용자가 2026-10-25 권장안 세 가지를 승인해 착수 |
| 기준 문서 | [Spec](../../../DEVELOPMENT_v1.5.md), [대표 배정표](../../WORK_ALLOCATION.md) |

## 1. 확정 조건과 구현 범위

사용자 승인: 빌드한 Core의 절대 경로 지정, 명시적 시작·연결, 정책 파일 없는 DenyRuns로 초기 검증. 실제 Provider·모델 호출, Core 자동 다운로드·업데이트·실행 중 Cargo 빌드는 포함하지 않습니다. Linux·Remote SSH의 기존 검증 대기는 유지합니다.

구현 파일은 `extension/**`와 이 증거 디렉터리뿐입니다. Rust 소스·Cargo·공통 Schema·Core API·RN·루트 Workspace 설정은 변경하지 않았습니다. Phase 0 UI는 StatusBar·Explorer TreeView·OutputChannel입니다. 전체 Task 목록·마스터 대화·승인·Artifact UI는 후속 범위입니다.

| 파일 | 구현 |
|---|---|
| `extension/src/locations.ts` | canonical Project Key, Windows/UNC/Unicode 정규화, Pipe/Socket·사용자 토큰 파일 위치 |
| `extension/src/client.ts` | 생성 타입 기반 API 0.2, NDJSON 분할·UTF-8·상한·상관 ID·동시 요청·시간 초과·연결 종료·원문 오류 비노출 |
| `extension/src/core.ts` | 토큰의 길이·형식 검사, hello·Health·정체성, 명시적 시작·기존 Core 재사용·Heartbeat·제한된 재연결·독립 수명 |
| `extension/src/runFeed.ts` | 알려진 Run 읽기 전용 구독, 실제 수신 커서 재생·중복 제거·다른 구독 제외·순서/버전 검사·종료 정리 |
| `extension/src/extension.ts` | 최소 UI·명령·프로젝트 선택·Trust 검사·Run 관찰과 같은 프로젝트 재연결 시 재구독 |
| `extension/package.json` | Workspace Host, 제한된 Untrusted 지원, 가상 Workspace 제외, 명령·상태 View·설정·시험 Script |
| `extension/scripts/gen-types.mjs` | 기존 타입 생성과 함께 이벤트 kind를 같은 JSON Schema에서 생성. 버전·프레임 상수는 Rust 원본과 시험으로 대조 |
| `extension/test/**` | Protocol·연결·재연결·구독·UI 통제 시험 및 명시적 실제 Core 시험 |
| `extension/src/test/host.ts`, `scripts/test-host.cjs` | 실제 VS Code Extension Host 시험과 격리된 프로필·상태, Core의 Host 종료 후 생존 검증·시험 PID 정리 |
| `extension/.vscode/**`, `extension/README.md` | Extension 폴더 전용 개발 구성과 사용·설정·검증 절차 |

## 2. 실제 동작과 경계

- Extension 활성화는 UI만 준비합니다. Start 명령은 기존 Core에 먼저 연결하고 미실행일 때 검증한 지정 바이너리를 별도 프로세스로 시작합니다. shell·policy 인자를 사용하지 않습니다.
- 연결 성공·Health Degraded·컴포넌트 unavailable을 구분합니다. 현재 Core의 Scheduler·영속성이 미구현인 사실을 숨기지 않습니다.
- Core 재시작 시 새 토큰으로 다시 인증하고 명시된 Project/Host 정체성을 대조합니다. 인증·정체성 실패를 새 Core 시작으로 우회하지 않습니다.
- Extension 연결 해제·종료는 Core를 죽이지 않습니다. Core 종료 RPC가 없는 상태에서 임의 종료 UI를 추가하지 않았습니다.
- Run 관찰은 사용자가 지정한 ID만 대상으로 합니다. 자동 재연결 때 현재 세션의 수신 cursor를 이어 쓰며, Core 재시작으로 사라진 Run을 재실행하지 않습니다. UI 재로드 후 전체 Run 복원·목록 조회는 지원하지 않습니다.
- 요청 32개·프레임 1 MiB·구독 초기 알림 64개·Output 256 KiB·표시 한 줄 4096자 제한이 있습니다. 연결/요청은 기본 5초, 연결 재시도와 slow_consumer 재구독은 각각 최대 5회입니다.

## 3. 환경·검사 결과

| 항목 | 확인 |
|---|---|
| OS | Windows 11 x64 로컬 |
| Node·npm | 외부 시험 셸 Node v24.14.0 / npm 11.9.0. Extension Host 내부 Node 버전과 같은 것으로 간주하지 않음 |
| VS Code | 1.143.1, commit e646868227c3e3a9e3f24277c8c7220be42c0be7, x64 |
| Core | 이 Worktree의 공식 Workspace에서 `cargo build -p telos-core --bin telos-weaver --locked --offline`로 빌드한 0.0.0, API 0.2 |
| 타입 | 현 JSON Schema에서 43개 타입 파일과 이벤트 kind 생성. 생성물은 무시 대상 |

| 명령 | 실제 결과 |
|---|---|
| `npm ci --ignore-scripts` | 통과. 기존 Lockfile 사용, 새 의존성 없음 |
| `npm test` | 생성·TypeScript 빌드 및 일반 시험 18개 통과 |
| `cargo build -p telos-core --bin telos-weaver --locked --offline` | 통과. Rust 구현을 변경한 것이 아니라 실제 연결 시험용 바이너리 빌드 |
| `npm run test:core` (명시한 TELOS_CORE_EXECUTABLE) | 실제 Core 시험 1개 통과. 인증·Degraded·유효 Task 시작 거부·연결 해제 후 생존·기존 PID 재사용 |
| `npm run test:host` (명시한 TELOS_CORE_EXECUTABLE·TELOS_VSCODE_EXECUTABLE) | 실제 Extension Host 종료 코드 0, 5개 확인 항목 및 Core의 Host 종료 후 생존 통과 |
| `git diff --check` | 통과 |

일반 시험은 Windows/UNC/Unicode/Unix 공통 경로 벡터, Rust 상수 일치, UTF-8 조각·혼재 응답·잘못된 프레임·버전·크기·시간 초과·진단/토큰 비노출·인증/정체성·토큰 회전·늦은 연결 응답·slow_consumer 재생·이벤트 유실·Restricted Mode·다중 루트 선택을 확인했습니다.

## 4. 실환경 증거와 정리

실제 Core 시험의 최종 PID는 42604였습니다. 정책 없이 인증·Health를 확인하고 유효한 runs.start 요청이 unavailable로 거부되는 것을 검사했습니다. 분리된 두 연결에서 같은 Core를 재사용했고, 시험이 만든 PID만 종료했습니다.

최종 Extension Host 결과는 `%TEMP%/telos-ext-host-4Q5261/result.json`에 있습니다. 결과: passed=true, Core PID 23928, coreSurvivedHostExit=true. 임시 프로젝트·별도 VS Code user-data-dir/extensions-dir·별도 LOCALAPPDATA/XDG 상태를 사용했습니다. 시험이 만든 Core는 바깥 Runner가 Host 종료 후 생존을 확인한 다음 종료했습니다. 사용자 Core·대화·정책·인증 파일은 시험 대상으로 삼지 않았습니다. 실제 Provider 호출·시험 세션 생성·계정 사용량 소모는 없습니다.

표준 CLI 시험의 전체 단위 수와 실제 Extension Host 확인 항목 수를 합산해 실환경 시험 개수로 표시하지 않습니다. Linux/Remote SSH는 경로·구조 고려와 공통 벡터 수준이며 실행 검증은 하지 않았습니다.

## 5. 통합 인계

- INTEGRATION은 `ph0/ext`의 소유 경로와 변경을 확인해 통합하고 `npm ci --ignore-scripts`, `npm test`, 필요한 Windows 실제 Core·Host 시험을 재검증해 주십시오. Core 바이너리 경로는 명시적으로 공급하며 CI·배포 자동 다운로드를 추가하지 않았습니다.
- 현재 Phase 0 범위에 공통 API·Cargo 변경 요청은 없습니다. 기본 연결은 RN의 후속 구현 없이 시험 가능합니다.
- 최소 명령·사용법·개발 구성은 [Extension README](../../../../extension/README.md)를 따릅니다. 전체 Workspace 디버그 목록에서 사용하려면 extension 폴더를 추가하거나 그 폴더를 별도 창으로 엽니다.
- Provider Health 세부 정보·전체 Run 목록·마스터 전용 API·Core 종료 API가 필요해지면 CONTRACT·INTEGRATION·HOST의 변경 요청으로 처리해야 합니다.
- 전체 G2·G3 판정과 integrated/verified 전환은 기존 통합 담당이 수행합니다. Windows PH0-EXT 증거를 Linux·Remote SSH나 전체 Phase 0 완료로 확대하지 않습니다.

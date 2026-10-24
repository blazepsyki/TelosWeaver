# PH0-EXT 배정 및 구현 전 준비

| 항목 | 값 |
|---|---|
| 작업 ID·담당 | PH0-EXT · `Codex / PH0-EXT-20261024` |
| 작성일 | 2026-10-24 |
| 상태 | reserved. 사용자 직접 배정에 따른 사전 조사·결정 정리만 수행. 구현 미착수 |
| Branch / Worktree | `ph0/ext` / `D:\worktree\TelosWeaver\ph0-ext` |
| 기준 | main `207c125`, 배정 기록 `1102d52`, Spec 개정 1.4.4, 공통 계약 `3a37919` |
| 소유 경로 | `extension/**`, `docs/phase-0/evidence/PH0-EXT/**` |
| 기준 문서 | [Spec](../../../DEVELOPMENT_v1.4.md), [대표 배정표](../../WORK_ALLOCATION.md) |

## 1. Phase 0 범위

Spec §19.5.3·§19.5.7에 따라 생성 타입을 사용하는 Core 실행 파일 탐지·시작·연결·Health/상태 표시·오류 표시와 Extension 종료 후 Core 수명 분리를 구현합니다. 최소 UI는 Spec §17의 내장 UI 우선 원칙에 맞춰 StatusBar·TreeView·OutputChannel을 사용하도록 계획합니다.

전체 마스터 대화, Task/Artifact 상세 UI, 승인 화면, Scheduler, RN 구현, Provider 실행 정책은 이번 범위에 포함하지 않습니다. 현재 API로 조회하지 못하는 전체 Run 목록·마스터 상태·Provider 세부 상태를 만들어 표시하지 않습니다. RN 미구현은 기본 Core 연결의 선행 차단 조건이 아닙니다.

이번 준비에서는 코드·Manifest·의존성·생성물·VS Code 설정을 수정하거나 Core를 시작하지 않았습니다. 실제 모델 호출과 Extension Host 시험도 실행하지 않았습니다.

## 2. 현재 확인한 계약과 환경

| 항목 | 확인 결과 |
|---|---|
| Extension | TypeScript 기본 골격. showOutput 명령만 있고 연결 Client·Core 시작·상태 UI는 없음 |
| 타입 생성 | `extension/scripts/gen-types.mjs`가 `schemas/*.schema.json`에서 `src/generated/`를 생성. 생성물은 무시 대상이며 직접 편집·커밋하지 않음 |
| Node·npm | 사전 조사 셸의 Node v24.14.0, npm 11.9.0. 실제 Extension Host의 Node 환경과 구분 |
| VS Code 계약 | package.json의 engines.vscode는 ^1.100.0. Extension Host 실환경 동작은 미검증 |
| 개발용 Core 파일 | 대표 저장소 `target/debug/telos-weaver.exe`가 존재함. 존재 확인만 했으며 이번에 버전·Protocol·재빌드 결과를 검증하지 않음 |
| Local API | 버전 0.2, Named Pipe/Unix Socket, UTF-8 NDJSON 한 줄 1 MiB(종료 LF 제외), requestId 상관 관계 |
| 주소·인증 | canonical project root로 projectKey 계산. Windows Pipe와 사용자 상태 디렉터리의 api-token 경로는 `telos_schema::api` 문서 계약을 따름. 토큰 내용은 이번 준비에서 읽지 않음 |
| 재연결 | Core 재시작 시 토큰 재조회와 core.hello 재수행. Project·Host·Protocol 정체성을 확인 |
| 기존 시험 벡터 | `schemas/vectors/project-key.json`에 Windows·UNC·Unicode·Unix 경로 정규화·해시 값이 있음 |
| Health | 현재 Core는 execution/scheduler/persistence 상태를 보고하며, Phase 0의 전체 Health는 Degraded일 수 있음. 연결 성공을 모든 기능 정상으로 표시하지 않음 |
| API 한계 | core.hello·core.health·runs.start/inspect/cancel/subscribe/unsubscribe가 있음. Run 목록·마스터 전용 메서드·Core 종료 메서드는 없음 |
| 현재 IDE launch 설정 | 상위 Workspace의 .vscode/launch.json은 다른 프로젝트의 Go 디버그 구성. EXT 소유 경로 밖이므로 수정하지 않음 |

확인한 실제 소스는 `crates/schema/src/api.rs`, `crates/core/src/api/{server,security}.rs`, `crates/core/src/main.rs`, `crates/core/tests/phase0/client.rs`입니다. Core 진입점은 `serve --project-root <path> --project-id <id> --host-id <id> [--policy <file>]`을 요구합니다.

## 3. 사용자 결정 항목

다음은 구현 전 권장안이며 아직 사용자 확정 조건으로 취급하지 않습니다.

| 번호 | 결정할 내용 | 권장안 |
|---|---|---|
| 1 | Phase 0 Core 바이너리 공급 방식 | 빌드한 실행 파일의 절대 경로를 사용자/Host 설정으로 지정. 현재 개발용 파일을 후보로 확인하고 구현 단계에서 버전·실행 가능 여부 검증. 자동 다운로드·업데이트·Extension 활성화 시 Cargo 빌드는 하지 않음. 제품 배포용 동봉은 후속 결정 |
| 2 | Core 시작 정책 | 사용자가 Start/Connect 명령으로 명시적으로 시작·연결. 동일 프로젝트의 기존 Core에 먼저 연결하고 중복 시작을 피함. VS Code 열기만으로 시작하지 않으며 Extension 종료·재로드는 연결만 해제해 Core를 유지 |
| 3 | 초기 실행 권한과 실환경 검증 범위 | 정책 파일 없이 DenyRuns인 Core로 인증·Health·오류·재연결·수명 분리를 검증. Run 이벤트·slow_consumer는 통제된 시험 대역으로 검증. 실제 AI Provider 호출·사용량 소모 시험은 별도 결정 |

사용자가 기존 Core 실행 파일·정책 파일·Project/Host ID를 지정하면 그 값을 우선합니다. 처음 사용할 프로젝트 루트는 연결 명령에서 명시적으로 선택하게 합니다. 다중 루트 Workspace에서는 하나의 프로젝트를 선택하고, 폴더가 없는 창에서는 시작하지 않는 방안을 기본 기술안으로 둡니다.

새 Core의 ID를 별도로 지정하지 않을 때는 `project-<projectKey>`와 Windows 로컬의 `windows-local`을 Phase 0 기본값으로 제안합니다. 기존 Core에 연결할 때는 core.hello의 Project/Host 값을 확인·표시하며 다른 프로젝트의 상태를 섞지 않습니다. 원격 Host ID는 명시한 실행 Host 설정을 사용하고 로컬 값을 임의 복사하지 않습니다.

## 4. 사용자 선택 없이 처리할 기술 준비

- Rust 타입을 TypeScript에서 손으로 복제하지 않고 생성물을 사용합니다. Project Key 정규화·SHA-256은 공통 벡터로 HOST와 일치하는지 확인합니다. Protocol 버전·프레임 상한·메서드 값도 공통 계약과 대조합니다.
- NDJSON 조각·여러 프레임·UTF-8 바이트 상한·잘못된 JSON·응답/알림 혼재·시간 초과·끊어진 연결을 처리합니다. 대기 요청·이벤트 버퍼·재시도 횟수에 상한을 둡니다.
- 인증 토큰은 계약상 사용자 파일에서 읽어 연결할 때만 사용하며 설정·workspaceState·OutputChannel·오류에 원문을 저장하지 않습니다. 재시작 시 다시 읽습니다.
- 프로젝트의 Workspace Trust를 확인하고 실행 파일·정책 경로 설정의 Scope를 제한합니다. Restricted Mode의 동작은 실제 명령 처리에서도 검사합니다.
- Remote SSH를 고려해 Workspace Extension Host에서 Core·파일·IPC를 사용하는 구조로 계획합니다. Linux·Remote SSH 실제 시험은 기존 대기 지시를 유지하며 Windows 통과로 대체하지 않습니다. Web/가상 Workspace의 지원을 주장하지 않습니다.
- 알려진 Run을 구독하는 Client 경로는 subscriptionId·sequence를 검사합니다. slow_consumer는 마지막 수신·보존 sequence와 계약의 lastSequence를 대조해 유실 없이 재구독합니다. Core 재시작 뒤 사라진 Run을 자동 재실행하지 않습니다. 현재 API에 없는 전체 Run 복원·열거를 구현했다고 표시하지 않습니다.
- Extension의 deactivate에서 Core를 종료하지 않습니다. 명시적 Core 종료 UI가 필요하면 안전한 종료 계약을 HOST·CONTRACT·INTEGRATION에 먼저 요청합니다. 임의 PID 종료를 추가하지 않습니다.
- 최소 UI에는 연결 중·미연결·인증 실패·비호환·Degraded·Host·Core 버전을 구분해 표시합니다. Phase 0 상태를 실행·검증·Task 완료로 확대하지 않습니다.

구현 시 확인할 공식 자료: [Remote Extension Host](https://code.visualstudio.com/api/advanced-topics/remote-extensions), [Workspace Trust](https://code.visualstudio.com/api/extension-guides/workspace-trust), [Extension Host 시험](https://code.visualstudio.com/api/working-with-extensions/testing-extension). 이는 구현 참고 자료이며 시험 통과 증거가 아닙니다.

## 5. 구현 순서와 인계 경계

1. 사용자 결정 후 대표 배정표를 reserved → in-progress로 갱신하고 최신 main·공통 계약을 반영합니다.
2. 생성 타입·Project Key·상한·시간 제한을 갖춘 Client와 통제된 서버 시험 대역을 구현합니다.
3. Core 실행 파일 확인·기존 연결·명시적 시작·인증·Health·재연결·Extension 종료 시 연결 해제를 구현합니다.
4. 내장 상태 UI·오류 표시·Workspace 선택·Trust 처리를 연결합니다. 디버그/시험 구성은 우선 `extension/**` 안에 둡니다.
5. 생성·TypeScript 빌드·Node 시험과 Windows 실제 Core·Extension Host 시험을 구분해 수행합니다. 초기 실제 Core는 정책 없이 사용하고 모델을 호출하지 않습니다(권장안 승인 시).
6. 실행한 환경·명령·결과·수명 분리·미지원·남은 계약 요청을 증거에 기록하고 INTEGRATION에 G2 인계합니다. 외부 바이너리·테스트용 VS Code 다운로드가 필요하면 출처·버전·대상을 기록하며 기존 사용자 프로필을 시험에 사용하지 않습니다.

EXT는 `extension/package.json`·package-lock.json과 생성 스크립트를 소유합니다. 공통 API/Schema는 CONTRACT, Core 모듈·진입점·Cargo·루트 설정·통합 Harness는 INTEGRATION, 공용 API 서버·실행 감독은 HOST 소유입니다. 현재 최소 범위는 기존 공개 계약으로 구현 가능하다고 판단하며, 실제 구현에서 계약 불일치가 확인되면 구체적인 변경 요청으로 제출합니다.

아직 구현 변경·빌드·시험·성공 판정은 없습니다. 이 기록은 담당 예약과 사전 준비의 증거입니다.

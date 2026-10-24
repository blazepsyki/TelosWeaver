# PH0-HOST 준비 및 검증 기록

> **작성일:** 2026-10-24  
> **담당:** Codex / `PH0-HOST-20261024` (작업 식별자)  
> **상태:** reserved — 사용자 직접 배정, 구현 전 준비  
> **Branch / Worktree:** `ph0/host` / `D:\worktree\TelosWeaver\ph0-host`  
> **준비 기반:** `23605b0` (Spec v1.3.7)  
> **공통 계약 기준:** `ba97e7b` (G0 개정 1)  
> **결과 Commit:** 구현 전, 없음  
> **기준:** [Spec](../../../DEVELOPMENT_v1.3.md), [배정표](../../WORK_ALLOCATION.md)

## 1. 배정과 현재 단계

사용자가 PH0-HOST를 Codex에 직접 배정하고 구현 전에 필요한 결정·준비를 정리하도록 요청했습니다. 이에 따라 담당을 예약 기록하며 아직 in-progress나 구현 완료로 표시하지 않습니다. 기존 PH0-INTEGRATION·PH0-CONTRACT 담당은 변경하지 않습니다.

이번 준비 기록과 배정표 갱신은 사용자 지시의 범위입니다. 이후 공통 Schema·Adapter Trait·모듈 등록·Cargo 의존성·Daemon 진입점 변경은 기존 변경 요청 절차와 파일 소유권을 따릅니다. 실제 구현은 별도 Worktree에서 진행합니다.

## 2. 담당 범위와 구현 방향

| 영역 | 담당 내용 | 경계 |
|---|---|---|
| 공용 런타임 | 프로세스 시작, stdin/stdout/stderr, 줄·버퍼 상한, 이벤트 수집, 대기·취소·자식 회수 | `crates/adapters/src/runtime/**`. Provider별 Protocol 해석은 포함하지 않습니다. |
| CLI 공통 탐색 | 확장 메타데이터·폐기 폴더·버전·플랫폼 해석, 후보 검증·해시·출처 기록 | `crates/adapters/src/discovery/**`. 확장 ID·상대 경로·버전 명령은 Provider 규칙으로 받습니다. |
| 최소 Core API | Pipe/Socket, 인증·프레임·Health·Run/구독의 최소 연결 | `crates/core/src/api/**`. 전체 UI·Task MCP·Scheduler를 구현하지 않습니다. |
| 실행 감독 | Adapter 호출과 이벤트·취소·실제 종료 상태 연결 | `crates/core/src/execution/**`. 마스터 정책·SQLite·Workspace Manager는 별도 작업입니다. |
| 시험 | runtime_*/discovery_*/host_* 전용 시험과 Fixture, Windows 및 준비된 Linux Host | 실제 AI 실행에 앞서 로컬 시험 프로세스·테스트 대역으로 검증합니다. |

구현의 기본 방향은 기존 Tokio를 사용한 비동기 프로세스·I/O·통신, Windows Job Object와 Unix Process Group의 플랫폼별 처리, G0 공유 Handle의 Actor 또는 내부 동기화입니다. 취소 경로는 출력 Backpressure에 막히지 않게 분리합니다. 이는 Spec 범위 안의 구현 계획이며 세부 API·의존성은 아래 요청과 맞춰 확정합니다.

## 3. 구현 전 변경 요청

### REQ-010 CLI 탐색 계약과 모듈 등록

- **요청 작업:** PH0-HOST.
- **처리 주체:** PH0-CONTRACT가 공통 타입·`crates/adapters/src/lib.rs` 등록을 처리하고 PH0-INTEGRATION이 기준을 조정합니다. HOST는 자기 discovery 경로를 구현합니다.
- **현재 상태:** Spec에 탐색 책임은 있지만 discovery 모듈 등록과 탐색 결과 계약은 아직 없습니다. 소비 작업이 각자 공통 타입을 만들지 않도록 먼저 맞춰야 합니다.
- **필요 계약:** 탐색 입력의 실행 Host·플랫폼·설정 경로·확장 루트, Provider 규칙의 확장 ID·실행 파일 상대 경로·버전 명령, 결과의 Provider·Host·선택 출처·실행 경로·확장 ID/버전·CLI 버전·파일 해시·진단/미탐지 이유.
- **요청 내용:** 공개 결과 타입의 위치와 직렬화 필요 범위를 정하고 `pub mod discovery` 등록을 연결합니다. Core API/Extension으로 넘기는 공통 DTO가 필요하면 Schema에서 생성합니다. Provider 전용 인자·Protocol·Agy 데이터 디렉터리 판정은 해당 Adapter에 남깁니다.
- **호환 계획:** 현재 G0 타입에 임시 필드를 추가하지 않습니다. 기존 Capability·Snapshot과의 연결이 바뀌면 CONTRACT가 먼저 기준에 반영하고 소비자가 갱신합니다.
- **수용 시험:** extensions.json·.obsolete, 여러 버전·플랫폼, metadata 누락, 후보 없음/여러 개, 명시 경로/확장/PATH 출처, 버전 확인 실패, 파일 해시를 Fixture로 검사합니다.
- **상태:** 검토 대기. 결과 타입과 등록을 제가 임의로 확정한 상태는 아닙니다.

### REQ-011 실행 의존성과 Daemon 진입점 연결

- **요청 작업:** PH0-HOST.
- **처리 주체:** PH0-INTEGRATION이 Cargo.toml·Cargo.lock과 `crates/core/src/lib.rs`·main.rs를 소유합니다. HOST는 자기 api/execution/runtime 경로와 전용 시험을 작성합니다.
- **현재 상태:** Tokio는 Workspace에 있지만 HOST 구현에 필요한 일반 dependencies와 기능 구성이 연결되어 있지 않습니다. Core 진입점도 버전 출력 외에는 미구현입니다.
- **필요 준비:** 기존 Tokio 1.54.1을 기준으로 process·net·io-util·sync·time·rt/rt-multi-thread·macros의 Crate별 필요한 기능을 연결합니다. Windows Job Object·현재 사용자/ACL, Unix 프로세스 그룹·UID, 안전한 토큰 난수·실행 식별자에 필요한 플랫폼 의존성은 기존 라이브러리 재사용 여부와 함께 통합 담당이 버전을 고정합니다.
- **등록 요청:** HOST가 공개할 API 서버·실행 감독의 시작/종료 인터페이스와 Core 진입점의 호출 경계를 정합니다. HOST가 공통 진입점을 직접 고치거나 다른 담당이 같은 서버를 중복 구현하지 않게 합니다.
- **호환 계획:** 불필요한 전체 기능 활성화나 Crate별 독자적인 버전 추가를 피합니다. 공용 모듈 등록·의존성 Commit을 먼저 배포하고 HOST 구현에서 사용합니다. 필요한 실행 인자나 공개 계약의 변경은 별도로 기록합니다.
- **수용 시험:** Windows/Linux 조건부 컴파일, 최소 서버 시작·Health·인증 거부·종료, 프로세스 취소·자식 회수의 시험 경로가 같은 기준에서 빌드되어야 합니다. Linux 검증은 환경 준비 뒤 수행합니다.
- **상태:** 검토 대기. 의존성·진입점 코드는 이번 준비 단계에서 변경하지 않습니다.

## 4. 환경 확인과 준비

2026-10-24 Windows에서 읽기 전용으로 확인한 값입니다.

| 항목 | 확인 결과 | 다음 준비 |
|---|---|---|
| Rust | rustc 1.94.0, cargo 1.94.0 | 현재 Workspace 기준 사용 |
| Node | v24.14.0 | HOST Rust 구현의 실행 의존은 아니며 공통 생성 검사에 사용 |
| WSL | `wsl --list --verbose`가 설치된 배포판 없음으로 응답 | 기존 결정대로 Ubuntu 26.04 Linux/Remote SSH Host 준비 필요 |
| 공용 런타임 | G0 개정 1 인터페이스·Stub 존재 | REQ-002·003의 실제 구현·수용 시험은 HOST가 담당 |
| CLI 공통 탐색 | discovery 디렉터리·모듈 등록 없음 | REQ-010 처리 뒤 작성 |
| Core API·Execution | 모듈 문서와 등록만 존재 | REQ-011 경계 확정 뒤 작성 |

WSL 구성은 [기존 절차](../../WSL_LINUX_HOST.md)와 PH0-INTEGRATION 소유권을 따릅니다. 공용 런타임 착수 시 병렬 준비하고 늦어도 G2 전에 마련한다는 기존 결정을 유지합니다. 배포판 설치, Linux 초기 사용자 설정, SSH 구성, 원격 확장 설치·로그인은 이번 준비 단계에서 실행하지 않았습니다.

Windows 구현·로컬 Fixture 준비와 Linux 환경 준비는 병렬로 진행할 수 있습니다. Linux/Remote SSH 증거가 확보되기 전 해당 환경 지원을 완료로 표시하지 않습니다. 운영 환경 설정 변경이나 초기 사용자 입력이 필요한 시점은 통합 담당과 조정합니다.

## 5. 착수 조건과 검증 계획

추가 제품 범위 결정은 요구하지 않습니다. 기술 준비와 계약·공통 파일 변경은 기존 담당자가 정한 절차로 처리합니다.

1. 예약 배정과 별도 Worktree를 확인합니다.
2. REQ-010·011의 공개 경계·필수 등록·의존성에 대한 처리 결과를 확인합니다. 미확정 공통 타입을 임시로 복제하지 않습니다.
3. INTEGRATION과 WSL 준비 시점·환경 증거 담당을 맞춥니다.
4. 코드 착수 시 상태를 in-progress로 갱신하고 공용 런타임 → CLI 탐색 → 최소 API/실행 감독 순서로 구현하되 독립된 Fixture 작업은 병렬 준비할 수 있습니다.
5. 대기 중 입력·취소, 반복/경합 취소, 출력 오류·긴 줄·느린 소비자, 구독 전 종료, CLI 후보 해석, 인증·프레임·API 연결을 전용 시험으로 확인합니다.

이번 기록은 준비 상태이며 HOST 구현 시험을 통과했다고 기록하지 않습니다. 이후 실제 명령·환경·결과·미검증 항목을 이 문서에 추가합니다.

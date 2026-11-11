# Phase 1 수용 대조표 (H0 등록)

Spec §19.6.2와 배정표 §5에 따라 §18의 P0 항목·혼합 단계 항목의 P0 부분, §20.3 P0 시나리오를 담당 작업·필수 환경·시험에 연결합니다. H0에서는 목록만 등록하고 결과 열은 비워 둡니다. H3에서 실제 실행 결과와 증거를 채우며, 목록이 있다는 이유로 통과로 보지 않습니다.

- 작성: `claude-opus-5.5/session-c9336d2c` (PH1-INTEGRATION), 2026-11-11
- 담당 열은 §19.6.3의 작업 ID에서 `PH1-`를 뺀 이름입니다. 첫 번째가 주 담당, 뒤는 연계 작업입니다. 담당이 정해지면 착수 때 자기 행을 확인하고 변경이 필요하면 변경 요청으로 올립니다.
- 시험 수준: **C** Contract·단위 시험, **I** Core 통합 시험(Mock Provider 허용), **E** 실환경(실제 CLI·확장·RN). E가 있는 항목은 Mock 통과만으로 완료하지 않습니다.
- 필수 환경: **W** Windows 로컬, **L** Linux `psyki-nas`(SSH 명령), **R** VS Code Remote SSH UI. Phase 0 G3의 환경 결정을 이어받으며(배정표 §6), 바꾸려면 사용자 결정이 필요합니다. 필수 환경을 검증하지 못하면 H3를 보류합니다.
- 시험 경로는 각 작업의 독점 경로 규칙(`crates/core/tests/<모듈>_*.rs`, Adapter·Extension·RN은 각 경로)을 따릅니다. E2E는 `crates/core/tests/phase1/`(INTEGRATION)에 둡니다.

## 1. §18 요구사항

### 1.1 Task·마스터·Provider (§18.1)

| ID | P0 범위 | 담당 | 수준 | 환경 | 결과 (H3) |
|---|---|---|---|---|---|
| TASK-01 | Task 생성·수정·조회·취소, Revision | SCHED, PERSIST, HOST | C·I | W | |
| TASK-02 | Task별 Provider/Model/Role, 시작 전 검사 | SCHED, CONFIG, 세 Provider | C·I·E | W | |
| TASK-03 | Provider/Host별 큐·동시성 상한 | SCHED | C·I | W | |
| TASK-05 | 의존·순환 검사 (P1 DAG 병렬 제외) | SCHED | C | W | |
| TASK-06 | 실패 분류·Timeout·재시도·취소·최종 상태 | SCHED, HOST | C·I | W | |
| TASK-09 | 코딩·비코딩 Task를 같은 기본 모델로 실행 | MASTER, SCHED, INTEGRATION | I·E | W·L | |
| TASK-10 | 기본 Workspace·권한·출력 계약 (P1 확장 Profile 제외) | SCHED, WORKSPACE, ARTIFACT, MASTER | C·I | W | |
| MASTER-01 | 마스터 Provider 선택, 구조화 결정 검증 | MASTER, CONFIG | C·I·E | W | |
| MASTER-02 | 위임·Worker 요청·검증·재작업·통합 총괄 | MASTER, SCHED, VALIDATION | I·E | W | |
| MASTER-03 | 권한 안의 판단과 사용자 개입 구분 | MASTER, HOST, EXT | C·I | W | |
| MASTER-04 | Epoch·중복 차단·안전 보류 (P1 자동 재개 제외) | MASTER, PERSIST, SCHED | C·I | W | |
| MASTER-05 | 마스터 사용량·예산을 Plan에 포함 | MASTER, CONTEXT | C·I | W | |
| AGENT-01 | Provider/Model/Role 분리, 같은 역할의 Provider 교체 | 세 Provider, SCHED | I·E | W | |
| AGENT-02 | 세 Provider 신규 Managed Run의 공통 결과·오류 | CODEX, CLAUDE, AGY | C·E | W·L | |
| AGENT-03 | 필수 Capability 탐지 (P1 상세 진단 제외), 판정 기준(이월 P1-1) | 세 Provider, CONTRACT | C·E | W·L | |
| AGENT-05 | 확장 대화 목록 표시 `listed`/`hidden`, 판정 근거, 이중 조작 방지 | 세 Provider, CONFIG | E | W·R | |

### 1.2 Session·문맥·포크 (§18.2)

| ID | P0 범위 | 담당 | 수준 | 환경 | 결과 (H3) |
|---|---|---|---|---|---|
| SESS-01 | 내부 ID·Native Session·Turn·Run 관계 저장 | PERSIST, SCHED | C·I | W | |
| SESS-02 | 실시간 이벤트·오류·도구·종료 분리 | HOST, 세 Provider | C·E | W | |
| SESS-03 | 확정 상태·결과 영속화와 UI 재접속 조회 | PERSIST, HOST, EXT | I·E | W·R | |
| SESS-05 | 유실·불명 상태 탐지 (P1 재연결 제외) | PERSIST, SCHED, HOST | C·I | W | |
| SESS-06 | 기본 Capsule로 계약·자료 전달 | CONTEXT | C·I | W | |
| SESS-07 | Provider 간 Capsule 인계 (P1 고도화 제외) | CONTEXT, 세 Provider | I·E | W | |
| CTX-01 | 실행 전 지침·입력·환경·설정 Snapshot 고정 | CONTEXT, CONFIG | C·I | W | |
| CTX-02 | 필수 입력 누락·충돌의 실패·질문·제한 실행 구분 | CONTEXT, MASTER | C·I | W | |
| CTX-03 | 출처·불확실성 표시 (P1 EvidenceRef 제외) | CONTEXT | C | W | |
| CTX-04 | 외부 콘텐츠와 지침의 신뢰 경계 | CONTEXT | C·I | W | |
| FORK-01 | 네 전략 계약·Capability·PoC 결과 표기 (Native Fork는 PoC 결과만) | CONTEXT, 세 Provider | C·E | W | |
| FORK-03 | 미지원·실패 시 명시적 대체, Workspace·권한 경계 | CONTEXT, SCHED | C·I | W | |
| FORK-04 | 전체 사용량 기록 (P1 전략 비교 제외) | CONTEXT, 세 Provider | C·I·E | W | |

### 1.3 위임·통신 (§18.3)

| ID | P0 범위 | 담당 | 수준 | 환경 | 결과 (H3) |
|---|---|---|---|---|---|
| DELEG-01 | 다른 Provider의 제한된 하위 Task 생성과 ID 반환 | MCP, SCHED, 세 Provider | C·I·E | W·L | |
| DELEG-02 | 비동기 결과 조회, 계약·결과 Version 연결 | MCP, ARTIFACT | C·I | W | |
| DELEG-03 | MCP와 UI의 조회·취소 상태 일치 | MCP, SCHED, EXT | C·I·E | W | |
| DELEG-04 | 깊이·전체 Task·예산 상한과 중복 자식 방지 | SCHED, MASTER | C·I | W | |
| DELEG-06 | 기본 Artifact 인계 (P1 Handoff 고도화 제외) | ARTIFACT, CONTEXT | C·I | W | |
| DELEG-09 | ACK만으로 Task Completed 처리 금지 | SCHED, MCP | C·I | W | |
| DELEG-10 | 동기 대기 자원·Lock 처리와 불가능한 위임의 명시적 거부 | SCHED, WORKSPACE | C·I | W | |
| COMM-01 | RN Fork 연결, Health·기능·Host 조회 | RN, HOST | I·E | W·L | |
| COMM-03 | 통제된 RPC 전송 (P1 기존 세션 전달 제외) | RN, HOST | C·E | W | |
| COMM-05 | 전달·실행·검증을 별도 상태·UI로 구분 | EXT, HOST | C·E | W·R | |
| COMM-07 | RPC 중복·TTL 검사 (P1 종단 시험 제외) | RN | C·I | W | |
| COMM-08 | Fork/RPC 비호환 오류와 Degraded 표시 | RN, EXT | C·E | W | |
| COMM-10 | Artifact 참조 원칙 (P1 Attached 제외) | ARTIFACT, RN | C | W | |

### 1.4 Workspace·Artifact·Validation (§18.4)

| ID | P0 범위 | 담당 | 수준 | 환경 | 결과 (H3) |
|---|---|---|---|---|---|
| WS-01 | none/directory/git-worktree | WORKSPACE | C·I | W·L | |
| WS-02 | 비Git Writer 소유권 (P1 수명·정리·운영 제외) | WORKSPACE, SCHED | C·I | W | |
| GIT-01 | Task별 Worktree/Branch 생성·정리 | WORKSPACE | I | W·L | |
| GIT-02 | 같은 Worktree의 동시 Writer 차단 | WORKSPACE, SCHED | C·I | W | |
| GIT-03 | Base·Dirty·기존 변경 기록과 보존 | WORKSPACE | C·I | W·L | |
| GIT-04 | 대상 Revision의 변경 파일·Diff 표시 | WORKSPACE, EXT | C·I·E | W | |
| GIT-05 | 원본 Branch 반영에 개별 행위 정책·승인 적용 | WORKSPACE, VALIDATION, MASTER | I | W | |
| GIT-08 | 검증·승인·반영 대상 Revision 동일성 | WORKSPACE, VALIDATION | C·I | W | |
| ART-01 | 텍스트·파일·JSON·Diff 등록·조회 | ARTIFACT | C·I | W | |
| ART-02 | Output 이름과 Producer 기반 InputBinding | ARTIFACT, CONTEXT | C·I | W | |
| ART-03 | 불변 Version·Checksum·생성 Run·참조 수명 | ARTIFACT | C·I | W | |
| VAL-01 | Schema/File/Command/Human 기본 검증 계약 | VALIDATION, CONFIG | C·I | W | |
| VERIFY-01 | 명령·종료 코드·로그·시간·대상 Version 저장 | VALIDATION | C·I | W | |
| VERIFY-02 | 실행 완료와 검증 통과·최종 완료 구분 | VALIDATION, SCHED | C·I | W | |
| VERIFY-03 | 사용자/마스터 승인·반려와 대상 고정 | VALIDATION, MASTER | C·I | W | |
| VERIFY-06 | 실행 Agent의 검사 정책 완화 차단 | MASTER, VALIDATION | C·I | W | |
| VERIFY-07 | 출력 후보 변경 시 재검증·재승인 | VALIDATION, SCHED | C·I | W | |

### 1.5 RN·설정·운영·UI·안전 (§18.5)

| ID | P0 범위 | 담당 | 수준 | 환경 | 결과 (H3) |
|---|---|---|---|---|---|
| RN-01 | Windows/Linux Build·설치·제거 재현 | RN | E | W·L | |
| RN-02 | Health/Discovery/Message RPC 계약·Version | RN | C·E | W | |
| RN-03 | Envelope·상관 ID 계약 (P1 종단 제외) | RN, CONTRACT | C | W | |
| RN-07 | Schema·Backup 기준 (P1 Migration 제외) | RN | C | W | |
| RN-08 | Fork 패치 기록 (P1 Upstream 추적 제외) | RN | 문서 | — | |
| RN-09 | 제거·유지 처리표 (Phase 0 완료, 회귀만) | RN | 회귀 | W·L | |
| RN-10 | 제거 대상 정리 (Phase 0 완료, 회귀만) | RN | 회귀 | W·L | |
| RN-11 | 신원·Endpoint 정체성 (Phase 0 완료, 회귀만) | RN | 회귀 | W | |
| RN-12 | 제거 결과 검증·재유입 거부 (Phase 0 완료, 회귀만) | RN | 회귀 | W·L | |
| CFG-01 | 설정 계층·우선순위·출처·해시 | CONFIG | C·I | W | |
| CFG-02 | 권한 상한의 교집합과 하위 위임 적용 | CONFIG, MASTER | C·I | W | |
| CFG-03 | RN·Task MCP 설치 설정 소유권·사용자 변경 보존 | RN, MCP, CONFIG | I·E | W | |
| OPS-01 | UI 수명 분리, 프로젝트별 단일 Scheduler 소유권 | SCHED, HOST, EXT | I·E | W·R | |
| OPS-02 | 로그·큐·컨텍스트 상한과 느린 UI | HOST, EXT | C·I | W | |
| OPS-03 | 유실 탐지·안전 보류 (P1 전체 재조정 제외) | PERSIST, SCHED, HOST | C·I | W | |
| UI-01 | Task/Agent/마스터·Provider·Host 표시 | EXT | E | W·R | |
| UI-02 | Run별 텍스트·오류·도구·검증 로그 | EXT, HOST | E | W·R | |
| UI-03 | 개입·질문·승인·취소 | EXT, HOST, MASTER | E | W·R | |
| UI-04 | 내장 파일·Diff 보기 | EXT | E | W·R | |
| UI-06 | Task/Artifact/검증 상세 (P1 Session 상세 제외) | EXT | E | W | |
| UI-07 | Reload 후 기록·상태 재조회 | EXT, HOST | E | W·R | |
| UI-10 | RN 오류·버전 (P1 상세 Health 제외) | EXT, RN | E | W | |
| SAFE-01 | 명령·파일·네트워크 실행 범위 집행 | MASTER, CONFIG, 세 Provider | C·I·E | W·L | |
| SAFE-02 | 위임·동시성·전체 작업 상한 | SCHED, MASTER | C·I | W | |
| SAFE-03 | Timeout/Cancel/Kill과 자식 종료 확인(이월 P1-5) | HOST, 세 Provider | I·E | W·L | |
| SAFE-04 | 비밀정보 마스킹·보존 정책 | HOST | C·I | W | |
| SAFE-05 | 부작용 행위와 사용자 전용 결정의 별도 통제 | MASTER, VALIDATION | C·I | W | |
| SAFE-06 | 상태·결정·이벤트 영속화 (P1 전체 복구 제외) | PERSIST | C·I | W | |
| SAFE-07 | 버전·필수 Capability 검사, 미지원 차단 | 세 Provider, SCHED | C·E | W·L | |
| SAFE-08 | Task MCP 인증·Project/Run Scope | MCP, HOST | C·I | W | |
| SAFE-09 | 외부 콘텐츠의 권한·정책 승격 금지 | CONTEXT, MASTER | C·I | W | |
| SAFE-10 | 단일 실행 Host 식별 (P2 다중 Host 제외) | CONFIG, HOST | C·I | W·L | |
| SAFE-11 | Agent 메시지의 출처·실행 권한 구분 | RN, MASTER | C·I | W | |
| SAFE-12 | RN 장애 중 Managed Run 추적·취소·결과 보존 | RN, HOST | I·E | W | |
| SAFE-13 | 중복 결정·전달·재실행 방지, 결과 불명 재조정 | PERSIST, SCHED | C·I | W | |
| SAFE-14 | 메시지 수신 승인과 실행 승인 분리 | RN, MASTER | C·I | W | |

§18.6(첫 확장·후속 자동화)에는 P0 항목이 없습니다.

## 2. §20.3 E2E 시나리오

E2E는 INTEGRATION이 `crates/core/tests/phase1/`에 두고 각 단계의 담당이 시험 대역·Fixture를 제공합니다. 실제 Provider가 필요한 단계는 E로 표시합니다.

| ID | P0 범위 | 연결 요구사항 | 참여 작업 | 수준 | 환경 | 결과 (H3) |
|---|---|---|---|---|---|---|
| E2E-01 | 비Git 초안 → 다른 Provider 검토 → 최종 문서, 출력 계약·Version·검증 연결 | TASK-09, MASTER-02, ART-01~03, VAL-01 | MASTER, SCHED, ARTIFACT, VALIDATION, 세 Provider | E | W | |
| E2E-02 | 필수 입력 누락·상충 조사, 질문·불확실성 (P1 주장별 근거 제외) | CTX-02·03, MASTER-03 | CONTEXT, MASTER, EXT | I·E | W | |
| E2E-04 | 동시성 1·Writer Lock 아래 자식 위임, 진행 또는 즉시 설명 가능한 거부 | TASK-03, DELEG-01·04·10, GIT-02 | SCHED, MCP, WORKSPACE | I·E | W | |
| E2E-05 | 승인 뒤 결과·행위 변경, 이전 승인 재사용 차단 | VERIFY-03·07, SAFE-05, GIT-05·08 | VALIDATION, WORKSPACE, MASTER | I | W | |
| E2E-06 | 취소/완료 경합·이전 Run 지연, 확정 상태와 새 결과 보호 | TASK-06, DELEG-09, SAFE-13 | SCHED, PERSIST, HOST | I | W | |
| E2E-07 | UI/RN/Core/마스터 장애, 보존·안전 보류 (P1 재개 제외) | SESS-05, OPS-03, SAFE-06·12, MASTER-04 | PERSIST, SCHED, HOST, RN, EXT | I·E | W·R | |
| E2E-08 | 대량 스트림·느린 UI·부분 JSON·디스크 부족, 상한·제어 이벤트·진단 | OPS-02, SAFE-04 | HOST, EXT, PERSIST | I | W | |
| E2E-09 | 기존 MCP/Hook 환경의 설치·수정·제거, 사용자 설정 보존 | CFG-03 | RN, MCP, CONFIG | E | W | |
| E2E-13 | 다른 Provider·포크 미지원·실패, Capsule/Fresh 대체와 필수 입력 유지 | FORK-03, SESS-07, AGENT-03 | CONTEXT, 세 Provider | I·E | W | |
| E2E-14 | Worker 승인·권한 초과·마스터 장애, 권한·중복 차단 (P1 재개 제외) | VERIFY-06, SAFE-05·13, MASTER-04, 이월 P1-2(실시간 승인) | MASTER, HOST, VALIDATION, CLAUDE | I·E | W | |

§19.2 통과 조건의 "코딩 작업 하나와 비코딩 작업 하나" 시나리오는 E2E-01(비코딩)과 E2E-04·05(코딩)의 실환경 실행으로 확인합니다. 별도 코딩·실패 시나리오는 H3 착수 때 이 표에 행을 추가합니다.

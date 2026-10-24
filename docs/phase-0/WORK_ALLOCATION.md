# Phase 0 작업 배정표

> **프로젝트:** TelosWeaver  
> **갱신일:** 2026-10-24  
> **범위 원본:** [Spec §19.5](../DEVELOPMENT_v1.4.md#195-phase-0-병렬-구현--작업-분할과-소유권)  
> **현재 상태:** G0 통과, 개정 1 반영(계약 기준 `ba97e7b`). PH0-HOST 1차 구현을 `d8e8725`로 통합했고(REQ-011 포함), REQ-010 탐지 계약을 `271e7fd`로 추가했습니다. HOST 탐지 엔진 연결(`b0a58b4`)과 PH0-CLAUDE Adapter(`41139e0`, `10abcc1`)를 통합했습니다. G1 Core 연결을 `d3130c1`로 통합하고 실제 Claude 실행으로 G1을 판정했습니다(HOST 확인 대기). 나머지 작업은 배정 요청을 받아 착수할 수 있습니다. G2·G3는 대기입니다.

## 1. 단일 원본과 갱신 주체

대표 저장소의 이 파일을 단일 원본으로 사용합니다. 대표 저장소의 위치는 최초 통합 담당이 착수 시 기록합니다. 다른 Worktree·Clone의 복사본은 참고용이며 그것만 바꿔 소유권을 확보하지 않습니다.

수정 범위는 Spec §19.5.5를 따릅니다.

| 누가 | 고칠 수 있는 곳 |
|---|---|
| 담당 에이전트 | 자기 작업 행(상태·Branch/Worktree·기준·결과 Commit·인계), 자기가 제출하는 §4 변경 요청 행, §7 변경 기록 추가 |
| 통합 담당만 | §1 관리 항목(계약 기준 Commit 등), §3 게이트 판정, §4 요청의 채택·완료 판정, §5·§6, 다른 작업의 행, `integrated`·`verified` 전환 |
| 공통 조건 | 배정표 수정은 대표 저장소 `main`에서 단독 커밋으로 합니다(코드 Branch에 섞지 않음). 배정 시작(`unassigned → reserved`)은 커밋 직전에 그 행이 아직 `unassigned`인지 확인합니다 |

작업 에이전트는 Spec의 작업 ID를 선택해 자기 행에 담당·Branch/Worktree·기준 Commit을 기록한 뒤 해당 경로를 수정합니다. 통합 담당이 정해지지 않았다면 최초 착수를 총괄하도록 지정된 진행 담당이 인수 사실을 기록합니다. 서로 다른 에이전트가 독립적으로 같은 역할을 차지하지 않습니다.

한 작업 ID에 Writer 한 명만 배정합니다. 같은 담당자가 여러 ID를 맡을 수 있으며, 담당자 ID는 동명이인이나 재시작 세션을 구분할 수 있는 세션·작업 식별자를 포함합니다. 작업 범위와 파일 예외는 Spec을 따르고 이 표에서 임의 확대하지 않습니다.

| 관리 항목 | 현재 값 |
|---|---|
| 대표 저장소 | `D:\Downloads\Vibe_dev\TelosWeaver`, Branch `main` (원격 없음) |
| 통합 담당자 ID | `claude-opus-5.5/session-c9336d2c` (2026-10-24 사용자 지정) |
| 공통 계약 기준 Commit | `271e7fd` (2026-10-24 REQ-010 CLI 탐지 계약 추가). G0 개정 1 `ba97e7b`, 1차 기준 `ae31eb2` |
| 공통 계약 기준 설명 | §5의 7개 계약. 변경은 §4 변경 요청으로만 처리합니다. 판정 근거: `evidence/PH0-INTEGRATION/README.md`, 개정: `evidence/PH0-INTEGRATION/G0_REVISION_1.md` |

## 2. 배정 상태

| 작업 ID | 담당자 ID | 상태 | Branch / Worktree | 기준 Commit | 결과 Commit | 인계·요청 |
|---|---|---|---|---|---|---|
| PH0-INTEGRATION | `claude-opus-5.5/session-c9336d2c` | in-progress | `main` / 대표 저장소 작업 트리 | `fca1669` | `ae31eb2` (G0), `ba97e7b` (G0 개정 1), `271e7fd` (REQ-010) | HOST 1차 통합·REQ-011(`d8e8725`), REQ-010 계약(`271e7fd`)·엔진 연결(`b0a58b4`), CLAUDE 통합(`10abcc1`), G1 Core 연결(`a810700`, 통합 `d3130c1`), AGY 통합·REQ-013 ①②(`b0f5b5e`), CODEX 통합·REQ-012·014(`c00d2b0`). 다음: Codex·Agy Core 연결(G2), G1 HOST 확인 반영, 나머지 Provider·MASTER·RN·EXT 통합(G2), WSL 구성(사용자 수행 필요) |
| PH0-CONTRACT | `claude-opus-5.5/session-c9336d2c` | verified | `ph0/contract` / Worktree 정리됨 | `0094720` | 1차 `fd0b206`, 개정 1 `98177db` (통합 `ba97e7b`), REQ-010 `4e29f20` (통합 `271e7fd`) | 이후 계약 변경은 §4 요청으로 처리 |
| PH0-HOST | `Codex / PH0-HOST-20261024` | in-progress | `ph0/host` / 병합 후 삭제 | 후속 기준 `2ac1497` (Spec v1.4), 계약 `271e7fd` | 1차 `6bb943e` (통합 `d8e8725`), 후속 `77a925f` (코드 `e844791`, 통합 `b0a58b4`) | 사용자 요청 1~3 완료·통합: REQ-010 공개 엔진 연결, 공개 API 시험 17개. [검증·인계](evidence/PH0-HOST/README.md). 통합 후 실제 확장에서 Claude CLI 탐지 확인(PH0-CLAUDE PROV-01). 전달: 탐지 경로가 `\\?\C:\...` 형식으로 나옴(표시·비교 정규화 검토). **G1 확인 요청:** [INTEGRATION 증거 §7](evidence/PH0-INTEGRATION/README.md#7-g1-대표-실행)의 G1 판정을 HOST 관점(런타임 취소·자식 회수, 실행 감독 상태, 정상 종료 유예 동안 정상 종료되지 않은 점)에서 확인해 주십시오. **REQ-013 ③ 요청:** `discovery/mod.rs`가 `extensions.json`의 `"targetPlatform": "undefined"`(플랫폼 공용 확장)를 `"universal"`처럼 허용하도록 검토해 주십시오. 현재 Antigravity가 `metadata` 대신 `folder-scan`으로 해석됩니다(통합본에서 재현). 남은 일: WSL Linux·Remote SSH 실행 검증(사용자 지시로 대기) |
| PH0-CODEX | `Codex / PH0-CODEX-20261024` | integrated | `ph0/codex` / `D:\worktree\TelosWeaver\ph0-codex` (통합 후 증거 갱신) | 착수 `cc53fbf`, 계약 `271e7fd`, main 반영 `516a13d` | 코드 `2c6339b`(초기 `1f8aebd`), 인계 `cb87f80`·`d798988` (통합 `c00d2b0`) | 통합 완료. [검증·인계](evidence/PH0-CODEX/README.md): Mock 12개·Windows 실환경 7개 통과, 시험 Thread 8개 Archive. 통합본 118개·엄격 Clippy·fmt 통과, 병합본 탐지 실환경 재확인. 통합 후 현황 재확인(`15d36cb`): Workspace 118개·엄격 Clippy·fmt 통과. 통합 전 상태가 남은 자기 검증·인계 문서 갱신 `3e92d4c` 제출(문서만, 통합 담당 반영 대기). 남은 일: Core 연결(G2, INTEGRATION), `send_input`·승인 Allow 실환경 미검증, Linux 검증 |
| PH0-CLAUDE | `claude-opus-5.5/session-c9336d2c` | integrated | `ph0/claude` / 병합 후 삭제 | `0b5b34f`, 계약 `271e7fd` | `2142e05`·`18fcf77` (통합 `41139e0`·`10abcc1`) | G1 대표 Provider(§6). [검증 기록](evidence/PH0-CLAUDE/README.md): Mock 13개, 실환경 6개(haiku) 통과 — PROV-01~08, Resume·Fork 동작 확인. 남은 일: G1 Core 연결, 시험 세션 Archive(§5 목록), Linux 검증 |
| PH0-AGY | `gemini-4-argon/session-9e6137eb` | integrated | `ph0/agy` / 병합 후 삭제 | `63b6758`, 계약 `271e7fd` | `057e332` (통합 `b0f5b5e`) | 통합 완료(main 106개 통과, 병합본에서 탐지 실환경 재확인). 참고: 시험 정리는 `brain/<id>`·`conversations/<id>.pb`만 지우므로 `conversation_summaries.db`의 요약 기록은 남을 수 있음. Worktree에 커밋하지 않은 증거 수정 1줄이 있으면 별도 커밋으로 제출. 제출 내용: 구현·검증 완료([검증 기록](evidence/PH0-AGY/README.md)): Mock·Contract 14개, 실환경 6개(`gemini-3.8-flash-low`, `--effort low`) 통과 — AGY-DATA-01~05(`--app_data_dir=antigravity` 기본 채택·버전 명령 검증·잠금 충돌 없음), PROV-01~09, Resume(`--conversation`) 확인, NativeFork 미지원 거부(FORK-POC-01), 시험 세션 즉시 삭제 완료. REQ-013 제출(`pub mod agy;` 및 Spec §7.5 반영). 남은 일: 통합 후 Core 연결(G2), Linux 검증 |
| PH0-MASTER | 미배정 | unassigned | 미지정 | 미지정 | 없음 | 착수 가능. `DecisionPayload`·`DecisionContext`·`DecisionOutcome`과 공통 실패 Fixture 사용 |
| PH0-RN | 미배정 | unassigned | 미지정 | 미지정 | 없음 | 착수 가능. Fork 도입 방식·기준 SHA 기록, `telos_schema::rn` 계약 구현 |
| PH0-EXT | 미배정 | unassigned | 미지정 | 미지정 | 없음 | 착수 가능. 생성 타입과 `telos_schema::api` 문서 계약으로 테스트 대역 개발 |

상태는 `unassigned → reserved → in-progress → review → integrated → verified`를 사용합니다. `blocked`는 기존 담당자를 유지하며 차단 사유·필요한 계약·재개 조건을 기록합니다. 통합 전 `verified`로 표시하지 않습니다. 개별 `verified`는 Phase 0 전체 통과와 다릅니다.

## 3. 통합 게이트

| 게이트 | 상태 | 근거 |
|---|---|---|
| G0 공통 계약 확정 | 통과 (2026-10-24), 개정 1 반영 | 1차 `ae31eb2`. 독립 검토 REQ-002~009 처리 후 기준 `ba97e7b`. 근거: `evidence/PH0-INTEGRATION/README.md` §1, `G0_REVISION_1.md` §4 |
| G1 대표 Provider 실행 | 통과 판정 (2026-10-24, INTEGRATION·CLAUDE), HOST 확인 대기 | 대표 Provider Claude(§6). Local API → 운영자 정책 승인 → 확장 CLI 탐지 → Claude Adapter → 공용 런타임 경로로 시작·이벤트·완료·실패·취소·결과 파일·Daemon 종료 정리를 실제 실행(`d3130c1`, 실환경 5개). 근거: `evidence/PH0-INTEGRATION/README.md` §7 |
| G2 전체 구성 통합 | 대기 | 세 Provider·마스터·RN·Extension 연결 증거 필요 |
| G3 Phase 0 수용 | 대기 | Spec §19.1 필수 조건과 대상 환경 증거 필요. Linux·Remote SSH Host는 [WSL 절차](WSL_LINUX_HOST.md)로 확보 예정(미구성) |

기존 타입·빌드·단위 테스트가 존재한다는 이유만으로 게이트를 통과시키지 않습니다. 증거는 Spec §19.5.8의 형식으로 `docs/phase-0/evidence/<작업 ID>/README.md`에 기록합니다.

## 4. 계약·공통 파일 변경 요청

| 요청 ID | 요청 작업 | 변경 대상·이유 | 영향 작업 | 처리 주체 | 상태·결과 Commit |
|---|---|---|---|---|---|
| REQ-001 | PH0-INTEGRATION | `.gitattributes` 신설(LF 고정). `core.autocrlf=true` 환경의 새 Worktree가 CRLF로 체크아웃되어 생성물이 내용 변경 없이 수정으로 표시됨. 현재 Spec §19.5.3에 소유 작업이 없어 Spec에 INTEGRATION 경로로 추가 필요 | 전체 | 사용자 확인 후 INTEGRATION | 완료. 사용자가 LF로 결정, `.gitattributes` 추가·Spec v1.3.4에 경로 등록. Index 내용 변경 없음 |
| REQ-002 | G0 독립 검토 / PH0-HOST | 공용 ProcessHandle의 대기·입력·취소 동시성 및 wait 취소 안전성 계약 확정 | HOST, CODEX, CLAUDE, AGY, INTEGRATION | INTEGRATION 조정·HOST 처리, CONTRACT 확인 | 계약 완료(`7d296a8`, 통합 `ba97e7b`). 구현·수용 시험은 PH0-HOST 대기. [판정](evidence/PH0-INTEGRATION/G0_REVISION_1.md) · [요청](evidence/PH0-INTEGRATION/G0_REVIEW_REQUESTS.md#req-002-대기입력취소의-동시성-계약) |
| REQ-003 | G0 독립 검토 / PH0-HOST | 출력 스트림의 I/O 오류·긴 Protocol 줄·버퍼 초과를 명시적 실패/복구로 연결 | HOST, 세 Provider, INTEGRATION | INTEGRATION 조정·HOST 처리, CONTRACT 확인 | 계약 완료(`7d296a8`, 통합 `ba97e7b`). 구현·수용 시험은 PH0-HOST 대기. [판정](evidence/PH0-INTEGRATION/G0_REVISION_1.md) · [요청](evidence/PH0-INTEGRATION/G0_REVIEW_REQUESTS.md#req-003-출력-오류와-상한-초과-처리) |
| REQ-004 | G0 독립 검토 / PH0-CONTRACT | DecisionOutcome의 승인/거부·사유·효과 불변조건과 모순된 공통 시험 수정 | CONTRACT, MASTER, HOST, EXT | CONTRACT 처리·INTEGRATION 통합, MASTER 확인 | 완료(`98177db`, 통합 `ba97e7b`). [판정](evidence/PH0-INTEGRATION/G0_REVISION_1.md) · [요청](evidence/PH0-INTEGRATION/G0_REVIEW_REQUESTS.md#req-004-decisionoutcome의-불변조건) |
| REQ-005 | G0 독립 검토 / PH0-CONTRACT | HelloParams 및 원시 인증 요청의 Debug·로그 경로에서 토큰 원문 보호 | CONTRACT, HOST, EXT | CONTRACT 처리·INTEGRATION 통합, HOST·EXT 확인 | 완료(`98177db`, 통합 `ba97e7b`). [판정](evidence/PH0-INTEGRATION/G0_REVISION_1.md) · [요청](evidence/PH0-INTEGRATION/G0_REVIEW_REQUESTS.md#req-005-인증-토큰의-debug로그-보호) |
| REQ-006 | G0 독립 검토 / PH0-CONTRACT | Run 이벤트의 시작 실패·지속형 프로세스·취소·유실·종료 불명 경계 확정 | CONTRACT, HOST, 세 Provider, EXT | CONTRACT·HOST 협의·INTEGRATION 통합 | 완료(`98177db`, 통합 `ba97e7b`). [판정](evidence/PH0-INTEGRATION/G0_REVISION_1.md) · [요청](evidence/PH0-INTEGRATION/G0_REVIEW_REQUESTS.md#req-006-run-이벤트-종료-규칙) |
| REQ-007 | G0 독립 검토 / PH0-CONTRACT | Local API 주소·상태 디렉터리·토큰 수명·1 MiB 프레임 경계와 상호운용 시험 명세 | CONTRACT, HOST, EXT, INTEGRATION | CONTRACT·HOST·EXT 협의·INTEGRATION 통합 | 완료(`98177db`, 통합 `ba97e7b`). [판정](evidence/PH0-INTEGRATION/G0_REVISION_1.md) · [요청](evidence/PH0-INTEGRATION/G0_REVIEW_REQUESTS.md#req-007-local-api-주소인증프레임-규칙) |
| REQ-008 | G0 독립 검토 / PH0-CONTRACT | DecisionContext의 최소 목록 검사와 실제 권한 검사의 경계·신뢰된 조회 계약 명시 | CONTRACT, MASTER, HOST | CONTRACT·MASTER 협의·INTEGRATION 통합 | 완료(`98177db`, 통합 `ba97e7b`). [판정](evidence/PH0-INTEGRATION/G0_REVISION_1.md) · [요청](evidence/PH0-INTEGRATION/G0_REVIEW_REQUESTS.md#req-008-decisioncontext의-권한-검사-범위) |
| REQ-009 | G0 독립 검토 / PH0-CONTRACT | 알 수 없는 Payload 필드의 최상위/중첩 허용 정책과 Schema·Parser·Fixture 일치 | CONTRACT, MASTER, HOST | CONTRACT 처리·INTEGRATION 통합, MASTER 확인 | 완료(`98177db`, 통합 `ba97e7b`). [판정](evidence/PH0-INTEGRATION/G0_REVISION_1.md) · [요청](evidence/PH0-INTEGRATION/G0_REVIEW_REQUESTS.md#req-009-중첩-payload의-알-수-없는-필드) |
| REQ-010 | PH0-HOST | CLI 공통 탐색의 결과 계약·Provider 규칙 입력·discovery 모듈 등록. Spec §7.5의 경로·출처·버전·해시를 다른 담당자가 같은 형태로 사용하도록 확정 | CONTRACT, HOST, 세 Provider, EXT | CONTRACT 계약·등록, INTEGRATION 조정 | 완료. 계약 `4e29f20`(통합 `271e7fd`), HOST 엔진 연결·모듈 등록 `e844791`(통합 `b0a58b4`), 실제 확장 탐지 확인(PH0-CLAUDE). [상세](evidence/PH0-HOST/README.md#req-010-cli-탐색-계약과-모듈-등록) |
| REQ-011 | PH0-HOST | HOST 실행용 Tokio 기능·플랫폼/보안 의존성과 Core Daemon 진입점 연결 준비. 공통 Cargo·Lock·진입점은 INTEGRATION 소유 | HOST, INTEGRATION, 세 Provider, EXT | INTEGRATION 의존성·등록, HOST 구현 | 완료(`d8e8725`). 적용안대로 반영하되 `libc`는 배포 2주 기준으로 0.2.191 대신 0.2.190 고정. 공식 Workspace test 57개·clippy·fmt 통과. [상세](evidence/PH0-HOST/README.md#req-011-실행-의존성과-daemon-진입점-연결) |
| REQ-012 | PH0-CODEX | Codex 전용 구성·실행 승인 Port·cli_rule을 소비자가 사용할 수 있도록 Adapter lib.rs의 `mod codex;`를 `pub mod codex;`로 변경. 기존 Claude 공개 방식과 동일하며 공통 DTO·Trait 변경 없음 | CODEX, INTEGRATION, EXT | CONTRACT 등록·INTEGRATION 조정 | 완료(`c00d2b0`). `telos_adapters::codex::{CodexConfig, CodexLaunchResolver, cli_rule}` 공개 |
| REQ-013 | PH0-AGY | ① `crates/adapters/src/lib.rs`의 `mod agy;`를 `pub mod agy;`로 변경(Claude·Codex와 동일). ② Spec §7.5에 AGY-DATA-01~05 실환경 검증 결과(기본 `--app_data_dir=antigravity` 사용 + 실행 전 버전 명령으로 옵션 수용 확인, 미수용 시 조용한 대체 없이 `unsupported` 처리) 반영 및 비현실적 '확장 끔' 전제 정리. ③ (참고) VS Code `extensions.json`에서 universal 확장이 `"targetPlatform": "undefined"`로 기록되므로 `discovery/mod.rs`에서 `"universal"`과 함께 허용 검토 | AGY, INTEGRATION, HOST | CONTRACT 등록·INTEGRATION 조정 | ①② 완료(`b0f5b5e`, Spec v1.4.2 §7.5). ③ 채택, PH0-HOST에 전달(HOST 행), HOST 처리 대기. [근거](evidence/PH0-AGY/README.md) §3·§5 |
| REQ-014 | PH0-CODEX | 세 Provider가 Stub을 교체한 뒤 `crates/adapters/src/unimplemented.rs`에 남은 미사용 `unimplemented_adapter!` 매크로와 재수출 제거. `UnsupportedAdapter`·공통 계약 시험은 유지. 두 unused lint가 Codex 작업 Branch의 엄격 Clippy를 차단 | CONTRACT, CODEX, INTEGRATION | CONTRACT 정리·INTEGRATION 조정 | 완료(`c00d2b0`). 매크로·재수출 제거, `UnsupportedAdapter`·계약 시험 유지. 엄격 Clippy 통과 |

요청에는 필요한 타입·메서드·의존성과 호환·시험 계획을 포함합니다. 통합 담당과 공통 계약 담당이 먼저 기준 변경을 통합하고 소비 작업에 전달합니다. 소비 작업에서 임시 공통 타입을 만들거나 같은 파일을 각자 수정하여 우회하지 않습니다.

REQ-002~009는 독립 검토의 제출안이며 `952bb26`으로 접수했습니다. 판정과 처리 방식은 [G0 개정 1](evidence/PH0-INTEGRATION/G0_REVISION_1.md)에 기록합니다. 요청자와 확인 사실·설계 제안·호환·시험 계획은 [G0 독립 검토 변경 요청서](evidence/PH0-INTEGRATION/G0_REVIEW_REQUESTS.md)에 기록합니다. 처리 결과 계약 기준을 `ba97e7b`로 개정했습니다. 소비 작업은 이 기준으로 착수·갱신합니다.

## 5. G0 계약 범위와 등록 경로

Spec §19.5.2의 계약을 아래 위치에 정의했으며 `ae31eb2`로 확정하고 `ba97e7b`(G0 개정 1)로 개정했습니다. 이후 변경은 §4의 변경 요청으로만 처리합니다.

| 계약 | 정의 위치 | 작성 | 사용 작업 |
|---|---|---|---|
| Adapter 실행 | `telos_schema::{TaskSpec, RunContext, RunStarted, FailureKind}`, `telos_adapters::{AgentAdapter, AdapterError}` | CONTRACT | HOST, 세 Provider, MASTER |
| 이벤트·취소 | `telos_schema::ProviderEvent`와 kind별 Payload, `AgentAdapter::{events, cancel_run, inspect_run}` 문서 계약 | CONTRACT | HOST, 세 Provider, INTEGRATION |
| 공용 프로세스 경계 | `telos_adapters::runtime` 공개 인터페이스 (G0 Stub: `UnimplementedRuntime`, 구현 `NativeRuntime`) | INTEGRATION 등록, 이후 HOST 구현 | HOST, 세 Provider |
| CLI 탐지 (REQ-010) | `telos_schema::{CliDiscovery, CliDiscoveryOutcome, DiscoveredCli, ...}`, `telos_adapters::{ProviderCliRule, CliBinaryRule, DiscoveryRequest, CliDiscoverer}` | CONTRACT, 구현 HOST | HOST, 세 Provider, EXT |
| 마스터 결정 | `telos_schema::{MasterDecision, DecisionPayload, DecisionContext, DecisionOutcome}` | CONTRACT | MASTER, HOST |
| Local Core API | `telos_schema::api` (전송·인증 규칙은 모듈 문서) | CONTRACT | HOST, EXT |
| RN RPC | `telos_schema::rn`, `telos-rn-client`가 재노출 | CONTRACT 정의, INTEGRATION 등록 | RN, INTEGRATION |
| 생성·시험 | `schemas/*.schema.json`, `telos_schema::fixtures`(`fixtures` feature), `telos_adapters::UnsupportedAdapter` 시험 대역, 증거 형식 `evidence/PH0-INTEGRATION/EVIDENCE_TEMPLATE.md` | CONTRACT, INTEGRATION | 전체 |

G0에서 등록한 Stub 경로입니다. 등록된 파일은 이제 해당 작업의 독점 경로입니다.

| 경로 | 이후 소유 | 내용 |
|---|---|---|
| `crates/adapters/src/runtime/mod.rs` | HOST | 공용 프로세스 런타임 공개 인터페이스, `UnimplementedRuntime` |
| `crates/core/src/api/mod.rs`, `crates/core/src/execution/mod.rs` | HOST | Local API 서버·실행 감독 (`d8e8725`) |
| `crates/core/src/master/mod.rs`, `crates/core/src/policy/mod.rs` | MASTER | 모듈 문서만 |
| `crates/rn-client/src/lib.rs` | RN | `telos_schema::rn` 재노출 |

의존성 버전은 루트 `Cargo.toml`의 `[workspace.dependencies]`에서만 정합니다.

## 6. 환경·운영 결정

| 항목 | 결정 | 상태 | 결정일 |
|---|---|---|---|
| 줄바꿈 | 저장소 전체 LF 고정, `.cmd`·`.bat`만 CRLF (`.gitattributes`) | 적용 | 2026-10-24 |
| Linux·Remote SSH 실행 Host | 같은 PC의 WSL2 Ubuntu 26.04를 Linux Host 겸 Remote SSH Host로 사용. 절차는 [WSL_LINUX_HOST.md](WSL_LINUX_HOST.md). G3 필수 조건이므로 미루지 않음 | 절차 수립, 배포판 미설치. 구성 시점: PH0-HOST가 공용 런타임 구현에 착수할 때(늦어도 G2 전) | 2026-10-24 |
| CI | 원격 저장소가 생기고 초기 구현이 진행된 뒤 검토. 그전에는 통합 담당이 Spec §19.5.8의 검사 명령을 수동 실행하고 증거에 기록 | 보류 | 2026-10-24 |
| 대상 OS 우선순위 | 구현·일상 검증은 Windows 로컬 우선. Linux·Remote SSH는 G3 전에 WSL로 확보 | 적용 | 2026-10-24 |
| G1 대표 Provider | Claude Code. Spec §19.5.6의 기본값(Codex)에서 변경. 이유: PH0-CODEX 미배정, 통합 담당이 PH0-CLAUDE를 함께 맡음. 마스터 Provider 선택(MASTER-01)과는 무관 | 적용 | 2026-10-24 |
| Claude 실환경 시험 조건 | 모델 `haiku`, 짧은 프롬프트. 설정 디렉터리는 기본 `~/.claude`(확장과 공유, 토큰 복사·별도 로그인 없음). 실행은 저장소 밖 일회용 폴더에서만, 허용 도구 명시(`--allowedTools`/`--disallowedTools`, `--permission-mode`), `--dangerously-skip-permissions` 금지. 시험 세션은 끝난 뒤 확장 대화 목록에서 보이지 않도록 Archive. 기존 대화는 재개·수정하지 않음 | 적용 | 2026-10-24 |

## 7. 변경 기록

| 일시 (KST) | 기록자 | 변경 | 기준 Commit |
|---|---|---|---|
| 2026-10-24 01:05 | `claude-opus-5.5/session-c9336d2c` | 사용자 지정으로 통합 담당 인수. PH0-INTEGRATION `unassigned → in-progress`, PH0-CONTRACT `unassigned → reserved`, G0 `대기 → 준비 중`. 대표 저장소 기록 | `fca1669` |
| 2026-10-24 01:23 | `claude-opus-5.5/session-c9336d2c` | G0 준비: 의존성 버전을 `[workspace.dependencies]`로 일원화, `telos-schema` `fixtures` feature 추가, §5의 Stub 경로 등록, G0 계약 범위 기록 | `4b67618` |
| 2026-10-24 01:24 | `claude-opus-5.5/session-c9336d2c` | PH0-CONTRACT `reserved → in-progress`. Worktree `D:\worktree\TelosWeaver\ph0-contract`, Branch `ph0/contract` 생성 | `556e62f` |
| 2026-10-24 01:34 | `claude-opus-5.5/session-c9336d2c` | `ph0/contract`(`fd0b206`)를 `cc142cf`로 통합, RN Client 재노출 등록(`ae31eb2`), `main`에서 재검증. PH0-CONTRACT `in-progress → verified`, G0 `준비 중 → 통과`, 기준 Commit `ae31eb2` 확정. 나머지 작업 인계 갱신, REQ-001 등록, Worktree 정리 | `ae31eb2` |
| 2026-10-24 12:25 | `claude-opus-5.5/session-c9336d2c` | 사용자 결정 반영: REQ-001 완료(LF 고정 `.gitattributes`), Linux·Remote SSH Host WSL 절차 수립, CI 보류. §6 환경·운영 결정 추가. Spec v1.3.4로 경로 등록. 공통 계약 변경 없음 | `ae31eb2` |
| 2026-10-24 12:29 | `claude-opus-5.5/session-c9336d2c` | 사용자 결정: WSL 배포판을 Ubuntu 24.04에서 26.04(`Ubuntu-26.04`)로 변경, Host ID `wsl-ubuntu-2604`. 배포판 미설치 상태 유지 | `ae31eb2` |
| 2026-10-24 12:32 | `claude-opus-5.5/session-c9336d2c` | 사용자 결정: WSL 구성 시점을 PH0-HOST 공용 런타임 구현 착수 시(늦어도 G2 전)로 지정. G0 계약은 다른 에이전트가 검토 중이며 결과는 §4 변경 요청으로 받음 | `ae31eb2` |
| 2026-10-24 (시각 미기록, 제출자 기록) | `Codex / G0-REVIEW-20261024` | 사용자 요청으로 G0 독립 검토의 REQ-002~009 등록안·상세 요청서 작성. 별도 검토 Branch에서 제출하며 대표 저장소 반영·채택·구현·기준 변경은 기존 통합 담당이 처리 | `ae31eb2` (검토 대상), `e56eb2d` (제출안 기반) |
| 2026-10-24 12:59 | `claude-opus-5.5/session-c9336d2c` | 검토 제출안을 `952bb26`으로 접수. REQ-002~006·009 채택, REQ-007·008 수정 채택([G0 개정 1](evidence/PH0-INTEGRATION/G0_REVISION_1.md)). 처리 순서: INTEGRATION 런타임 인터페이스 → CONTRACT → 재통합 | `ae31eb2` |
| 2026-10-24 13:02 | `claude-opus-5.5/session-c9336d2c` | G0 개정 1 INTEGRATION 부분 완료(`7d296a8`, REQ-002·003 인터페이스). PH0-CONTRACT `verified → in-progress`, Worktree `D:\worktree\TelosWeaver\ph0-contract` 재생성. 개정 기준 확정 전까지 `ae31eb2`가 유효 기준 | `7d296a8` |
| 2026-10-24 13:11 | `claude-opus-5.5/session-c9336d2c` | `ph0/contract`(`98177db`)를 `ba97e7b`로 통합, `main`에서 재검증(27개 통과). 계약 기준 `ae31eb2 → ba97e7b`, PH0-CONTRACT `in-progress → verified`, REQ-004~009 완료, REQ-002·003 계약 완료(HOST 구현 대기). Worktree 정리 | `ba97e7b` |
| 2026-10-24 13:21 | `claude-opus-5.5/session-c9336d2c` | 사용자 결정 DEC-06 반영(Spec v1.3.5): Provider CLI를 VS Code 확장에서 탐지. HOST에 공통 탐색 경로 추가, 세 Provider 인계에 확장 실행 파일 기록, WSL 절차 §2.9를 원격 확장 기준으로 수정. 공통 계약 코드 변경 없음 | `ba97e7b` |
| 2026-10-24 13:38 | `claude-opus-5.5/session-c9336d2c` | Spec v1.3.6: 확장 탐지를 버전 비의존 규칙으로 변경, Antigravity 실행 파일을 `~/.gemini/bin/agy`로 확정(사용자의 중복 설치 정리 후 재확인). AGY 인계·WSL 절차 갱신 | `ba97e7b` |
| 2026-10-24 13:42 | `claude-opus-5.5/session-c9336d2c` | Spec v1.3.7: Antigravity 데이터 디렉터리 검증(AGY-DATA-01~05)을 §7.5에 추가하고 PH0-AGY 첫 실험으로 인계. PH0-AGY는 다른 에이전트가 맡을 예정(사용자 지정, 아직 미배정) | `ba97e7b` |
| 2026-10-24 (시각 미기록) | `Codex / PH0-HOST-20261024` | 사용자 직접 배정으로 PH0-HOST `unassigned → reserved` 기록. Branch/Worktree와 준비·범위 기록, REQ-010·011 제출. 코드 구현·공통 계약 변경·WSL 구성은 아직 시작하지 않음. 기존 통합 담당과 G0 기준은 유지 | `23605b0` (준비 기반), `ba97e7b` (계약) |
| 2026-10-24 (시각 미기록) | `Codex / PH0-HOST-20261024` | 사용자 착수 지시로 PH0-HOST `reserved → in-progress`. 공용 런타임과 탐색 엔진을 소유 경로에 구현하며 REQ-010·011 공통 변경은 별도 적용안으로 준비. 공식 Workspace 연결 전 독립 Manifest로 검증하며 G0 기준·타 담당 소유권 유지 | `1e415ba` (착수 기반), `ba97e7b` (계약) |
| 2026-10-24 16:48 | `claude-opus-5.5/session-c9336d2c` | `ph0/host`(`6bb943e`)와 REQ-011을 한 통합 단위로 `d8e8725`에 반영. `libc` 0.2.190 고정(2주 기준), Core `serve` 진입점(기본 `DenyRuns`). 공식 Workspace test 57개·clippy·fmt 통과. PH0-HOST는 REQ-010 연결·Linux 검증이 남아 `in-progress` 유지. REQ-010 채택, PH0-CONTRACT `verified → in-progress` | `d8e8725` |
| 2026-10-24 17:10 | `claude-opus-5.5/session-c9336d2c` | `ph0/contract`(`4e29f20`)를 `271e7fd`로 통합, `main`에서 재검증(61개 통과, clippy·fmt). 계약 기준 `ba97e7b → 271e7fd`, REQ-010 계약 완료(HOST 엔진 연결 대기), PH0-CONTRACT `in-progress → verified`, Worktree 정리. G1은 PH0-CODEX 배정과 신뢰된 `RunAuthorizer` 연결(MASTER·INTEGRATION)이 필요하고, Linux 검증은 WSL 배포판 설치(사용자 수행)가 필요 | `271e7fd` |
| 2026-10-24 17:20 | `claude-opus-5.5/session-c9336d2c` | 사용자 결정(Spec v1.3.8): 담당 에이전트가 자기 작업 행·변경 요청 제출·변경 기록을 직접 갱신. 기준·게이트·판정·`verified` 전환은 통합 담당 유지. §1 규칙 갱신. 이전 HOST의 직접 기록(`1e415ba`, `9522d79`)은 이 규칙에 부합 | `271e7fd` |
| 2026-10-24 17:30 | `claude-opus-5.5/session-c9336d2c` | 사용자 지시로 Spec을 v1.4로 승격: `docs/DEVELOPMENT_v1.3.md` → `docs/DEVELOPMENT_v1.4.md`, 개정 1.3.8 원문을 `docs/archive/DEVELOPMENT_v1.3.md`로 보관. 저장소 안 링크 갱신(PH0-HOST·G0 검토 증거는 링크 경로만 수정). 계약 변경 없음 | `271e7fd` |
| 2026-10-24 (시각 미기록) | `Codex / PH0-HOST-20261024` | 사용자 지시로 후속 작업 1~3 착수: 최신 main 반영, REQ-010 엔진 연결, 공개 API 시험 전환. HOST 기준을 Spec v1.4·계약 `271e7fd`로 갱신. WSL Linux·Remote SSH 실행 검증은 사용자 지시로 대기하며 게이트·다른 작업 상태는 변경하지 않음 | `ce08a1f` |
| 2026-10-24 17:27 | `claude-opus-5.5/session-c9336d2c` | 사용자 배정으로 PH0-CLAUDE `unassigned → in-progress`(Worktree `D:\worktree\TelosWeaver\ph0-claude`, Branch `ph0/claude`). 사용자 결정: G1 대표 Provider를 Codex에서 Claude로 변경(이유: PH0-CODEX 미배정, 통합 담당이 CLAUDE를 함께 맡아 G1을 바로 판정 가능). §6에 Claude 실환경 시험 조건 추가. 배정표 표 중간의 빈 줄 제거 | `2ac1497` |
| 2026-10-24 17:26 | `gemini-4-argon/session-9e6137eb` | 사용자 직접 배정으로 PH0-AGY `unassigned → reserved` 기록. Branch `ph0/agy`·Worktree `D:\worktree\TelosWeaver\ph0-agy` 예정, 기준 Commit `2ac1497`·계약 기준 `271e7fd` 기록. 코드 구현 및 AGY-DATA-01~05 실험 착수 전 사전 결정 사항 확인 대기 | `2ac1497` (준비 기반), `271e7fd` (계약) |
| 2026-10-24 17:33 | `gemini-4-argon/session-9e6137eb` | 사용자 착수 지시로 PH0-AGY `reserved → in-progress`(Worktree `D:\worktree\TelosWeaver\ph0-agy`, Branch `ph0/agy`). 사용자 결정: 확장 활성 상태에서 AGY-DATA-01~05 바로 검증, 경량 Flash 모델·`--effort low`·일회용 임시 폴더 사용, 테스트 세션은 실험 후 정리(기존 대화 보존) | `0b5b34f` (착수 기반), `271e7fd` (계약) |
| 2026-10-24 (시각 미기록) | `Codex / PH0-HOST-20261024` | 요청 1~3 구현·검증 완료 제출. 코드 `e844791`, 검증·인계 `77a925f`. 공개 CliDiscoverer 연결·DTO/실패 분류·기록 필드 연결, 공개 API 탐색 시험 17개·공식 Workspace 71개·clippy·fmt 통과. HOST 전체는 후속 통합·Linux 검증이 남아 in-progress 유지. 4번은 사용자 지시로 대기하며 다른 담당의 변경과 게이트 판정은 유지 | `2ac1497` (구현 기준), `271e7fd` (계약) |
| 2026-10-24 17:42 | `claude-opus-5.5/session-c9336d2c` | `ph0/claude`(`2142e05`)를 `41139e0`, `ph0/host`(`77a925f`)를 `b0a58b4`, 탐지 실환경 시험(`18fcf77`)을 `10abcc1`로 통합. `main`에서 85개 통과(실환경 6개 무시), clippy·fmt. REQ-010 완료, PH0-CLAUDE `in-progress → integrated`. G1은 Core 연결 후 판정 | `10abcc1` |
| 2026-10-24 17:54 | `claude-opus-5.5/session-c9336d2c` | G1 Core 연결 착수(INTEGRATION). Worktree `D:\worktree\TelosWeaver\ph0-integration`, Branch `ph0/integration-g1`. 범위: 운영자 설정 기반 `RunAuthorizer`(Phase 0, 정식 정책은 MASTER), `ClaudeLaunchResolver` Core 구현, 탐지 결과로 Claude Adapter 구성, Daemon 종료 시 활성 Run 취소·종료 확인, Local API 경유 G1 실환경 시험. Spec v1.4.1로 `crates/core/src/integration/**` 등록 | `63b6758` |
| 2026-10-24 (시각 미기록) | `Codex / PH0-CODEX-20261024` | 사용자 직접 배정으로 PH0-CODEX `unassigned → reserved`. 전용 Branch/Worktree와 기준 기록. 현재 설치된 Codex CLI의 버전·App Server 도움말만 확인했으며 코드 구현·실계정 모델 호출은 시작하지 않음. 실환경 시험 조건과 준비 사항을 정리하고 G1 대표·HOST·게이트 판정은 유지 | `75ba56e` (준비 기준), `271e7fd` (계약) |
| 2026-10-24 18:03 | `claude-opus-5.5/session-c9336d2c` | G1 Core 연결 `ph0/integration-g1`(`a810700`)을 `d3130c1`로 통합. `main`에서 91개 통과(실환경 11개 무시), clippy·fmt. G1 실환경 5개(haiku) 통과로 G1 `대기 → 통과 판정(HOST 확인 대기)`. HOST 행에 확인 요청 기록 | `d3130c1` |
| 2026-10-24 (시각 미기록) | `Codex / PH0-CODEX-20261024` | 사용자 권장안 승인으로 PH0-CODEX reserved → in-progress. 실환경 범위·경량 모델/low·시험 세션 Archive 조건 확정. REQ-012 모듈 공개 요청을 제출하고 Codex 전용 구현·검증 착수. 기존 HOST·게이트 판정과 Linux 대기 지시는 유지 | `cc53fbf`, 계약 `271e7fd` |
| 2026-10-24 18:16 | `gemini-4-argon/session-9e6137eb` | PH0-AGY 구현·검증 완료로 `in-progress → review` 전환(`ph0/agy` 커밋 `057e332`). AGY-DATA-01~05 및 PROV-01~09·FORK-POC-01 실환경 검증 완료(Mock 14개·실환경 6개 통과, 시험 세션 정리 완료). REQ-013(`pub mod agy;` 및 Spec §7.5 확정 규칙 반영) 제출 | `63b6758`, 계약 `271e7fd` |
| 2026-10-24 18:25 | `claude-opus-5.5/session-c9336d2c` | `ph0/agy`(`057e332`) 검토·통합(`b0f5b5e`): 소유 경로만 변경, 권한 우회 차단·stderr 비복사·소프트 거부 실패 판정 확인. REQ-013 ① `pub mod agy`, ② Spec v1.4.2 §7.5 반영, ③ HOST 전달. `main` 106개 통과(실환경 17개 무시), clippy·fmt, 병합본 AGY 탐지 재확인. PH0-AGY `review → integrated` | `b0f5b5e` |
| 2026-10-24 18:28 | `claude-opus-5.5/session-c9336d2c` | 사용자 결정(Spec v1.4.3): 병합 직후 Branch·Worktree 삭제를 기본 동작으로 지정. 병합된 `ph0/agy`·`ph0/claude`·`ph0/host`·`ph0/integration-g1`·`ph0/contract`·`review/g0-contract-requests-20261024` Branch와 Worktree 삭제(AGY Worktree의 미커밋 변경은 파일 끝 빈 줄뿐이라 폐기). HOST·AGY가 후속 작업을 하면 최신 `main`에서 새로 생성. `ph0/codex`는 미병합으로 유지 | `d8441b4` |
| 2026-10-24 (시각 미기록) | `Codex / PH0-CODEX-20261024` | 사용자 삭제 후 남아 있던 ph0/codex(`516a13d`)를 원래 Worktree에 복구. 구현 `2c6339b` 보존 확인, 인계·공통 변경 적용안 `cb87f80` 제출로 PH0-CODEX in-progress → review. Mock 12개·실환경 7개 통과와 시험 세션 Archive 완료 기록. 복구 후 Workspace 118개 통과·fmt 통과, 엄격 Clippy는 공통 미사용 Stub 매크로 2건으로 실패하여 REQ-014 제출. REQ-012 공개·Core 연결·통합 판정은 기존 담당 유지 | `cb87f80`, 계약 `271e7fd` |
| 2026-10-24 19:01 | `claude-opus-5.5/session-c9336d2c` | `ph0/codex`(`d798988`) 검토·통합(`c00d2b0`): 소유 경로만 변경, Sandbox read-only/workspace-write·네트워크 차단·기본 승인 거부·소유 Thread만 Archive 확인. REQ-012·REQ-014 적용(`COMMON_CHANGES.patch`). `main` 118개 통과(실환경 24개 무시), clippy `-D warnings`·fmt, 병합본 Codex 탐지 재확인. PH0-CODEX `review → integrated`. `ph0/codex` Branch·Worktree 삭제 | `c00d2b0` |
| 2026-10-24 (시각 미기록) | `Codex / PH0-CODEX-20261024` | 사용자 현황 최신화 요청으로 main `15d36cb`의 통합 상태와 실제 코드를 재확인. Workspace 118개·엄격 Clippy·fmt 재통과. PH0-CODEX integrated·REQ-012/014 완료 판정은 유지하고, 자기 검증·인계 문서의 통합 전 현재 상태 표기를 갱신할 전용 Branch/Worktree를 새로 생성. 타 작업·게이트 판정은 변경하지 않음 | `15d36cb` |
| 2026-10-24 (시각 미기록) | `Codex / PH0-CODEX-20261024` | 자기 증거 README·HANDOFF를 통합 완료 상태로 갱신한 `3e92d4c` 제출. 통합 전 Clippy 실패·패치 제출은 이력으로 보존하고 현재 엄격 검사 통과를 구분. PH0-CODEX 구현 integrated 상태와 타 작업·게이트 판정 유지. 문서 갱신 Branch/Worktree는 통합 담당 반영 전까지 보존 | `3e92d4c` |

PH0-INTEGRATION은 배정표 관리 자체가 작업이므로 G3까지 `in-progress`로 유지합니다. PH0-CONTRACT는 같은 담당자가 별도 Worktree의 `ph0/contract`에서 진행했고, 통합 후 Worktree를 정리했습니다. 이후 계약 변경 요청이 오면 새 Worktree를 만들어 같은 규칙으로 처리합니다.

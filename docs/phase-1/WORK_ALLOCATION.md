# Phase 1 작업 배정표

> **프로젝트:** TelosWeaver  
> **갱신일:** 2026-11-07  
> **범위 원본:** [Spec §19.6](../DEVELOPMENT_v1.5.md#196-phase-1-병렬-구현--작업-분할과-소유권), 절차는 §19.5를 PH1로 읽어 적용  
> **현재 상태:** Phase 0 완료(G3 통과 확정, [Phase 0 배정표](../phase-0/WORK_ALLOCATION.md)). Phase 1의 **H0(공통 계약) 준비 중**. 다른 작업도 H0 계약 검토를 위해 먼저 배정할 수 있지만, 기능 구현은 H0 확정 뒤 시작합니다.  
> **확정:** 2026-11-07 사용자 확정, `c696cb4`로 `master`에 병합(Spec v1.5.10). 이 파일이 운영 원본입니다.

## 1. 단일 원본과 갱신 주체

이 파일이 Phase 1 배정·게이트·변경 요청의 단일 원본입니다. 수정 범위와 갱신 규칙은 Phase 0 배정표 §1과 같습니다(담당은 자기 행·자기 변경 요청·§7 기록, 통합 담당은 관리 항목·게이트·요청 판정·다른 행·`integrated`/`verified` 전환. 운영 배정표 수정은 대표 저장소 `master`에서 단독 커밋, §7 표 행 사이 빈 줄 금지).

| 관리 항목 | 현재 값 |
|---|---|
| 대표 저장소 | `D:\Downloads\Vibe_dev\TelosWeaver`, Branch `master`. 공개 원격 `origin` = https://github.com/blazepsyki/TelosWeaver (`master`만 push) |
| 통합 담당자 ID | `claude-opus-5.5/session-c9336d2c` (Phase 0에서 유지, 2026-10-26 사용자 결정) |
| 공통 계약 기준 Commit | H0 전: Phase 0 최종 계약 `6a4f58a`. H0 확정 시 갱신 |
| Phase 0 이월 | [G3 대조표](../phase-0/evidence/PH0-INTEGRATION/G3_ACCEPTANCE.md) §3 P1-1~6, [지원 백로그](../SUPPORT_BACKLOG.md). 사용자 결정: 관련 작업에 편입 |

## 2. 배정 상태

| 작업 ID | 담당자 ID | 상태 | Branch / Worktree | 기준 Commit | 결과 Commit | 인계·요청 |
|---|---|---|---|---|---|---|
| PH1-INTEGRATION | `claude-opus-5.5/session-c9336d2c` | in-progress | `master` / 대표 저장소 작업 트리 | `c696cb4` | 계획 `56f6091`, 리뷰 반영 `9626249`(Codex)·`274ea35`(Antigravity), 정정 `b24120d`, 병합 `c696cb4` | Spec v1.5.10 §19.6·이 배정표 확정(2026-11-07 사용자 확정). 다음: PH1-CONTRACT 배정과 H0 계약 확정 |
| PH1-CONTRACT | `claude-opus-5.5/session-c9336d2c` | in-progress | `ph1/contract` / `D:\worktree\TelosWeaver\ph1-contract` | `b7655c1` | — | 사용자 지정(2026-11-07, INTEGRATION 겸임). H0 결정 사항 [H0_DECISIONS.md](H0_DECISIONS.md) D1~D15 확정(2026-11-10, `38cccec`). 겸임 보완: 각 계약은 사용 작업 담당(미배정이면 검토 대행자)의 확인을 받음(Spec §19.6.2) |
| PH1-PERSIST | — | unassigned | — | — | — | H0 뒤 |
| PH1-SCHED | — | unassigned | — | — | — | H0 뒤. 단일 Scheduler 소유권, 대기 자원·취소 전파, 재시작 대조·안전 보류 |
| PH1-CONTEXT | — | unassigned | — | — | — | H0 뒤. 지원 백로그 SB-11(Capsule) |
| PH1-WORKSPACE | — | unassigned | — | — | — | H0 뒤. Writer·기존 변경 보존, 고정 Revision의 승인된 Git 반영 |
| PH1-ARTIFACT | — | unassigned | — | — | — | H0 뒤 |
| PH1-VALIDATION | — | unassigned | — | — | — | H0 뒤. SCHED·ARTIFACT 공개 인터페이스 통합 후 연결 |
| PH1-MCP | — | unassigned | — | — | — | H0 뒤. SCHED 공개 인터페이스 통합 후 연결 |
| PH1-MASTER | — | unassigned | — | — | — | H0 뒤. 정식 정책(SB-11 Capsule 허용 포함), 이월 P1-2의 Core 판단·제어 연결, VALIDATION 승인 기록·HOST API·Provider·EXT와 연계 |
| PH1-HOST | — | unassigned | — | — | — | H0 뒤. PERSIST·SCHED 통합 후 연결. Task MCP 주입·승인/제어 라우팅, 이월 P1-5(Windows 정상 종료), SB-12·13·18 |
| PH1-CODEX | — | unassigned | — | — | — | H0 뒤. 이월 P1-1 적용, SB-08·09 검토. SB-05 특정 Turn Resume는 제품 P1 범위의 백로그로 유지 |
| PH1-CLAUDE | — | unassigned | — | — | — | H0 뒤. 이월 P1-1(SB-10)·P1-2(실시간 승인, SB-07)·P1-3(SB-06) 검토 |
| PH1-AGY | — | unassigned | — | — | — | H0 뒤. 이월 P1-1(SB-10)·P1-3(SB-06) 검토 |
| PH1-CONFIG | — | unassigned | — | — | — | H0 뒤 |
| PH1-RN | — | unassigned | — | — | — | H0 뒤. 설치 설정 소유권·사용자 후속 변경 보존, P0 RPC·TTL·중복·Degraded |
| PH1-EXT | — | unassigned | — | — | — | H0 뒤. HOST API 확장 통합 후 연결 |

총 17개 작업입니다. 상태는 `unassigned → reserved → in-progress → review → integrated → verified`(Spec §19.5.5). H0 전 배정은 계약 검토·초안 제출 범위이며 기능 구현 허가가 아닙니다.

## 3. 통합 게이트

| 게이트 | 상태 | 근거 |
|---|---|---|
| H0 Phase 1 공통 계약 | 준비 중 | Spec §19.6.2. 아래 §5의 10종 계약·Core 공개 인터페이스·확인자·Contract 시험·기준 Commit 확정 전 통과 불가 |
| H1 수직 경로 | 대기 | Spec §19.6.4 3번. 실제 Provider 경로, 재시작 후 확정 결과 보존과 미완 실행 안전 보류를 각각 확인 |
| H2 구성 통합 | 대기 | — |
| H3 Phase 1 수용 | 대기 | Spec §19.2, §18 P0 및 혼합 항목 P0 부분, E2E-01·04·05·06·08·09·13과 E2E-02·07·14의 P0 부분, 별도 코딩·실패 시나리오. 필수 환경 미검증 시 보류 |

## 4. 계약·공통 파일 변경 요청

| 요청 ID | 요청 작업 | 변경 대상·이유 | 영향 작업 | 처리 주체 | 상태·결과 Commit |
|---|---|---|---|---|---|

## 5. H0 계약 범위와 등록 경로

다음은 H0의 확인 목록이며 아직 확정된 계약이 아닙니다. 각 행에 실제 정의 경로·제공/사용 작업·확인자·Contract 시험·기준 Commit을 채워야 합니다. 공유 타입은 CONTRACT, Core 서비스 인터페이스는 해당 모듈 담당, 등록·Stub 연결은 INTEGRATION이 반영합니다. 미배정 작업은 통합 담당이 검토 대행자와 근거를 기록합니다.

| 계약 | 확인할 경계 | 정의 경로·확인자·시험·기준 Commit |
|---|---|---|
| Task·Run·Session | 상태·Revision·의존성·자식 한도·대기·취소 전파·재시도 | 미확정 |
| 영속 기록 | 상태·이벤트·실행 의도의 동일 Transaction, 멱등성·Epoch·재시작 대조·커서 | 미확정 |
| Task MCP | 여섯 도구의 입출력·오류·연결 자격·Project/Run Scope, Run 시작 시 `RunContext` 주입 경계 | 미확정 |
| 산출물·검증 | 불변 Version·참조 수명, Validator 상태, 고정 대상 승인·만료·소비 | 미확정 |
| Workspace | Writer Lock·살아 있는 소유자, 기존 변경, Diff·승인된 Revision 반영 | 미확정 |
| 문맥 | Fresh/Capsule(SB-11 정책 허용·Adapter 전달 포함), 필수 입력·신뢰 경계, 사용량·Unknown·집계 | 미확정 |
| 마스터 결정·정책 | 8종 kind, 권한 교집합·예산, Provider 승인·사용자 판단 | 미확정 |
| Local Core API | Task·Master·Artifact·승인·개입·P0 RN, Snapshot·커서·호환성 | 미확정 |
| 설정 | 계층·출처·해시, Host 귀속, RN·Task MCP 설정 소유권·사용자 변경 보존 | 미확정 |
| Capability 판정 | C5 공통 기준, 환경·버전별 증거, 미지원 거부·명시적 대체 | 미확정 |

Core 연결은 SCHED↔PERSIST, SCHED↔HOST, SCHED↔WORKSPACE/CONTEXT/ARTIFACT/VALIDATION, MASTER/MCP/API↔SCHED, MCP↔HOST/세 Provider(Run별 자격 주입), HOST↔MASTER/VALIDATION(실시간 승인·제어 라우팅)의 공개 인터페이스·오류·취소 경계를 포함합니다(Spec §19.6.2). 통합 담당은 §18 전체 P0 요구사항과 §20.3 P0 시나리오를 담당·필수 환경·시험·증거에 연결한 수용 대조표를 H0에서 등록합니다. H3 때 실행 결과를 채우며, 목록 작성만으로 시험을 통과한 것으로 보지 않습니다.

이월 P1-1~6의 담당과 완료 경계는 Spec §19.6.6을 따릅니다. P1-4는 CODEX(SB-08·09), HOST(SB-13·18), 세 Provider·EXT·INTEGRATION(SB-16·17)이 나누어 검토합니다. P1-6(CI)은 INTEGRATION이 현재 보류 결정을 유지하고 변경 결정만 기록합니다.

## 6. 환경·운영 결정

Phase 0 배정표 §6의 결정(줄바꿈, Linux·Remote SSH Host `psyki-nas`, CI 보류, 대상 OS 우선순위, 실환경 시험 조건, 시험 세션 정리, RN 범위 등)을 그대로 이어받습니다. Phase 1에서 바뀌는 결정만 아래에 기록합니다.

| 항목 | 결정 | 상태 | 결정일 |
|---|---|---|---|
| Phase 1 진행 방식 | §19.6 병렬 절차, 계약 게이트(H0) 먼저, 통합 담당 유지, 이월 과제는 관련 작업에 편입 | 적용 | 2026-10-26 |

Phase 0 환경의 설치 버전·로그인 상태가 현재도 같다고 가정하지 않습니다. 실제 시험 시 Host·탐지 경로·CLI/확장/RN 버전과 인증 조건을 다시 기록합니다. Linux SSH 명령 실행과 VS Code Remote SSH UI 증거는 구분합니다. 사용자 전역 MCP/Hook 설정 변경은 기존 결정에 따른 명시적 실행·확인 범위를 지킵니다.

## 7. 변경 기록

| 일시 (KST) | 기록자 | 변경 | 기준 Commit |
|---|---|---|---|
| 2026-10-26 18:20 | `claude-opus-5.5/session-c9336d2c` | Phase 1 착수 준비: 사용자 결정(병렬 절차·H0 먼저·통합 담당 유지·이월 과제 편입)에 따라 Spec v1.5.9 §19.6과 이 배정표 작성. PH1-INTEGRATION `in-progress`, 나머지 `unassigned` | Phase 0 최종 `master` |
| 2026-11-07 (시각 미기록) | `Codex / PH1-PLAN-REVIEW` | 사용자 요청으로 계획 초안 리뷰·수정. H0 Core 인터페이스·확인 목록, 17개 작업과 이월 책임, H3 E2E-13·14·코딩·실패 경로, 초안/운영 원본 구분 보완. 담당 배정·지원 판정·게이트 변경과 기능 구현·실환경 시험 없음 | 초안 `441a89e`, 비교 `master` `4bb6ed4` |
| 2026-11-07 17:44 | `gemini-4-argon/session-c2e931ec` | 사용자 요청으로 Codex 리뷰 수정본(`06ea45a`) 추가 리뷰·보완: H0 Task MCP Run별(`RunContext`) 자격 주입 경계·Capsule(SB-11) 정책 허용(`PH1-MASTER`) 및 전달 경계·HOST↔MASTER/VALIDATION 실시간 승인/제어 라우팅 명시, §19.6.3 작업별 P0 요구사항(`SESS`·`DELEG-06/09`·`COMM-10`·`VERIFY-06`·`SAFE-11/12/14`) 및 `docs/phase-0/WORK_ALLOCATION.md` 보존 경로 보완, §19.6.6 지원 백로그(`SB-01~19`) 매핑 정합화 | `06ea45a` |
| 2026-11-07 18:09 | `claude-opus-5.5/session-c9336d2c` | 두 리뷰 검토: 작업 17개·E2E-13·14 보완, 책임 경계·H0 계약 보강, 요구사항 ID 25개 실재 확인, §18 P0 전 항목의 작업 배정 확인. 정정: SB-19(RN 관찰 한계)를 UI 범위로 인용한 3곳 수정(`b24120d`). 사용자 결정: 이번 리뷰는 직접 배정이라 REQ1 제출 없이 수용. 최신 `master` 위로 Rebase(이전 Hash `441a89e`→`56f6091`, `06ea45a`→`9626249`, `ea209cd`→`274ea35`) 뒤 **사용자 확정으로 병합 `c696cb4`**. 운영 원본으로 전환 | `c696cb4` |
| 2026-11-07 18:15 | `claude-opus-5.5/session-c9336d2c` | 사용자 지정: PH1-CONTRACT를 통합 담당이 겸임(`unassigned → in-progress`). H0 결정 사항 D1~D13·사전 작업 A1~A3 정리(`ff29e3f`). 발견: Claude Code 2.1.318·Antigravity 1.5.2로 갱신돼 검증 버전 밖(A1) | `ff29e3f` |
| 2026-11-10 20:46 | `claude-opus-5.5/session-c9336d2c` | A1(CLI 버전 변경 재시험: Claude 2.1.318 9개·Antigravity 1.5.3 8개 통과)·A2(Antigravity Workspace 플러그인 + 세션 한정 권한으로 MCP 주입 성공) 기록(`2088bb5`). 사용자 결정: H0 결정 D15는 (b) 요약 원본 조사, 나머지 14개 권장안(`38cccec`) | `38cccec` |
| 2026-11-10 20:52 | `claude-opus-5.5/session-c9336d2c` | 사용자 지시: H0 D15(b) 조사 완료. Antigravity 요약 DB는 `jetbox_summaries_proto.pb`에서 다시 만들어짐. 원본·DB·`last_conversations.json`의 시험 항목 52개를 백업 후 함께 삭제, CLI 시작·실제 세션 뒤에도 유지 확인. 정리 도구를 증거로 등록(`4a24f65`). 후속: PH1-AGY 시험 정리 함수에 반영 | `4a24f65` |
| 2026-11-10 20:55 | `claude-opus-5.5/session-c9336d2c` | 사용자 지시로 H0 계약 작업 착수. Branch `ph1/contract`, Worktree `D:\worktree\TelosWeaver\ph1-contract`, 기준 `b7655c1`. 첫 커밋 Spec v1.5.11(H0 결정 D1~D14 반영, `86d7c8b`) | `b7655c1` |

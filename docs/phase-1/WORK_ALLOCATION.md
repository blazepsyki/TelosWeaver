# Phase 1 작업 배정표

> **프로젝트:** TelosWeaver  
> **갱신일:** 2026-10-26  
> **범위 원본:** [Spec §19.6](../DEVELOPMENT_v1.5.md#196-phase-1-병렬-구현--작업-분할과-소유권), 절차는 §19.5를 PH1로 읽어 적용  
> **현재 상태:** Phase 0 완료(G3 통과 확정, [Phase 0 배정표](../phase-0/WORK_ALLOCATION.md)). Phase 1 착수, **H0(공통 계약) 준비 중**. 다른 작업은 H0 확정 뒤 배정 요청을 받아 착수합니다.

## 1. 단일 원본과 갱신 주체

이 파일이 Phase 1 배정·게이트·변경 요청의 단일 원본입니다. 수정 범위와 갱신 규칙은 Phase 0 배정표 §1과 같습니다(담당은 자기 행·자기 변경 요청·§7 기록, 통합 담당은 관리 항목·게이트·요청 판정·다른 행·`integrated`/`verified` 전환. 배정표 수정은 대표 저장소 `master`에서 단독 커밋, §7 표 행 사이 빈 줄 금지).

| 관리 항목 | 현재 값 |
|---|---|
| 대표 저장소 | `D:\Downloads\Vibe_dev\TelosWeaver`, Branch `master`. 공개 원격 `origin` = https://github.com/blazepsyki/TelosWeaver (`master`만 push) |
| 통합 담당자 ID | `claude-opus-5.5/session-c9336d2c` (Phase 0에서 유지, 2026-10-26 사용자 결정) |
| 공통 계약 기준 Commit | H0 전: Phase 0 최종 계약 `6a4f58a`. H0 확정 시 갱신 |
| Phase 0 이월 | [G3 대조표](../phase-0/evidence/PH0-INTEGRATION/G3_ACCEPTANCE.md) §3 P1-1~6, [지원 백로그](../SUPPORT_BACKLOG.md). 사용자 결정: 관련 작업에 편입 |

## 2. 배정 상태

| 작업 ID | 담당자 ID | 상태 | Branch / Worktree | 기준 Commit | 결과 Commit | 인계·요청 |
|---|---|---|---|---|---|---|
| PH1-INTEGRATION | `claude-opus-5.5/session-c9336d2c` | in-progress | `master` / 대표 저장소 작업 트리 | Phase 0 최종 `master` | — | Spec v1.5.9 §19.6·이 배정표 작성. 다음: PH1-CONTRACT와 H0 계약 확정 |
| PH1-CONTRACT | — | unassigned | — | — | — | H0 계약(§19.6.2)을 INTEGRATION과 함께 확정. 이월 P1-1(C5 Capability 판정 기준) 포함 |
| PH1-PERSIST | — | unassigned | — | — | — | H0 뒤 |
| PH1-SCHED | — | unassigned | — | — | — | H0 뒤 |
| PH1-CONTEXT | — | unassigned | — | — | — | H0 뒤. 지원 백로그 SB-11(Capsule) |
| PH1-WORKSPACE | — | unassigned | — | — | — | H0 뒤 |
| PH1-ARTIFACT | — | unassigned | — | — | — | H0 뒤 |
| PH1-VALIDATION | — | unassigned | — | — | — | H0 뒤. SCHED·ARTIFACT 공개 인터페이스 통합 후 연결 |
| PH1-MCP | — | unassigned | — | — | — | H0 뒤. SCHED 공개 인터페이스 통합 후 연결 |
| PH1-MASTER | — | unassigned | — | — | — | H0 뒤. 이월 P1-2(실시간 승인 정책의 Core 측) |
| PH1-HOST | — | unassigned | — | — | — | H0 뒤. PERSIST·SCHED 통합 후 연결. 이월 P1-5(Windows 정상 종료), SB-12·13 |
| PH1-CODEX | — | unassigned | — | — | — | H0 뒤. 이월 P1-1 적용, SB-05·08·09 |
| PH1-CLAUDE | — | unassigned | — | — | — | H0 뒤. 이월 P1-1·P1-2(실시간 승인, SB-07)·P1-3 검토 |
| PH1-AGY | — | unassigned | — | — | — | H0 뒤. 이월 P1-1·P1-3 검토 |
| PH1-CONFIG | — | unassigned | — | — | — | H0 뒤 |
| PH1-RN | — | unassigned | — | — | — | H0 뒤 |
| PH1-EXT | — | unassigned | — | — | — | H0 뒤. HOST API 확장 통합 후 연결 |

상태는 `unassigned → reserved → in-progress → review → integrated → verified`(Spec §19.5.5).

## 3. 통합 게이트

| 게이트 | 상태 | 근거 |
|---|---|---|
| H0 Phase 1 공통 계약 | 준비 중 | Spec §19.6.2 |
| H1 수직 경로 | 대기 | Spec §19.6.4 3번 |
| H2 구성 통합 | 대기 | — |
| H3 Phase 1 수용 | 대기 | Spec §19.2 통과 조건, §20.3 P0 E2E |

## 4. 계약·공통 파일 변경 요청

| 요청 ID | 요청 작업 | 변경 대상·이유 | 영향 작업 | 처리 주체 | 상태·결과 Commit |
|---|---|---|---|---|---|

## 5. H0 계약 범위와 등록 경로

H0 확정 때 계약별 정의 위치·기준 Commit을 기록합니다(Spec §19.6.2의 10종).

## 6. 환경·운영 결정

Phase 0 배정표 §6의 결정(줄바꿈, Linux·Remote SSH Host `psyki-nas`, CI 보류, 대상 OS 우선순위, 실환경 시험 조건, 시험 세션 정리, RN 범위 등)을 그대로 이어받습니다. Phase 1에서 바뀌는 결정만 아래에 기록합니다.

| 항목 | 결정 | 상태 | 결정일 |
|---|---|---|---|
| Phase 1 진행 방식 | §19.6 병렬 절차, 계약 게이트(H0) 먼저, 통합 담당 유지, 이월 과제는 관련 작업에 편입 | 적용 | 2026-10-26 |

## 7. 변경 기록

| 일시 (KST) | 기록자 | 변경 | 기준 Commit |
|---|---|---|---|
| 2026-10-26 18:20 | `claude-opus-5.5/session-c9336d2c` | Phase 1 착수 준비: 사용자 결정(병렬 절차·H0 먼저·통합 담당 유지·이월 과제 편입)에 따라 Spec v1.5.9 §19.6과 이 배정표 작성. PH1-INTEGRATION `in-progress`, 나머지 `unassigned` | Phase 0 최종 `master` |

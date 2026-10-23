# Phase 0 작업 배정표

> **프로젝트:** TelosWeaver  
> **갱신일:** 2026-10-24  
> **범위 원본:** [Spec §19.5](../DEVELOPMENT_v1.3.md#195-phase-0-병렬-구현--작업-분할과-소유권)  
> **현재 상태:** 사용자가 PH0-INTEGRATION·PH0-CONTRACT를 배정함. G0 준비 중이며 계약 동결·통합 게이트 통과는 아직 없습니다.

## 1. 단일 원본과 갱신 주체

대표 저장소의 이 파일을 단일 원본으로 사용합니다. 대표 저장소의 위치는 최초 통합 담당이 착수 시 기록합니다. 다른 Worktree·Clone의 복사본은 참고용이며 그것만 바꿔 소유권을 확보하지 않습니다.

통합 담당 한 명만 이 표를 직렬 갱신합니다. 작업 에이전트는 Spec의 작업 ID를 선택해 배정을 요청하고, 담당·Branch/Worktree·기준 Commit이 확정된 뒤 해당 경로를 수정합니다. 통합 담당이 정해지지 않았다면 최초 착수를 총괄하도록 지정된 진행 담당이 인수 사실을 기록합니다. 서로 다른 에이전트가 독립적으로 같은 역할을 차지하지 않습니다.

한 작업 ID에 Writer 한 명만 배정합니다. 같은 담당자가 여러 ID를 맡을 수 있으며, 담당자 ID는 동명이인이나 재시작 세션을 구분할 수 있는 세션·작업 식별자를 포함합니다. 작업 범위와 파일 예외는 Spec을 따르고 이 표에서 임의 확대하지 않습니다.

| 관리 항목 | 현재 값 |
|---|---|
| 대표 저장소 | `D:\Downloads\Vibe_dev\TelosWeaver`, Branch `main` (원격 없음) |
| 통합 담당자 ID | `claude-opus-5.5/session-c9336d2c` (2026-10-24 사용자 지정) |
| 공통 계약 기준 Commit | 미확정 (착수 기준: `fca1669`) |
| 공통 계약 기준 설명 | G0 준비 중. `fca1669`의 `telos-schema`·`AgentAdapter`는 동결되지 않은 초안이며 다른 작업은 이를 계약 기준으로 사용하지 않습니다. |

## 2. 배정 상태

| 작업 ID | 담당자 ID | 상태 | Branch / Worktree | 기준 Commit | 결과 Commit | 인계·요청 |
|---|---|---|---|---|---|---|
| PH0-INTEGRATION | `claude-opus-5.5/session-c9336d2c` | in-progress | `main` / 대표 저장소 작업 트리 | `fca1669` | 없음 | G0 준비 완료(`556e62f`). CONTRACT 통합 후 G0 확정 |
| PH0-CONTRACT | `claude-opus-5.5/session-c9336d2c` | in-progress | `ph0/contract` / `D:\worktree\TelosWeaver\ph0-contract` | `556e62f` | 없음 | §5 계약 작성 중. G0 확정 전 초안 |
| PH0-HOST | 미배정 | unassigned | 미지정 | 미지정 | 없음 | 공용 런타임·Core API 계약에 의존 |
| PH0-CODEX | 미배정 | unassigned | 미지정 | 미지정 | 없음 | G0 후 Parser/Fixture 병렬 진행 가능 |
| PH0-CLAUDE | 미배정 | unassigned | 미지정 | 미지정 | 없음 | G0 후 Parser/Fixture 병렬 진행 가능 |
| PH0-AGY | 미배정 | unassigned | 미지정 | 미지정 | 없음 | G0 후 Parser/Fixture 병렬 진행 가능 |
| PH0-MASTER | 미배정 | unassigned | 미지정 | 미지정 | 없음 | 결정 Payload·검증 입출력 계약에 의존 |
| PH0-RN | 미배정 | unassigned | 미지정 | 미지정 | 없음 | Fork 도입 방식·기준 SHA·RPC 계약 기록 |
| PH0-EXT | 미배정 | unassigned | 미지정 | 미지정 | 없음 | G0 후 고정 API 테스트 대역으로 진행 가능 |

상태는 `unassigned → reserved → in-progress → review → integrated → verified`를 사용합니다. `blocked`는 기존 담당자를 유지하며 차단 사유·필요한 계약·재개 조건을 기록합니다. 통합 전 `verified`로 표시하지 않습니다. 개별 `verified`는 Phase 0 전체 통과와 다릅니다.

## 3. 통합 게이트

| 게이트 | 상태 | 근거 |
|---|---|---|
| G0 공통 계약 확정 | 준비 중 | 담당 배정 완료. 기준 Commit과 인터페이스·소유권 기록 필요 |
| G1 대표 Provider 실행 | 대기 | 실제 시작·이벤트·완료/실패·취소·결과 경로 필요 |
| G2 전체 구성 통합 | 대기 | 세 Provider·마스터·RN·Extension 연결 증거 필요 |
| G3 Phase 0 수용 | 대기 | Spec §19.1 필수 조건과 대상 환경 증거 필요 |

기존 타입·빌드·단위 테스트가 존재한다는 이유만으로 게이트를 통과시키지 않습니다. 증거는 Spec §19.5.8의 형식으로 `docs/phase-0/evidence/<작업 ID>/README.md`에 기록합니다.

## 4. 계약·공통 파일 변경 요청

| 요청 ID | 요청 작업 | 변경 대상·이유 | 영향 작업 | 처리 주체 | 상태·결과 Commit |
|---|---|---|---|---|---|
| 없음 | 해당 없음 | 등록된 요청 없음 | 해당 없음 | 해당 없음 | 해당 없음 |

요청에는 필요한 타입·메서드·의존성과 호환·시험 계획을 포함합니다. 통합 담당과 공통 계약 담당이 먼저 기준 변경을 통합하고 소비 작업에 전달합니다. 소비 작업에서 임시 공통 타입을 만들거나 같은 파일을 각자 수정하여 우회하지 않습니다.

## 5. G0 계약 범위와 등록 경로

Spec §19.5.2의 계약을 아래 위치에 정의합니다. G0 확정 전에는 모두 초안이며, 확정 후 변경은 §4의 변경 요청으로만 처리합니다.

| 계약 | 정의 위치 | 작성 | 사용 작업 |
|---|---|---|---|
| Adapter 실행 | `telos_schema::{TaskSpec, RunContext, RunStarted, FailureKind}`, `telos_adapters::{AgentAdapter, AdapterError}` | CONTRACT | HOST, 세 Provider, MASTER |
| 이벤트·취소 | `telos_schema::ProviderEvent`와 kind별 Payload, `AgentAdapter::{events, cancel_run, inspect_run}` 문서 계약 | CONTRACT | HOST, 세 Provider, INTEGRATION |
| 공용 프로세스 경계 | `telos_adapters::runtime` 공개 인터페이스 (G0 Stub: `UnimplementedRuntime`) | INTEGRATION 등록, 이후 HOST 구현 | HOST, 세 Provider |
| 마스터 결정 | `telos_schema::{MasterDecision, DecisionPayload, DecisionContext, DecisionOutcome}` | CONTRACT | MASTER, HOST |
| Local Core API | `telos_schema::api` (전송·인증 규칙은 모듈 문서) | CONTRACT | HOST, EXT |
| RN RPC | `telos_schema::rn`, `telos-rn-client`가 재노출 | CONTRACT 정의, INTEGRATION 등록 | RN, INTEGRATION |
| 생성·시험 | `schemas/*.schema.json`, `telos_schema::fixtures`(`fixtures` feature), `telos_adapters::UnsupportedAdapter` 시험 대역, 증거 형식 `evidence/PH0-INTEGRATION/EVIDENCE_TEMPLATE.md` | CONTRACT, INTEGRATION | 전체 |

G0 준비에서 등록한 Stub 경로입니다. 등록된 파일은 이후 해당 작업의 독점 경로이며 G0 확정 전까지는 등록 내용 외의 구현을 넣지 않습니다.

| 경로 | 이후 소유 | 내용 |
|---|---|---|
| `crates/adapters/src/runtime/mod.rs` | HOST | 공용 프로세스 런타임 공개 인터페이스, `UnimplementedRuntime` |
| `crates/core/src/api/mod.rs`, `crates/core/src/execution/mod.rs` | HOST | 모듈 문서만 |
| `crates/core/src/master/mod.rs`, `crates/core/src/policy/mod.rs` | MASTER | 모듈 문서만 |

의존성 버전은 루트 `Cargo.toml`의 `[workspace.dependencies]`에서만 정합니다.

## 6. 변경 기록

| 일시 (KST) | 기록자 | 변경 | 기준 Commit |
|---|---|---|---|
| 2026-10-24 01:05 | `claude-opus-5.5/session-c9336d2c` | 사용자 지정으로 통합 담당 인수. PH0-INTEGRATION `unassigned → in-progress`, PH0-CONTRACT `unassigned → reserved`, G0 `대기 → 준비 중`. 대표 저장소 기록 | `fca1669` |
| 2026-10-24 01:23 | `claude-opus-5.5/session-c9336d2c` | G0 준비: 의존성 버전을 `[workspace.dependencies]`로 일원화, `telos-schema` `fixtures` feature 추가, §5의 Stub 경로 등록, G0 계약 범위 기록 | `4b67618` |
| 2026-10-24 01:24 | `claude-opus-5.5/session-c9336d2c` | PH0-CONTRACT `reserved → in-progress`. Worktree `D:\worktree\TelosWeaver\ph0-contract`, Branch `ph0/contract` 생성 | `556e62f` |

PH0-INTEGRATION은 배정표 관리 자체가 작업이므로 `in-progress`로 기록합니다. 두 작업의 담당자는 같지만 PH0-INTEGRATION은 대표 저장소 `main`, PH0-CONTRACT는 별도 Worktree의 `ph0/contract`에서 진행하며 Writer는 각 작업 트리에 한 명입니다.

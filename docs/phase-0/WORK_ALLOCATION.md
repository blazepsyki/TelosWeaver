# Phase 0 작업 배정표

> **프로젝트:** TelosWeaver  
> **갱신일:** 2026-10-24  
> **범위 원본:** [Spec §19.5](../DEVELOPMENT_v1.3.md#195-phase-0-병렬-구현--작업-분할과-소유권)  
> **현재 상태:** G0 통과(계약 기준 `ae31eb2`). 나머지 작업은 배정 요청을 받아 착수할 수 있습니다. G1~G3는 대기입니다.

## 1. 단일 원본과 갱신 주체

대표 저장소의 이 파일을 단일 원본으로 사용합니다. 대표 저장소의 위치는 최초 통합 담당이 착수 시 기록합니다. 다른 Worktree·Clone의 복사본은 참고용이며 그것만 바꿔 소유권을 확보하지 않습니다.

통합 담당 한 명만 이 표를 직렬 갱신합니다. 작업 에이전트는 Spec의 작업 ID를 선택해 배정을 요청하고, 담당·Branch/Worktree·기준 Commit이 확정된 뒤 해당 경로를 수정합니다. 통합 담당이 정해지지 않았다면 최초 착수를 총괄하도록 지정된 진행 담당이 인수 사실을 기록합니다. 서로 다른 에이전트가 독립적으로 같은 역할을 차지하지 않습니다.

한 작업 ID에 Writer 한 명만 배정합니다. 같은 담당자가 여러 ID를 맡을 수 있으며, 담당자 ID는 동명이인이나 재시작 세션을 구분할 수 있는 세션·작업 식별자를 포함합니다. 작업 범위와 파일 예외는 Spec을 따르고 이 표에서 임의 확대하지 않습니다.

| 관리 항목 | 현재 값 |
|---|---|
| 대표 저장소 | `D:\Downloads\Vibe_dev\TelosWeaver`, Branch `main` (원격 없음) |
| 통합 담당자 ID | `claude-opus-5.5/session-c9336d2c` (2026-10-24 사용자 지정) |
| 공통 계약 기준 Commit | `ae31eb2` (2026-10-24 G0 확정) |
| 공통 계약 기준 설명 | §5의 7개 계약. 변경은 §4 변경 요청으로만 처리합니다. 판정 근거: `evidence/PH0-INTEGRATION/README.md` |

## 2. 배정 상태

| 작업 ID | 담당자 ID | 상태 | Branch / Worktree | 기준 Commit | 결과 Commit | 인계·요청 |
|---|---|---|---|---|---|---|
| PH0-INTEGRATION | `claude-opus-5.5/session-c9336d2c` | in-progress | `main` / 대표 저장소 작업 트리 | `fca1669` | `ae31eb2` (G0) | G0 통과. 다음: 배정 요청 처리, G1(HOST + Codex) 통합 |
| PH0-CONTRACT | `claude-opus-5.5/session-c9336d2c` | verified | `ph0/contract` / Worktree 정리됨 | `556e62f` | `fd0b206` (통합 `cc142cf`) | G0 계약 완료. 이후 계약 변경은 §4 요청으로 처리 |
| PH0-HOST | 미배정 | unassigned | 미지정 | 미지정 | 없음 | 착수 가능. `telos_adapters::runtime`·`telos_schema::api` 구현 |
| PH0-CODEX | 미배정 | unassigned | 미지정 | 미지정 | 없음 | 착수 가능. Parser/Fixture 병렬, 실제 실행은 HOST 런타임 사용. G1 대표 Provider |
| PH0-CLAUDE | 미배정 | unassigned | 미지정 | 미지정 | 없음 | 착수 가능. Parser/Fixture 병렬, 실제 실행은 HOST 런타임 사용 |
| PH0-AGY | 미배정 | unassigned | 미지정 | 미지정 | 없음 | 착수 가능. Parser/Fixture 병렬, 실제 실행은 HOST 런타임 사용 |
| PH0-MASTER | 미배정 | unassigned | 미지정 | 미지정 | 없음 | 착수 가능. `DecisionPayload`·`DecisionContext`·`DecisionOutcome`과 공통 실패 Fixture 사용 |
| PH0-RN | 미배정 | unassigned | 미지정 | 미지정 | 없음 | 착수 가능. Fork 도입 방식·기준 SHA 기록, `telos_schema::rn` 계약 구현 |
| PH0-EXT | 미배정 | unassigned | 미지정 | 미지정 | 없음 | 착수 가능. 생성 타입과 `telos_schema::api` 문서 계약으로 테스트 대역 개발 |

상태는 `unassigned → reserved → in-progress → review → integrated → verified`를 사용합니다. `blocked`는 기존 담당자를 유지하며 차단 사유·필요한 계약·재개 조건을 기록합니다. 통합 전 `verified`로 표시하지 않습니다. 개별 `verified`는 Phase 0 전체 통과와 다릅니다.

## 3. 통합 게이트

| 게이트 | 상태 | 근거 |
|---|---|---|
| G0 공통 계약 확정 | 통과 (2026-10-24) | 기준 Commit `ae31eb2`, `evidence/PH0-INTEGRATION/README.md` §1. 같은 담당자의 자기 검토임을 기록 |
| G1 대표 Provider 실행 | 대기 | 실제 시작·이벤트·완료/실패·취소·결과 경로 필요 |
| G2 전체 구성 통합 | 대기 | 세 Provider·마스터·RN·Extension 연결 증거 필요 |
| G3 Phase 0 수용 | 대기 | Spec §19.1 필수 조건과 대상 환경 증거 필요. Linux·Remote SSH Host는 [WSL 절차](WSL_LINUX_HOST.md)로 확보 예정(미구성) |

기존 타입·빌드·단위 테스트가 존재한다는 이유만으로 게이트를 통과시키지 않습니다. 증거는 Spec §19.5.8의 형식으로 `docs/phase-0/evidence/<작업 ID>/README.md`에 기록합니다.

## 4. 계약·공통 파일 변경 요청

| 요청 ID | 요청 작업 | 변경 대상·이유 | 영향 작업 | 처리 주체 | 상태·결과 Commit |
|---|---|---|---|---|---|
| REQ-001 | PH0-INTEGRATION | `.gitattributes` 신설(LF 고정). `core.autocrlf=true` 환경의 새 Worktree가 CRLF로 체크아웃되어 생성물이 내용 변경 없이 수정으로 표시됨. 현재 Spec §19.5.3에 소유 작업이 없어 Spec에 INTEGRATION 경로로 추가 필요 | 전체 | 사용자 확인 후 INTEGRATION | 완료. 사용자가 LF로 결정, `.gitattributes` 추가·Spec v1.3.4에 경로 등록. Index 내용 변경 없음 |

요청에는 필요한 타입·메서드·의존성과 호환·시험 계획을 포함합니다. 통합 담당과 공통 계약 담당이 먼저 기준 변경을 통합하고 소비 작업에 전달합니다. 소비 작업에서 임시 공통 타입을 만들거나 같은 파일을 각자 수정하여 우회하지 않습니다.

## 5. G0 계약 범위와 등록 경로

Spec §19.5.2의 계약을 아래 위치에 정의했으며 `ae31eb2`로 확정했습니다. 이후 변경은 §4의 변경 요청으로만 처리합니다.

| 계약 | 정의 위치 | 작성 | 사용 작업 |
|---|---|---|---|
| Adapter 실행 | `telos_schema::{TaskSpec, RunContext, RunStarted, FailureKind}`, `telos_adapters::{AgentAdapter, AdapterError}` | CONTRACT | HOST, 세 Provider, MASTER |
| 이벤트·취소 | `telos_schema::ProviderEvent`와 kind별 Payload, `AgentAdapter::{events, cancel_run, inspect_run}` 문서 계약 | CONTRACT | HOST, 세 Provider, INTEGRATION |
| 공용 프로세스 경계 | `telos_adapters::runtime` 공개 인터페이스 (G0 Stub: `UnimplementedRuntime`) | INTEGRATION 등록, 이후 HOST 구현 | HOST, 세 Provider |
| 마스터 결정 | `telos_schema::{MasterDecision, DecisionPayload, DecisionContext, DecisionOutcome}` | CONTRACT | MASTER, HOST |
| Local Core API | `telos_schema::api` (전송·인증 규칙은 모듈 문서) | CONTRACT | HOST, EXT |
| RN RPC | `telos_schema::rn`, `telos-rn-client`가 재노출 | CONTRACT 정의, INTEGRATION 등록 | RN, INTEGRATION |
| 생성·시험 | `schemas/*.schema.json`, `telos_schema::fixtures`(`fixtures` feature), `telos_adapters::UnsupportedAdapter` 시험 대역, 증거 형식 `evidence/PH0-INTEGRATION/EVIDENCE_TEMPLATE.md` | CONTRACT, INTEGRATION | 전체 |

G0에서 등록한 Stub 경로입니다. 등록된 파일은 이제 해당 작업의 독점 경로입니다.

| 경로 | 이후 소유 | 내용 |
|---|---|---|
| `crates/adapters/src/runtime/mod.rs` | HOST | 공용 프로세스 런타임 공개 인터페이스, `UnimplementedRuntime` |
| `crates/core/src/api/mod.rs`, `crates/core/src/execution/mod.rs` | HOST | 모듈 문서만 |
| `crates/core/src/master/mod.rs`, `crates/core/src/policy/mod.rs` | MASTER | 모듈 문서만 |
| `crates/rn-client/src/lib.rs` | RN | `telos_schema::rn` 재노출 |

의존성 버전은 루트 `Cargo.toml`의 `[workspace.dependencies]`에서만 정합니다.

## 6. 환경·운영 결정

| 항목 | 결정 | 상태 | 결정일 |
|---|---|---|---|
| 줄바꿈 | 저장소 전체 LF 고정, `.cmd`·`.bat`만 CRLF (`.gitattributes`) | 적용 | 2026-10-24 |
| Linux·Remote SSH 실행 Host | 같은 PC의 WSL2 Ubuntu 26.04를 Linux Host 겸 Remote SSH Host로 사용. 절차는 [WSL_LINUX_HOST.md](WSL_LINUX_HOST.md). G3 필수 조건이므로 미루지 않음 | 절차 수립, 배포판 미설치 | 2026-10-24 |
| CI | 원격 저장소가 생기고 초기 구현이 진행된 뒤 검토. 그전에는 통합 담당이 Spec §19.5.8의 검사 명령을 수동 실행하고 증거에 기록 | 보류 | 2026-10-24 |
| 대상 OS 우선순위 | 구현·일상 검증은 Windows 로컬 우선. Linux·Remote SSH는 G3 전에 WSL로 확보 | 적용 | 2026-10-24 |

## 7. 변경 기록

| 일시 (KST) | 기록자 | 변경 | 기준 Commit |
|---|---|---|---|
| 2026-10-24 01:05 | `claude-opus-5.5/session-c9336d2c` | 사용자 지정으로 통합 담당 인수. PH0-INTEGRATION `unassigned → in-progress`, PH0-CONTRACT `unassigned → reserved`, G0 `대기 → 준비 중`. 대표 저장소 기록 | `fca1669` |
| 2026-10-24 01:23 | `claude-opus-5.5/session-c9336d2c` | G0 준비: 의존성 버전을 `[workspace.dependencies]`로 일원화, `telos-schema` `fixtures` feature 추가, §5의 Stub 경로 등록, G0 계약 범위 기록 | `4b67618` |
| 2026-10-24 01:24 | `claude-opus-5.5/session-c9336d2c` | PH0-CONTRACT `reserved → in-progress`. Worktree `D:\worktree\TelosWeaver\ph0-contract`, Branch `ph0/contract` 생성 | `556e62f` |
| 2026-10-24 01:34 | `claude-opus-5.5/session-c9336d2c` | `ph0/contract`(`fd0b206`)를 `cc142cf`로 통합, RN Client 재노출 등록(`ae31eb2`), `main`에서 재검증. PH0-CONTRACT `in-progress → verified`, G0 `준비 중 → 통과`, 기준 Commit `ae31eb2` 확정. 나머지 작업 인계 갱신, REQ-001 등록, Worktree 정리 | `ae31eb2` |
| 2026-10-24 12:25 | `claude-opus-5.5/session-c9336d2c` | 사용자 결정 반영: REQ-001 완료(LF 고정 `.gitattributes`), Linux·Remote SSH Host WSL 절차 수립, CI 보류. §6 환경·운영 결정 추가. Spec v1.3.4로 경로 등록. 공통 계약 변경 없음 | `ae31eb2` |
| 2026-10-24 12:29 | `claude-opus-5.5/session-c9336d2c` | 사용자 결정: WSL 배포판을 Ubuntu 24.04에서 26.04(`Ubuntu-26.04`)로 변경, Host ID `wsl-ubuntu-2604`. 배포판 미설치 상태 유지 | `ae31eb2` |

PH0-INTEGRATION은 배정표 관리 자체가 작업이므로 G3까지 `in-progress`로 유지합니다. PH0-CONTRACT는 같은 담당자가 별도 Worktree의 `ph0/contract`에서 진행했고, 통합 후 Worktree를 정리했습니다. 이후 계약 변경 요청이 오면 새 Worktree를 만들어 같은 규칙으로 처리합니다.

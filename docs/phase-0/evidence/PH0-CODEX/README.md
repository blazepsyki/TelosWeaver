# PH0-CODEX 배정 및 착수 준비

| 항목 | 값 |
|---|---|
| 작업 ID·담당 | PH0-CODEX · Codex / `PH0-CODEX-20261024` |
| 작성일 | 2026-10-24 |
| 상태 | in-progress. 사용자가 권장안 세 가지를 승인해 구현 착수. 초기 Mock 9개·Windows 실환경 생성 1개 통과, 나머지 실환경 시험 진행 중 |
| Branch / Worktree | `ph0/codex` / `D:\worktree\TelosWeaver\ph0-codex` |
| 준비 기준 | `75ba56e` (Spec v1.4 개정 1.4.1), 공통 계약 `271e7fd` |
| 배정 기록 | 대표 main의 `2fa172d` |
| 구현 결과 | 없음. 이 문서는 실행 PoC나 구현 완료 증거가 아님 |
| 기준 문서 | [Spec](../../../DEVELOPMENT_v1.4.md), [대표 배정표](../../WORK_ALLOCATION.md) |

사용자가 PH0-CODEX를 직접 배정하고 먼저 배정 문서와 착수 전 결정 사항을 정리하도록 요청했습니다. 기존 PH0-HOST 배정과 Linux·Remote SSH 검증 대기는 유지합니다. G1 대표 Provider는 배정표의 기존 결정대로 Claude이며 이 배정으로 바꾸지 않습니다.

## 1. 담당 범위

| 소유 경로 | 담당 내용 |
|---|---|
| `crates/adapters/src/codex.rs`, `crates/adapters/src/codex/**` | Codex 확장 CLI 규칙, App Server 연결, 요청·응답·Thread/Turn 상관 관계, 공통 이벤트 변환, 오류·취소·Capability |
| `crates/adapters/tests/codex_*.rs` | Protocol·Adapter 시험 대역, 명시 실행하는 실환경 시험 |
| `crates/adapters/tests/fixtures/codex/**` | Codex 전용 Protocol Fixture와 재현 자료 |
| `docs/phase-0/evidence/PH0-CODEX/**` | 준비·버전·설정·검증 결과·미지원 근거 |

공용 프로세스 제어는 기존 `NativeRuntime`, 공통 탐색은 `NativeCliDiscoverer`를 사용합니다. 공통 Schema·Adapter Trait·lib.rs 등록·Cargo 의존성·Core 진입점은 기존 담당자의 변경 요청 절차를 따릅니다. CLAUDE·AGY의 소유 경로를 수정하지 않습니다.

## 2. 현재 확인한 사실

다음은 2026-10-24 Windows 로컬에서 읽기 전용으로 확인했습니다. 확장 메타데이터와 실행 파일의 `--version`, `app-server --help`만 확인했으며 모델 호출은 하지 않았습니다.

| 항목 | 확인 결과 |
|---|---|
| 현재 Codex Adapter | `unimplemented_adapter!`를 사용하는 Stub |
| VS Code 확장 ID·버전 | `openai.chatgpt` · `26.1021.22027` · `win32-x64` |
| 실행 파일 | `C:\Users\Psyki-HOME\.vscode\extensions\openai.chatgpt-26.1021.22027-win32-x64\bin\windows-x86_64\codex.exe` |
| CLI 버전 | `codex-cli 0.164.0-alpha.11.1` |
| App Server | 도움말에 실험 기능으로 표시됨. 기본 연결은 `stdio://`이며 `--stdio`를 제공함 |
| Protocol 생성 기능 | 도움말에 `generate-json-schema`, `generate-ts`가 있음. 실제 Schema 생성·검토는 착수 후 수행 |
| HOST 기반 | 런타임·공개 탐색 엔진과 REQ-010·011이 main에 반영됨 |
| 사용할 모델·인증 유효성 | 확인하지 않음. 기존 인증 파일을 읽거나 복사하지 않음 |

Spec §7.5에 있는 이전 관찰 버전과 설치 버전이 달라졌습니다. 실행 경로를 버전 문자열로 고정하지 않고 공통 탐색 결과를 사용하며, 실제 버전·Protocol Schema를 근거로 호환 범위를 기록해야 합니다.

## 3. 사용자 결정이 필요한 시험 조건

아래는 제안이며 아직 확정된 조건이 아닙니다. Claude·Agy에 대해 승인된 실환경 조건을 Codex에도 승인된 것으로 간주하지 않습니다.

| 번호 | 결정할 내용 | 권장안 |
|---|---|---|
| 1 | 기존 Codex 인증을 사용한 실계정 시험을 진행할지와 작업 범위 | 기존 인증을 CLI가 그대로 사용하고 저장소 밖 일회용 폴더에서 짧은 시험만 수행. 성공·실패·취소·도구 허용/거부와 시험 세션의 Resume/Fork를 검증. 파일 쓰기·로컬 명령은 시험 폴더와 지정한 도구로 제한. 기존 프로젝트·대화는 대상에서 제외 |
| 2 | 실환경 시험 모델과 추론 강도 | 계정에서 사용 가능한 경량 모델과 `low`를 사용. 정확한 모델 ID는 승인 후 사용 가능 목록을 확인해 기록. 사용자가 모델을 지정하면 그 ID를 우선하며 사용할 수 없을 때 조용히 다른 모델로 바꾸지 않음 |
| 3 | 시험으로 생성한 세션 정리 방식 | 신규 시험 세션 ID를 기록하고 시험 후 Archive 지원을 확인해 보관 처리. 기존 대화와 원본 세션은 변경하지 않고 시험 세션 원본도 임의 삭제하지 않음 |

1번의 실제 모델 호출은 계정 사용량을 소모하고 시험 프롬프트·결과를 외부 서비스와 주고받습니다. 권장안은 짧은 프롬프트·순차 시험을 사용하며, 실패 원인을 확인하지 않은 반복 호출은 하지 않는 것입니다. 추가 MCP·외부 도구 연동 시험은 포함하지 않습니다. 필요 도구와 승인·Sandbox 조건의 실제 적용 여부를 시험 전에 확인합니다.

Resume·Fork는 이번 시험에서 새로 만든 세션만 대상으로 합니다. 기존 프로젝트 세션이나 현재 사용자와 작업 중인 에이전트 세션을 재개·수정하는 시험이 아닙니다. UI와의 체계적인 동시 사용·Attached 연동은 Spec의 별도 단계에 남깁니다.

## 4. 사용자 선택 없이 처리할 기술 준비

- Spec의 Managed 연동 기준인 `codex app-server`와 Thread/Turn 이벤트를 따릅니다. 현재 설치 CLI의 생성 Schema·도움말로 요청·응답·버전 차이를 확인합니다.
- `cli_rule(target_platform)`과 공통 탐색을 연결합니다. 실행 파일은 복사하지 않으며 같은 Run에서는 확인한 경로를 고정합니다.
- G0의 Run ID·멱등성·sequence·이벤트 보존·종료 규칙을 따르고 Protocol 원본 DTO는 Codex 전용 모듈에 둡니다. 프로세스 수명·Thread/Turn과 Run의 대응은 해당 계약 안에서 설계합니다.
- 승인된 Workspace·모델·권한 설정을 받아 실행하도록 경계를 둡니다. 현재 에이전트의 권한이나 시험용 승인을 제품 기본 권한으로 사용하지 않습니다.
- 공통 계약이나 Cargo·모듈 등록 변경이 필요하면 CONTRACT·INTEGRATION에 구체적인 요청을 제출합니다. 구현 편의를 위해 공통 타입을 복제하지 않습니다.
- Protocol Fixture·Mock 시험과 실제 계정 시험을 분리하고 실환경 시험은 명시 실행으로 둡니다. 테스트 통과만으로 실제 CLI 지원을 주장하지 않습니다.

## 5. 착수 후 순서와 검증

1. 예약 배정·전용 Worktree·최신 계약을 확인하고 착수 상태를 대표 배정표에 기록합니다.
2. 설치 버전의 App Server Schema와 탐색 규칙을 확인하고 Protocol Parser·Fixture·요청 상관 관계를 구현합니다.
3. 공용 런타임으로 시작·입력·출력·취소·실제 종료·재구독·멱등성을 연결하고 Mock 반례를 검증합니다.
4. 사용자 확정 조건으로 Windows 실환경의 PROV-01~04·06~09를 확인합니다. Resume·Native Fork는 새 실행과 별도로 PROV-05·FORK-POC-01~02 결과를 기록합니다.
5. 실제 사용한 버전·설정·세션 ID·증거 수준·미지원 기능과 후속 Core 연결 경계를 인계합니다. Linux·Remote SSH는 기존 사용자 지시대로 대기합니다.

## 6. REQ-012 공개 모듈 등록 요청

App Server 연결과 Codex 전용 구성·실행 승인 Port를 구현했습니다. 단위 시험 9개가 통과했고 Protocol Schema는 현재 CLI에서 생성해 확인했습니다. 경량 모델은 실제 `model/list`에서 `low` 지원을 확인한 `gpt-6-luna`를 선택했습니다. Windows 실제 생성 시험 1개가 `TELOS_PONG` 응답으로 통과했고 해당 시험 세션을 Archive했습니다. 나머지 실환경 시험과 지원 Capability 판정은 진행 중입니다.

CONTRACT·INTEGRATION에 다음 한 줄의 등록 변경을 요청합니다. 공통 DTO·Trait·Cargo 변경은 없습니다. 기존 Claude·Agy 모듈 공개 방식과 같으며 적용 전에는 Codex 소유 모듈 안의 단위 시험으로 검증합니다.

```diff
-mod codex;
+pub mod codex;
```

공개 대상은 `cli_rule`, `CodexConfig`, `CodexLaunch`, `CodexLaunchResolver`, `CodexSandbox`, `CodexApprovals`입니다. 기존 `CodexAdapter::default()`는 실행 구성이 없는 상태로 유지하고 `new(config)`가 승인된 실행 조건을 받습니다.

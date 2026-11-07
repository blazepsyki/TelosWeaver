# H0 결정 사항

| 항목 | 값 |
|---|---|
| 작성 | PH1-INTEGRATION·PH1-CONTRACT (`claude-opus-5.5/session-c9336d2c`), 2026-11-07 |
| 기준 | `master` `3eea778`, Spec v1.5.10 §19.6.2 |
| 목적 | H0 계약 10종을 확정하려면 필요하지만 Spec이 답하지 않는 선택을 모읍니다. 결정되면 "결정" 열에 날짜와 함께 적고, Spec 개정이 필요한 항목은 INTEGRATION이 반영합니다 |

## 1. 사용자 결정이 필요한 항목

| # | 주제 | 관련 계약 | 선택지 | 권장 | 결정 |
|---|---|---|---|---|---|
| D1 | 영속 DB의 범위와 위치 | 영속 기록 | (a) 프로젝트별 SQLite를 사용자 상태 디렉터리에(`%LOCALAPPDATA%\telos-weaver\<projectKey>\`, Linux `$XDG_STATE_HOME/telos-weaver/<projectKey>/`) (b) 사용자 전체 DB 하나 (c) 프로젝트 안 `.telos-weaver/` | (a). Phase 0 토큰 위치와 같고, OPS-01 "프로젝트별 단일 Scheduler 소유권"과 맞으며, 저장소를 오염시키지 않음 | |
| D2 | Artifact 내용 저장 위치·한도 | 산출물·검증 | (a) D1 디렉터리 아래 내용 주소(`artifacts/<sha256>`) 저장, Version당 기본 상한 64 MiB(설정 가능), 초과는 등록 거부 (b) 프로젝트 안 `.telos-weaver/artifacts/` | (a). 불변·중복 제거가 쉽고 Workspace 정리와 분리됨 | |
| D3 | 마스터 결정 전달 방식 | 마스터 결정·정책 | (a) Phase 0처럼 마스터가 Turn 끝에 구조화 JSON 결정을 내고 Core가 Resume으로 결과를 돌려줌 (b) 마스터도 MCP 도구로 결정 | (a). Phase 0에서 검증됐고 Provider에 덜 의존함. MCP는 Worker 위임·보고에만 씀 | |
| D4 | Task MCP 연결 방식 | Task MCP | (a) `telos-weaver` 실행 파일의 stdio MCP 하위 명령을 Run마다 띄우고, Run 범위 토큰으로 Local API에 연결. 설정은 Run 단위로만 주입(Claude `--mcp-config`, Codex Thread 설정 override, Antigravity는 아래 D5) (b) 로컬 HTTP MCP 서버 | (a). 사용자 전역 설정을 바꾸지 않고, 기존 Local API 인증을 그대로 씀 | |
| D5 | Antigravity의 Task MCP 주입 수단이 없을 때 | Task MCP, Capability | `agy` 1.5.2는 `mcp add`(영구 설정 변경)와 `--settings`(세션 한정 JSON)만 보임. 세션 한정 주입은 H0에서 실험 필요. 안 될 때: (a) Antigravity Worker는 Task MCP 없이 실행(결과는 출력 계약 파일로 수집, 하위 위임·`report_task_result` 불가, Capability에 기록) (b) `agy mcp add`로 사용자 설정을 바꿈 | (a). (b)는 기존 결정(사용자 전역 설정 무변경)에 어긋남 | |
| D6 | 기본 마스터 Provider·모델 | 마스터 결정·정책, 설정 | Phase 0은 시험용 Claude `haiku`. 실제 사용 기본값: (a) Claude, 모델은 설정값으로 두고 기본 `sonnet` (b) Codex (c) 고정하지 않고 Plan 생성 때 선택 | (a). 시험은 계속 가벼운 모델 사용(D10). MASTER-01에 따라 교체 가능하게 구현 | |
| D7 | 설정 파일 형식·위치 | 설정 | (a) TOML. 사용자 `%APPDATA%\telos-weaver\config.toml`(Linux `$XDG_CONFIG_HOME`), 프로젝트 `.telos-weaver/config.toml`(저장소에 커밋 가능, 권한은 축소만 가능). Phase 0 정책 JSON은 이 형식으로 대체 (b) JSON 유지 | (a). 사람이 편집하기 쉽고 주석 가능 | |
| D8 | 검증 명령의 허용 방식 | 산출물·검증, 설정 | (a) 프로젝트·사용자 설정에 이름 붙인 명령 목록만 허용하고, Task 계약은 이름과 허용된 인자로만 참조 (b) 마스터가 임의 명령을 제안하고 정책이 패턴으로 검사 | (a). 마스터·Worker가 검증을 완화하지 못하게 함(Spec §11.1) | |
| D9 | Git Worktree 위치와 원본 Branch 반영 | Workspace | 위치: (a) D1 디렉터리 아래 `worktrees/` (b) 저장소 옆 별도 폴더. 반영: (a) 승인 뒤 로컬 원본 Branch에 Merge Commit, push는 P0에서 하지 않음 (b) Branch만 남기고 반영은 사용자가 직접 | 위치 (a), 반영 (a). push는 외부 변경이라 별도 결정 전까지 제외 | |
| D10 | Phase 1 실환경 시험 모델·사용량 | 전체 | (a) Phase 0과 같음: Claude `haiku`, Codex `gpt-6-luna` low, Antigravity Flash low, 저장소 밖 일회용 폴더, 병합 시 세션 정리 (b) 별도 상한 지정 | (a). E2E가 늘어나므로 상한이 필요하면 지정 | |
| D11 | C5: Resume·Fork 판정 기준 통일 | Capability 판정 | (a) 실환경 통과면 `supported`, 세부 제한은 `reason`과 실행 전 거부 (b) Phase 1 조건(세션 소유권 등)이 끝날 때까지 `unverified` | (a). Phase 1 Core가 Capability로 집행하므로, (b)면 검증된 Resume도 쓰지 못함. Spec §7.1의 "supported가 세부 조건을 보장하지 않음"과 맞음 | |
| D12 | Master View 입력 UI | Local API, EXT | (a) 명령·QuickInput으로 입력, 표시는 TreeView·OutputChannel·내장 Diff (b) Webview 대화 패널 | (a). Spec §17은 기본 UI를 우선하고 Webview를 표시 계층으로 제한 | |
| D13 | Phase 0 Local API·정책과의 호환 | Local API, 설정 | (a) 호환하지 않음. API 버전을 올리고 Extension을 함께 갱신, Phase 0 정책 파일은 D7로 대체 (b) Phase 0 API·정책을 유지하며 확장 | (a). 같은 저장소에서 함께 바뀌고 §19.2가 호환을 요구하지 않음. 안전 경계 시험은 회귀로 유지 | |

## 2. 결정 전에 할 일 (사용자 결정 아님)

| # | 내용 | 담당 |
|---|---|---|
| A1 | **CLI 버전 변경:** Claude Code 2.1.310 → 2.1.318, Antigravity 1.4.7 → 1.5.2로 올라감(2026-11-07 확인, Codex는 0.164.0-alpha.11.1 그대로). Adapter의 검증 버전 목록에 없어 Capability가 `unverified`로 보고됨. 지원 백로그 운영 규칙 2에 따라 "미지원" 항목 재확인과 기본 실환경 시험 재실행 필요 | INTEGRATION 조정, 각 Provider 담당 |
| A2 | Antigravity 1.5.2의 세션 한정 MCP 주입(`--settings`) 실험(D5 판단 근거) | PH1-AGY 또는 INTEGRATION |
| A3 | 서버 `~/TelosWeaver` Clone 재생성(`master` 기준) | 사용자 또는 INTEGRATION |

## 3. 제가 정하고 진행할 항목 (이의가 있으면 알려 주세요)

- SQLite 접근은 `rusqlite`(bundled)로 하고 Transaction은 동기 코드에서 처리합니다. 빌드 시 DB가 필요 없고 Transaction 경계가 분명합니다.
- 식별자는 Phase 0과 같이 Core가 발급하는 접두어+무작위 ID(`run-…`)를 유지하고, Plan·Task·Artifact 등에도 같은 형식을 씁니다.
- 공유 Wire 타입은 `telos-schema`, Core 서비스 인터페이스는 각 모듈에 둡니다(Spec §19.6.2).
- 예산 기본값은 Spec §13.2의 초기 제안(`providerConcurrency=1`, `maxDelegationDepth=3`, `maxPlanTasks=32`, `executionRetryLimit=1`, `reworkLimit=2`)을 씁니다. 토큰 계측이 안 되는 CLI는 시간·호출·Task 상한으로 집행하고 비용 상한을 정확히 집행한다고 표시하지 않습니다.

# 지원 백로그 — 미지원·미구현·미검증 기능

> **관리:** PH0-INTEGRATION(이후 통합 담당). 각 담당은 자기 범위 항목의 변경을 제안하고, 통합 담당이 반영합니다.
> **시작:** 2026-10-26, [G3 대조표](phase-0/evidence/PH0-INTEGRATION/G3_ACCEPTANCE.md) §2·§2.1·§3에서 옮김
> **목적:** 지금 지원하지 않는 기능을 잊지 않고, 지원 조건이 생기면 다시 판정하기 위한 단일 목록입니다. Spec §2.1 원칙 7에 따라 여기 있는 기능은 지원하는 것처럼 표시하지 않습니다.

## 1. 상태 정의

| 상태 | 뜻 | 다시 판정하는 계기 |
|---|---|---|
| **미지원** | Provider CLI·플랫폼에 수단이 없어 Adapter가 거부 | CLI·확장 업데이트(`--help`·Protocol Schema 변화) |
| **설계상 미지원** | CLI 수단은 있지만 현재 공통 계약·설계와 맞지 않아 거부 | 계약(Spec·`telos-schema`) 변경 결정 |
| **미구현** | 수단이 확인됐지만 Adapter·Core에 구현하지 않음 | 해당 Phase의 구현 계획 |
| **미검증** | 구현 또는 수단은 있지만 해당 환경 실환경 시험이 없음(`unverified`) | 시험 실행 |

## 2. 운영 규칙

1. Capability를 `unsupported`·`unverified`로 판정하거나, 기능을 미구현으로 남기면 이 표에 행을 추가합니다. 근거 Commit·증거 문서를 함께 적습니다.
2. Provider CLI·확장 버전이 바뀌면(§7.5 탐지 결과의 버전 변화) "미지원" 행의 재확인 방법을 실행합니다.
3. 항목이 해소되면 행을 지우지 않고 §4로 옮겨 해소 Commit과 날짜를 남깁니다.
4. 상태를 바꾸는 근거는 실제 실행 결과입니다. 문서·추정만으로 "지원"으로 옮기지 않습니다.

## 3. 현재 항목

기준: `master` `fa8e69b`, Spec v1.5.7, Claude Code 2.1.310, Codex CLI 0.164.0-alpha.11.1(확장 26.1021.22027), Antigravity `agy` 1.4.7(확장 1.9.0). 2026-11-17 Claude 항목 갱신: `c9df89e`, Claude Code 2.1.325.

### 3.1 Provider 기능

| ID | 대상 | 키·기능 | 상태 | 이유·근거 | 지원하려면 | 재확인 방법 |
|---|---|---|---|---|---|---|
| SB-01 | Antigravity | `native_fork` | 미지원 | Headless print 모드가 `/fork`를 거부하고 `--fork` 옵션이 없음(FORK-POC-01, [AGY](phase-0/evidence/PH0-AGY/README.md)) | CLI가 Headless Fork 수단을 제공 | `agy --help`의 Fork 옵션, `-p`에서 `/fork` 실행 |
| SB-02 | Antigravity | `answer_approval` | 미지원 | 권한 수단이 시작 전 `--mode`와 `--dangerously-skip-permissions`뿐, 실행 중 승인 응답 Protocol 없음 | CLI가 실행 중 승인 요청·응답 경로를 제공 | `agy --help`, stream-json 출력의 승인 요청 이벤트 |
| SB-03 | Codex | `vscode_listing` = `hidden` | 미지원 | App Server가 출처 지정과 무관하게 Thread를 `vscode`로 저장(REQ-021). 운영자 정책에서 거부 | App Server가 자동화 출처로 저장하는 수단을 제공 | `thread/start`의 출처 관련 필드와 저장된 `source` 확인 |
| SB-04 | Claude·Antigravity | 특정 Turn 기준 재개·분기(`source_turn_id`) | 미지원 | CLI에 수단 없음. 실행 전 `native_resume`·`native_fork`로 거부(Claude `ffdc70a`, Antigravity `423b2a3`). Claude 2.1.325 `--help`에서도 변화 없음(2026-11-17, [PH1-CLAUDE 증거](phase-1/evidence/PH1-CLAUDE/README.md)) | CLI가 메시지·Turn 기준 재개·분기 옵션을 제공 | `claude --help`·`agy --help` |
| SB-05 | Codex | 특정 Turn 기준 Resume(`source_turn_id` + `resume`) | 미구현 | Adapter가 "not implemented"로 거부. App Server 수단은 확인하지 않음. Fork는 `lastTurnId`로 지원 | App Server `thread/resume`의 Turn 지정 수단 확인 뒤 구현·시험 | 설치 CLI의 생성 Schema |
| SB-06 | Claude·Antigravity | `send_input`(진행 중 Turn 조정) | 설계상 미지원 | "Run당 Turn 1개"(Spec §7.3, REQ-006)와 맞지 않아 후속 입력은 Resume Run. Claude 2.1.325: 실행 중 메시지는 뒤에 도구 호출 경계가 남아 있을 때만 같은 Turn에 들어가고 아니면 새 Turn이 되어 보내는 쪽이 정할 수 없음(2026-11-17, [PH1-CLAUDE 증거](phase-1/evidence/PH1-CLAUDE/README.md)). Antigravity는 Phase 0 판정 그대로(PH1-AGY 재검토 예정) | 같은 Turn에 반영하는 수단 확인, 또는 다중 Turn Run을 계약에 도입 | Claude `env_mid_turn_message_joins_the_turn_only_at_a_tool_boundary`, Antigravity는 CLI 문서·실험 |
| SB-07 | Claude·Core | 실시간 승인(이월 P1-2) | 미구현(Core 측) | Claude Adapter는 구현·Windows 실환경 통과(§4, `c9df89e`). `ClaudeApprovals::Prompt`로 만든 Adapter만 켜지며 Core가 쓰지 않아 지금은 꺼져 있음. 남은 것: 마스터 판단·사용자 경로·기한, HOST API 라우팅, EXT 승인 UI | MASTER·VALIDATION·HOST·EXT가 `ProviderApprovalRequest` 경로 구현 뒤 Core에서 켬 | H1·H2 승인 경로 시험 |
| SB-08 | Codex | `send_input`·`answer_approval` (Linux) | 미검증 | Windows만 실환경 통과(C3 `9db02ff`) | `psyki-nas`에서 같은 시험 실행 | `env_steering_updates_the_same_turn`, `env_approval_allow_and_deny_control_the_write` |
| SB-09 | Codex | 명령 실행 외 승인(`fileChange`·권한 확장·MCP) | 미검증 | C3는 명령 실행 승인만 시험 | 종류별 실환경 시험 | — |
| SB-10 | Antigravity | `native_resume`·`native_fork` 상태 | 미검증(`unverified`) | Claude 부분은 해소(§4). Antigravity는 Phase 0 기준 그대로이며 H0 판정 기준(D11·D14) 적용은 PH1-AGY 몫 | PH1-AGY가 `judge`와 사전 점검 적용(P1-1) | — |
| SB-20 | Claude (Linux) | `answer_approval` | 미검증 | Windows 2.1.325에서만 실환경 통과. Linux에서는 증거가 없어 `unverified`로 보고됨(2026-11-17, [PH1-CLAUDE 증거](phase-1/evidence/PH1-CLAUDE/README.md)) | `psyki-nas`에서 실환경 시험 실행 | `env_live_approval_allow_and_deny`, `env_live_approval_cancel_before_answer` |
| SB-21 | Claude (Linux) | Task MCP 주입·Capsule·사전 점검 | 미검증 | Windows에서만 실행. Linux의 기존 기능은 2.1.310 증거와 같은 major.minor 사전 점검 규칙으로 판정되지만 사전 점검 자체를 Linux에서 실행해 보지 않음 | `psyki-nas`에서 실환경 시험 실행 | `env_preflight_passes_and_capabilities_follow_the_evidence`, `env_task_mcp_tool_is_called_only_in_the_injected_run`, `env_capsule_is_a_new_session_without_the_previous_conversation` |
| SB-22 | Claude | Task MCP 실제 서버 연결 | 미검증 | 시험용 stdio MCP 서버로만 확인. Core의 `telos-weaver` Task MCP 하위 명령은 아직 없음 | PH1-MCP·HOST 통합 | H1·H2 위임 경로 시험 |

### 3.2 Core·플랫폼

| ID | 대상 | 기능 | 상태 | 이유·근거 | 지원하려면 |
|---|---|---|---|---|---|
| SB-11 | Core | `capsule` 세션 모드 | 미구현 | Phase 0 정책·마스터 검증기가 거부(`integration/policy.rs`, `policy/validator.rs`). Claude Adapter는 새 세션으로 시작하고 `capsuleVersionId` 없는 요청을 거부하며 실환경 통과(2026-11-17, [PH1-CLAUDE 증거](phase-1/evidence/PH1-CLAUDE/README.md)). Codex·Antigravity Adapter는 `fresh`와 같이 처리. Core의 Capsule 조립(CONTEXT)과 정책 허용(MASTER)이 남음 | Capsule 문맥 조립(Spec §8.1)·정책 허용 구현 |
| SB-12 | Core(Windows) | 정상 종료 신호 | 미구현 | 취소 시 stdin을 닫고 유예 뒤 Job 강제 종료(통합 증거 §7.4) | Windows 정상 종료 수단 설계(P1-5) |
| SB-13 | Core(Linux) | Local API 다른 사용자 접근 거부, `XDG_RUNTIME_DIR` 없는 대체 경로 | 미검증 | 권한(0600/0700)만 확인(통합 증거 §9.3) | 다른 사용자 계정·변수 미설정 환경에서 시험 |
| SB-14 | Core(Linux) | Core와 감시 프로세스 동시 유실, 종료할 수 없는 프로세스, PID Namespace | 범위 밖 | REQ-023 한계(통합 증거 §9.9) | 필요 시 cgroup 등 별도 설계 |
| SB-15 | 전체 | macOS | 범위 밖 | Phase 0 대상 아님 | 대상 플랫폼 추가 결정 |

### 3.3 확장·환경

| ID | 대상 | 기능 | 상태 | 이유·근거 | 지원하려면 |
|---|---|---|---|---|---|
| SB-16 | 세 Provider | DEC-08 Linux·Remote SSH 화면 확인, 실시간 화면 반영 | 미검증 | 자동 목록 판정만 있음(§7.6) | 사용자 VS Code 화면 확인 |
| SB-17 | Linux Antigravity | `vscode_listing` = `listed` | 미검증(운영 제한) | Remote SSH의 `agy`는 확장 디렉터리에 CLI 토큰이 없어 `hidden` 운영(Spec v1.5.5 §7.5) | 확장 디렉터리 토큰으로 로그인한 상태에서 시험 |
| SB-18 | 서버 확장 | 다중 버전 설치 중 활성 버전 선택 | 미검증(현재 환경) | 2026-10-25 두 벌 설치 때 통과, 지금은 한 벌 | 두 벌 설치 환경에서 `discovery_environment` 재실행 |
| SB-19 | RN Fork | 팀 가입·중계 사용 중 외부 통신 관찰, Linux 바이너리 문자열 검사 | 범위 밖 | RN-08 검증 한계(통합 증거 §9.7) | 필요 시 별도 관찰 |

## 4. 해소된 항목

| 이전 내용 | 해소 | 근거 |
|---|---|---|
| Codex `send_input`·승인 Allow 미검증(Windows) | 2026-10-26 `supported`(Windows) | C3 `9db02ff` |
| Codex Linux 기본 기능·목록 표시가 `unverified`로 보고됨 | 2026-10-26 `supported` | C3 `9db02ff` |
| Claude·Antigravity가 Turn 지정 재개를 무시하고 최신 지점에서 실행 | 2026-10-26 실행 전 거부(SB-04로 정리) | `ffdc70a`, `423b2a3` |
| Linux에서 Core 강제 종료 뒤 Provider 잔존 | 2026-10-26 감시 프로세스로 회수 | REQ-023 `33c2995` |
| Claude `answer_approval` 미구현(SB-07의 Adapter 부분) | 2026-11-17 Adapter 구현, Windows 2.1.325 실환경 통과(Allow·Deny·응답 전 취소) | `c9df89e`, [PH1-CLAUDE 증거](phase-1/evidence/PH1-CLAUDE/README.md) |
| Claude `native_resume`·`native_fork`가 Phase 1 조건 때문에 `unverified`(SB-10의 Claude 부분) | 2026-11-17 H0 판정 기준(`judge`·사전 점검)으로 판정, 증거가 있으면 `supported` | `c9df89e`, [PH1-CLAUDE 증거](phase-1/evidence/PH1-CLAUDE/README.md) |

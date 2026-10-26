# G3 Phase 0 수용 대조표

| 항목 | 값 |
|---|---|
| 작성 | PH0-INTEGRATION (`claude-opus-5.5/session-c9336d2c`), 2026-10-26 |
| 기준 | `main` `96e61b8`(발견 3 통합 포함), Spec v1.5.6, 공통 계약 `7497877` |
| 근거 규칙 | Spec §19.1(체크리스트·통과 조건), §19.5.6 G3("§19.1의 모든 필수 조건과 Windows/Linux·Remote SSH 해당 환경 증거, 미지원 기능 판정 확보, INTEGRATION이 근거를 모아 보고") |
| 대상 환경 | Windows 11 로컬 `windows-local`, Linux `psyki-nas`(Ubuntu 26.04.1 LTS), 단일 Remote SSH(Windows VS Code → `psyki-nas`) |
| 판정 | **통과 판정(INTEGRATION).** 사용자 확인 뒤 확정 |

아래 "근거"의 `§n`은 [통합 증거](README.md)의 절입니다. 모든 실환경 시험은 실행한 것만 적었고 Mock 통과와 구분했습니다.

## 1. §19.1 체크리스트

| # | 항목 | Windows 로컬 | Linux·Remote SSH (`psyki-nas`) | 근거 | 판정 |
|---|---|---|---|---|---|
| 1 | 세 Provider의 탐지·버전·신규 Managed 실행·스트림·완료·실패·취소 | Claude 2.1.310·Codex 0.164.0-alpha.11.1·Antigravity 1.4.7: 각 Adapter 실환경(Claude 6~7개, Codex 7개, Agy 6개), Core 경유 G1 5개·G2 4개(한 Daemon에서 세 Provider 완료·결과 파일·실패·실제 종료 취소·Daemon 종료 정리) | Claude 7개, Codex 7개, Agy 7개, G1 5개, G2 4개 | §7.3·§8.4·§9.3·§9.4, [CLAUDE](../PH0-CLAUDE/README.md)·[CODEX](../PH0-CODEX/README.md)·[AGY](../PH0-AGY/README.md), 각 `LINUX.md` | 충족 |
| 2 | VS Code 확장에서 CLI 탐지, 확장 로그인의 Managed 실행 유효성, Antigravity 데이터 디렉터리. Windows 로컬과 Remote SSH Host(`~/.vscode-server/extensions`) 각각 | 세 Provider `extension` 출처 탐지(`metadata`), 확장 로그인으로 실행, AGY-DATA-01~05(`--app_data_dir=antigravity` 채택·잠금 충돌 없음) | HOST H3 활성 버전·`agy` 탐지, `discovery_environment` 통과(REQ-023 뒤 한 벌 설치는 "다중 버전 선택 미검증" 보고), Remote SSH 확장 동봉 CLI로 실행. Antigravity는 확장 디렉터리에 CLI 토큰이 없어 CLI 기본 디렉터리 사용(Spec v1.5.5 §7.5 Host별 차이, 사용자 결정) | §8.4·§9.3·§9.4·§9.9, [AGY](../PH0-AGY/README.md) §AGY-DATA, [HOST Linux](../PH0-HOST/LINUX.md) | 충족(Linux Antigravity는 `hidden` 운영으로 한정) |
| 3 | 마스터 후보의 구조화 결정과 Core의 잘못된 결정·중복·권한 초과 거부 | 실제 Claude 마스터(haiku) 2개: 위임→Worker 실행→결과 왕복, 권한 밖 Provider `permission_denied`. 검증기 8개·Controller 6개 | `phase0_master` 2개 | [MASTER](../PH0-MASTER/README.md), §9.4 | 충족 |
| 4 | Agy의 Turn 직렬화·도구 거부·Exit Code와 결과 차이 | Run당 한 프로세스·한 Turn(`send_input` 거부, 후속은 `--conversation`), `env_tool_denied`(`denied`·`tool_denied`), 종료 코드 0 소프트 거부를 실패로 판정 | 실환경 7개로 같은 대조, SSH 미로그인 `Authentication` 분류, SIGTERM 직전 `result(ERROR)`를 취소로 정규화 | [AGY](../PH0-AGY/README.md), [AGY Linux](../PH0-AGY/LINUX.md), §9.3 | 충족 |
| 5 | 세 Provider Managed Run 세션의 확장 목록 조회 대상 판정(§7.6 `vscode_listing`)과 화면 확인 별도 기록 | 세 Provider `listed`: 자동 판정·사용자 화면 확인·G2 정책 기본값 통과. `hidden`: Claude·Antigravity 통과, Codex 미지원(정책 거부, 사용자 결정) | 필수 아님. G2 목록 기록(Claude·Codex `listed`, Antigravity `hidden`)만 자동 확인, 화면 확인 미실행 | §10.4, §9.4, 각 `VSCODE_LISTING.md` | 충족(Windows 기준) |
| 6 | RN Fork에서 Reason CLI 연동·자동 업데이트·계정 로그인 제거, Windows/Linux Build, Health·Version·Discovery·통제된 Message RPC | RN-02·03 제거, RN-08 제거 검증, 격리 시험 204개, RN-06 RPC(인증·탐색·전송·구독·재동기화)·실제 Claude 세션 왕복, RN-07 Core 시작·재시작·정지 | 첫 Linux 빌드, 격리 시험 212개, `phase0_rn` 2개(Core가 띄운 RN과 왕복·재시작·정지, RN 정지 중 Run 독립) | §8.9·§9.2·§9.4·§9.7, [RN](../PH0-RN/README.md) | 충족(검증 한계는 §9.7) |
| 7 | VS Code 로컬과 단일 Remote SSH에서 실제 CLI·Workspace·Core의 Host 확인 | 로컬 Extension·Core·Provider 모두 `windows-local` | Remote-SSH 원격 Extension Host에서 Core 시작·연결·재연결, 창 종료 뒤 Core 생존, Core·Provider·Workspace 모두 `psyki-nas`. Run 중 SSH 단절에도 원격 Run 지속·중복 없음 | §8.7, §9.5 | 충족 |
| 8 | 이벤트 구독 전 종료, 잘못된 Stream, 프로세스·자식 종료와 기본 권한 적용 | 끝난 Run 재생(`api_replays_finished_run_and_unsubscribes_live_run`), 긴 줄·UTF-8·Protocol 오류(`oversized_line_…`, `exact_frame_utf8_…`), Job Object 자식 회수와 Core 강제 종료 시 트리 종료(`owner_crash_…`, 실제 Core·Claude 24ms), 도구 거부(Claude·Codex·Agy `env_tool_denied`) | Process Group 회수, Core 강제 종료 시 감시 프로세스로 Provider·도구·세션 분리 자손까지 회수(최종 코드 실제 Core·Claude 67ms), `turn_finished` 뒤 이어진 Turn 차단(발견 2) | §9.5·§9.6·§9.9, [HOST REQ-023](../PH0-HOST/REQ-023.md), HOST·Provider 시험 | 충족 |
| 9 | Native Resume·Fork 실험과 Provider별 증거·제한(신규 실행과 별개 판정) | Claude: Resume·Fork 동작(부모 Turn 지정 미검증). Codex: Resume 같은 Thread·Fork 새 Thread·부모 불변. Antigravity: Resume 동작, Headless Fork **미지원**(`/fork is not available in print mode`, `UnsupportedCapability`) | Claude `claude_environment`의 Resume/Fork 통과 | 각 Provider README §Resume·Fork, §9.4 | 충족(Agy Fork는 미지원으로 판정, §19.1에서 필수 아님) |
| 10 | Core/RN·Adapter Event·MasterDecision의 Schema와 Contract Test | 계약 기준 `7497877`, `telos-schema` Contract·Vector 시험, `json_schemas`(생성물 불일치 시 실패) | 같은 시험이 Linux Workspace에 포함 | [CONTRACT](../PH0-CONTRACT/README.md), §1·§6·§8.9 | 충족 |
| 11 | Crate 경계 고정, 공통 Schema → JSON Schema·TypeScript 생성, Extension 사용 경로와 불일치 검출 | 의존 방향 `schema ← adapters, rn-client ← core`(Cargo 선언), `schemas/` 커밋본과 생성 결과 비교(`json_schemas`), Extension은 빌드마다 `schemas/`에서 TypeScript 재생성(커밋 안 함)·`tsc`로 사용 불일치 검출, Project Key Vector·Rust 상수 대조 시험 | Extension `npm test` 29개(당시) 통과 | AGENTS.md 구조 규칙, §3·§8.7, `extension/scripts/gen-types.mjs`, `extension/test/client.test.cjs` | 충족 |
| 12 | Rust Core Daemon의 Windows/Linux Build와 Extension의 실행 파일 탐지·시작·연결 | Workspace 169개 통과, Extension Node 32개·실제 Core·격리 Host 6항목(발견 3 통합 `96e61b8`) | Workspace 171개 통과, Extension E1(Socket·토큰 0600·디렉터리 0700·잘못된 토큰 거부), E2 원격 Extension Host 시작·연결 | §9.3·§9.5·§9.9, [EXT Linux](../PH0-EXT/LINUX.md), [EXT HEALTH_STALE](../PH0-EXT/HEALTH_STALE.md) | 충족 |

**통과 조건 대조:** 세 Provider가 같은 상위 계약으로 신규 작업의 실행·성공·실패·취소를 보고합니다(1). RN Fork Build와 필수 RPC(6), 마스터 결정 검증 경로(3)가 Windows·Linux에서 재현됩니다. Agy Headless Fork와 기존 IDE Attached 성공은 필수가 아니며, 실패한 실험은 지원 기능으로 표시하지 않았습니다(§2).

## 2. 미지원·미검증 판정 (지원으로 표시하지 않음)

| 항목 | 판정 | 처리 |
|---|---|---|
| Antigravity Headless Native Fork | 미지원 | `UnsupportedCapability`로 거부 |
| Antigravity·Claude `send_input`, Antigravity 실행 중 승인 응답 | 미지원 | Capability `unsupported`, 후속 입력은 Resume Run |
| Codex `vscodeListing: hidden` | 미지원 | 운영자 정책 로드에서 거부(사용자 결정, REQ-021) |
| Codex `send_input`·승인 Allow 실환경 | 미검증 | 구현·Mock만. Phase 1에서 검증 |
| Claude Fork의 부모 기준 Turn 지정(`source_turn_id`) | 미검증 | 사용하지 않음 |
| DEC-08 Linux·Remote SSH 화면 확인, 실시간 화면 반영 | 미검증 | §19.1 필수 아님(Windows 기준 판정) |
| 확장 다중 버전 선택(서버) | 미검증(현 서버) | 2026-10-25 두 벌 설치 시점 HOST H3 통과 기록. 현재 한 벌이라 시험이 전제 미충족으로 보고 |
| Core·감시 프로세스 동시 유실, 종료할 수 없는 프로세스, macOS, PID Namespace | 범위 밖 | §9.9 한계 |
| RN 팀 가입·중계 사용 중 외부 통신, Linux 바이너리 문자열 검사 | 범위 밖 | §9.7 한계 |

## 3. Phase 0 범위 밖으로 남는 것 (Phase 1 P0)

영속 저장소·재시작 복구(Spec §14.2·§14.3의 미완 Run·Lock 대조), Scheduler·Workspace 관리·검증·Artifact, 정식 정책(`core/policy`), 미집행 마스터 결정 kind 5종, Extension 전체 UI(Task 목록·마스터 대화·승인·Artifact), 메시지 기능의 외부 노출. 모두 Phase 0에서 지원한다고 표시하지 않습니다.

## 4. 남은 운영 결정

- Antigravity 확장 요약 인덱스(`conversation_summaries.db`)의 시험 행 24개: 확장 내부 DB라 수정하지 않았으며 사용자 결정 대기(§10.5). G3 판정과 무관합니다.
- CI 없음: 통합 검사는 수동입니다(§4, 사용자 결정으로 보류).

## 5. 판정

§19.1의 필수 항목 12개 모두 대상 환경 증거가 있고, 미지원 기능은 §2와 같이 판정했습니다. INTEGRATION은 **G3 통과**로 판정하며, 사용자 확인을 받으면 배정표 §3을 "통과 확정"으로 바꿉니다.

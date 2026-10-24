# PH0-RN-04 Daemon 기반 정리

| 항목 | 값 |
|---|---|
| 서브 태스크·담당 | PH0-RN-04 · `claude-opus-5.5/session-c9336d2c` |
| 작성일 | 2026-10-25 |
| Branch / Worktree | `ph0/rn-04` / `D:\worktree\TelosWeaver\ph0-rn` |
| 기준 | PH0-RN-00 처리표 §3.8·§4·§5-2·§5-4, Spec v1.5 DEC-07·§12.3·§12.5, 배정표 §6 "RN 범위" |
| 사용자 결정 (2026-10-25) | 이름은 가칭 `telos-rn`으로 확정. 팀 링크는 주소 없이 `rn1-…` 문자열만 사용 |

## 1. 이름·상태 분리

| 대상 | 원본 | 이 Fork |
|---|---|---|
| 패키지·실행 파일 | `reason-network` | `telos-rn` (`target/debug/telos-rn.exe`) |
| MCP 서버·Hook·플러그인·확장 이름(`AGENT_NAME`) | `reason-network` | `telos-rn` |
| 상태 폴더 | `~/.reason/network` (`REASON_NETWORK_HOME`) | `~/.telos-rn` (`TELOS_RN_HOME`) |
| 환경 변수 | `REASON_NETWORK_*`, 없으면 `TINCAN_*` | `TELOS_RN_*`만 |
| Windows Pipe / Unix 짧은 소켓 폴더 | `reason-network-<hash>` / `/tmp/reason-network-<uid>` | `telos-rn-<hash>` / `/tmp/telos-rn-<uid>` |
| 설정 파일 Backup | `<file>.bak-reason` | `<file>.bak-telos-rn` |

원본 설치의 상태와 에이전트 설정(`reason-network` 항목)은 읽지도, 옮기지도, 지우지도 않습니다(§12.5). 이전 제품 이름(tincan)의 상태 이동(`paths::migrate`, 예전 소켓 링크)과 `setup`의 tincan 항목 정리(`LEGACY_NAME`)도 지웠습니다.

**바꾸지 않은 것:** 통신 식별자(ALPN `tincan/1`·`tincan/join/1`, 팀 항목 서명 문맥, 초대 코드 키 유도 문자열, `rn1-` 티켓 형식)와 Relay 목록의 서명 형식(`reason-network-relays`, `aud`)입니다. 원본 `CONTRIBUTING.md`가 "모든 피어가 쓰는 통신 식별자는 바꾸지 않는다"고 정하고 있고, Relay 목록은 Reason이 서명하는 문서라 바꿀 수 없습니다. 화면 문구의 제품명 "Reason Network"도 그대로 둡니다.

## 2. 동작 변경

- **팀 링크:** `join_url`이 티켓 문자열(`rn1-…`)을 그대로 돌려줍니다. 초대 문구는 `Join the telos-rn team: rn1-…`입니다. 입력 쪽(`Ticket::decode`)은 원본 형식의 전체 링크도 계속 받습니다. 원격 에이전트용 웹 주소 안내는 지웠습니다.
- **에이전트 연결은 `setup`만:** `init`·`join`과 안내형 흐름이 에이전트 MCP·Hook을 자동으로 연결하지 않습니다(RN-02에서 넘긴 항목). `--no-setup` 플래그, `setup::quietly`, `--json`의 `set_up_for`를 지웠고, 끝에 `telos-rn setup`(`--dry-run`)을 안내합니다.
- **Relay 목록 조회:** 팀에 들어 있고 그 팀이 Reason의 Relay를 쓸 때만 받습니다. 팀이 없으면 받지 않습니다(RN-00 §5-2).
- **Codex CLI 위치(RN-00 §5-4):** 검토만 했습니다. `codex queue`는 여전히 `PATH`의 `codex`를 씁니다. DEC-06의 확장 동봉 CLI를 쓸지는 메시지 전달 의미를 다루는 PH0-RN-05에서 정합니다.

## 3. 문서

- `rn-fork/README.md`를 이 Fork용으로 새로 썼습니다(차이점, 사용법, 격리 시험 방법).
- 원본 `DESIGN.md`·`CONTRIBUTING.md`·`SECURITY.md`에는 "원본 문서, 참고용" 머리말을 달았습니다.
- `tests/e2e/README.md`: 원본 Python E2E는 원본 이름과 지운 기능(로그인 증명, 구버전 혼합, `--no-setup`)을 써서 이 Fork에서 유지하지 않는다고 적었습니다. 이식할지 지울지는 PH0-RN-08에서 정합니다.
- 원본 배너 이미지(`assets/reason-network-banner.png`)를 지웠습니다. `assets/reason.icns`는 macOS 팝업 아이콘으로 코드가 쓰고 있어 남겼습니다.

## 4. 검증 (Windows 11, rustc 1.94.0, `rn-fork/`에서)

| 명령 | 결과 |
|---|---|
| `cargo build --locked` | 성공, 경고 0건. `Cargo.lock`은 이 패키지 이름만 바뀜 |
| `cargo test` (격리 환경, `TELOS_RN_HOME` 포함) | 단위 시험 179개 통과(RN-03의 178개 + 상태 분리 시험 `the_state_is_its_own`) |
| `cargo clippy --locked --all-targets` | 경고 0건 |
| 실행 확인 (격리된 `USERPROFILE`·`HOME`·`APPDATA`·`LOCALAPPDATA`) | `telos-rn --version` → `telos-rn 0.12.11`. 도움말에 `proof`·`endpoint`·`upgrade` 없음. `telos-rn team`이 Daemon을 띄우고 "not in a team yet"으로 답함. 상태는 `~/.telos-rn`(`network.db`, `machine.key`, `daemon.log`, `daemon.lock`)에만 생기고 `~/.reason`은 생기지 않음. 로그는 `listening on \\.\pipe\telos-rn-…` 한 줄이며 `relays.json`을 받지 않음. `telos-rn stop` 후 프로세스 없음 |
| 잔존 검색 | `src`에 `REASON_NETWORK_`, `TINCAN_`, `LEGACY_NAME`, `SITE_URL`, `.reason/network`, `reason network` 명령 표기 0건. `reason-network` 문자열은 Relay 목록 서명 형식 상수만 남음 |

**실행하지 않은 것:** `setup`(실제 에이전트 설정 변경), 팀 생성·가입(외부 네트워크), Python E2E.

## 5. 후속

| 항목 | 넘긴 곳 |
|---|---|
| Codex CLI 위치(`PATH` vs 확장 동봉 CLI) | PH0-RN-05 |
| 버전 명시 로컬 RPC, Windows Pipe DACL | PH0-RN-06 |
| 원본 E2E 이식·삭제, 외부 접속 0건 관찰(RN-POC-14) | PH0-RN-08 |

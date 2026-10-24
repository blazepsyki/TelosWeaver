# PH0-RN-00 범위·제거 정책 — 원본 조사와 처리표

| 항목 | 값 |
|---|---|
| 서브 태스크·담당 | PH0-RN-00 · `claude-opus-5.5/session-c9336d2c` |
| 작성일 | 2026-10-25 |
| Branch / Worktree | `ph0/rn-00` / `D:\worktree\TelosWeaver\ph0-rn` |
| 기준 | Spec v1.5 DEC-07·§12.1~12.5·§19.5.10, 배정표 §6 "RN 범위" |
| 조사 대상 | `reason-machines/reason-network` `main` `540b690`(2026-10-24). 최신 릴리스 태그 v0.12.11 = `b3557ff`(그 뒤 커밋 2개: 재연결 시험, README 라이선스 링크) |
| 원본 규모 | Rust 약 19,500줄(`src/` 41개 + `src/daemon/` 8개 + `src/setup/` 1개), TypeScript 플러그인 2개, Python E2E 시험 9개. 라이선스 `MIT OR Apache-2.0` + `NOTICE` |

## 1. 방법과 한계

- 원본을 저장소 밖 임시 폴더에 내려받아 **읽기만** 했습니다. 빌드·설치·실행·설치 스크립트 실행은 하지 않았습니다.
- 근거는 소스의 모듈 설명, 외부 URL·프로세스 실행·환경 변수·설정 항목 전수 검색, 호출 지점 추적입니다. 아래 줄 번호는 `540b690` 기준입니다.
- 정적 조사이므로 "이 경로가 실제로 실행되지 않는다"는 증명이 아닙니다. 제거 결과는 PH0-RN-08에서 빌드 의존성·실행 관찰(파일·프로세스·외부 통신)로 다시 검증합니다(RN-POC-13·14).

## 2. 핵심 결론

1. **Reason 계정 로그인은 RN이 아니라 클로즈드 소스 `reason` CLI가 합니다.** RN은 CLI가 받아 온 서명 증명(JWS)을 `proof` 명령으로 넘겨받아, 내장된 Reason 공개 키로 **오프라인 검증만** 합니다(`proof.rs`). 검증 중 Reason 서버를 호출하지 않습니다.
2. **로컬 동작은 로그인에 기대지 않습니다.** 원본도 로그인 없이 설치·동작하는 경로가 있고(`main.rs:78`), 그 경우 팀원 이름이 "미검증"으로 표시될 뿐입니다. 팀 가입도 증명 없이 네 단어 코드·링크로 됩니다.
3. **팀이 없으면 P2P 네트워크(iroh)를 시작하지 않습니다**(`daemon/mod.rs:58` `team_state().is_some()`일 때만 `start_net`). 따라서 Phase 0 로컬 전용 시험이 가능합니다.
4. 다만 **팀이 없어도 Reason 서버에 주기적으로 접속하는 경로가 하나 있습니다.** 중계 서버 목록을 `https://reasonmachines.com/network/relays.json`에서 받아 옵니다(`daemon/mod.rs:74`에서 항상 시작하는 `relays_loop`(`:367`) → `relays.rs` `curl`). 로그인·CLI·업데이트가 아니라 네트워크 기능이므로 유지 대상이지만, 로컬 전용 시험에서 외부 접속을 만들지 않도록 조정을 제안합니다(§5-2).
5. **`reason` CLI 연동과 자동 업데이트는 코드 경계가 뚜렷합니다.** `daemon/reason.rs`·`daemon/update.rs`는 파일 단위로 제거할 수 있고, 나머지는 `cfg.reason`·`managed()`·`installer()` 참조를 정리하면 됩니다.
6. **`ring` 의존성은 로그인 증명 검증(`proof.rs`)에서만 쓰입니다.** 로그인을 제거하면 함께 빠집니다.

## 3. 처리표

### 3.1 제거 — 클로즈드 소스 Reason CLI 연동 (PH0-RN-03)

| 위치 | 내용·부작용 | 처리 |
|---|---|---|
| `src/daemon/reason.rs` (269줄 전체) | `reason session list --json`을 몇 초마다 실행해 Reason 클라우드 세션을 목록에 넣고, `reason session send`로 전달. CLI 관리 모드에서 `reason network __relay`를 계속 띄움(`:52~77`). 래퍼 스크립트를 `sh`로 작성(`:213~224`) | 파일 삭제 |
| `src/daemon/mod.rs:67~70` | `cfg.reason`이면 `reason_loop`, `managed()`이면 `relay_loop` 시작 | 호출 삭제 |
| `src/daemon/mail.rs:41·328·346~347·542` | 전달 채널 `Command("reason", session send …)`, 실패 시 `reason` 전용 처리 | `reason` 분기 삭제(Codex 분기는 유지) |
| `src/daemon/sessions.rs:36` | `"reason"` 세션을 push 등급으로 분류 | `reason` 삭제 |
| `src/agent.rs:64~65·184` | 실행 중인 에이전트를 `reason`으로 판별·표시 | 삭제 |
| `src/main.rs:48~72·673`, `src/mcp.rs:422`, `src/migrate.rs:52~53`, `src/daemon/update.rs:32` | `managed()`: 실행 파일이 `~/.reason/bin` 아래면 "Reason CLI가 관리하는 설치"로 보고 이름·동작을 바꿈 | `managed()`와 분기 삭제 |
| `src/setup.rs:821~830·964~980·1055·1304~1330` | `reason` 에이전트 연결 토글, 서비스 환경에 `REASON_NETWORK_REASON_BIN` 주입, `reason` 에이전트 정의 | 삭제 |
| `src/config.rs` `reason` 필드 | Reason 세션 연동 설정 | 삭제(구 설정 파일의 키는 무시) |
| `src/paths.rs:136`, `src/render.rs:303` | `~/.reason` 경로, `reason` 에이전트 응답 문구 | 삭제·정리 |

검증: `reason` 실행 파일 탐색(`find_bin("reason")`)과 `~/.reason/bin` 참조가 0건, `reason` 세션이 목록·전달 대상에 나타나지 않음(RN-POC-13·14).

### 3.2 제거 — 자동 업데이트·받아서 실행하는 설치 (PH0-RN-02)

| 위치 | 내용·부작용 | 처리 |
|---|---|---|
| `src/daemon/update.rs` (125줄 전체) | 6시간마다 `curl`로 GitHub 최신 릴리스 확인 → `installer()`로 설치 스크립트 실행 → 실행 파일 교체 → 재시작 | 파일 삭제 |
| `src/daemon/mod.rs:75~76` | `update::allowed(cfg.auto_update)`이면 `update_loop` 시작 | 호출 삭제 |
| `src/main.rs:78~101` `install_lines()`·`installer()` | GitHub 릴리스의 `install.sh`/`install.ps1`을 받아 바로 실행(`curl … \| sh`, `irm … \| iex`) | 삭제 |
| `install_lines()` 사용처(초대 안내 `main.rs:571`, `wizard.rs:262`) | 팀원에게 원본 설치 명령을 한 줄로 안내 | 안내 문구를 TelosWeaver 배포 절차로 대체하거나 설치 명령 없이 링크만 안내 |
| `src/migrate.rs` (100줄 전체) | 원본의 이전 제품(`tincan`)에서 이전: 실행 파일 이름 교체, 시작 서비스 교체, 업데이트 관련 사본 유지 | 파일 삭제(`LEGACY_NAME` 포함) |
| `install.sh`, `install.ps1`, `tests/install_script.rs` | 원격 설치 스크립트와 그 시험 | 삭제 |
| `src/config.rs` `auto_update`, 환경 변수 `NO_UPDATE`·`REASON_NETWORK_BIN_DIR`·`REASON_NETWORK_NO_PROMPT` | 업데이트·설치 설정 | 삭제 |

검증: `curl`로 GitHub·설치 스크립트를 받는 경로 0건, 실행 파일 교체·재시작 경로 0건. 실행 중 외부 통신 관찰에서 GitHub 릴리스 조회가 없음(RN-POC-14).

### 3.3 제거 — Reason 계정 로그인 (PH0-RN-03)

| 위치 | 내용·부작용 | 처리 |
|---|---|---|
| `src/proof.rs` (440줄 전체) | Reason이 서명한 JWS 증명 검증, 내장 공개 키(`oidc.reasonmachines.com` 발급자), 시험용 키(`src/testdata/proof-*.pk8`) | 파일·시험 데이터 삭제 |
| `src/proto.rs:177~188` `Request::Proof`, `src/daemon/mod.rs:350`, `src/client.rs:48` | 로컬 API의 증명 첨부 요청 | 삭제 |
| `src/main.rs:177~226·622` `proof`·`endpoint` 명령 | CLI가 증명을 넘기는 진입점, 증명 대상(endpoint id) 출력 | 삭제 |
| `src/daemon/teamops.rs:183·222·266~267·384~404·684~697` | 증명을 가진 가입, 증명 첨부·갱신, "로그인은 링크로만 가입" 규칙 | 증명 경로 삭제. 가입은 코드·링크만 |
| `src/team.rs` `verified_people`·`has_proof`, `MachineInfo.verified`, `src/net.rs:86·655` | 이름의 "검증됨" 표시와 같은 계정 판별 | 삭제. 이름은 팀 안에서 미검증 |
| `Cargo.toml` `ring` | 증명 검증 전용 | 의존성 삭제 |

영향: 팀원 이름이 모두 미검증이 됩니다. 팀 가입 자체는 초대 코드·링크로 통제되며, 원본도 로그인 없는 가입을 허용합니다. Phase 0은 로컬 전용이라 시험에 영향이 없습니다.

### 3.4 유지 — Provider 세션 탐지·메시지 전달

| 위치 | 내용 | 비고 |
|---|---|---|
| `src/claude.rs` | `~/.claude/sessions/<pid>.json`으로 실행 중인 Claude Code 세션(이름·cwd·Busy/Idle) 탐지 | DEC-06과 같은 위험 수준으로 수용 |
| `src/codex.rs` | Codex Thread 기록 파일에서 Turn 시작·종료 판단 | 〃 |
| `src/doing.rs` | 세션 제목·할 일 목록을 에이전트 기록에서 읽음 | 〃 |
| `src/daemon/sessions.rs`, `src/agent.rs` | 세션 등록(MCP 연결·Hook·환경 변수·프로세스 계보), 에이전트 판별 | 〃 |
| `src/daemon/mail.rs` 전달 채널 | `ClaudeSocket`(`CLAUDE_CODE_MESSAGING_SOCKET`), `ClaudeDirect`, `Command`(`codex queue --thread`), `Hook`, `Pull` | `codex`는 **PATH에서 찾습니다.** DEC-06의 확장 CLI 탐지와 맞출지 PH0-RN-04·05에서 검토 |
| `src/hook.rs`, `src/opencode-plugin.ts`, `src/pi-extension.ts` | Hook 처리기와 opencode·pi 플러그인 | 유지 |

### 3.5 유지(명시적 적용) — MCP·Hook·설정 변경, 자동 서비스 등록은 제거

| 위치 | 내용 | 처리 |
|---|---|---|
| `src/setup.rs` 에이전트 연결 | claude·codex·grok·cursor·zcode·gemini·copilot·opencode·pi·dsh(`reason` 제외)의 MCP 서버·Hook 등록. 예: `~/.claude/settings.json`, Codex `config.toml`·`hooks.json`, `~/.cursor/mcp.json`·`hooks.json`, Gemini·Copilot 설정, opencode `opencode.json`. 처음 수정할 때 파일별 Backup을 남기고 `setup --remove`로 되돌림 | 유지. 사용자가 명시적으로 실행할 때만 적용(§12.3). Phase 0 시험은 사전 확인·Backup·Diff·복원 |
| `src/setup.rs:851~1145` 시작 서비스 등록 | macOS launchd, Linux systemd user·`loginctl enable-linger`, Windows `HKCU\…\Run`(`reg`) | **제거**(Spec v1.5 §12.3 "자동 서비스 등록은 사용하지 않음"). Daemon은 명시적으로 시작 |
| `src/setup/zcode.rs` | ZCode Desktop MCP 설정 | 유지 |
| `src/wizard.rs` | 터미널 안내형 설정(`init`·`join`·`invite`·`setup`) | 유지, 설치·업데이트·서비스 안내 문구 정리 |
| `src/mcp.rs` | 에이전트가 띄우는 stdio MCP 서버(`list_team`·`send_message`·`ask_user`·`check_inbox`) | 유지(`managed()` 분기만 삭제) |

### 3.6 유지 — 네트워크·Relay·팀·초대 (Phase 0 시험 범위 밖)

| 위치 | 내용 | 비고 |
|---|---|---|
| `src/net.rs` | 팀 기계 간 iroh QUIC(직접·홀펀칭·HTTPS 중계), 길이 접두 메시지 | 팀이 있을 때만 시작. 로그인 증명 필드만 정리(§3.3) |
| `src/relays.rs` | Reason 지역 중계 서버 목록을 `reasonmachines.com`에서 `curl`로 받아 가장 가까운 곳 선택. 팀 자체 중계 서버 지정 가능 | 유지. **팀이 없을 때도 조회**하므로 조정 제안(§5-2) |
| `src/team.rs`, `src/invite.rs`, `src/daemon/teamops.rs` | 서명된 팀 구성원 상태, 네 단어 초대 코드(서버 없이 iroh 임시 키), 생성·가입·초대·제거·탈퇴 | 유지(증명 경로만 삭제) |
| `src/daemon/directory.rs` | 주소 해석(`alice/api-cursor`, `repo:api`, `*`) | 유지 |
| 의존성 `iroh`, `iroh-mdns-address-lookup` | 네트워크 계층. 팀이 있으면 n0 주소 조회·중계 서버 사용 | 유지 |

### 3.7 유지 — 기타 기능

| 위치 | 내용 | 비고 |
|---|---|---|
| `src/daemon/ask.rs` | `ask_user` 팝업(macOS osascript, Windows 패널, Linux 알림) | 유지. Spec v1.5 제거 대상 아님. Phase 0 시험 범위 밖 |
| `src/daemon/notify.rs` | 데스크톱 알림(`notify`) | 〃 |
| `src/tray.rs` + macOS 의존성(`tray-icon`, `objc2*`) | 메뉴 막대 아이콘. 서비스로 실행될 때만 시작 | 〃. 서비스 등록 제거로 기본 실행되지 않음 |
| `src/files.rs` | 메시지 첨부 텍스트 파일(최대 10개·256KB), 수신함 저장 | 유지 |
| `src/git.rs` | 세션의 저장소·Branch 판별(`git` 실행) | 유지 |

### 3.8 유지·조정 — 저장소·IPC·이름 (PH0-RN-04·05·06)

| 위치 | 내용 | 처리 |
|---|---|---|
| `src/store.rs` | SQLite(메시지, 본 세션, 사람별 메일함), 첨부 파일 수신함 | 유지. Receipt 의미를 Spec §12.2와 대조(PH0-RN-05) |
| `src/ipc.rs`, `src/proto.rs`, `src/client.rs` | 로컬 API: 줄 단위 JSON, Unix Socket(`0600`) / Windows Named Pipe. 클라이언트가 필요하면 Daemon을 자동 시작 | 유지. Windows Pipe는 **클라이언트가 서버 PID를 확인**하지만 서버 쪽 접근 제어(DACL)는 별도 확인 필요(PH0-RN-06) |
| 로컬 API 요청 24종 | `Register SetStatus Who Send Inbox Unread Ask StopWaiting Delivered PushFailed Hook Attach CreateTeam Join Proof Invite Team Link Remove Leave SetRelay Held Settle Shutdown` | `Proof` 삭제. Spec §12.1 RPC(`health.get`·`providers.list`·`sessions.*`·`messages.*`)와 `telos_schema::rn` 계약에 맞춘 버전 명시 RPC는 PH0-RN-06에서 별도 정의 |
| 이름·경로 | 실행 파일 `reason-network`, 상태 `~/.reason/network`(`REASON_NETWORK_HOME`), 환경 변수 `REASON_NETWORK_*`, MCP 서버 이름 `reason-network`, `SITE_URL`·`REPO` | 실행 파일·상태 디렉터리·MCP 이름·환경 변수 접두를 TelosWeaver용으로 분리(가칭 `telos-rn`). 원본 설치와 공유·이관하지 않음(§12.5) |

## 4. 외부 접속 목록 (정리 후 기준)

| 대상 | 원본에서 쓰는 곳 | 정리 후 |
|---|---|---|
| 로컬 `reason` 실행 파일(클로즈드 소스) | `daemon/reason.rs`, `mail.rs` | **없음** |
| `github.com/reason-machines/reason-network/releases` | 업데이트 확인·설치 스크립트 | **없음** |
| 원격 설치 스크립트 실행(`curl \| sh`, `irm \| iex`) | 업데이트, 초대 안내 | **없음** |
| `oidc.reasonmachines.com` | 증명 공개 키의 출처(코드에 내장, 실행 중 호출 없음) | **없음**(증명 삭제) |
| `reasonmachines.com/network/relays.json` | 중계 서버 목록 | 유지. 팀이 있을 때만 조회하도록 조정 제안 |
| `reasonmachines.com/network/join/#…` | 팀 링크 문자열(접속 아님) | 유지. TelosWeaver용으로 바꿀지 PH0-RN-04에서 결정 |
| iroh 중계 서버·n0 주소 조회 | 팀 네트워크 | 유지(팀이 있을 때만) |

## 5. 후속 서브 태스크에 넘기는 경계와 제안

1. **원본 기준 (PH0-RN-01):** 릴리스 태그 v0.12.11(`b3557ff`)을 기준으로 `rn-fork/`에 subtree(squash) 도입을 제안합니다. 원본 `Cargo.lock`은 수정 전 빌드 재현을 위해 그대로 고정합니다. RN Fork는 TelosWeaver 루트 Workspace(`crates/*`)에 넣지 않고 독립 Workspace로 둡니다(Spec §3.4, §19.5.10: RN Fork의 Cargo.toml·Lock 변경은 INTEGRATION).
2. **중계 서버 목록 조회 (PH0-RN-04):** 팀이 없을 때는 `reasonmachines.com`에 접속하지 않도록 `relays_loop`를 팀 상태에 묶을 것을 제안합니다. 네트워크 기능 자체는 유지합니다. Phase 0 로컬 시험에서 외부 접속이 0건이어야 RN-POC-14를 깨끗하게 판정할 수 있습니다.
3. **자동 서비스 등록 제거 (PH0-RN-02):** Spec v1.5 §12.3에 따라 시작 서비스 등록 코드를 업데이트 제거와 함께 정리합니다. Daemon은 명시적 시작과 클라이언트의 자동 시작(`client.rs`)만 남습니다.
4. **Codex CLI 위치 (PH0-RN-04·05):** `codex queue`는 PATH의 `codex`를 씁니다. TelosWeaver는 확장이 포함한 Codex CLI(DEC-06)를 쓰므로, 같은 실행 파일을 쓰도록 맞출지 검토합니다.
5. **Windows Pipe 접근 제어 (PH0-RN-06):** 서버 쪽 DACL을 확인하고, 필요하면 Core Local API와 같은 현재 사용자 전용 규칙을 적용합니다.
6. **RPC 계약 (PH0-RN-06, CONTRACT):** 원본 로컬 API 24종과 Spec §12.1·`telos_schema::rn`의 차이를 정리해, 버전이 명시된 RPC를 정의합니다. Core 소유 Managed Run의 등록 방식은 PH0-RN-05~07에서 필요성을 확인한 뒤 CONTRACT가 계약을 먼저 정의합니다(§12.5).
7. **파일 소유권:** PH0-RN 전체를 같은 담당자가 순차로 진행하므로 `rn-fork/**` 단일 Writer입니다. 병렬 Writer는 두지 않습니다.

## 6. 미확정 항목

| 항목 | 상태 |
|---|---|
| 팀 링크 도메인(`reasonmachines.com/network/join`) 유지·교체 | PH0-RN-04에서 결정. 접속이 아니라 문자열이며 Phase 0 시험에 영향 없음 |
| 팝업·알림·트레이의 Phase 0 이후 사용 | Spec v1.5에서 유지로 정함. 쓸지는 P0 UI 설계에서 판단 |
| 정리 후 의존성 최종 목록 | PH0-RN-02~04 결과로 확정 |

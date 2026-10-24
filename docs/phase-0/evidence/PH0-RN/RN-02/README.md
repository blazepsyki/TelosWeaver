# PH0-RN-02 업데이트·설치 제거

| 항목 | 값 |
|---|---|
| 서브 태스크·담당 | PH0-RN-02 · `claude-opus-5.5/session-c9336d2c` |
| 작성일 | 2026-10-25 |
| Branch / Worktree | `ph0/rn-02` / `D:\worktree\TelosWeaver\ph0-rn` |
| 기준 | PH0-RN-00 처리표 §3.2·§3.5·§5-2~3, Spec v1.5 DEC-07·§12.3 |

## 1. 제거한 것

| 대상 | 내용 | 파일 |
|---|---|---|
| 자동 업데이트 | Daemon의 `update_loop`(GitHub Release 확인 → 설치 스크립트 실행 → 재시작)와 `auto_update` 설정 | `src/daemon/update.rs` 삭제, `src/daemon/mod.rs`, `src/config.rs` |
| `upgrade` 명령 | `installer()`로 설치 스크립트를 받아 실행하던 명령 | `src/main.rs` |
| 자기 교체·이전 | 시작 시 `migrate::as_ours()`, Daemon의 `migrate::run`(tincan 설치를 이 실행 파일로 옮기고 서비스 재등록) | `src/migrate.rs` 삭제, `src/main.rs`, `src/daemon/mod.rs`, `src/setup.rs`(`legacy_wiring`), `src/util.rs`(`from_source`) |
| 설치 스크립트 | `curl … \| sh`, `irm … \| iex` 설치 경로와 그 시험 | `install.sh`, `install.ps1`, `tests/install_script.rs` 삭제, `src/main.rs`(`install_lines`, `installer`, `REPO`) |
| 시작 서비스 등록 | macOS launchd, Linux systemd `--user`, Windows `HKCU\…\Run`(`reg add`) 등록·시작·제거 | `src/setup.rs`, `src/client.rs`, `src/daemon/mod.rs`, `src/wizard.rs` |

## 2. 바뀐 동작

- **Daemon 시작:** 클라이언트 명령이 필요할 때 `start_daemon`으로 직접 띄우는 경로만 남았습니다. 재시작(`restart_soon`) 후에는 다음 클라이언트 명령이 새 Daemon을 띄웁니다.
- **`setup`:** `--no-service` 플래그와 서비스 단계가 없어졌습니다. 에이전트 MCP·Hook 연결만 합니다. `setup --remove`도 에이전트 연결만 되돌립니다.
- **`doctor`:** "login service" 줄이 없어졌습니다.
- **안내형 설정(wizard):** "백그라운드에서 계속 실행" 질문, macOS 메뉴 막대 질문, "Runs in the background, from login" 안내를 제거했습니다.
- **초대 출력:** `invite`와 wizard의 초대 코드 안내에서 설치 명령을 빼고 코드와 `join` 명령만 보여 줍니다. 팀 링크(`link`) 출력은 그대로입니다.
- **Tray(macOS):** `daemon --service`를 직접 실행했을 때만 뜹니다. 서비스가 등록되어 있는지는 더 이상 보지 않습니다.
- **구버전 피어 거절 문구:** `reason update` 안내를 뺐습니다.
- **설정 호환:** 기존 `config.toml`의 `auto_update` 키는 읽을 때 무시됩니다(`#[serde(default)]`, 알 수 없는 키 거부 없음).

`LEGACY_NAME`(tincan)의 상태 이동(`paths::migrate`)과 에이전트 설정의 tincan 항목 정리는 이름 변경과 함께 PH0-RN-04에서 다룹니다.

## 3. 검증 (Windows 11, rustc 1.94.0, `rn-fork/`에서)

| 명령 | 결과 |
|---|---|
| `cargo build` | 성공, 경고 0건 |
| `cargo test` (격리 환경, RN-01과 같은 방식) | 단위 시험 190개 통과. RN-01의 191개에서 삭제한 `update.rs`의 시험 1개가 빠짐 |
| `cargo clippy --all-targets` | 경고 0건 |
| `cargo clippy --target x86_64-unknown-linux-gnu` | 실행 불가(`ring` 빌드에 Linux C 컴파일러 필요). 대신 삭제한 코드가 부르던 함수가 모두 플랫폼 공통 코드에서도 쓰이는지 확인 |
| 잔존 검색 | `src`·`tests`의 Rust 코드에 `releases/`, `irm `, `launchctl`, `systemctl`, `LaunchAgents`, `CurrentVersion\Run`, `"reg"`, `update_loop`, `install_service` 없음. 남은 `curl`은 `relays.rs`의 Relay 목록(JSON 데이터) 조회와 `net.rs` 주석뿐이며 코드를 받아 실행하지 않음 |
| `Cargo.toml`·`Cargo.lock` | 변경 없음(제거한 코드 전용 의존성 없음) |

**실행하지 않은 것:** `setup`(에이전트 설정 변경), Daemon, Python E2E 시험. RN-01과 같습니다.

## 4. 후속으로 넘긴 것

| 항목 | 넘긴 곳 |
|---|---|
| `init`·`join`이 `--no-setup` 없이 실행되면 에이전트 MCP·Hook을 자동으로 연결함(`setup::run`·`setup::quietly`). Spec §12.3의 "사용자가 명시적으로 실행할 때만 적용"과 어긋남 | PH0-RN-04 |
| 원본 `README.md`·`DESIGN.md`·`tests/e2e/*.py`의 설치 스크립트·서비스·`REASON_NETWORK_NO_SERVICE`/`NO_UPDATE` 언급 | PH0-RN-04(이름 변경 시 문서 재작성) |
| `relays_loop`가 팀이 없어도 `reasonmachines.com/network/relays.json`을 조회 | PH0-RN-04(RN-00 처리표대로) |

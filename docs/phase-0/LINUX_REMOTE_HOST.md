# Phase 0 Linux·Remote SSH 실행 Host 절차 (Psyki-NAS)

> **소유:** PH0-INTEGRATION (Spec §19.5.3)  
> **근거:** Spec v1.5.2 §19.1(Windows/Linux Build, VS Code 로컬과 단일 Remote SSH)·§19.5.8 G3, 사용자 결정 2026-10-25  
> **상태:** 서버 조사 완료(2026-10-25, §2). Toolchain(Rust 1.99.1·Node 24.22.1)과 Git 이메일 정렬, `~/TelosWeaver` Clone, 공통 검사 통과(`836fb11`, [INTEGRATION 증거](evidence/PH0-INTEGRATION/README.md) §9.2). 작업별 실환경 시험(§4.3)은 아직 하지 않았습니다.  
> **예비:** 같은 PC의 WSL2 `Ubuntu-26.04`는 [WSL 절차](WSL_LINUX_HOST.md)대로 예비로 둡니다. WSL 결과는 Linux 증거로만 쓰고 Remote SSH 증거로 쓰지 않습니다.

## 1. 결정 사항 (2026-10-25)

| 항목 | 결정 |
|---|---|
| 실행 Host | 사용자의 Ubuntu 서버 하나가 Linux 실행 Host와 단일 Remote SSH 실행 Host를 함께 맡습니다 |
| 접속 | Windows `~/.ssh/config`의 `Psyki-NAS`(사용자 `gth1919`, 키 `~/.ssh/psyki.pem`). `Psyki-NAS-backup`은 같은 서버의 다른 경로이며 증거에는 `Psyki-NAS`를 씁니다 |
| Host ID | `psyki-nas` (Windows 로컬은 `windows-local`). 증거·설정에서 Host를 섞지 않습니다 (SAFE-10, RN-POC-11) |
| 저장소 | 서버의 `~/TelosWeaver` (검증용 Clone, 대표 저장소 아님, 배정표를 고치지 않음) |
| Toolchain | Windows와 버전을 맞춥니다(사용자 결정): Rust 1.99.1(사용자가 Windows·서버 모두 올림, clippy 0.1.99·rustfmt 1.10.0, rustup 1.29.2), Node 24.22.1·npm 11.19.1(Windows를 24.14.0에서 올림) |
| Provider | 서버에 원격 설치된 세 VS Code 확장과 그 로그인 상태를 그대로 씁니다. Windows의 인증 파일·토큰을 복사하지 않습니다 |
| 실환경 시험 | 실제 계정으로 수행(사용자 허락). 가벼운 모델·서버의 일회용 임시 폴더, 서버의 기존 대화·설정은 건드리지 않음, 시험 세션은 병합 시점에 삭제(배정표 §6 "시험 세션 정리") |
| 자원 | 시간·자원 제한 없음(사용자). 사용자 단위 프로세스·사용자 전용 Socket만 쓰고 시스템 설정은 바꾸지 않습니다 |

## 2. 서버 조사 (2026-10-25, 읽기 전용)

| 항목 | 관찰 |
|---|---|
| OS | Ubuntu 26.04.1 LTS(resolute), 커널 7.0.0-38-generic, x86_64, hostname `psyki-nas-x86` |
| 자원 | 8 CPU, 메모리 31 GiB(가용 25 GiB), 홈 디스크 여유 405 GB / 467 GB |
| systemd | `degraded`(실패한 Unit이 있음. 시험에 쓰는 사용자 서비스와는 무관한지 시험 시 확인) |
| 실행 환경 | `XDG_RUNTIME_DIR=/run/user/1000`, `XDG_STATE_HOME` 미설정(기본 `~/.local/state`). TelosWeaver·telos-rn 상태 폴더 없음 |
| 패키지 | `build-essential` 12.12ubuntu2.26.04.2(gcc 15.2.0), `git` 2.53.0, `curl` 8.18.0. `pkg-config`·`libssl-dev` 없음 — OpenSSL 의존성은 없어 `libssl-dev`는 필요 없음. `pkg-config`는 RN Fork 첫 Linux 빌드에서 필요한지 확인 |
| Rust | rustup 1.28.2, `stable` = rustc·cargo 1.93.0, clippy 0.1.93, rustfmt 1.8.0. 당시 Windows(1.94.0)와 다름 → 사용자가 양쪽을 1.99.1로 올림(§3) |
| Node | nvm 0.40.3, 기본 v24.22.1, npm 11.19.1. Windows를 같은 버전으로 올려 맞춤(winget `OpenJS.NodeJS.LTS` 24.14.0 → 24.22.1, npm 11.9.0 → 11.19.1, Extension 시험 29개 재통과) |
| Git 전역 설정 | `user.name=blazepsyki`. `user.email`이 개인 주소였으나 사용자 요청으로 전역 값을 GitHub noreply 주소(`22346903+blazepsyki@users.noreply.github.com`)로 바꿈 |
| 원격 확장 | `anthropic.claude-code` 2.1.309·2.1.310, `openai.chatgpt` 26.1007.21434·26.1021.22027, `google.google-antigravity` 1.9.0(`extensions.json`에 세 개). 버전이 두 벌인 확장은 §7.5의 활성 버전 선택 규칙 시험 대상입니다 |
| CLI | Claude `…/anthropic.claude-code-2.1.310-linux-x64/resources/native-binary/claude` 2.1.310(이전 2.1.309도 있음), Codex `…/openai.chatgpt-26.1021.22027-linux-x64/bin/linux-x86_64/codex` codex-cli 0.164.0-alpha.11.1(이전 0.162.0-alpha.17.2도 있음), Antigravity `~/.gemini/bin/agy` 1.4.7. `PATH`에 별도 설치된 `codex`·`claude`·`agy` 없음 |
| 로그인 흔적 | `~/.claude`·`~/.codex`·`~/.gemini` 있음(내용은 읽지 않음) |

CLI 경로 패턴은 2026-10-24 Windows 관찰(Spec §7.5)과 같은 구조이며, Linux 패키지 접미사는 `linux-x64`·`bin/linux-x86_64`입니다.

## 3. 남은 구성

| 단계 | 내용 | 수행 |
|---|---|---|
| Clone | 완료(2026-10-25, `836fb11`): `git clone https://github.com/blazepsyki/TelosWeaver ~/TelosWeaver`, `core.autocrlf=false`(이메일은 전역 noreply 값) | INTEGRATION |
| Rust | 완료: Windows·서버 모두 `stable` 1.99.1(rustc·cargo 1.99.1, clippy 0.1.99, rustfmt 1.10.0, rustup 1.29.2) | 사용자 |
| Node | 완료: 서버 기본 v24.22.1에 Windows를 맞춤 | 완료 |
| `pkg-config` | 불필요: RN Fork Linux 빌드가 시스템 `pkg-config` 없이 성공 | — |

작업 Branch를 서버로 옮길 때는 Windows 대표 저장소에서 `git push Psyki-NAS:TelosWeaver <Branch>`로 보내고, 서버의 Linux 전용 수정은 해당 작업 Branch에 커밋한 뒤 Windows에서 `git fetch Psyki-NAS:TelosWeaver <Branch>`로 가져옵니다. 서버 Clone에서 `main`에 통합하지 않습니다.

## 4. 검증 실행

### 4.1 환경 기록

```bash
echo "host_id=psyki-nas"; uname -r; . /etc/os-release && echo "$PRETTY_NAME"
rustc -V; cargo -V; node -v; npm -v; git -C ~/TelosWeaver rev-parse --short HEAD
```

### 4.2 공통 검사

```bash
cd ~/TelosWeaver
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all --check
(cd rn-fork && cargo build --locked && cargo clippy --locked --all-targets -- -D warnings)
(cd extension && npm ci && npm run build && npm test)
git status --short
```

### 4.3 작업별 실환경 시험

| 대상 | 서버에서 확인할 내용 | 담당 |
|---|---|---|
| Core Daemon | Linux Build, Unix Socket Local API(`${XDG_RUNTIME_DIR}/telos-weaver/<projectKey>.sock`), 토큰·Socket 사용자 전용 권한 | HOST |
| 공용 런타임 | Process Group 기반 취소·자식 회수, 구독 전 종료 | HOST |
| 확장 탐지 | `~/.vscode-server/extensions`에서 활성 버전 선택(두 벌 설치), `~/.gemini/bin/agy` | HOST·각 Provider |
| Provider | 각 CLI의 신규 Managed 실행·스트림·완료·실패·취소(G1·G2 실환경 시험의 Linux 실행) | CLAUDE·CODEX·AGY·INTEGRATION |
| RN Fork | Linux Build(RN-06에서 고친 Unix 결함 포함), Socket·토큰 0600, Health/Discovery/Message RPC, Core 연결(`phase0_rn`) | RN |
| Extension | Remote-SSH 창에서 Core 시작·연결·Health, 창 종료 후 Core 수명 분리 | EXT |

### 4.4 Remote SSH 시나리오 (§14.2)

1. Windows VS Code → Remote-SSH `Psyki-NAS` 접속. Extension은 원격 Extension Host에서 실행하고 Core·CLI·Workspace가 서버에 있는지 확인합니다.
2. Run 진행 중 SSH 단절: Windows에서 해당 `ssh` 프로세스를 끊거나 네트워크를 잠시 끊습니다.
3. 재접속 후 원격 Core의 실제 Run 상태를 조회하고 중복 실행이 없는지 확인합니다.
4. Host 유실은 서버의 Core 프로세스 강제 종료로 재현하며 SSH 단절과 구분해 기록합니다(서버 재부팅은 하지 않습니다).

## 5. 증거 기록 규칙

- 각 작업의 `evidence/<작업 ID>/README.md` 시험 표에 Host 열을 `psyki-nas`로 적고 Windows 결과와 행을 나눕니다.
- 실제 CLI·프로세스로 실행한 시험만 실환경으로 적습니다.
- 시험 세션 정리: 서버의 `~/.claude/projects/<시험 폴더>`, Codex는 App Server `thread/archive`, Antigravity는 `brain/<id>`·`conversations/<id>.db`. 세션 ID를 증거에 남깁니다.
- PH0-INTEGRATION은 G3 판정 때 Windows·Linux·Remote SSH 증거를 `evidence/PH0-INTEGRATION/README.md`에 모읍니다.

## 6. 한계

- 같은 LAN의 실제 장비입니다. 인터넷 구간의 지연·패킷 손실은 재현하지 않습니다.
- 사용자의 실제 계정·로그인 상태를 씁니다. 시험이 사용량을 쓰고, 서버의 Provider 대화 목록에 시험 세션이 잠시 생깁니다(병합 시점에 삭제).
- 서버에는 다른 용도의 서비스가 있을 수 있습니다. 시험은 사용자 단위 프로세스와 임시 폴더로 한정합니다.

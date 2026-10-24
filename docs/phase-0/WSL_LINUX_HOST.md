# Phase 0 WSL Linux·Remote SSH 실행 Host 절차

> **소유:** PH0-INTEGRATION (Spec §19.5.3)  
> **근거:** Spec §19.1(Windows/Linux Build, VS Code 로컬과 단일 Remote SSH), §19.5.8 G3  
> **상태:** 절차 수립. 2026-10-24 기준 이 PC에는 WSL 2.6.3(커널 6.6.87.2)만 있고 배포판은 설치되지 않았습니다. 아래 구성은 아직 실행하지 않았습니다.

## 1. 목적과 결정 사항

Phase 0 G3에 필요한 Linux 실행 Host와 단일 Remote SSH 실행 Host를 같은 PC의 WSL2로 확보합니다. 한 배포판이 두 역할을 함께 맡습니다.

| 항목 | 결정 |
|---|---|
| 배포판 | Ubuntu 26.04 LTS (`Ubuntu-26.04`) |
| Host ID | `wsl-ubuntu-2604` (Windows 로컬은 `windows-local`). 증거·설정에서 두 Host를 섞지 않습니다 (SAFE-10, RN-POC-11) |
| 작업 위치 | WSL 내부 ext4의 `~/src/TelosWeaver`. `/mnt/d/...`에서 빌드·실행하지 않습니다 |
| 저장소 지위 | 검증용 Clone입니다. 대표 저장소가 아니며 배정표를 수정하지 않습니다 |
| Remote SSH | WSL 안의 OpenSSH 서버, `127.0.0.1:2222`, 공개키 인증만 허용 |
| Toolchain | Windows와 같은 버전: rustc/cargo 1.94.0, Node 24.14.0, npm 11.9.0 |

`/mnt/d`(drvfs)를 쓰지 않는 이유: 파일 권한·대소문자·줄바꿈 동작이 Linux와 다르고, Unix Socket(`<stateDir>/core.sock`) 생성이 제한되며, 빌드가 크게 느립니다. 이 경로에서 얻은 결과는 Linux 증거로 쓰지 않습니다.

## 2. 최초 구성 (한 번만)

### 2.1 배포판 설치 (Windows PowerShell)

```powershell
wsl --install -d Ubuntu-26.04
wsl -l -v                      # VERSION 2인지 확인
```

설치 중 Linux 사용자 이름과 암호를 만듭니다. 이하 `<user>`로 표기합니다.

### 2.2 systemd 활성화 (WSL)

```bash
sudo tee /etc/wsl.conf >/dev/null <<'EOF'
[boot]
systemd=true
EOF
```

Windows에서 `wsl --shutdown` 후 다시 들어가 `systemctl is-system-running`이 `running` 또는 `degraded`인지 확인합니다.

### 2.3 기본 패키지·Git (WSL)

```bash
sudo apt update && sudo apt install -y build-essential pkg-config libssl-dev git curl openssh-server
git config --global user.name "Psyki"
git config --global user.email "22346903+blazepsyki@users.noreply.github.com"
git config --global core.autocrlf false   # 줄바꿈은 저장소의 .gitattributes가 정합니다
```

### 2.4 Rust (WSL)

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain 1.94.0 --component clippy,rustfmt
. "$HOME/.cargo/env"
rustc -V && cargo -V
```

### 2.5 Node.js (WSL)

```bash
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.3/install.sh | bash
. "$HOME/.nvm/nvm.sh"
nvm install 24.14.0 && nvm alias default 24.14.0
node -v && npm -v
```

### 2.6 SSH 서버 (WSL)

```bash
sudo tee /etc/ssh/sshd_config.d/10-telos.conf >/dev/null <<'EOF'
Port 2222
ListenAddress 127.0.0.1
PasswordAuthentication no
KbdInteractiveAuthentication no
PubkeyAuthentication yes
EOF
sudo systemctl daemon-reload
sudo systemctl restart ssh.socket ssh   # socket 활성화 사용 여부는 `systemctl status ssh.socket`으로 확인해 기록합니다
ss -ltn | grep 2222                     # 127.0.0.1:2222 대기 확인
```

### 2.7 SSH 키와 접속 설정 (Windows)

기존 `~/.ssh/config`와 키는 그대로 두고 전용 키와 Host 항목을 **추가**합니다.

```powershell
ssh-keygen -t ed25519 -f $HOME\.ssh\telos_wsl_ed25519 -C "telos-wsl"
Get-Content $HOME\.ssh\telos_wsl_ed25519.pub | wsl -d Ubuntu-26.04 -- sh -c "mkdir -p ~/.ssh && chmod 700 ~/.ssh && cat >> ~/.ssh/authorized_keys && chmod 600 ~/.ssh/authorized_keys"
Add-Content $HOME\.ssh\config "`nHost telos-wsl`n  HostName 127.0.0.1`n  Port 2222`n  User <user>`n  IdentityFile ~/.ssh/telos_wsl_ed25519`n  IdentitiesOnly yes"
ssh telos-wsl uname -a
```

WSL2의 기본 localhost 전달로 Windows의 `127.0.0.1:2222`가 WSL에 연결됩니다. 연결되지 않으면 `wsl --version`과 `%UserProfile%\.wslconfig`의 네트워크 모드를 확인해 증거에 기록합니다.

### 2.8 VS Code

- Windows VS Code에 **Remote - SSH** 확장을 설치하고 `telos-wsl`에 접속해 `~/src/TelosWeaver`를 엽니다.
- Remote - WSL 확장으로 연 창은 Linux 증거로만 쓰고 Remote SSH 증거로 쓰지 않습니다.

### 2.9 Provider CLI

Spec DEC-06·§7.5에 따라 Provider CLI는 실행 Host에 설치된 각 에이전트의 VS Code 확장에서 찾습니다. Windows 쪽 확장은 WSL 실행 Host의 CLI를 대신하지 않습니다.

1. Windows VS Code에서 Remote-SSH `telos-wsl`로 접속한 창에서 Codex(`openai.chatgpt`)·Claude Code(`anthropic.claude-code`)·Antigravity(`google.google-antigravity`) 확장을 **원격(SSH: telos-wsl)에 설치**합니다. 설치 위치는 WSL의 `~/.vscode-server/extensions`입니다.
2. 원격 창의 각 확장에서 로그인합니다. Windows의 인증 파일·토큰을 복사하지 않습니다 (Spec §13.1, §14.1).
3. 확장 폴더에서 실행 파일과 버전을 확인합니다. Antigravity는 확장이 `agy`를 내려받아 관리하므로 실제 위치를 찾아 기록합니다.

```bash
ls ~/.vscode-server/extensions | grep -E 'openai\.chatgpt|anthropic\.claude-code|google\.google-antigravity'
~/.vscode-server/extensions/openai.chatgpt-*/bin/linux-x86_64/codex --version
~/.vscode-server/extensions/anthropic.claude-code-*/resources/native-binary/claude --version
```

경로 패턴은 2026-10-24 Windows 확장에서 관찰한 것이며 Linux 패키지에서는 다를 수 있습니다. 실제 경로·확장 버전·CLI 버전은 해당 Provider 작업의 증거에 기록하고, 다르면 Spec §7.5 갱신을 PH0-INTEGRATION에 요청합니다.

## 3. 코드 동기화

원격 저장소가 없으므로 대표 저장소를 Git 원격으로 씁니다.

```bash
git config --global --add safe.directory /mnt/d/Downloads/Vibe_dev/TelosWeaver
mkdir -p ~/src && git clone /mnt/d/Downloads/Vibe_dev/TelosWeaver ~/src/TelosWeaver
```

검증할 때는 대상 Commit을 정확히 지정합니다.

```bash
cd ~/src/TelosWeaver && git fetch origin && git checkout --detach <대상 Commit> && git status --short
```

- Linux에서만 필요한 수정은 WSL Clone에서 해당 작업의 Branch(예: `ph0/host-linux`)에 커밋합니다. 담당자는 자기 작업의 독점 경로만 수정합니다.
- Windows에서 가져올 때: `git fetch \\wsl.localhost\Ubuntu-26.04\home\<user>\src\TelosWeaver <Branch>` (필요하면 Windows에서도 `safe.directory`에 이 경로를 추가합니다).
- WSL Clone에서 `main`에 직접 통합하지 않습니다. 통합은 대표 저장소의 PH0-INTEGRATION이 합니다.

## 4. 검증 실행

### 4.1 환경 기록

모든 증거에 아래 출력을 붙입니다.

```bash
echo "host_id=wsl-ubuntu-2604"; uname -r; . /etc/os-release && echo "$PRETTY_NAME"
rustc -V; cargo -V; node -v; npm -v; git -C ~/src/TelosWeaver rev-parse --short HEAD
```

Windows에서 `wsl --version`의 WSL·커널 버전도 함께 기록합니다.

### 4.2 공통 검사

```bash
cd ~/src/TelosWeaver
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked
cargo fmt --all --check
(cd extension && npm ci && npm run build)
git status --short    # 생성물 외 변경이 없어야 합니다
```

### 4.3 작업별 실환경 시험

| 대상 | WSL에서 확인할 내용 | 담당 |
|---|---|---|
| Core Daemon | Linux Build, `<stateDir>/core.sock` Local API, 사용자 전용 권한 | HOST |
| 공용 런타임 | Process Group 기반 취소·자식 회수, 구독 전 종료 | HOST |
| Provider | 각 CLI의 신규 Managed 실행·스트림·실패·취소 | CODEX·CLAUDE·AGY |
| RN Fork | Linux Build·설치·제거, Health/Discovery/Message RPC | RN |
| Extension | Remote-SSH 창에서 Core 탐지·시작·Health 연결, 창 종료 후 Core 수명 분리 | EXT |

### 4.4 Remote SSH 시나리오 (§14.2)

1. Windows VS Code → Remote-SSH `telos-wsl` 접속. Extension은 원격 Extension Host에서 실행하고 Core·CLI·Workspace는 WSL에 있는지 확인합니다.
2. Run 진행 중 SSH 단절: Windows에서 해당 `ssh` 프로세스를 종료하거나 WSL에서 `sudo systemctl stop ssh.socket ssh`를 실행합니다.
3. 재접속 후 원격 Core의 실제 Run 상태를 조회하고 중복 실행이 없는지 확인합니다.
4. Host 전체 유실은 `wsl --terminate Ubuntu-26.04`로 재현하며 SSH 단절과 구분해 기록합니다.

## 5. 증거 기록 규칙

- 각 작업의 `evidence/<작업 ID>/README.md` 시험 표에 OS/Host 열을 `wsl-ubuntu-2604`로 적고 Windows 결과와 행을 나눕니다.
- 실제 CLI·프로세스로 실행한 시험만 `environment-tested`로 적습니다.
- PH0-INTEGRATION은 G3 판정 시 Windows·WSL(Linux)·Remote SSH 증거를 모아 `evidence/PH0-INTEGRATION/README.md`에 정리합니다.

## 6. 한계

- WSL2는 같은 물리 장비의 가상 머신입니다. 네트워크는 Loopback이며 실제 원격 지연·패킷 손실은 재현하지 않습니다.
- Windows와 시계·전원·디스크를 공유합니다. 장비 단위 장애는 시험할 수 없습니다.
- WSL은 Windows 파일(`/mnt/*`)에 접근할 수 있습니다. 이 경로를 Workspace로 쓰지 않아야 하며, Host 경계 시험(경로 혼동 차단)에서는 의도적인 반례로만 사용합니다.
- 이 한계로 판정이 달라지는 시험은 해당 증거에 "WSL 한계"로 표시하고 G3 판정에서 별도로 보고합니다.

## 7. 유지·정리

- Toolchain 버전을 바꾸면 Windows와 WSL을 함께 맞추고 이 문서 §1을 갱신합니다.
- 배포판 백업: `wsl --export Ubuntu-26.04 D:\worktree\backup\ubuntu-2604.tar` (Syncthing 대상 밖에 둡니다).
- 다시 구성할 때는 §2부터 반복합니다.

# PH0-EXT Linux Core 연결 검증 및 Remote-SSH 후속 절차

## 1. 담당과 범위

| 항목 | 값 |
|---|---|
| 담당 | `Codex / PH0-EXT-LINUX-cli` |
| 실행일 | 2026-10-25, KST; 정리 확인 시각 `2026-10-25T22:08:21+09:00` |
| 배정 | [G3 Linux 지시서](../PH0-INTEGRATION/G3_LINUX_ASSIGNMENTS.md) §1·§4, 이번 실행의 사용자 지시 |
| 기준 | `94e1369a5808a3f248c0974dc1cfedc73dc51caf`, Spec v1.5.2 §19.1·§19.5, 계약 `7497877` |
| Branch | `ph0/ext-linux` |
| Windows Worktree | `D:\worktree\TelosWeaver\ph0-ext-linux` |
| 서버 Worktree | `/home/gth1919/TelosWeaver-wt/ext-linux` |
| 시험 보강 Commit | `86da91dede849abf5b2ecfa8abe3891bc035a0e9` |
| 실제 변경 파일 | `extension/test/live-core.cjs`, 이 문서 |

E1은 Linux에서 Extension의 `CoreConnection`·`CoreClient` 구현과 실제 Rust Core를 연결하는 시험입니다. VS Code API와 Remote-SSH Extension Host는 사용하지 않습니다. E2는 이번 비대화형 실행에서 사용자에게 조작을 요청할 수 없으므로 **미실행: 사용자 VS Code 필요 (`not run: needs the user`)**입니다. E1 결과로 E2나 G3 전체 통과를 선언하지 않습니다.

배정표 착수·완료 기록은 이번 사용자 지시에 따라 INTEGRATION이 담당합니다. `WORK_ALLOCATION.md`는 수정하지 않았습니다. 공용 서버 Clone의 `master` checkout·파일, 다른 작업자의 Worktree, GitHub `origin`, 사용자 설정·로그인 상태는 수정하지 않았습니다. 서버 공유 Git 저장소에 이번 작업 Branch를 전달하고 전용 Worktree를 추가했습니다.

## 2. 확인한 실행 환경

SSH 접속은 Windows에서 `ssh -o BatchMode=yes Psyki-NAS`로 수행했습니다. 아래 값은 이번 실행에서 서버의 전용 Worktree에서 직접 확인했습니다.

| 항목 | 실제 값 |
|---|---|
| Host ID / hostname | `psyki-nas` / `psyki-nas-x86` |
| 사용자 | `gth1919` |
| OS / 커널 | Ubuntu 26.04.1 LTS / `7.0.0-38-generic` |
| rustc | `rustc 1.99.1 (58cf8f9c3 2026-10-13)` |
| cargo | `cargo 1.99.1 (5f94df478 2026-08-27)` |
| Node / npm | `v24.22.1` / `11.19.1` |
| Core | `/home/gth1919/TelosWeaver-wt/ext-linux/target/debug/telos-weaver`, `telos-weaver 0.0.0` |
| Extension | 저장소 `extension/`, package version `0.0.0`, 생성 계약 50개 |
| 초기 Commit | `94e1369a5808a3f248c0974dc1cfedc73dc51caf` |
| 보강 시험 Commit | `86da91dede849abf5b2ecfa8abe3891bc035a0e9`; Rust 소스 변경 없음 |

각 SSH 시험 명령은 다음 환경을 로드했습니다. 아래 서버 명령은 별도 표시가 없으면 전용 Worktree에서 실행한 것입니다.

```bash
. ~/.cargo/env
export NVM_DIR=$HOME/.nvm
. $NVM_DIR/nvm.sh
cd ~/TelosWeaver-wt/ext-linux
printf 'host_id=psyki-nas\n'
hostname
uname -r
. /etc/os-release
printf '%s\n' "$PRETTY_NAME"
rustc -V
cargo -V
node -v
npm -v
git rev-parse HEAD
```

## 3. E1 실행 및 결과

### 3.1 명령

최초 전송·Worktree 생성은 Windows에서 다음과 같이 실행했습니다.

```powershell
git push Psyki-NAS:TelosWeaver HEAD:ph0/ext-linux
ssh -o BatchMode=yes Psyki-NAS 'git -C ~/TelosWeaver worktree add ~/TelosWeaver-wt/ext-linux ph0/ext-linux'
```

서버에서 다음을 실행했습니다.

```bash
cargo build -p telos-core --bin telos-weaver --locked
cd extension
npm ci --ignore-scripts
npm test
TELOS_CORE_EXECUTABLE="$HOME/TelosWeaver-wt/ext-linux/target/debug/telos-weaver" \
  node --test test/live-core.cjs
```

`npm test`는 타입 생성·TypeScript 빌드 후 `test/*.test.cjs` 29개를 실행합니다. 실제 Core 시험은 별도 명령으로 실행했으며, `TELOS_CORE_EXECUTABLE`이 설정되어 있어 skip되지 않았습니다. `npm ci`는 19개 패키지를 설치했고 audit 결과 취약점 0개를 보고했습니다. 시스템 패키지는 설치하지 않았습니다.

기존 E1 통과 후 시험에 실제 Socket·토큰 권한 및 소유자, Core의 토큰 교체, 잘못된 토큰 거부 검사를 추가했습니다. Linux에서 이 보강 시험을 다시 실행했습니다. Windows에서는 `node --check extension/test/live-core.cjs`와 `git diff --check`를 실행했습니다.

### 3.2 결과표

| 항목 | Host | 구분 | 결과 | 확인 근거 |
|---|---|---|---|---|
| Core Linux Build | `psyki-nas` | 실제 빌드 | 통과 | `cargo build` exit 0, dev profile 완료, 약 18.69초 |
| Extension 생성·빌드 | `psyki-nas` | 계약 생성·컴파일 | 통과 | `npm test` 안의 `npm run build`, 계약 50개 생성·tsc 완료 |
| Extension 시험 29개 | `psyki-nas` | Socket 대역·UI 대역 | 통과 | pass 29 / fail 0 / skipped 0, 약 10.82초; 실제 Provider·VS Code 시험 아님 |
| E1 실행 파일 확인·명시적 시작 | `psyki-nas` | 실제 Core | 통과 | 절대 경로·`--version` 검증을 거쳐 `CoreConnection.connect(..., true)`가 Core 하나 시작 |
| E1 Unix Socket·토큰 | `psyki-nas` | 실제 Core | 통과 | Socket 타입, Socket·토큰 `0600`, 각 부모 디렉터리 `0700`, 현재 UID 소유, Core가 fixture 토큰 교체 |
| E1 인증 성공·실패 | `psyki-nas` | 실제 Core | 통과 | `core.hello` 성공·Project ID 일치; 토큰 한 글자를 바꾼 별도 연결은 `unauthenticated` 거부 |
| E1 Health·RN 미설정 | `psyki-nas` | 실제 Core | 통과 | `connected`, Health `degraded`, `rn.status=degraded`, detail에 `not configured` |
| E1 DenyRuns | `psyki-nas` | 실제 Core | 통과 | 정책 없이 시작; `runs.start` 요청을 `unavailable`로 거부; 모델 호출 없음 |
| E1 연결 해제 후 생존 | `psyki-nas` | 실제 Core | 통과 | 첫 연결 해제 뒤 `process.kill(pid, 0)` 성공 |
| E1 PID 재사용 | `psyki-nas` | 실제 Core | 통과 | 두 번째 `CoreConnection` 연결 성공, 시작 이벤트 총 1회 유지 |
| E1 정리 | `psyki-nas` | 실제 OS 확인 | 통과 | 시험이 시작한 PID 3개 모두 `/proc/<pid>` 없음; 보강 시험 임시 폴더 없음 |
| E2 Remote-SSH Extension Host | `psyki-nas` | 사용자 VS Code 필요 | 미실행 | `not run: needs the user`; §4에 후속 절차 |
| E2 격리 자동 Host 시험 | `psyki-nas` | 데스크톱 VS Code 필요 | 미실행 | `scripts/test-host.cjs`를 실행하지 않음; 아래 환경 확인 참조 |
| E3 증거 | `psyki-nas` | 문서 | 작성 완료 | 이 문서와 최종 인계 Commit |

보강 시험의 실제 출력 일부입니다. 토큰 값은 출력하지 않았습니다.

```text
86da91dede849abf5b2ecfa8abe3891bc035a0e9
Unix Socket/token 0600; directories 0700; current UID owns IPC
Temporary project: /tmp/telos-ext-NBdzyK/project; invalid token rejected
Core PID 2879147: authenticated, degraded, Run denied, retained after detach, reused
✔ real Core startup, DenyRuns Health, detach and reattach
tests 1 / pass 1 / fail 0 / cancelled 0 / skipped 0
duration_ms 392.241699
```

시험의 `environment()`는 `/tmp/telos-ext-<무작위>/project`와 형제 `state`를 만들고 `XDG_STATE_HOME`·`XDG_RUNTIME_DIR`·`LOCALAPPDATA`를 이 `state`로 제한합니다. Socket은 `<state>/telos-weaver/<projectKey>.sock`, 토큰은 `<state>/telos-weaver/<projectKey>/api-token`에 생깁니다. 따라서 기본 `/run/user/1000` 경로 자체의 검증은 이 시험 범위가 아닙니다. 토큰은 프로젝트 밖에 있습니다. 시험은 자신이 생성한 Core에만 종료 신호를 보내고 자신이 만든 임시 폴더를 삭제합니다.

물리적 실행 Host는 `psyki-nas`이며, 이 기존 CLI 시험은 별도 `hostId` 옵션 없이 Unix 기본값 `unix-local`로 시작합니다(구현 확인). 따라서 E1은 원격 Extension Host의 `psyki-nas` 설정·UI 표시를 검증한 결과가 아닙니다. E2에서는 `hostId=psyki-nas`를 명시해야 합니다.

### 3.3 전송 중 실패와 복구

시험 보강 Commit의 첫 push는 서버 전용 Worktree가 해당 Branch를 checkout하고 있어 `refusing to update checked out branch: refs/heads/ph0/ext-linux`로 거부됐습니다. 바로 다음에 실행된 reset·시험은 구버전 `94e1369`였으며 PID `2878478`의 기존 시험 통과로만 기록합니다. 이 실행을 보강 시험 통과로 취급하지 않습니다.

서버 설정을 바꾸지 않고 다음 순서로 복구했습니다. 후속 전송도 같은 절차를 사용합니다. 작업 트리가 clean일 때만 전환하며, 단계가 실패하면 다음 단계를 실행하지 않습니다.

```powershell
ssh -o BatchMode=yes Psyki-NAS 'cd ~/TelosWeaver-wt/ext-linux && test -z "$(git status --porcelain)" && git switch --detach'
# 위 명령이 성공한 경우에만 실행합니다.
git push Psyki-NAS:TelosWeaver HEAD:ph0/ext-linux
# push 성공을 확인한 뒤 실행합니다.
ssh -o BatchMode=yes Psyki-NAS 'git -C ~/TelosWeaver-wt/ext-linux switch ph0/ext-linux'
```

복구 후 HEAD `86da91d`를 확인하고 보강 시험을 실행했습니다. 공용 Clone·다른 Worktree·수신 설정은 수정하지 않았습니다.

## 4. E2: 사용자 VS Code에서 실행할 후속 절차

**상태: 미실행: 사용자 VS Code 필요 (`not run: needs the user`). 아래는 실행 결과가 아닌 후속 절차입니다.** 이번 실행에서는 VS Code를 열거나 설정·프로필·확장 설치를 변경하지 않았습니다.

서버에서 `command -v code`, `command -v code-insiders`는 모두 경로를 반환하지 않았고, `DISPLAY`·`WAYLAND_DISPLAY`는 비어 있었습니다. `/usr/share/code`, `/opt/visual-studio-code`, `/snap/code/current`도 확인되지 않았습니다. 이는 이번 SSH 환경에서 데스크톱 VS Code 실행 환경을 확보하지 못했다는 근거입니다. 서버 전체의 모든 설치 경로를 조사한 결과는 아닙니다. `scripts/test-host.cjs`에는 데스크톱 실행 파일을 가리키는 `TELOS_VSCODE_EXECUTABLE`이 필요하므로 실행하지 않았습니다. VS Code Server나 SSH CLI가 있다는 것만으로 데스크톱 자동 Host 시험·Remote-SSH UI 통과를 대신하지 않습니다.

다음 절차는 사용자가 실행하며, 기존 설정은 값을 기록한 뒤 복구합니다. 실제 Provider나 기존 대화를 사용할 필요가 없습니다.

1. Windows VS Code에서 **Remote-SSH: Connect to Host...** → **Psyki-NAS**를 선택합니다. 왼쪽 아래 원격 표시가 `SSH: Psyki-NAS`인지 확인합니다. **Open Folder...**로 `/home/gth1919/TelosWeaver-wt/ext-linux/extension`을 엽니다. 신뢰할 수 있는 이번 저장소 창에서만 Workspace Trust를 허용합니다.

2. 그 창의 원격 터미널에서 아래를 실행합니다. `remote-e2`가 이미 있으면 이전 시험을 재사용하지 말고 중단해 경로를 새로 정합니다. 아래 고정 경로를 바꿨다면 이후 launch 설정·명령에도 같은 절대 경로를 씁니다.

   ```bash
   . ~/.cargo/env
   export NVM_DIR=$HOME/.nvm
   . $NVM_DIR/nvm.sh
   cd ~/TelosWeaver-wt/ext-linux
   hostname
   git branch --show-current
   git rev-parse HEAD
   cargo build -p telos-core --bin telos-weaver --locked
   cd extension
   npm ci --ignore-scripts
   npm run build
   cd ..
   mkdir -p .telos-weaver
   test ! -e .telos-weaver/remote-e2 && \
     mkdir .telos-weaver/remote-e2 && \
     mkdir .telos-weaver/remote-e2/project .telos-weaver/remote-e2/state
   ```

3. 원격 저장소의 `extension/.vscode/launch.json`에서 기존 구성은 유지하고 아래 임시 구성을 하나 추가합니다. 이번 실행이 추가한 파일은 아니며, 사용자 시험이 끝나면 추가한 구성만 지웁니다. `args`가 일회용 프로젝트를 열고 `env`가 시험 상태·Socket을 전용 경로로 제한합니다.

   ```json
   {
     "name": "TelosWeaver Remote E2",
     "type": "extensionHost",
     "request": "launch",
     "runtimeExecutable": "${execPath}",
     "args": [
       "--extensionDevelopmentPath=${workspaceFolder}",
       "/home/gth1919/TelosWeaver-wt/ext-linux/.telos-weaver/remote-e2/project"
     ],
     "env": {
       "XDG_STATE_HOME": "/home/gth1919/TelosWeaver-wt/ext-linux/.telos-weaver/remote-e2/state",
       "XDG_RUNTIME_DIR": "/home/gth1919/TelosWeaver-wt/ext-linux/.telos-weaver/remote-e2/state"
     },
     "outFiles": ["${workspaceFolder}/dist/**/*.js"],
     "preLaunchTask": "TelosWeaver: build extension"
   }
   ```

4. **Run and Debug**에서 **TelosWeaver Remote E2**를 선택하고 F5를 누릅니다. 열린 Extension Development Host 창도 `SSH: Psyki-NAS`인지 확인합니다. **Developer: Show Running Extensions**에서 TelosWeaver가 원격 Extension Host에서 실행되는지 확인하고 화면을 보관합니다. 로컬 Host로 표시되면 이후 진행을 멈추고 실패로 기록합니다. 활성화만으로 Core가 시작되지 않고 TelosWeaver 패널이 `미연결`인지 확인합니다.

5. 개발 Host에서 **Preferences: Open Remote Settings (JSON)**을 엽니다. 다음 세 키의 기존 값·존재 여부를 따로 기록한 뒤 시험 중에만 아래 값을 적용합니다. `corePath`·`hostId`는 machine 범위이므로 프로젝트 `.vscode/settings.json`에 넣지 않습니다. 시험 폴더의 Workspace 설정이 이 값을 덮어쓰지 않는지도 확인합니다.

   ```json
   {
     "telosWeaver.corePath": "/home/gth1919/TelosWeaver-wt/ext-linux/target/debug/telos-weaver",
     "telosWeaver.hostId": "psyki-nas",
     "telosWeaver.projectId": ""
   }
   ```

6. **TelosWeaver: Core 시작 / 연결**을 실행합니다. Explorer **TelosWeaver** 패널에서 `연결됨 (degraded)`, 서버의 임시 프로젝트 경로, Core `0.0.0`, `Host=psyki-nas`, `rn=degraded`를 확인합니다. **TelosWeaver: 출력 보기**, **상태 새로 고침**도 실행합니다. 이 화면과 Output을 보관하되 토큰 파일 내용은 열거나 복사하지 않습니다. 정책이 없고 RN도 구성하지 않았으므로 `degraded`가 기대 결과입니다.

7. 별도 Windows PowerShell에서 아래처럼 시험 프로젝트에 해당하는 Core 한 개만 조회합니다. 출력된 PID·PPID·SID·시작 시각·명령행을 기록합니다. 조회 결과가 0개 또는 2개 이상이면 정상 통과로 처리하지 않습니다.

   ```powershell
   ssh Psyki-NAS 'ps -eo pid,ppid,sid,lstart,args | grep "[t]elos-weaver serve --project-root /home/gth1919/TelosWeaver-wt/ext-linux/.telos-weaver/remote-e2/project"'
   ```

   원래 디버그 창의 서버 터미널에서 다음도 실행합니다. `<기록한 PID>`는 방금 확인한 숫자로 바꿉니다. XDG 환경 변수가 임시 경로로 전달됐는지, Socket `0600`·토큰 `0600`·관련 디렉터리 `0700`인지 확인합니다. 토큰 내용이나 전체 프로세스 환경은 출력하지 않습니다.

   ```bash
   tr '\0' '\n' < /proc/<기록한 PID>/environ | grep '^XDG_\(STATE_HOME\|RUNTIME_DIR\)='
   find /home/gth1919/TelosWeaver-wt/ext-linux/.telos-weaver/remote-e2/state \
     -maxdepth 4 -printf '%m %u %p\n'
   ```

8. 개발 Host에서 **TelosWeaver: 연결 해제 (Core 유지)**를 누릅니다. 같은 PID가 살아 있는지 위 명령으로 확인합니다. 이어 **TelosWeaver: Core 연결**을 실행하고 같은 PID로 재연결·Health 표시가 돌아오는지 확인합니다. 새 PID가 생기면 실패로 기록합니다.

9. Extension Development Host 창을 완전히 닫습니다. 디버그 세션이 끝난 뒤 별도 PowerShell에서 같은 조회를 반복하여 동일 PID·시작 시각의 Core가 남는지 확인합니다. 이것이 E1에서 수행하지 못한 실제 창·Extension Host 종료 후 생존 확인입니다. 재시험할 때도 동일 프로젝트의 기존 Core를 먼저 확인하여 불필요한 새 Core를 만들지 않습니다.

10. 결과에 Host·날짜·Commit, 원격 Host 실행 화면, 연결·Health 화면, 창 종료 전후 PID·시작 시각, 실제 상태·Socket 경로, 실패가 있다면 정확한 단계·메시지를 남깁니다. 시험 후 Remote Settings 세 키를 원래 값·존재 여부대로 복구하고, 추가한 launch 구성만 제거합니다. 시험 Core의 명령행을 다시 대조한 뒤 **해당 PID 하나에만** `kill -TERM <기록한 PID>`를 보냅니다. 종료 확인 전에는 상태 폴더를 지우지 않습니다. 임시 폴더는 증거 수집·INTEGRATION 병합 시점까지 보존하고 정리 대상 경로와 PID를 인계합니다.

`scripts/test-host.cjs`는 로컬 격리 데스크톱 Host 시험이며 그 자체로 Remote-SSH 시나리오를 구성하지 않습니다. 따라서 나중에 이 스크립트를 다른 환경에서 통과해도 위 원격 Host·창 종료 확인을 별도로 기록해야 합니다. Run 중 SSH 단절·재접속과 Core 강제 종료 시나리오는 [Linux 절차](../../LINUX_REMOTE_HOST.md) §4.4의 INTEGRATION 후속 범위입니다.

## 5. 생성 자원·정리·인계

| 자원 | 생성 위치 / 식별자 | 이번 실행 종료 시 상태 |
|---|---|---|
| 최초 E1 Core | `psyki-nas`, PID `2875810`, 기준 `94e1369` | 시험 teardown 종료, `/proc/2875810` 없음 |
| 전송 실패 뒤 구버전 E1 Core | `psyki-nas`, PID `2878478`, 기준 `94e1369` | 시험 teardown 종료, `/proc/2878478` 없음 |
| 보강 E1 Core | `psyki-nas`, PID `2879147`, 기준 `86da91d` | 시험 teardown 종료, `/proc/2879147` 없음 |
| 보강 E1 임시 폴더 | `/tmp/telos-ext-NBdzyK` | 시험 teardown 삭제, 존재하지 않음 확인 |
| 앞선 E1 임시 폴더 | `environment()`가 만든 `/tmp/telos-ext-*`; 구버전 시험은 경로를 출력하지 않음 | teardown exit 0; 개별 경로의 사후 존재 검사는 하지 못함 |
| 실제 Provider 세션·Thread | 생성 없음 | 모델·CLI 대화 호출 없음, 정리할 세션 ID 없음 |
| Windows·서버 Worktree | §1의 두 경로 | 보존, INTEGRATION 통합 후 정리 대상 |
| 서버 Build 산출물 | 전용 Worktree의 `target/`, `extension/node_modules`, `dist`, `src/generated` | ignored 산출물 보존 |

정리 확인 명령은 `for pid in 2875810 2878478 2879147; do ... /proc/$pid ...; done`과 `test ! -e /tmp/telos-ext-NBdzyK`였습니다. 시험이 생성한 자원 외에는 삭제하지 않았습니다. 실제 Provider 시험 세션 ID, RN 세션 ID, 대화 ID는 없습니다.

E1과 E3는 완료했으며 E2는 사용자 실행을 기다립니다. Rust 소스·공통 계약 변경은 없으므로 이번 담당 실행에서는 Workspace Rust test·clippy·fmt 전체 검사를 다시 수행하지 않았습니다. 최종 통합 검사는 INTEGRATION이 수행합니다. 남은 위험은 Remote-SSH에서의 실제 Extension 배치·machine 설정 적용·원격 디버그 환경 변수 전달·창 종료 후 수명이며, 모두 E2 후속 절차에서 확인해야 합니다. 이번 결과는 같은 LAN의 서버 하나에 대한 Linux CLI 시험이며 인터넷 구간 SSH 장애나 전체 UI 수용 결과로 확대하지 않습니다.

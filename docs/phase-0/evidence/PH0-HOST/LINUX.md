# PH0-HOST Linux 검증·인계

## 1. 담당과 범위

| 항목 | 값 |
|---|---|
| 담당자 | `Codex / PH0-HOST-LINUX-cli` |
| 일자 | 2026-10-25 |
| 배정 | [G3 Linux 작업 지시서](../PH0-INTEGRATION/G3_LINUX_ASSIGNMENTS.md) §1·§2, H1~H4 |
| 기준 | Spec v1.5.2 §7.3·§7.5·§19.1·§19.5, 계약 `7497877` |
| 착수 Commit | `94e1369a5808a3f248c0974dc1cfedc73dc51caf` |
| 시험 추가 Commit | `fe1b49d29a4f15f39f96b29a0444d39e035a6326` |
| 최종 시험 코드 Commit | `ba8df90551e0ba37a0fd607a304a002cdee59e8c` |
| Branch | `ph0/host-linux` |
| Windows Worktree | `D:\worktree\TelosWeaver\ph0-host-linux` |
| Linux Worktree | `/home/gth1919/TelosWeaver-wt/host-linux` |

H1~H3 검증과 H4 증거 작성을 완료했습니다. 제품 코드·Spec·계약 변경은 없습니다. HOST 독점 시험 경로에 시험 3개를 추가했습니다. 배정표는 이번 실행 지시대로 수정하지 않았으며, INTEGRATION의 완료 기록과 통합을 요청합니다. G3 전체 판정은 이 문서의 범위가 아닙니다.

## 2. 확인한 환경

서버 명령은 매번 아래 Toolchain 초기화 후 전용 Worktree에서 실행했습니다.

```bash
. ~/.cargo/env
export NVM_DIR=$HOME/.nvm
. $NVM_DIR/nvm.sh
cd ~/TelosWeaver-wt/host-linux
echo host_id=psyki-nas
hostname
uname -r
. /etc/os-release
echo "$PRETTY_NAME"
rustc -V
cargo -V
node -v
npm -v
git rev-parse HEAD
echo XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR
id -u
```

최종 시험 코드에서 확인한 출력입니다.

```text
host_id=psyki-nas
psyki-nas-x86
7.0.0-38-generic
Ubuntu 26.04.1 LTS
rustc 1.99.1 (58cf8f9c3 2026-10-13)
cargo 1.99.1 (5f94df478 2026-08-27)
v24.22.1
11.19.1
ba8df90551e0ba37a0fd607a304a002cdee59e8c
XDG_RUNTIME_DIR=/run/user/1000
1000
```

Windows에서 처음 `git status --short`는 비어 있었고 Branch는 `ph0/host-linux`였습니다. 서버 공용 Clone `~/TelosWeaver`는 마지막 읽기 전용 확인에서도 `main`, 작업 트리 변경 없음이었습니다. 서버·Windows의 사용자 설정과 Provider 인증 정보는 변경하지 않았습니다.

## 3. 실행 명령과 결과

아래 명령은 §2 초기화 후 서버에서 실행했습니다. 기본 Workspace 시험은 실제 Provider 모델 호출을 기본 제외하며, 별도로 실행한 실환경 시험과 구분합니다.

| 항목 | Host | 명령 | 결과 | 시험 성격 |
|---|---|---|---|---|
| H1 런타임 | `psyki-nas` | `cargo test -p telos-adapters --test runtime_native --locked` | 통과, 12개 | 실제 Unix 프로세스·Rust Fixture·`/bin/sh`, 모델 호출 없음 |
| H1/Core 감독 | `psyki-nas` | `cargo test -p telos-core --test host_api --test host_execution --locked` | 통과, 7+7개 | 실제 Unix Socket과 테스트 대역 Adapter |
| H3 탐지 규칙 회귀 | `psyki-nas` | `cargo test -p telos-adapters --test discovery_extensions --test discovery_resolution --locked` | 통과, 10+8개 | 주로 파일·프로세스 테스트 대역, 실제 로컬 버전 프로세스 시험 1개 포함 |
| H2 Core Linux Build | `psyki-nas` | `cargo build -p telos-core --bin telos-weaver --locked` | 통과 | Linux 네이티브 빌드 |
| H2 실제 Daemon | `psyki-nas` | `cargo test -p telos-core --test host_linux --locked -- --ignored --nocapture` | 통과, 1개 | 실제 `telos-weaver serve`, 정책·Provider 실행 없음 |
| H3 실제 확장 탐지 | `psyki-nas` | `cargo test -p telos-adapters --test discovery_environment --locked -- --ignored --nocapture` | 통과, 1개 | 실제 설치 폴더와 세 CLI의 `--version`, 모델 호출 없음 |
| 공통 회귀 | `psyki-nas` | `cargo test --workspace --locked` | 통과, 153개·기본 제외 34개 | 기본 제외 시험을 통과 개수에 포함하지 않음 |
| 정적 검사 | `psyki-nas` | `cargo clippy --workspace --all-targets --locked -- -D warnings` | 통과 | 최종 시험 코드 포함 |
| 포맷 검사 | `psyki-nas` | `cargo fmt --all --check` | 통과 | 변경 없는 검사 |
| Patch 검사 | `windows-local` | `git diff --check` | 통과 | HOST 변경만 검사 |

H2·H3의 새 실환경 시험에는 `#[ignore]`와 Linux 조건을 붙였습니다. 서버에 해당 설치 상태가 없는 일반 개발 환경에서 기본 Workspace 시험이 사용자 CLI를 실행하지 않도록 했습니다. H3의 예상 버전은 이번 배정에 지정된 서버 상태의 검증 조건입니다. 제품 탐지 규칙에 버전을 고정한 것이 아닙니다.

## 4. H1: Unix 공용 런타임

기존 Linux 런타임 시험 11개는 모두 통과했습니다. 다음 사항을 확인했습니다.

- 프로세스가 출력 구독 전에 종료해도 출력과 exit code 7을 보존하며, 종료 조회를 반복할 수 있습니다.
- `wait` 취소 후 재조회, stdin 동시 사용, 반복·동시 취소, 취소 Future 해제 후 종료 확인이 동작합니다.
- 자식 프로세스를 포함한 취소와 부모 자연 종료 후 자식 정리가 동작합니다.
- 출력 Backpressure와 막힌 stdin writer가 종료를 막지 않습니다.
- 외부 `SIGKILL`은 자연 종료와 구분해 `TerminationCause::External`, exit code 없음으로 보고합니다.
- 긴 출력 줄은 Protocol 데이터로 넘기지 않고 `LineTooLong`으로 보고하며, 출력 순서와 후속 줄을 보존합니다.
- 네이티브 인자·환경 변수와 실행 전 오류를 확인합니다.

기존 시험은 `SIGTERM`을 무시하는 그룹의 강제 종료 승격을 직접 요구하지 않았습니다. `runtime_native.rs`에 `unix_sigterm_resistance_escalates_and_removes_the_process_group`을 추가해 이 경로를 보강했습니다.

```text
/bin/sh -c "trap '' TERM; sleep 60 & echo $!; wait"
```

이 시험은 부모·자식의 `getpgid`가 모두 런타임 부모 PID와 같은지 확인합니다. 이후 `terminate(50ms)`가 5초 이내에 `Killed`와 `ChildrenReaped::All`을 반환해야 합니다. 보고 이후 `kill(-pgid, 0)`과 `kill(child_pid, 0)`이 모두 `ESRCH`여야 통과하며, 출력 Stream도 닫혀야 합니다. Linux에서 실제 실행해 통과했습니다.

잘못된 Provider Stream과 종료 상태의 관계는 `host_execution`의 `terminal_boundaries_fail_closed_and_persistent_turn_can_complete` 등으로 확인했습니다. 이는 테스트 대역 이벤트에 대한 Core 감독 검증이며, 실제 Provider Protocol 재시험 결과로 해석하지 않습니다.

## 5. H2: 실제 Core Unix Socket Local API

`host_linux.rs`는 빌드된 실제 `telos-weaver`를 다음 형태로 실행합니다. `XDG_STATE_HOME`은 해당 자식 프로세스에만 임시 경로를 전달하고, 실제 서버의 `XDG_RUNTIME_DIR`은 그대로 씁니다. 사용자 환경 설정 파일은 수정하지 않습니다.

```bash
XDG_STATE_HOME=<시험 임시 폴더>/state target/debug/telos-weaver serve \
  --project-root <시험 임시 폴더>/project \
  --project-id project-fixture --host-id psyki-nas
```

최종 Host ID 보정 후 재실행에서 확인한 값입니다.

| 확인 | 실제 결과 |
|---|---|
| 시험 폴더 | `/tmp/telos-host-linux-8d6cebace8f56498bec93ebb97aa1a92` |
| Project Key | `745d17782c685a8c` — canonical project root를 계약 함수로 정규화·해시 |
| Daemon PID | `2887024` |
| Socket 디렉터리 | `/run/user/1000/telos-weaver`, UID 1000, 0700 |
| Socket | `/run/user/1000/telos-weaver/745d17782c685a8c.sock`, 실제 Socket 타입, UID 1000, 0600 |
| 토큰 디렉터리 | `<시험 폴더>/state/telos-weaver/745d17782c685a8c`, UID 1000, 0700 |
| 토큰 파일 | 위 디렉터리의 `api-token`, 프로젝트 외부, UID 1000, 0600, 64자 |
| 인증 전 Health | `unauthenticated`, `core.hello is required` |
| 잘못된 토큰 | `unauthenticated`, `authentication failed` |
| 올바른 토큰 | `core.hello` 성공, `hostId=psyki-nas`, `projectId=project-fixture`, Protocol `0.2` |
| 무정책 Run 시작 | `unavailable`, `trusted run policy is not configured` |
| 종료 | 해당 PID에 `SIGTERM`, exit code 0, Socket 제거 확인 |
| 정리 | 연결 해제·Daemon 회수 후 위 시험 임시 폴더 제거 확인 |

인증 후 Health의 확인된 값은 다음과 같습니다. RN 미구성은 현재 계약에 따라 `degraded`이며 RN 프로세스가 실행됐다는 뜻이 아닙니다.

```json
{
  "serverVersion": "0.0.0",
  "status": "degraded",
  "components": [
    {"name": "execution", "status": "ok", "detail": "phase0 in-memory registry; no crash recovery"},
    {"name": "scheduler", "status": "unavailable", "detail": "not implemented"},
    {"name": "persistence", "status": "unavailable", "detail": "not implemented"},
    {"name": "rn", "status": "degraded", "detail": "not configured (the operator policy has no rn section)"}
  ]
}
```

초기 시험 `fe1b49d`에서도 같은 검사에 통과했으나, Core Host ID에 공통 Fixture의 `host-local`을 사용했습니다. 최종 코드 `ba8df90`에서는 `psyki-nas`로 바꾸고 `core.hello` 응답의 Host ID까지 검증했습니다. 초기 시험 PID `2884435`, Socket Key `d1ba9be48ac060a1`, 임시 폴더 `/tmp/telos-host-linux-b65d6b95a5f9e579d87110a2a707d541`도 종료·Socket 제거·임시 폴더 제거가 확인됐습니다.

## 6. H3: Remote SSH Host의 실제 확장 탐지

`discovery_environment.rs`는 공용 `NativeCliDiscoverer`와 각 Provider가 공개한 `cli_rule("linux-x64")`를 사용합니다. 명시 경로 없이 서버의 `~/.vscode-server/extensions`를 탐색하며, CLI를 복사하지 않고 원래 경로에서 `--version`만 실행합니다. 최종 재실행의 탐지 입력 시각은 실제 서버 시각 `2026-10-25T13:10:03Z`입니다.

| Provider | Host | 실제 설치 폴더 | 활성 선택 | 실제 CLI 버전 |
|---|---|---|---|---|
| Claude | `psyki-nas` | `anthropic.claude-code-2.1.309-linux-x64`, `anthropic.claude-code-2.1.310-linux-x64` | 2.1.310 | `2.1.310 (Claude Code)` |
| Codex | `psyki-nas` | `openai.chatgpt-26.1007.21434-linux-x64`, `openai.chatgpt-26.1021.22027-linux-x64` | 26.1021.22027 | `codex-cli 0.164.0-alpha.11.1` |
| Antigravity | `psyki-nas` | `google.google-antigravity-1.9.0` | 1.9.0 | `1.4.7` |

세 Provider 모두 `source=extension`, `extension.resolution=metadata`였습니다. Claude·Codex는 `location=relative-path`, Antigravity는 `location=managed-path`였습니다. 확인한 실행 파일은 다음과 같습니다.

```text
/home/gth1919/.vscode-server/extensions/anthropic.claude-code-2.1.310-linux-x64/resources/native-binary/claude
/home/gth1919/.vscode-server/extensions/openai.chatgpt-26.1021.22027-linux-x64/bin/linux-x86_64/codex
/home/gth1919/.gemini/bin/agy
```

공용 엔진이 기록한 SHA-256은 다음과 같습니다. 공용 탐지 처리는 버전 명령 실행 전후의 바이너리 해시가 같은지도 검사합니다.

```text
claude       b73676430af12f97316a798b8371f81d640f90a58bdcb8f3ce56a835bb47df56
codex        e6d386e8822288f6d5f327e966a5a5e1de928de8795bb6149716005a410f38f2
antigravity  40349135371a8ed1f1af1e9f8b741de99174ae9fce6380b96c2d1f543480af7a
```

`extensions.json`의 각 `relativeLocation`과 실제 탐지된 확장 경로·버전이 일치했고, 선택된 폴더에는 `.obsolete`의 `true` 항목이 없었습니다. 별도 읽기 전용 조회에서 `.obsolete`에 이전 Claude `2.1.309`와 Codex `26.1007.21434`가 `true`로 표시된 것도 확인했습니다. 폴더명만 보고 최신 버전을 고른 결과로 간주하지 않았습니다.

실제 서버의 활성 메타데이터가 있는 경로와 별개로, 메타데이터 누락·플랫폼 불일치·동일 버전 모호성·오래된 활성 항목 거부·`.obsolete` 제외 등의 규칙은 `discovery_extensions` 10개와 `discovery_resolution` 8개로 확인했습니다. 실제 사용자 확장 메타데이터를 변경해 Fallback을 강제하지 않았습니다.

## 7. 실행 중 문제와 동기화 절차

첫 서버 Worktree 생성은 다음 명령으로 성공했습니다.

```powershell
git push Psyki-NAS:TelosWeaver HEAD:ph0/host-linux
ssh Psyki-NAS 'git -C ~/TelosWeaver worktree add ~/TelosWeaver-wt/host-linux ph0/host-linux'
```

이후 같은 Branch가 서버 Worktree에 checkout된 상태라 후속 push가 Git의 기본 `receive.denyCurrentBranch` 정책으로 한 번 거부됐습니다. 그때 서버는 기존 기준 코드에 머물렀으므로 새 시험 Target 2개를 찾지 못했습니다. 이 실행을 새 시험의 통과로 기록하지 않았습니다. 설정을 바꾸지 않고, 깨끗한 제 Worktree만 일시적으로 detached HEAD로 전환해 이후 동기화했습니다.

```powershell
ssh Psyki-NAS 'git -C ~/TelosWeaver-wt/host-linux switch --detach'
git push Psyki-NAS:TelosWeaver HEAD:ph0/host-linux
ssh Psyki-NAS 'git -C ~/TelosWeaver-wt/host-linux switch ph0/host-linux'
```

`ba8df90` 반영 직후 첫 시험 명령은 `Connection reset by peer`로 종료돼 결과를 확보하지 못했습니다. 원인은 확인하지 못했습니다. 이는 의도적으로 재현한 Remote SSH 단절 시험이 아니므로 그 증거로 쓰지 않습니다. 재접속 후 H2·H3를 다시 실행했습니다. 이후 읽기 전용 조회에서 `/tmp/telos-host-linux-*` 잔여 폴더와 제 Worktree의 실행 파일을 사용하는 Daemon 잔여 프로세스가 없었습니다.

## 8. 시험 세션·미실행 항목·남은 위험

- 실제 Provider 대화·Thread·Turn ID: 생성하지 않았습니다. 모델 호출은 없으며 세 CLI의 버전 명령만 실행했습니다. Provider 세션 삭제·Archive 대상도 없습니다.
- 실제 Core Daemon 시험은 §5의 두 실행과 결과를 받지 못한 SSH 중단 시도를 구분합니다. 확인 가능한 두 PID는 종료됐고, 중단 후에도 제 시험 폴더·Daemon 잔여 없음이 확인됐습니다.
- 런타임 Fixture는 기존 시험 방식대로 `/tmp/telos-runtime-tests-<시험 프로세스 PID>` 아래에서 컴파일됩니다. 이 실행에서 만든 시험용 파일을 다른 에이전트의 파일과 섞어 일괄 삭제하지 않았습니다. Windows·Linux Worktree와 빌드 산출물은 인계용으로 남깁니다.
- 실제 VS Code Remote SSH Extension Host의 UI 실행·창 종료 후 Core 생존·SSH 단절 중 Run 유지 시험은 미실행입니다. PH0-EXT·INTEGRATION 범위이며, 이 실행에는 사용자의 VS Code 조작이 없었습니다. 서버 경로에서 실제 CLI를 탐지한 H3 결과를 UI 검증으로 확대하지 않습니다.
- 실제 RN Daemon 연결·재시작, 실제 Provider의 Run·로그인·승인·취소·Resume/Fork 시험은 미실행입니다. 이번 H2는 RN 미구성·DenyRuns만 검증했습니다.
- Unix Socket의 다른 사용자 계정에 의한 실제 접근 시도는 미실행입니다. 확인한 보안 증거는 소유 UID와 디렉터리·소켓·토큰 모드, Local API 토큰 인증·거부입니다. 시스템 계정이나 설정을 변경하지 않았습니다.
- 기본 `~/.local/state` 경로에 상태를 쓰는 시험과 `XDG_RUNTIME_DIR` 미설정 시 `/tmp` Fallback 실환경 시험은 미실행입니다. H2에서는 자식 프로세스의 `XDG_STATE_HOME`만 임시 경로로 격리했습니다.
- 확인된 Health 자체가 명시하듯 Phase 0의 실행 Registry는 메모리 내 상태이며, Scheduler·Persistence·Crash Recovery의 제품 완료 증거가 아닙니다.
- 확장 내부 실행 파일 경로는 비공개 구조이므로 업데이트 후 다시 검증해야 합니다. H3의 버전·해시는 위 일자의 실제 서버 상태입니다.

INTEGRATION에 `ph0/host-linux`의 시험·증거 검토와 통합, 배정표 완료 기록을 요청합니다. 서버 공용 Clone의 `main`, `origin`, 다른 에이전트의 Branch·Worktree는 수정하지 않았습니다.

# G3 Linux·Remote SSH 실환경 작업 지시 (psyki-nas)

| 항목 | 값 |
|---|---|
| 작성 | PH0-INTEGRATION (`claude-opus-5.5/session-c9336d2c`), 2026-10-25 |
| 기준 | `main` `0e8126b`, Spec v1.5.2, 계약 `7497877`, Rust 1.99.1·Node 24.22.1 |
| Host | `psyki-nas` — [Linux·Remote SSH Host 절차](../../LINUX_REMOTE_HOST.md). 공통 검사 통과([INTEGRATION 증거](README.md) §9.2) |
| 배정 | 사용자 지시: Codex = PH0-HOST·PH0-CODEX·PH0-EXT, Gemini = PH0-AGY. INTEGRATION 몫(CLAUDE·RN·G1/G2 Linux 재실행)은 대기 |

## 1. 공통 규칙

1. **접속:** Windows의 `ssh Psyki-NAS`(사용자 `gth1919`). 서버 사용자 설정·로그인 상태를 바꾸지 않습니다. Windows의 인증 정보를 서버로 옮기지 않습니다.
2. **서버 작업 위치:** 공용 Clone `~/TelosWeaver`(`main`)는 건드리지 않고, **작업마다 서버 Worktree**를 만듭니다. 여러 에이전트가 동시에 쓰므로 같은 디렉터리를 공유하지 않습니다.
   ```bash
   git -C ~/TelosWeaver fetch origin
   git -C ~/TelosWeaver worktree add ~/TelosWeaver-wt/<작업> -b <Branch> origin/main
   ```
3. **Branch와 Windows 반영:** 문서·코드 변경은 Windows의 `D:\worktree\TelosWeaver\<작업>`에서 하고, 서버로는 `git push Psyki-NAS:TelosWeaver <Branch>`로 보낸 뒤 서버 Worktree에서 `git reset --hard <Branch>`로 맞춥니다. 서버에서만 생긴 Linux 수정은 그 Branch에 커밋하고 Windows에서 `git fetch Psyki-NAS:TelosWeaver <Branch>`로 가져옵니다. 서버에서 `main`에 통합하거나 GitHub로 push하지 않습니다. **주의(2026-10-25 실행에서 확인):** 서버 Worktree가 그 Branch를 체크아웃하고 있으면 Git이 push를 거부합니다. 서버 Worktree는 `git worktree add --detach ~/TelosWeaver-wt/<작업> <Branch>`처럼 detached HEAD로 만들고, push 뒤 `git reset --hard <Branch>`로 맞춥니다.
4. **소유권:** 자기 작업의 독점 경로만 수정합니다. 공통 파일·계약 변경이 필요하면 배정표 §4에 변경 요청을 올립니다.
5. **환경 기록:** 증거마다 [절차](../../LINUX_REMOTE_HOST.md) §4.1의 출력(Host ID `psyki-nas`, 커널, OS, rustc·cargo·node·npm, Commit)을 붙이고, 시험 표의 Host 열을 `psyki-nas`로 적어 Windows 행과 나눕니다.
6. **실환경 시험:** 서버의 실제 계정으로 수행(사용자 허락). 가벼운 모델·서버의 일회용 임시 폴더만 쓰고 서버의 기존 대화를 재개·수정하지 않습니다. 실행하지 않은 시험을 통과로 적지 않습니다.
7. **시험 세션 정리:** 세션 ID를 증거에 남기고 병합 시점에 지웁니다(배정표 §6 "시험 세션 정리"). 서버에서 Claude는 `~/.claude/projects/<시험 폴더>`, Codex는 App Server `thread/archive`, Antigravity는 `brain/<id>`·`conversations/<id>.db`(`-wal`·`-shm`).
8. **배정표:** 착수·완료를 자기 행과 §7에 단독 커밋으로 기록합니다. §7 표 행 사이에 빈 줄을 넣지 않습니다(표가 끊김).
9. **끝나면:** 자기 Worktree(Windows·서버)는 그대로 두고 INTEGRATION에 통합을 요청합니다. 병합 후 정리는 INTEGRATION이 합니다.

## 2. Codex — PH0-HOST

Branch `ph0/host-linux`. 대상: G3 §19.1 "이벤트 구독 전 종료·잘못된 Stream·프로세스·자식 종료와 기본 권한", "Rust Core Daemon의 Linux Build", 확장 탐지의 Remote SSH Host.

| # | 확인 | 방법 |
|---|---|---|
| H1 | 공용 런타임의 Process Group 취소·자식 회수·외부 신호·구독 전 종료 | 서버에서 `cargo test -p telos-adapters --test runtime_native --locked` 등 기존 Unix 경로 시험 결과를 정리하고, 빠진 Unix 경로가 있으면 HOST 시험을 더합니다 |
| H2 | Core Daemon의 Unix Socket Local API | 서버에서 `telos-weaver serve`(정책 없음)를 띄워 `${XDG_RUNTIME_DIR}/telos-weaver/<projectKey>.sock` 위치, 디렉터리 0700·토큰 0600, 인증·Health(`rn` 구성 요소 포함)·DenyRuns를 확인하고 정지 후 Socket 정리를 기록합니다 |
| H3 | Remote SSH Host 확장 탐지(§7.5) | 서버 `~/.vscode-server/extensions`에 Claude·Codex가 **두 버전씩** 있습니다. `extensions.json`·`.obsolete` 기준으로 활성 버전(Claude 2.1.310, Codex 26.1021.22027)을 고르는지, `~/.gemini/bin/agy`를 찾는지 실제 탐지로 확인합니다(모델 호출 없음) |
| H4 | 증거 | `evidence/PH0-HOST/LINUX.md` 작성 |

## 3. Codex — PH0-CODEX

Branch `ph0/codex-linux`.

| # | 확인 | 방법 |
|---|---|---|
| C1 | Codex 실환경 시험의 Linux 실행 | 서버에서 `cargo test -p telos-adapters --lib codex::tests::environment --locked -- --ignored --test-threads=1 --nocapture`(7개). 모델·effort는 Windows와 같은 조건(배정표 PH0-CODEX 행) |
| C2 | 탐지된 CLI | 확장 `openai.chatgpt-26.1021.22027-linux-x64`의 `codex-cli 0.164.0-alpha.11.1`이 쓰이는지(이전 26.1007이 아닌지) |
| C3 | 정리 | 시험 Thread를 `thread/archive`하고 ID를 기록. 과거 Windows 시험에서 정리가 누락된 Thread(INTEGRATION 증거 §8.5)도 상태를 확인해 기록 |
| C4 | 증거 | `evidence/PH0-CODEX/LINUX.md` |

`send_input`·승인 Allow 실환경은 이번 범위가 아니며(G3 필수 아님) 하지 않아도 됩니다. 하면 별도 행으로 적습니다.

## 4. Codex — PH0-EXT

Branch `ph0/ext-linux`. 대상: G3 §19.1 "Extension의 실행 파일 탐지·시작·연결"과 "VS Code 단일 Remote SSH의 Host 확인".

| # | 확인 | 방법 |
|---|---|---|
| E1 | Linux Core와 실제 연결 | 서버에서 `cargo build -p telos-core --bin telos-weaver --locked` 후 `TELOS_CORE_EXECUTABLE=<서버 바이너리> node --test test/live-core.cjs`(Unix Socket·토큰·DenyRuns·RN 설정 없음·연결 해제 후 생존·PID 재사용) |
| E2 | Remote-SSH Extension Host | Windows VS Code → Remote-SSH `Psyki-NAS`로 연 창에서 Extension을 원격 Extension Host로 실행해 서버 Core를 시작·연결·Health 표시하고, 창을 닫은 뒤 Core가 남는지 확인합니다. **사용자의 VS Code 조작이 필요하면 사용자에게 단계별로 요청합니다.** 격리 Extension Host 자동 시험(`scripts/test-host.cjs`)은 서버에 데스크톱 VS Code가 없으면 실행할 수 없으므로 그 사실을 기록합니다 |
| E3 | 증거 | `evidence/PH0-EXT/LINUX.md` |

## 5. Gemini — PH0-AGY

Branch `ph0/agy-linux`.

| # | 확인 | 방법 |
|---|---|---|
| A1 | Antigravity 실환경 시험의 Linux 실행 | 서버에서 `cargo test -p telos-adapters --test agy_environment --locked -- --ignored --test-threads=1 --nocapture`(7개: 탐지·데이터 디렉터리 옵션, 완료·데이터 디렉터리 비교, 도구 거부, 잘못된 모델, 취소·프로세스 트리, Resume·Fork 거부). 모델 `gemini-3.8-flash-low`, `--effort low` |
| A2 | AGY-DATA (Spec §7.5) | 서버 확장 `google.google-antigravity-1.9.0`과 `~/.gemini/bin/agy` 1.4.7에서 `--app_data_dir=antigravity` 수용 여부와 실제 쓰기 위치. Windows에서 본 `\\?\` 경로 문제는 Linux에 없을 것으로 예상되지만 결과로 확인 |
| A3 | Turn 직렬화·도구 거부·Exit Code와 결과 차이(G3 §19.1) | A1 결과로 정리하고 Windows 결과와 대조 |
| A4 | 정리 | 시험 대화의 `brain/<id>`·`conversations/<id>.db`(`-wal`·`-shm`) 삭제, ID 기록 |
| A5 | 증거 | `evidence/PH0-AGY/LINUX.md` |

## 6. INTEGRATION 몫 (대기)

사용자 지시로 대기합니다. 다른 담당의 결과를 통합한 뒤 진행합니다.

- PH0-CLAUDE: `claude_environment` 7개의 Linux 실행
- G1(`phase0_g1` 5개)·G2(`phase0_g2` 4개)·마스터(`phase0_master` 2개)의 Linux 재실행
- PH0-RN: `phase0_rn` 2개(실제 RN·실제 Claude)의 Linux 실행
- Remote SSH 시나리오(절차 §4.4), G3 대조표와 판정

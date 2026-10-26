# 외부 에이전트가 실행한 CLI 세션을 VS Code에서 확인하는 방법 (Codex·Claude Code·Antigravity)

> 확인일: 2026-10-26 (KST). Codex REQ-022 갱신, Claude·Antigravity의 세부 확인일은 각 절·증거 참조
>
> 대상: 다른 에이전트·스크립트가 Codex App Server 또는 `codex exec`, `claude -p`, 또는 `agy` Headless 모드로 실행한 작업을 VS Code의 **OpenAI Codex 확장**, **Claude Code 확장**, **Google Antigravity 확장(및 Antigravity IDE/2.0)** 대화 목록에서 찾는 경우. 세 에이전트(Codex·Claude Code·Antigravity)가 서로를 CLI로 호출하는 모든 조합을 다룹니다.

## 1. 요약

Codex는 실행 경로를 구분해야 합니다. `exec`의 자동화 출처는 출처 옵션을 붙여도 기본 목록에서 제외되지만, App Server로 만든 대화는 표시할 수 있습니다. Claude의 비대화형 출처와 Antigravity의 데이터 디렉터리·프로젝트도 확장의 기본 목록에 영향을 줍니다. 같은 저장소·실제 작업 경로를 사용하고 아래 Provider별 검증된 절차를 따릅니다.

| 항목 | Codex CLI (→ OpenAI Codex 확장) | Claude Code CLI (→ Claude Code 확장) | Antigravity CLI (`agy` → Antigravity 확장·IDE) |
|---|---|---|---|
| 비대화형 기본 동작 | 출처 `exec` (목록 기본 필터 밖) | `entrypoint: "sdk-cli"` (목록 기본 필터 밖) | 기본 저장소 `~/.gemini/antigravity-cli` (확장 저장소와 분리) + `project_id: "default-cli-project"` |
| 목록·프로젝트에 포함시키는 방법 | App Server `thread/start`·`turn/start` ([A3](#a3-새-작업을-위임하는-방법)). `exec --thread-source cli`는 표시 수단 아님 | 환경 변수 `CLAUDE_CODE_ENTRYPOINT=claude-vscode` | `--app_data_dir=antigravity --project <프로젝트 이름 또는 UUID>` |
| 세션 저장소 | `CODEX_HOME` (기본 `~/.codex`) | `CLAUDE_CONFIG_DIR` (기본 `~/.claude`)의 `projects/<인코딩된 cwd>/` | `~/.gemini/antigravity` (`conversation_summaries.db`, `brain/<ID>`, `conversations/<ID>.db`) |
| 프로젝트 정의 위치 | Workspace 경로 기준 | `projects/<cwd 인코딩>` 폴더 이름 | `~/.gemini/config/projects/<UUID>.json` (`id`, `name`, `projectResources`) |
| 작업 경로 지정 | App Server의 `cwd` / `exec`의 `--cd` | 프로세스 `cwd` (전용 옵션 없음) | 프로세스 `cwd` + `--add-dir <절대 경로>` |
| 작업 경로와 목록의 관계 | 프로젝트 필터를 바꾸거나 전체 목록에서 확인 가능 | 확장은 **VS Code 첫 Workspace 폴더와 같은 cwd**의 세션만 조회. 하위 폴더·Worktree 세션은 기본 목록에 나오지 않음 | `--project <이름 또는 UUID>`를 첫 실행에 지정하면 Worktree에서 실행해도 해당 프로젝트에 귀속되며 프로젝트 기본 경로와 `--add-dir` 경로가 함께 `workspace_uris`에 기록됨 |
| 세션 ID 수집 | 참고 예제 `client.thread.threadId` / `exec --json`의 `thread.started.thread_id` | `--session-id <UUID>`로 미리 지정하거나 `stream-json`의 `system/init.session_id` | `--output-format stream-json`의 첫 `init.conversation_id` |
| 영속성을 끄는 옵션 (사용 금지) | `--ephemeral` / App Server `ephemeral: true` | `--no-session-persistence` | 해당 옵션 없음 (`--app_data_dir` 누락 금지) |
| 기존 자동화 세션 전환 | App Server `thread/fork`로 새 ID 생성 ([A5](#a5-기존-exec-대화를-목록-대상의-새-세션으로-분기하기)) | `claude -p --resume <ID> --fork-session` + 환경 변수 | Headless 모드에서는 `/fork` 미지원 및 `--conversation` 재개 시 `project_id` 변경 불가 → **첫 실행부터** `--app_data_dir=antigravity --project` 지정 필요 |

이 절차는 **대화 목록의 조회 대상 및 특정 프로젝트에 포함시키기 위한 방법**입니다. 실행 중인 CLI 작업의 모든 진행 내용이 확장 화면에 실시간으로 반영되거나, 확장에서 그 프로세스를 제어할 수 있다는 의미는 아닙니다. 자동화 측에서는 JSONL 이벤트와 프로세스 종료 코드를 계속 수집합니다. 실행 중에는 CLI 호출자 한쪽에서만 작업을 지시하고, VS Code에서 후속 작업을 보낼 때는 기존 실행의 종료를 먼저 확인합니다.

## 2. 공통 원칙

1. VS Code에서 사용하는 확장의 CLI 실행 파일을 확인하고, 호출 에이전트에 그 절대 경로를 전달합니다. 버전별 디렉터리는 업데이트 때 바뀌므로 영구 고정하지 않습니다. 프로젝트의 CLI 탐지 원칙은 [Spec §7](DEVELOPMENT_v1.5.md)을 따릅니다.
2. CLI와 확장을 같은 Host·OS 사용자·세션 저장소(`CODEX_HOME`, `CLAUDE_CONFIG_DIR`, 또는 `~/.gemini/antigravity`)에서 실행합니다. 터미널에서만 환경 변수를 바꾸면 이미 실행 중인 확장에는 반영되지 않습니다.
3. 세션 영속성을 끄는 옵션(`--ephemeral`, `--no-session-persistence`)을 사용하지 않으며, `agy`에서는 확장 공유 디렉터리 옵션(`--app_data_dir=antigravity`)과 `--project`를 생략하지 않습니다.
4. 셸 문자열을 조합하지 않고 실행 파일과 인자 배열을 분리해 실행하며, 프롬프트는 stdin으로 전달합니다.
5. 세션 ID를 호출자의 작업 ID와 연결해 기록하고, 후속 작업에는 `--last`·`--continue`(`-c`) 대신 그 ID(`resume <ID>`, `--resume <ID>`, `--conversation <ID>`)를 사용합니다.
6. stdout JSONL과 stderr를 분리해 보관하고, 세션 시작 이벤트가 아니라 완료·실패 이벤트와 종료 코드(특히 `agy`는 `result.denied_actions` 포함)로 작업 결과를 판정합니다.
7. 표시 경로·데이터 디렉터리·프로젝트를 맞추는 설정은 권한 확대 수단이 아닙니다. 승인·Sandbox·모델은 허용된 정책을 유지합니다. Codex의 `thread_source`는 analytics 분류이며 실제 `source`를 바꾸지 않습니다(A1).
8. 검증 기록에는 **CLI 작업 성공 여부**, **VS Code 목록 표시 여부**, **대화 내용 열기 여부**, **실시간 진행 반영 여부**를 각각 남기고, 직접 확인하지 않은 항목은 미검증으로 기록합니다.

---

# A. Codex CLI

## A1. 확인한 사실과 아직 확인하지 않은 동작

**확장 목록에 보여야 하는 작업은 `codex app-server`의 Thread/Turn API로 실행합니다. `codex exec --thread-source cli`와 `codex exec fork … --thread-source cli`는 표시 수단이 아닙니다.**

| 항목 | 확인 결과 (Windows, 2026-10-26) |
|---|---|
| 대상 | `openai.chatgpt` 26.1021.22027, 동봉 `codex-cli 0.164.0-alpha.11.1` |
| `exec --thread-source cli` | 저장된 `thread_source`는 `cli`, **`source`는 `exec`**. 완료 후에도 기본 DB 목록에서 제외되고 전체 출처 조회에는 포함 |
| `exec fork <ID> --thread-source cli` | 새 ID가 생기지만 `source`는 `exec`. 해당 분기에서 Turn까지 완료한 뒤에도 기본 DB 목록에서 제외 |
| 실제 기본 목록 필터 | 확장은 빈 `sourceKinds`와 `useStateDbOnly: true`로 조회. `sourceKinds` 생략·빈 배열은 interactive 출처(`cli`·`vscode`)를 선택 |
| `source`와 `thread_source` | 전자는 목록 필터가 보는 실행 출처, 후자는 analytics 분류. `threadSource`를 `cli`로 지정한다고 전자가 바뀌지 않음 |
| App Server 새 Thread | 아래 참고 클라이언트로 완료한 Thread는 `source: vscode`, 기본 DB 목록 포함. **사용자 화면에서 목록 표시·대화 열기 확인** |
| App Server `thread/resume` | App Server로 만든 동일 ID를 이어 쓰고 완료하는 것 확인. `exec` 원본을 표시 출처로 변환하는 절차는 아님 |
| App Server `thread/fork` | `exec` 원본에서 새 `vscode` Thread 생성·완료·기본 DB 목록 포함 확인. 원본은 `exec` 그대로 유지. **이 분기 자체의 화면 열기는 미검증** |
| 내장 `debug app-server send-message-v2` | `vscode`·기본 DB 목록 포함 확인. 진단용 명령이며 시험 때 `approval_policy="never"`를 전달해도 시작 응답이 `on-request`였으므로 기본 위임 예제로 쓰지 않음 |
| 미검증 | 이 참고 클라이언트의 Linux·Remote SSH 실행, 실시간 화면 갱신, 실행 중 확장 입력과의 경합 |

직접 만든 실환경 증거와 실패·정리 기록은 [REQ-022 검증](phase-0/evidence/PH0-CODEX/REQ-022.md), Adapter의 별도 구현·판정은 [DEC-08 검증](phase-0/evidence/PH0-CODEX/VSCODE_LISTING.md)에 있습니다. CLI 도움말에 출처 옵션이 있다는 사실만으로 표시 성공을 판단하지 않습니다.

## A2. 호출 전에 실행 환경 확인

### Windows 로컬

확장이 사용하는 CLI 절대 경로를 선택하고 버전·지원 필드를 확인합니다. 아래는 확인한 버전의 예이며 업데이트 뒤에는 설치 경로를 다시 찾습니다.

```powershell
Get-Command codex -All | Select-Object CommandType, Source
$codexExe = 'C:\Users\Psyki-HOME\.vscode\extensions\openai.chatgpt-26.1021.22027-win32-x64\bin\windows-x86_64\codex.exe'
& $codexExe --version
& $codexExe app-server --help
& $codexExe app-server generate-json-schema --help
$env:CODEX_HOME
```

`Get-Command` 결과와 실제 프로세스 실행 파일은 다를 수 있습니다. 확장 동봉 파일을 절대 경로로 선택합니다. 기본 `CODEX_HOME`은 `~/.codex`이며 이 사용자는 `C:\Users\Psyki-HOME\.codex`입니다. 사용자 지정 값을 쓰면 호출 측과 확장의 실행 환경이 같아야 합니다. 이 값을 맞추려고 인증 파일을 복사하지 않습니다.

### Linux·WSL·Remote SSH

확장이 실행되는 Host에서 `hostname`, `id -un`, `CODEX_HOME`, 선택한 CLI의 `--version`을 확인합니다. 원격 확장은 `~/.vscode-server/extensions` 아래 CLI를 사용합니다. 이 절의 참고 클라이언트는 Windows에서 검증했으며 다른 Host도 자동으로 같은 결과라고 보장하지 않습니다.

VS Code에서 연 폴더와 **실제로 작업해야 하는 cwd**를 함께 기록합니다. 표시를 위해 다른 폴더에서 작업하지 않습니다. 프로젝트 필터를 실제 작업 폴더에 맞추거나 전체 목록에서 확인합니다. 이 Host 구분은 Claude·Antigravity에도 적용됩니다.

## A3. 새 작업을 위임하는 방법

### 확장 목록에 표시할 작업: App Server 참고 클라이언트

[실행 가능한 Python 예제](phase-0/evidence/PH0-CODEX/req022_app_server.py)는 Python 3.10 이상·표준 라이브러리만 사용합니다. 로그인은 네이티브 CLI가 처리하며 API 키를 복사하지 않습니다. TelosWeaver Adapter를 호출하거나 Rust를 빌드할 필요는 없습니다.

저장소 루트의 **PowerShell 7(UTF-8 파이프)**에서 실행합니다. `--cwd`는 위임받은 실제 Workspace로, 모델은 해당 계정에서 확인한 값으로 바꿉니다. 아래 모델은 이번 시험에 쓴 값입니다.

```powershell
$runner = (Resolve-Path '.\docs\phase-0\evidence\PH0-CODEX\req022_app_server.py').Path
'Reply only PONG. Do not use tools.' |
  python $runner --codex $codexExe --cwd 'D:\work\assigned-worktree' `
    --model gpt-6-luna --sandbox read-only --name 'Assigned task' `
    > task.events.jsonl 2> task.stderr.log
$LASTEXITCODE
```

기본값은 `read-only`, `approvalPolicy: never`, effort `low`, Turn 대기 120초입니다. 이미 승인된 작업에 쓰기가 필요하면 `--sandbox workspace-write`를 명시합니다. 예제는 응답의 실제 Sandbox 종류·네트워크 끔·승인 정책을 검사해 다르면 Turn 전에 거부합니다. Workspace 쓰기 응답에서는 추가 쓰기 루트가 비어 있고 임시 경로 두 종류도 제외됐는지 확인합니다. MCP 서버를 비활성화하는 요청을 사용하며 전역 설정 파일은 바꾸지 않습니다. 공통 권한 정책을 우회하는 플래그는 사용하지 않습니다.

stdout은 `kind`가 있는 JSONL입니다.

| 레코드 | 의미 |
|---|---|
| `client.thread` | 작업 ID와 연결할 `threadId`, Native `source`, `cwd` |
| `client.policy` | 시작 응답에서 확인한 Sandbox·승인 정책 |
| `server.notification` | `message` 안의 네이티브 진행·완료 알림 |
| `client.result` | `threadId`, `turnId`, `turnStatus`, `serverExitCode`, 예제의 `exitCode` |
| `client.error`·`client.archive_failed` | 실패. 성공으로 처리하지 않음 |

예제 종료 코드 0은 **Turn `completed`와 서버 프로세스 정상 종료**를 모두 확인한 경우입니다. 작업 산출물 검증·Task 완료를 뜻하지 않습니다. Native Turn `failed`는 서버 프로세스가 0으로 끝나도 예제는 1로 끝납니다. 입력 오류는 2, 시간 초과는 124이며 정리 중 Archive나 서버 종료가 실패해도 시간 초과 코드를 유지합니다. 승인·클라이언트 도구 요청은 자동 허용하지 않습니다.

기본 실행은 완료한 Thread를 남겨 목록에서 열 수 있게 합니다. `--archive`는 시험 정리용 선택이며, 이를 지정하면 목록에서 사라집니다. 영속성을 끄는 `--ephemeral`은 쓰지 않습니다. 이 예제는 단일 Turn 참고 코드이며 Core의 프로세스 감독·복구·이벤트 영속화를 대체하지 않습니다. 지원 범위 밖의 MCP 서버 이름은 명시적으로 거부합니다.

### 직접 구현할 때의 최소 Protocol

OpenAI 공식 문서의 [App Server 수명 주기](https://learn.chatgpt.com/docs/app-server)는 연결마다 초기화한 뒤 Thread·Turn을 만들고 알림을 수집하는 흐름을 설명합니다. 아래 필드 값은 **설치 CLI 0.164.0-alpha.11.1의 생성 Schema와 실제 응답**으로 확인했습니다.

```json
{"id":1,"method":"initialize","params":{"clientInfo":{"name":"delegator","version":"0.1.0"},"capabilities":{"experimentalApi":true}}}
{"method":"initialized","params":{}}
{"id":2,"method":"thread/start","params":{"cwd":"D:/work/assigned-worktree","model":"gpt-6-luna","sandbox":"read-only","approvalPolicy":"never","ephemeral":false}}
{"id":3,"method":"turn/start","params":{"threadId":"<응답의 ID>","model":"gpt-6-luna","effort":"low","approvalPolicy":"never","input":[{"type":"text","text":"<프롬프트>"}]}}
```

2번 응답의 실제 정책과 `thread.source`를 확인한 뒤 3번을 보냅니다. 응답은 `id`로 짝을 맞추고, 그 사이 알림을 버리지 않습니다. 해당 `threadId`·`turnId`의 `turn/completed`와 마지막 상태를 기다린 뒤 stdin을 닫아 서버 종료 코드를 확인합니다. 구현하지 않은 서버 요청(`method`와 `id`가 함께 있음)에 허용 응답을 만들지 않습니다. 참고 예제에는 이 처리와 제한 시간·UTF-8·프레임 상한이 포함돼 있습니다.

### 표시가 필요 없는 `exec` 작업

`exec` 자체는 계속 사용할 수 있지만 이 버전에서는 기본 확장 목록 표시를 약속하지 않습니다.

```powershell
'Reply only PONG. Do not use tools.' |
  & $codexExe exec --cd 'D:\work\assigned-worktree' --sandbox read-only `
    --model gpt-6-luna --json - > exec.events.jsonl 2> exec.stderr.log
```

`thread.started.thread_id`, 완료·실패 이벤트, 프로세스 종료 코드를 기록합니다. `--thread-source cli`를 추가해도 기본 목록에 표시되는 것은 아닙니다.

## A4. 같은 세션에 후속 작업 보내기

목록에 표시되는 App Server Thread는 참고 클라이언트의 `--resume <ID>`로 이어 씁니다.

```powershell
'Reply only PONG. Do not use tools.' |
  python $runner --codex $codexExe --cwd 'D:\work\assigned-worktree' `
    --model gpt-6-luna --resume '<THREAD_ID>' > next.events.jsonl 2> next.stderr.log
```

기존 `source`는 바뀌지 않습니다. 예제는 기본 목록 밖의 출처를 거부하므로 `exec` 원본은 A5의 새 분기를 사용합니다. 목록 표시가 필요 없는 기존 `exec` 작업은 `codex exec resume <ID>`로 이어 쓸 수 있으나 계속 `exec`입니다. `--last`로 사용자의 다른 세션을 선택하지 않습니다.

동일 Thread에 CLI와 확장에서 쓰기 Turn을 동시에 만들지 않습니다. VS Code에서 단순 열기만 해도 writer를 보유해 다른 프로세스의 `read`·`archive`가 `already has an active writer`로 거부되는 경우를 확인했습니다. 먼저 해당 작업·화면을 종료하고 재시도하며 잠금을 무시하거나 다른 사용자 프로세스를 강제 종료하지 않습니다.

## A5. 기존 `exec` 대화를 목록 대상의 새 세션으로 분기하기

`codex exec fork <ID> --thread-source cli`는 원본을 `cli`·`vscode`로 전환하지 않습니다. **App Server의 `thread/fork`로 새 Thread를 만들고 Turn을 완료하는 방법**은 기본 DB 목록 포함까지 확인했습니다.

```powershell
'Reply only PONG. Do not use tools.' |
  python $runner --codex $codexExe --cwd 'D:\work\assigned-worktree' `
    --model gpt-6-luna --fork '<AUTHORIZED_SOURCE_THREAD_ID>' `
    > fork.events.jsonl 2> fork.stderr.log
```

이 요청은 새 ID를 만들고 원본은 그대로 유지합니다. 실험에서는 원본 `source: exec`, 새 Thread `source: vscode`였습니다. 새 분기의 **기본 DB 목록 포함은 자동 확인**, 이 분기 대화 자체의 **화면 열기는 미검증**입니다. 새 App Server 대화의 화면 표시·열기는 A3 경로에서 사용자 확인했습니다.

원본이 Archive 상태면 현재 App Server는 분기를 거부합니다. 실제로 계속 사용할 권한이 있는 원본만 먼저 A3처럼 초기화한 연결에서 공식 `thread/unarchive` 요청으로 복원한 뒤 분기합니다. 예를 들어 `{"id":2,"method":"thread/unarchive","params":{"threadId":"<원본 ID>"}}`에 성공 응답을 받은 뒤 `thread/fork`를 보냅니다. 예제는 원본을 자동 복원·수정하지 않습니다. 세션 JSONL·SQLite의 `source`를 직접 고치지 않습니다. 저장되지 않은 ephemeral 세션의 복구는 이 절차의 대상이 아닙니다.

## A6. VS Code에서 확인하고 문제를 구분하는 순서

1. `client.thread`의 ID·실제 `source`·`cwd`를 기록합니다. 실행 결과는 `client.result`의 Turn 상태와 두 종료 코드로 따로 확인합니다.
2. 같은 Host·사용자·`CODEX_HOME`의 확장에서 실제 작업 폴더 또는 전체 목록을 선택합니다. 다른 폴더에서 재실행해 표시를 맞추지 않습니다.
3. 목록을 다시 열고, 필요하면 **시험용 새 창에서** `Ctrl+Shift+P` → `Developer: Reload Window`, 이어서 `Codex: Open Codex Sidebar`를 실행합니다.
4. 대화 제목·ID·본문을 확인합니다. 목록 표시와 본문 열기는 별도 판정이며 실시간 반영은 미검증입니다.
5. 시험 대화를 정리할 때는 창을 닫아 writer를 해제한 뒤 공식 `thread/archive`를 사용합니다. 일반 위임 작업을 목록에서 계속 보려면 자동 Archive하지 않습니다.

| 증상 | 확인할 항목 |
|---|---|
| `exec --thread-source cli`인데 안 보임 | 이 버전의 확인된 동작. `thread_source`가 아니라 실제 `source`가 `exec`인지 확인하고 App Server 경로 사용 |
| App Server 대화가 안 보임 | Native 완료·저장, `source`, Archive 여부, Host·사용자·`CODEX_HOME`, cwd·프로젝트 필터 |
| 파일은 있지만 DB 목록에 없음 | 확장은 `useStateDbOnly: true`로 조회. JSONL 존재만으로 색인·표시 성공을 판단하지 않음 |
| 기존 세션의 분기가 거부됨 | 원본 Archive 여부·사용 권한·active writer 확인 |
| 서버 종료 코드 0인데 작업 실패 | Native Turn `failed`와 RPC 오류 확인. 프로세스 종료만으로 성공 처리하지 않음 |
| 목록은 보이지만 진행 내용이 안 바뀜 | 실시간 갱신 미검증. 호출 측 이벤트 스트림으로 진행 상태 확인 |

---

# B. Claude Code CLI

## B1. 확인한 사실과 아직 확인하지 않은 동작

| 항목 | 근거와 확인 결과 |
|---|---|
| 설치된 Claude Code 확장 | `anthropic.claude-code` 버전 `2.1.310` (win32-x64) |
| 확장 동봉 CLI | `resources\native-binary\claude.exe`, `claude --version` 결과 `2.1.310 (Claude Code)`. 이 Windows 환경에서는 `PATH`에 `claude`가 없음 |
| 세션 저장 위치 | `CLAUDE_CONFIG_DIR`(기본 `~/.claude`)의 `projects/<cwd 인코딩>/<세션 ID>.jsonl`. cwd의 영숫자가 아닌 문자는 `-`로 바뀜 (예: `D:\Downloads\Vibe_dev` → `D--Downloads-Vibe-dev`) |
| 출처 기록 방식 | 세션 JSONL의 각 줄에 `"entrypoint"`가 기록됨. 확장에서 연 대화는 `claude-vscode` |
| CLI의 출처 결정 | 동봉 CLI 코드에서 확인: `CLAUDE_CODE_ENTRYPOINT`가 있으면 그 값을 사용(단 `cli`이면서 비대화형이면 `sdk-cli`로 변경). 없으면 비대화형(`-p`)은 `sdk-cli`, 대화형은 `cli`. Agent SDK(TypeScript·Python)는 값이 없을 때 `sdk-ts`/`sdk-py`를 지정 |
| 확장의 목록 필터 | 확장 `extension.js`에서 확인: 첫 줄 부근의 `entrypoint`가 `sdk-cli`·`sdk-ts`·`sdk-py`이거나 `sessionKind`가 `daemon`·`daemon-worker`이면 **programmatic 세션**으로 판정. 확장 패널의 목록은 `claudeCode.claudeProcessWrapper` 설정이 없으면 programmatic 세션을 제외 |
| 목록의 작업 경로 범위 | 확장은 `dir: <VS Code 첫 Workspace 폴더>`, `includeWorktrees: false`로 조회. 해당 경로의 `projects/<인코딩>` 폴더만 읽으므로 하위 폴더나 Git Worktree에서 실행한 세션은 포함되지 않음 |
| 실제 기록 시험 (2026-10-25, Windows) | 부모 세션 변수를 제거한 환경에서 `claude -p` 실행 → 모든 줄이 `sdk-cli`. 같은 명령에 `CLAUDE_CODE_ENTRYPOINT=claude-vscode` 추가 → 모든 줄이 `claude-vscode` |
| fork 전환 시험 | `sdk-cli` 세션을 `CLAUDE_CODE_ENTRYPOINT=claude-vscode`와 `--resume <ID> --fork-session`으로 fork → 새 세션 ID, 복사된 줄을 포함해 모든 줄이 `claude-vscode`. 원본은 `sdk-cli` 유지 |
| resume 혼합 시험 | `claude-vscode` 세션을 환경 변수 없이 `-p --resume`으로 이어가면 같은 ID에 `sdk-cli` 줄이 추가됨. 판정은 첫 줄 부근 값을 사용하므로 목록 대상은 유지되는 것으로 판단하지만 화면은 미확인 |
| 기존 Adapter 실행 방식 | `crates/adapters/src/claude.rs`는 `-p --input-format stream-json --output-format stream-json --verbose`로 실행하며 `CLAUDE_CODE_ENTRYPOINT`를 지정하지 않음 |
| 실제 VS Code 화면 표시 | 시험 세션은 Workspace 밖 임시 경로에서 실행했으므로 확장 목록·대화 열기는 직접 확인하지 않음 |
| `claude-vscode` 지정의 부수 효과 | 확장 자신의 세션과 같은 값이며 위 시험에서 실행·응답에 문제는 없었음. 원격 측정 분류와 IDE 전용 동작에 미치는 영향은 전체 확인하지 않음 |
| Linux·Remote SSH | 직접 시험하지 않음 |

**판단:** 비대화형 Claude Code 세션은 기본값으로 VS Code 목록에서 숨겨집니다. `CLAUDE_CODE_ENTRYPOINT=claude-vscode`로 실행하고, cwd를 VS Code에서 연 Workspace 폴더와 같게 하면 확인한 목록 필터의 대상에 들어갑니다. 확장 버전이 바뀌면 이 판정 코드가 바뀔 수 있으므로 위 항목을 다시 확인합니다.

`claudeCode.claudeProcessWrapper`를 설정해도 programmatic 세션이 목록에 포함되지만, 이 설정은 확장이 Claude 프로세스를 실행할 실행 파일 자체를 바꾸는 설정이므로 표시 목적으로 사용하지 않습니다. 세션 JSONL의 `entrypoint`를 직접 수정하는 절차도 이 가이드에 포함하지 않습니다.

공식 근거: [CLI 참조](https://code.claude.com/docs/en/cli-reference), [VS Code 확장](https://code.claude.com/docs/en/vs-code), [Headless 실행](https://code.claude.com/docs/en/headless). `CLAUDE_CODE_ENTRYPOINT`와 목록 필터는 공식 문서가 아니라 위 설치 버전의 코드에서 확인한 동작입니다.

## B2. 호출 전에 실행 환경 확인

### Windows 로컬

```powershell
Get-Command claude -All -ErrorAction SilentlyContinue | Select-Object CommandType, Source
Get-ChildItem "$env:USERPROFILE\.vscode\extensions" -Directory -Filter 'anthropic.claude-code-*'
$env:CLAUDE_CONFIG_DIR
```

이 환경에서 확인한 실행 파일은 다음 경로입니다.

```text
C:\Users\Psyki-HOME\.vscode\extensions\anthropic.claude-code-2.1.310-win32-x64\resources\native-binary\claude.exe
```

선택한 절대 경로에 `--version`과 `--help`를 실행해 `-p`, `--session-id`, `--resume`, `--fork-session`, `--output-format` 지원을 확인합니다. `CLAUDE_CONFIG_DIR`을 지정하지 않았다면 저장소는 `C:\Users\Psyki-HOME\.claude`입니다. 사용자 지정 경로를 쓴다면 확장의 실행 환경에도 같은 값이 있어야 합니다.

### VS Code Workspace 경로와 cwd 맞추기

확장은 VS Code 창의 **첫 번째 Workspace 폴더**와 같은 cwd의 세션만 보여 줍니다. 예를 들어 VS Code에서 `D:\Downloads\Vibe_dev`를 열었다면 `D:\Downloads\Vibe_dev\TelosWeaver`에서 실행한 세션은 다른 저장 폴더(`D--Downloads-Vibe-dev-TelosWeaver`)에 기록되어 목록에 나오지 않습니다.

- 작업은 배정된 Worktree에서 실행하고, 확인은 **그 Worktree 폴더를 VS Code 창으로 열어** 진행합니다. 표시를 위해 작업 경로를 대표 저장소나 상위 폴더로 바꾸지 않습니다.
- 경로의 대소문자·드라이브 문자 표기가 달라도 Windows에서는 같은 폴더로 처리하지만, 심볼릭 링크나 Junction 경로는 실제 경로와 다른 폴더로 기록될 수 있으므로 `Resolve-Path` 결과를 기록합니다.
- Linux·WSL·Remote SSH의 Host 구분은 [A2](#linuxwslremote-ssh)와 같습니다. Remote SSH 창에서는 원격 Host의 `~/.claude`와 원격 확장 동봉 CLI를 사용합니다.

### 호출자가 Claude Code 세션 안에서 실행 중인 경우

VS Code의 Claude Code 세션이 부르는 자식 프로세스에는 `CLAUDE_CODE_ENTRYPOINT=claude-vscode`, `CLAUDECODE`, `CLAUDE_CODE_SESSION_ID`, `CLAUDE_CODE_CHILD_SESSION` 등의 부모 세션 변수가 이미 들어 있습니다. 상속된 값에 기대지 말고, 별도 작업 세션으로 실행할 때는 부모 세션 변수를 제거한 뒤 `CLAUDE_CODE_ENTRYPOINT`를 명시합니다. 이 문서의 시험도 부모 세션 변수를 제거한 환경에서 수행했으며, 상속한 상태로 실행했을 때의 영향은 확인하지 않았습니다.

## B3. 새 작업을 위임하는 명령

### PowerShell

`--session-id`로 UUID를 미리 지정하면 호출자가 작업 ID와 세션 ID를 실행 전에 연결할 수 있습니다. `--permission-mode`·`--allowedTools`·`--model`은 해당 작업에 허용된 기존 정책 값을 사용합니다. 예시는 파일 수정이 없는 분석 요청입니다.

```powershell
$claudeExe = 'C:\Users\Psyki-HOME\.vscode\extensions\anthropic.claude-code-2.1.310-win32-x64\resources\native-binary\claude.exe'
$workspace = (Resolve-Path 'D:\Downloads\Vibe_dev\TelosWeaver').Path   # VS Code에서 연 폴더와 같은 경로
$sessionId = [guid]::NewGuid().ToString()
$prompt = 'AGENTS.md와 README.md를 읽고 프로젝트 구조를 한국어로 설명해 주세요. 파일은 수정하지 마세요.'
$OutputEncoding = [System.Text.UTF8Encoding]::new($false)

'CLAUDECODE', 'CLAUDE_CODE_SESSION_ID', 'CLAUDE_CODE_CHILD_SESSION' |
    ForEach-Object { Remove-Item "Env:$_" -ErrorAction SilentlyContinue }
$env:CLAUDE_CODE_ENTRYPOINT = 'claude-vscode'

Push-Location $workspace
try {
    $prompt | & $claudeExe -p --session-id $sessionId --permission-mode plan `
        --output-format stream-json --verbose
    $runExitCode = $LASTEXITCODE
} finally {
    Pop-Location
}
```

환경 변수는 현재 PowerShell 프로세스에 남으므로, 실제 자동화에서는 아래처럼 자식 프로세스 전용 환경을 구성합니다. `--output-format stream-json`을 `-p`와 함께 쓸 때는 `--verbose`가 필요합니다. 결과 한 건만 필요하면 `--output-format json`을 사용합니다.

### 에이전트 코드에서 프로세스를 직접 실행하는 경우

```python
import json
import os
import subprocess
import uuid
from pathlib import Path

claude_exe = Path(
    r"C:\Users\Psyki-HOME\.vscode\extensions\anthropic.claude-code-2.1.310-win32-x64"
    r"\resources\native-binary\claude.exe"
)
workspace = Path(r"D:\Downloads\Vibe_dev\TelosWeaver").resolve(strict=True)
session_id = str(uuid.uuid4())
prompt = "AGENTS.md와 README.md를 읽고 프로젝트 구조를 설명해 주세요. 파일은 수정하지 마세요."

PARENT_SESSION_VARS = {
    "CLAUDECODE", "CLAUDE_CODE_SESSION_ID", "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_SESSION_ATTENDED", "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN", "CLAUDE_PID",
}
env = {k: v for k, v in os.environ.items() if k not in PARENT_SESSION_VARS}
env["CLAUDE_CODE_ENTRYPOINT"] = "claude-vscode"

result = subprocess.run(
    [str(claude_exe), "-p", "--session-id", session_id,
     "--permission-mode", "plan",
     "--output-format", "stream-json", "--verbose"],
    input=prompt,
    cwd=workspace,
    env=env,
    encoding="utf-8",
    capture_output=True,
    shell=False,
)

final = None
for line in result.stdout.splitlines():
    event = json.loads(line)
    if event.get("type") == "system" and event.get("subtype") == "init":
        assert event["session_id"] == session_id
    elif event.get("type") == "result":
        final = event

print(json.dumps({
    "sessionId": session_id,
    "exitCode": result.returncode,
    "isError": None if final is None else final.get("is_error"),
}))
```

`system/init`은 세션 시작 알림이며 작업 성공 판정이 아닙니다. 마지막 `result` 이벤트의 `subtype`·`is_error`와 종료 코드를 함께 확인합니다. 호출자는 CLI 절대 경로·버전, Host, cwd, 실제 `CLAUDE_CONFIG_DIR`, `CLAUDE_CODE_ENTRYPOINT` 값, 세션 ID, 종료 코드를 작업 기록에 남깁니다.

Agent SDK(TypeScript·Python)로 호출한다면 SDK 옵션의 `env`에 `CLAUDE_CODE_ENTRYPOINT`를 넣지 않는 한 `sdk-ts`/`sdk-py`로 기록되어 목록에서 제외됩니다. SDK 경로는 이 문서에서 시험하지 않았습니다.

## B4. 같은 세션에 후속 작업 보내기

```powershell
$env:CLAUDE_CODE_ENTRYPOINT = 'claude-vscode'
Push-Location $workspace
'앞선 분석에서 확인한 수정 사항을 적용해 주세요.' |
    & $claudeExe -p --resume '<SESSION_ID>' --output-format stream-json --verbose
Pop-Location
```

- 첫 실행과 **같은 cwd**에서 실행합니다. 세션 파일은 cwd별 폴더에 저장됩니다.
- `--continue`(`-c`)는 그 cwd의 가장 최근 세션을 고르므로 여러 에이전트 환경에서는 사용하지 않습니다.
- 후속 실행에도 같은 `CLAUDE_CODE_ENTRYPOINT`를 지정해 한 세션 안에 출처가 섞이지 않게 합니다.
- VS Code에서 같은 세션을 열어 둔 상태로 CLI가 동시에 이어 쓰지 않도록, 한쪽의 실행 종료를 확인한 뒤 다른 쪽에서 작업합니다.

## B5. 이미 만들어진 sdk-cli 세션을 목록 조회 대상에 포함하기

기존 실행의 종료를 확인한 뒤, 원래 세션과 같은 cwd에서 fork합니다.

```powershell
$env:CLAUDE_CODE_ENTRYPOINT = 'claude-vscode'
Push-Location $workspace
'이전 대화 내용을 이어서 확인할 수 있도록 현재 상태를 한 문단으로 요약해 주세요.' |
    & $claudeExe -p --resume '<SESSION_ID>' --fork-session --output-format json
Pop-Location
```

- `--fork-session`은 새 세션 ID를 만듭니다. 결과의 `session_id`를 새 ID로 기록하고 원본 ID와 함께 남깁니다. 원본 세션은 그대로 유지됩니다.
- Claude Code의 fork는 프롬프트 한 Turn을 함께 실행합니다. Codex처럼 Turn 없이 fork만 만드는 방법은 확인하지 않았으므로, 작업 상태를 바꾸지 않는 요약 요청을 사용합니다.
- 원래 세션이 Workspace와 다른 cwd에서 실행되었다면 fork도 그 cwd에 저장됩니다. 그 경우 해당 폴더를 VS Code에서 열어 확인합니다.
- 저장되지 않은 `--no-session-persistence` 세션은 복구할 수 없습니다.

터미널에서는 `claude --resume`(인자 없음)으로 그 cwd의 세션 선택 목록을 열 수 있습니다. 이 명령은 VS Code의 표시 필터나 세션 출처를 바꾸지 않습니다.

## B6. VS Code에서 확인하고 문제를 구분하는 순서

1. 호출 측에서 세션 ID와 종료 코드·`result` 이벤트를 확인합니다.
2. `CLAUDE_CONFIG_DIR\projects\<cwd 인코딩>\<세션 ID>.jsonl`이 있고, 첫 줄 부근의 `"entrypoint"`가 `claude-vscode`인지 확인합니다.

   ```powershell
   $dir = Join-Path $env:USERPROFILE '.claude\projects'
   $file = Get-ChildItem $dir -Recurse -Filter '<SESSION_ID>.jsonl' | Select-Object -First 1
   $file.FullName
   Select-String -Path $file.FullName -Pattern '"entrypoint":"[^"]*"' -AllMatches |
       ForEach-Object { $_.Matches.Value } | Group-Object | Select-Object Count, Name
   ```

3. 세션 파일이 들어 있는 폴더 이름이 VS Code 첫 Workspace 폴더 경로의 인코딩과 같은지 확인합니다.
4. Claude Code 확장의 세션 목록(과거 대화)을 다시 열고, 갱신되지 않으면 `Developer: Reload Window` 후 조회합니다. 자동 보관(`archiveInactiveSessions`) 설정이 있다면 Archive 목록도 확인합니다.
5. 대화를 열어 사용자 요청과 응답이 보이는지 확인하고, 목록 표시와 내용 열기를 따로 기록합니다.

| 증상 | 확인할 항목 |
|---|---|
| 세션 파일은 있지만 목록에 없음 | 첫 줄의 `entrypoint`가 `sdk-cli`·`sdk-ts`·`sdk-py`인지, `CLAUDE_CODE_ENTRYPOINT`가 자식 프로세스에 실제로 전달되었는지 |
| 같은 저장소인데 목록에 없음 | 세션 폴더 이름과 VS Code 첫 Workspace 폴더가 같은지. 하위 폴더·Worktree·상위 폴더에서 실행했는지 |
| 세션 파일 자체가 없음 | `--no-session-persistence` 사용 여부, `CLAUDE_CONFIG_DIR`, Host·OS 사용자, 실행 오류 |
| CLI와 확장에서 서로 다른 목록이 보임 | Host·OS 사용자·`CLAUDE_CONFIG_DIR`, 로컬/WSL/Remote SSH 확장 실행 위치 |
| 실행이 권한 요청에서 멈추거나 거부됨 | `-p`에는 대화형 승인 창이 없음. 허용된 정책 안에서 `--permission-mode`·`--allowedTools`를 지정 |
| 목록에는 있지만 실행 중 진행 내용이 안 바뀜 | 실시간 반영은 미검증. 자동화의 `stream-json` 이벤트를 기준으로 진행 상태 확인 |

---

# C. Antigravity CLI (`agy`)

## C1. 확인한 사실과 아직 확인하지 않은 동작

| 항목 | 근거와 확인 결과 |
|---|---|
| 설치된 Antigravity 확장 | `google.google-antigravity` 버전 `1.9.0` (Windows 로컬 및 Linux `psyki-nas`) |
| 확장이 관리하는 CLI | `~/.gemini/bin/agy.exe` (Windows) / `~/.gemini/bin/agy` (Linux), `agy --version` 결과 `1.4.7` |
| 데이터 디렉터리 분리 | 확장은 허브 프로세스를 `--hub --app_data_dir=antigravity`로 띄워 `~/.gemini/antigravity`를 사용(`extension.js` 확인). 반면 `agy` CLI 기본값은 `~/.gemini/antigravity-cli`이므로 `--app_data_dir=antigravity`를 생략하면 확장 DB(`conversation_summaries.db`)와 완전히 분리됨 ([AGY-DATA-03](phase-0/evidence/PH0-AGY/README.md)) |
| 프로젝트 정의 위치 | `~/.gemini/config/projects/*.json`에 프로젝트별 `id`(UUID), `name`, `projectResources`가 저장됨. 이 환경에서는 `TelosWeaver` (`c1ad552c-317e-44a5-b059-c8f9df88d085.json`), `Vibe_dev` (`75201d01-42ea-48a2-b4b5-504013554c6f.json`), `default-cli-project.json` (`"name": "CLI Project"`), `outside-of-project.json` 확인 |
| 기본 프로젝트 귀속 | `agy changelog`(1.0.12, 1.1.18) 및 DB 확인: CLI에서 `--project`를 생략하면 현재 작업 폴더와 무관하게 `project_id`가 `"default-cli-project"`로 기록되어 특정 프로젝트 대화 목록에서 분리됨. `--project <프로젝트 이름 또는 UUID>`를 지정해야 해당 프로젝트의 UUID로 기록됨 |
| 작업 경로(`workspace_uris`) 기록 | Headless 실행에서 `--add-dir <절대 경로>`를 생략하면 `conversation_summaries.db`의 `workspace_uris`가 빈 문자열(`""`)로 남음. `--project TelosWeaver --add-dir <작업 폴더>`를 함께 지정하면 `workspace_uris`에 `<작업 폴더>` URI와 프로젝트 기본 폴더(`file:///d:/Downloads/Vibe_dev/TelosWeaver`) URI가 모두 기록됨 |
| 출처(`source`) 기록 | `conversation_summaries.db` 확인: 대화형 CLI(TUI)는 `CORTEX_TRAJECTORY_SOURCE_CLI`로 기록되고, VS Code 확장 대화와 Headless Print(`--input-format stream-json --output-format stream-json`) 실행은 모두 빈 문자열(`""`)로 기록됨 |
| 실제 기록 시험 (2026-10-25, Windows) | `--app_data_dir=antigravity --project TelosWeaver --add-dir <임시 폴더> --input-format stream-json --output-format stream-json` 실행 → `~/.gemini/antigravity/conversation_summaries.db`에 `project_id: "c1ad552c-317e-44a5-b059-c8f9df88d085"`, `workspace_uris: ["file:///<임시 폴더>","file:///d:/Downloads/Vibe_dev/TelosWeaver"]`, `app_data_dir: "antigravity"` 기록 확인 (시험 세션 `21278063-6511-418f-a873-6f0fdc900389` 즉시 정리 완료) |
| Resume 시 프로젝트 변경 시험 | `--project` 없이 생성되어 `default-cli-project`로 기록된 세션을 `--conversation <ID> --project TelosWeaver`로 재개(Resume)해도 기존 세션의 `project_id`와 `workspace_uris`는 `default-cli-project`에서 바뀌지 않음을 확인 (시험 세션 `22feb09b-4617-463a-b0cf-7a6124888145` 즉시 정리 완료) |
| 실제 VS Code 화면 표시 (2026-10-26, Windows) | C3 명령(`--app_data_dir=antigravity --project TelosWeaver --add-dir D:\Downloads\Vibe_dev\TelosWeaver`, cwd 같음, `gemini-3.8-flash-low`·`--effort low`, Stream JSON) 실행 → `SUCCESS`·종료 코드 0. `conversation_summaries.db`에 `project_id` = TelosWeaver UUID, `app_data_dir: "antigravity"`, `source: ""` 기록. VS Code(작업 폴더 `Vibe_dev`)의 Antigravity 확장 목록에 **`Developer: Reload Window` 뒤 표시됨**(사용자 확인). Reload 전에는 갱신되지 않을 수 있음. 시험 대화 `dae9515c-9f51-4275-a227-96386f0b1985` |
| `--mode plan`과 `--disable-slash-commands` | 위 실행에서 `agy` 1.4.7이 `warning: --mode plan has no effect while slash command expansion is disabled.`를 출력함. C3 예시처럼 두 옵션을 함께 쓰면 **plan 모드가 적용되지 않습니다**. 읽기 전용이 필요할 때 대안(옵션 조합·`--mode` 기본값)은 미검증 |
| Headless Fork 미지원 | `agy` 1.4.7은 `--fork` 플래그가 없고 Headless Print 모드에서 `/fork`를 거부함 (`/fork is not available in print mode`, [FORK-POC-01](phase-0/evidence/PH0-AGY/README.md)) |
| Linux Remote SSH 인증 차이 | Windows 로컬은 OS 자격 증명 저장소를 공유하므로 `--app_data_dir=antigravity`가 별도 CLI 로그인 없이 동작하지만, Linux Remote SSH(`psyki-nas`)에서는 파일 기반 토큰(`~/.gemini/antigravity/antigravity-oauth-token`)을 요구하므로 원격 터미널에서 `agy --app_data_dir=antigravity`로 1회 로그인되어 있어야 함 ([PH0-AGY Linux 검증 기록](phase-0/evidence/PH0-AGY/LINUX.md)) |
| 기존 Adapter 실행 방식 | `crates/adapters/src/agy.rs`는 기본값으로 `AgyDataDirMode::ExtensionShared`(`--app_data_dir=antigravity`)를 사용하나 `--project`는 기본 인자로 넣지 않음 |

**판단:** `agy` 세션을 VS Code(Antigravity 확장·IDE·Antigravity 2.0)에서 보이면서 특정 프로젝트에 포함시키려면 **첫 번째 실행(세션 생성 시점)부터** `--app_data_dir=antigravity`, `--project <프로젝트 이름 또는 UUID>`, `--add-dir <작업 폴더 절대 경로>`를 함께 지정해야 합니다.

공식 근거: `agy --help`, `agy changelog` (1.0.12 `--project`·`--new-project` 추가, 1.1.18 `--project`에 프로젝트 이름 전달 지원), [PH0-AGY 검증 기록](phase-0/evidence/PH0-AGY/README.md), [PH0-AGY Linux 검증 기록](phase-0/evidence/PH0-AGY/LINUX.md). `--app_data_dir=antigravity`와 `conversation_summaries.db` 스키마 동작은 설치된 확장 `1.9.0` 및 `agy` `1.4.7`에서 확인한 동작입니다.

## C2. 호출 전에 실행 환경과 프로젝트 식별자 확인

### Windows 로컬

```powershell
$agyExe = "$env:USERPROFILE\.gemini\bin\agy.exe"
& $agyExe --version
& $agyExe --app_data_dir=antigravity --version
& $agyExe --help
Get-ChildItem "$env:USERPROFILE\.gemini\config\projects\*.json" | ForEach-Object {
    $j = Get-Content $_.FullName -Raw | ConvertFrom-Json
    [PSCustomObject]@{ File = $_.Name; Id = $j.id; Name = $j.name }
}
```

이 환경에서 확인한 실행 파일과 프로젝트 설정은 다음과 같습니다.

```text
실행 파일: C:\Users\Psyki-HOME\.gemini\bin\agy.exe (버전 1.4.7)
프로젝트 목록:
- TelosWeaver : c1ad552c-317e-44a5-b059-c8f9df88d085
- Vibe_dev    : 75201d01-42ea-48a2-b4b5-504013554c6f
```

- `--app_data_dir=antigravity --version`이 종료 코드 `0`으로 버전을 출력하는지 먼저 확인합니다 (Spec §7.5 규칙 5).
- Windows에서 `Resolve-Path` 대신 Rust `canonicalize()` 등으로 얻은 `\\?\D:\...` 확장 경로 접두사를 `--add-dir`에 그대로 넘기면 `agy`가 Workspace 밖 경로로 오인해 파일 쓰기를 `scratch/`로 우회시키므로, `\\?\` 접두사 없는 일반 절대 경로(`D:\Downloads\Vibe_dev\TelosWeaver` 등)를 전달합니다 ([PH0-AGY Linux 검증 기록 §5](phase-0/evidence/PH0-AGY/LINUX.md)).

### Linux·WSL·Remote SSH

```sh
hostname
id -un
~/.gemini/bin/agy --app_data_dir=antigravity --version
ls -la ~/.gemini/config/projects/
ls -la ~/.gemini/antigravity/
```

- Remote SSH 호스트(예: `psyki-nas`)에서는 `agy`가 `SSH_CONNECTION`을 감지해 데이터 디렉터리 안의 파일 토큰(`~/.gemini/antigravity/antigravity-oauth-token`)을 찾습니다. 원격 VS Code 확장은 토큰을 파일로 저장하지 않으므로, 원격 호스트에서 `--app_data_dir=antigravity`로 Headless 실행을 하려면 사용자가 원격 터미널에서 `~/.gemini/bin/agy --app_data_dir=antigravity`로 1회 로그인을 마친 상태여야 합니다.
- 원격 호스트의 `~/.gemini/config/projects/`에 대상 프로젝트 JSON이 있는지도 함께 확인합니다.

## C3. 새 작업을 위임하는 명령

### PowerShell

`-p` 뒤에 다른 플래그가 오면 프롬프트 문자열로 오인할 수 있으므로([PH0-AGY 검증 기록 §2](phase-0/evidence/PH0-AGY/README.md)), 비대화형 자동화에서는 `--input-format stream-json --output-format stream-json`으로 실행하고 stdin에 Stream JSON 사용자 메시지 한 줄을 보낸 뒤 stdin을 닫는 방식을 권장합니다.

```powershell
$agyExe = "$env:USERPROFILE\.gemini\bin\agy.exe"
$workspace = (Resolve-Path 'D:\Downloads\Vibe_dev\TelosWeaver').Path
$project = 'TelosWeaver'   # 또는 'c1ad552c-317e-44a5-b059-c8f9df88d085'
$prompt = 'AGENTS.md와 README.md를 읽고 프로젝트 구조를 한국어로 설명해 주세요. 파일은 수정하지 마세요.'
$OutputEncoding = [System.Text.UTF8Encoding]::new($false)

$inputJson = @{
    event = 'user'
    message = @{
        role = 'user'
        content = $prompt
    }
} | ConvertTo-Json -Compress

Push-Location $workspace
try {
    $inputJson | & $agyExe `
        --app_data_dir=antigravity `
        --project $project `
        --add-dir $workspace `
        --mode plan `
        --disable-slash-commands `
        --input-format stream-json `
        --output-format stream-json
    $runExitCode = $LASTEXITCODE
} finally {
    Pop-Location
}
```

**주의:** `agy` 1.4.7은 `--disable-slash-commands`와 함께 쓴 `--mode plan`을 무시한다고 경고합니다(C1). 이 예시를 읽기 전용 실행으로 간주하지 말고, 프롬프트와 작업 정책으로 수정 범위를 제한합니다.

Worktree에서 작업할 때는 `$workspace`에 배정된 Worktree 절대 경로를 지정하고 `--project TelosWeaver`를 그대로 유지합니다. 그러면 작업은 Worktree에서 격리되어 실행되면서도, 대화 목록에서는 `TelosWeaver` 프로젝트 아래에 묶입니다.

### 에이전트 코드에서 프로세스를 직접 실행하는 경우

```python
import json
import os
import subprocess
from pathlib import Path

agy_exe = Path.home() / ".gemini" / "bin" / "agy.exe"
workspace = Path(r"D:\Downloads\Vibe_dev\TelosWeaver").resolve(strict=True)
# Windows에서 \\?\ 접두사가 붙은 경우 제거
workspace_str = str(workspace).removeprefix("\\\\?\\")
project_name = "TelosWeaver"
prompt = "AGENTS.md와 README.md를 읽고 프로젝트 구조를 설명해 주세요. 파일은 수정하지 마세요."

stdin_payload = json.dumps({
    "event": "user",
    "message": {"role": "user", "content": prompt},
}, ensure_ascii=False) + "\n"

result = subprocess.run(
    [
        str(agy_exe),
        "--app_data_dir=antigravity",
        "--project", project_name,
        "--add-dir", workspace_str,
        "--mode", "plan",
        "--disable-slash-commands",
        "--input-format", "stream-json",
        "--output-format", "stream-json",
    ],
    input=stdin_payload,
    cwd=workspace_str,
    env=os.environ.copy(),
    encoding="utf-8",
    capture_output=True,
    shell=False,
)

conversation_id = None
final_result = None
for line in result.stdout.splitlines():
    event = json.loads(line)
    if event.get("event") == "init" and conversation_id is None:
        conversation_id = event.get("conversation_id")
    elif event.get("event") == "result":
        final_result = event.get("result", {})

denied = [] if final_result is None else (final_result.get("denied_actions") or [])
status = None if final_result is None else final_result.get("status")
soft_denied_empty = (
    status == "SUCCESS"
    and bool(denied)
    and not (final_result.get("response") or "").strip()
)

print(json.dumps({
    "conversationId": conversation_id,
    "exitCode": result.returncode,
    "status": status,
    "deniedActions": denied,
    "softDeniedFailure": soft_denied_empty,
}))
```

`agy` 1.4.7은 Headless 모드에서 도구 권한이 소프트 거부되어 아무 응답을 만들지 못해도 종료 코드 `0`과 `result.status: "SUCCESS"`를 반환할 수 있습니다([PH0-AGY 검증 기록 §2](phase-0/evidence/PH0-AGY/README.md)). 따라서 호출자는 종료 코드만 보지 않고 `result.status`, `result.denied_actions`, `result.response`를 함께 확인해 성공 여부를 판정합니다.

## C4. 같은 세션에 후속 작업 보내기

```powershell
$inputJson = @{
    event = 'user'
    message = @{
        role = 'user'
        content = '앞선 분석에서 확인한 수정 사항을 적용해 주세요.'
    }
} | ConvertTo-Json -Compress

Push-Location $workspace
try {
    $inputJson | & $agyExe `
        --app_data_dir=antigravity `
        --project $project `
        --conversation '<CONVERSATION_ID>' `
        --add-dir $workspace `
        --mode accept-edits `
        --disable-slash-commands `
        --input-format stream-json `
        --output-format stream-json
} finally {
    Pop-Location
}
```

- `--continue`(`-c`)는 가장 최근 대화를 고르므로 다중 에이전트 환경에서는 사용하지 않고 항상 `--conversation <CONVERSATION_ID>`를 지정합니다.
- 첫 실행과 동일하게 `--app_data_dir=antigravity`와 `--add-dir`를 유지합니다.

## C5. 이미 만들어진 CLI 세션을 특정 프로젝트로 옮기거나 포함하기

- **`--app_data_dir=antigravity` 없이 만들어진 세션 (`~/.gemini/antigravity-cli`)**:
  - 별도 데이터 디렉터리와 DB에 저장되어 있으므로 `--app_data_dir=antigravity --conversation <ID>`로 찾을 수 없습니다 (`Conversation Not Found`).
- **`--app_data_dir=antigravity`는 지정했으나 `--project`를 생략해 `default-cli-project`로 만들어진 세션**:
  - VS Code·Antigravity UI의 `CLI Project`(`default-cli-project`) 목록에는 나타나지만, 이후 `--conversation <ID> --project TelosWeaver`로 재개해도 `project_id`가 `TelosWeaver`로 바뀌지 않습니다(C1 시험 확인).
  - 또한 `agy` 1.4.7 Headless Print 모드는 `/fork`를 지원하지 않습니다(`FORK-POC-01`).
- **권장 절차**:
  - 내부 SQLite(`conversation_summaries.db`)나 `.pb` 파일을 직접 수정하지 않습니다(공통 원칙 및 DEC-08).
  - 특정 프로젝트 아래에 표시되어야 하는 작업은 **새 세션을 시작할 때부터** `--app_data_dir=antigravity --project <프로젝트 이름 또는 UUID> --add-dir <Workspace>`를 지정해 생성합니다.

## C6. VS Code에서 확인하고 문제를 구분하는 순서

1. 호출 측에서 `init.conversation_id`, 종료 코드, `result.status`, `result.denied_actions`를 확인합니다.
2. `~/.gemini/antigravity/conversation_summaries.db`에 해당 `conversation_id` 행이 생성되었고 `project_id`가 대상 프로젝트 UUID와 일치하는지 확인합니다(읽기 전용 조회).

   ```powershell
   python -c "import sqlite3, os; conn = sqlite3.connect(os.path.expanduser('~/.gemini/antigravity/conversation_summaries.db')); print(conn.execute('SELECT conversation_id, step_count, workspace_uris, project_id, app_data_dir FROM conversation_summaries WHERE conversation_id = ?', ('<CONVERSATION_ID>',)).fetchall())"
   ```

3. VS Code(Antigravity 확장·IDE 또는 Antigravity 2.0)에서 해당 프로젝트(`TelosWeaver` 등)의 대화 목록을 확인합니다. 갱신되지 않으면 `Developer: Reload Window` 후 다시 확인합니다.
4. 대화를 열어 사용자 요청과 응답이 정상 표시되는지 확인하고, 목록 표시와 대화 열기를 구분해 기록합니다.

| 증상 | 확인할 항목 |
|---|---|
| CLI 실행은 성공했으나 VS Code 어디에도 없음 | `--app_data_dir=antigravity`가 누락되어 `~/.gemini/antigravity-cli`에 저장되었는지 확인 |
| 전체·`CLI Project`에는 있으나 대상 프로젝트에 없음 | 첫 실행에 `--project <프로젝트 이름 또는 UUID>`를 지정했는지 확인. `default-cli-project`로 생성된 뒤 Resume에만 `--project`를 붙였는지 확인 |
| `--project` 이름이 거부됨 | `~/.gemini/config/projects/*.json`의 `name` 또는 `id`와 정확히 일치하는지, `agy` 버전이 `1.1.18` 이상인지 확인 |
| 파일 쓰기가 프로젝트 폴더가 아닌 `scratch/`에 생성됨 | `--add-dir`가 누락되었거나 Windows `\\?\` 확장 경로 접두사가 포함된 경로를 전달했는지, `--mode accept-edits`(또는 `auto`)를 지정했는지 확인 |
| 종료 코드는 0인데 응답이 비어 있고 작업이 안 됨 | Headless 모드에서 도구 권한이 소프트 거부되었는지 `result.denied_actions`와 stderr 확인 |
| 시험 대화를 지울 때 `conversations/<ID>.db`(`-wal`·`-shm`)가 사용 중이라 삭제 실패 | 확장 허브(`agy --hub`)가 화면에 연 대화의 DB를 잡고 있음(2026-10-26 확인, `brain/<ID>`는 삭제됨). 확장에서 대화를 닫거나 UI에서 삭제한 뒤 다시 지움. 화면 확인을 요청할 때는 확인 뒤 대화를 닫아 달라고 함께 요청 |
| Linux Remote SSH에서 `authentication failed or timed out` | 원격 호스트의 `~/.gemini/antigravity/antigravity-oauth-token` 유무 확인 ([C2](#linuxwslremote-ssh)) |

---

# D. 서로 호출하는 경우

세 에이전트(Codex, Claude Code, Antigravity `agy`)가 서로를 비대화형 CLI로 호출할 때 호출 측에서 지정할 핵심 옵션과 확인 위치는 다음과 같습니다.

| 호출 방향 | 호출 측에서 지정할 것 | 확인할 곳 |
|---|---|---|
| Claude Code / Agy → **Codex** | 확장 동봉 CLI의 App Server 경로 또는 A3 참고 클라이언트, 실제 `cwd`·Sandbox·승인 정책 명시 | OpenAI Codex 확장 대화 목록 ([A](#a-codex-cli)) |
| Codex / Agy → **Claude Code** | Claude Code 확장 동봉 `claude.exe`, 부모 `CLAUDE*` 변수 제거 후 `CLAUDE_CODE_ENTRYPOINT=claude-vscode`, `cwd` = VS Code 첫 Workspace 폴더, `-p --session-id <UUID> --output-format stream-json --verbose` | Claude Code 확장 세션 목록 ([B](#b-claude-code-cli)) |
| Codex / Claude Code → **Antigravity (`agy`)** | `~/.gemini/bin/agy[.exe]`, `--app_data_dir=antigravity --project <프로젝트 이름 또는 UUID> --add-dir <Workspace> --input-format stream-json --output-format stream-json` (stdin에 JSON 사용자 메시지 1줄 전송 후 EOF) | Antigravity 확장·IDE 해당 프로젝트 대화 목록 ([C](#c-antigravity-cli-agy)) |
| Claude Code → **Claude Code** | 부모 세션 변수 제거 후 `CLAUDE_CODE_ENTRYPOINT=claude-vscode` 명시 ([B2](#호출자가-claude-code-세션-안에서-실행-중인-경우)) | Claude Code 확장 세션 목록 |
| Codex → **Codex** | A3 App Server 경로. 부모 세션 변수에 기대지 않고 새 ID·권한·cwd 기록 | OpenAI Codex 확장 대화 목록 |
| Antigravity (`agy`) → **Antigravity (`agy`)** | `--app_data_dir=antigravity --project <프로젝트 이름 또는 UUID> --add-dir <Workspace>` (동일 `conversation_id`를 동시에 열지 않고 새 세션 또는 종료 후 `--conversation <ID>`로 직렬화) | Antigravity 확장·IDE 해당 프로젝트 대화 목록 |

- 호출하는 에이전트의 Sandbox나 파일 접근 정책이 상대 CLI의 세션 저장소(`~/.codex`, `~/.claude`, `~/.gemini/antigravity`) 쓰기나 네트워크 접근을 막으면 세션이 저장되지 않거나 실행이 실패할 수 있습니다. 이 경우 임의로 권한 우회 플래그를 쓰지 말고 해당 작업의 승인 정책에 따라 처리합니다.
- 호출 측의 환경 변수가 자식 프로세스로 그대로 상속되므로, 상대 CLI의 출처·저장소 변수는 호출 코드에서 명시적으로 설정(또는 부모 세션 변수 제거)합니다.
- 양쪽 확장에서 같은 작업을 추적하려면 작업 기록에 호출 에이전트 세션 ID와 피호출 CLI의 세션 ID(`thread_id` / `session_id` / `conversation_id`)를 함께 남깁니다.

# E. TelosWeaver Adapter와의 관계

TelosWeaver Adapter가 시작한 Run도 확장 목록에 보이도록 Spec DEC-08·[§7.6](DEVELOPMENT_v1.5.md#76-vs-code-확장-대화-목록-표시)에 따라 구현했습니다(v1.5.4, 2026-10-26 통합). 운영자 정책의 Provider별 `vscodeListing`(`listed` 기본 / `hidden`)으로 정합니다.

| Adapter | `listed` (기본) | `hidden` |
|---|---|---|
| Claude (`crates/adapters/src/claude.rs`) | 자식 환경 `CLAUDE_CODE_ENTRYPOINT=claude-vscode`, 부모 Claude 세션 변수 제거 | `CLAUDE_CODE_ENTRYPOINT=sdk-cli` |
| Codex (`crates/adapters/src/codex.rs`) | App Server `thread/start`·`thread/fork`에 `ephemeral: false`, `threadSource: "vscode"`. 응답 출처가 `cli`·`vscode`가 아니면 거부 | **미지원.** App Server는 `threadSource`(analytics 분류)와 관계없이 저장된 Thread를 `vscode`로 기록하므로 정책 로드에서 거부 |
| Antigravity (`crates/adapters/src/agy.rs`) | `--app_data_dir=antigravity` | 옵션 생략(`~/.gemini/antigravity-cli`). 확장 인덱스에 CLI 출처로 색인되지만 화면에는 나타나지 않음 |

세 Provider 모두 자동 판정과 사용자 화면 확인을 통과했습니다. 근거는 각 담당의 `VSCODE_LISTING.md`와 [통합 기록](phase-0/evidence/PH0-INTEGRATION/README.md) §10입니다. 위 A~D절은 Adapter를 거치지 않는 외부 호출 절차입니다. Codex의 경우 A3의 App Server 경로를 사용하며 `exec --thread-source cli`로 Adapter 동작을 재현할 수 없습니다. App Server에서 `thread/list.sourceKinds`는 **조회할 출처를 고르는 필터**이며, 지정한다고 생성된 Thread의 출처가 바뀌지는 않습니다. Antigravity Adapter는 `--project`를 넘기지 않으므로 `project_id`는 `default-cli-project`로 기록됩니다.

이 문서는 외부 에이전트의 CLI 호출 절차를 정리한 운영 문서이며 Adapter 구현, 공통 계약, Phase 0의 완료 판정은 변경하지 않습니다. 기존 구현·시험 근거는 [PH0-CODEX 검증 기록](phase-0/evidence/PH0-CODEX/README.md), [PH0-CLAUDE 검증 기록](phase-0/evidence/PH0-CLAUDE/README.md), [PH0-AGY 검증 기록](phase-0/evidence/PH0-AGY/README.md)([Linux 검증 기록](phase-0/evidence/PH0-AGY/LINUX.md))을 참고합니다.

# F. 위임 에이전트에 전달할 지시 예시

### Codex를 호출하게 할 때

```text
VS Code 목록에 표시할 Codex 작업은 같은 Host·사용자·CODEX_HOME의 App Server 경로로 실행해 주세요.
exec --thread-source cli 또는 exec fork --thread-source cli를 표시 방법으로 사용하지 마세요.
A3의 req022_app_server.py 또는 같은 Protocol을 쓰고, 프롬프트는 UTF-8 stdin으로 전달해 주세요.
확장 CLI의 절대 경로·버전을 확인하고 실제 작업 cwd와 승인된 Sandbox·모델을 명시해 주세요.
서버 응답의 실제 Sandbox·승인 정책·source를 확인한 뒤 Turn을 시작하고, ephemeral은 사용하지 마세요.
client.thread의 threadId를 작업 ID와 연결하고, 후속 작업에는 --resume <ID>를 사용해 주세요.
기존 exec 대화는 A5의 App Server fork로 새 ID를 만들며 원본을 직접 고치지 마세요.
Turn 완료 상태·서버 종료 코드·클라이언트 종료 코드를 모두 확인하고 stdout JSONL과 stderr를 분리해 주세요.
목록에 남겨야 하는 실제 작업에는 --archive를 지정하지 마세요. 시험은 화면 확인·writer 해제 후 공식 Archive하세요.
자동 목록 판정·사용자 화면 확인·실시간 반영을 각각 기록하고, 확인하지 않은 동작은 미검증으로 보고해 주세요.
```

### Claude Code를 호출하게 할 때

```text
Claude Code 작업을 호출할 때 VS Code Claude Code 확장과 같은 Host, OS 사용자, CLAUDE_CONFIG_DIR을 사용해 주세요.
확장에 동봉된 claude 실행 파일의 절대 경로와 --version, --help를 확인해 주세요.
자식 프로세스 환경에서 CLAUDECODE, CLAUDE_CODE_SESSION_ID, CLAUDE_CODE_CHILD_SESSION 등 부모 세션 변수를 제거하고 CLAUDE_CODE_ENTRYPOINT=claude-vscode를 지정해 주세요.
cwd는 VS Code에서 연 Workspace 폴더와 같은 절대 경로로 하고, 새 작업은 -p --session-id <새 UUID> --output-format stream-json --verbose 로 실행하며 프롬프트는 stdin으로 전달해 주세요.
기존 권한 모드·허용 도구·모델 정책을 유지하고, --no-session-persistence와 bypassPermissions는 사용하지 마세요.
세션 ID를 작업 ID와 연결해 기록하고, 후속 작업에는 --continue 대신 같은 cwd에서 --resume <세션 ID>를 사용해 주세요.
종료 코드와 result 이벤트의 is_error를 확인하고, stdout JSONL과 stderr를 분리해 보관해 주세요.
CLI 실행 결과와 VS Code 목록·대화 표시 확인 결과를 구분하고, 확인하지 않은 화면 동작은 미검증으로 보고해 주세요.
```

### Antigravity (`agy`)를 호출하게 할 때

```text
Antigravity(agy) 작업을 호출할 때 VS Code Antigravity 확장과 같은 Host, OS 사용자, ~/.gemini/antigravity 데이터 디렉터리를 사용해 주세요.
~/.gemini/bin/agy[.exe] 실행 파일의 절대 경로와 --app_data_dir=antigravity --version, ~/.gemini/config/projects/*.json의 대상 프로젝트 이름(또는 UUID)을 확인해 주세요.
새 작업은 첫 실행부터 --app_data_dir=antigravity --project <프로젝트 이름 또는 UUID> --add-dir <배정된 Workspace 표준 절대 경로> --disable-slash-commands --input-format stream-json --output-format stream-json 으로 실행하고, 프롬프트는 stdin에 {"event":"user","message":{"role":"user","content":"..."}} 한 줄을 쓴 뒤 즉시 stdin(EOF)을 닫아 전달해 주세요.
Windows에서는 --add-dir와 cwd에 \\?\ 접두사가 없는 표준 절대 경로를 전달해 주세요.
기존 모드(--mode plan 또는 accept-edits)·모델·추론 강도(--effort) 정책을 유지하고, --dangerously-skip-permissions는 사용하지 마세요.
init 이벤트의 conversation_id를 작업 ID와 연결해 기록하고, 후속 작업에는 --continue(-c) 대신 --conversation <conversation_id>를 사용해 주세요.
종료 코드뿐 아니라 result 이벤트의 status, denied_actions, response를 함께 확인해 소프트 거부 여부를 판정하고, stdout JSONL과 stderr를 분리해 보관해 주세요.
CLI 실행 결과와 VS Code 목록·대화 표시 확인 결과를 구분하고, 확인하지 않은 화면 동작은 미검증으로 보고해 주세요.
```

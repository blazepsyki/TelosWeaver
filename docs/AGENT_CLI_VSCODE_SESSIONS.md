# 외부 에이전트가 실행한 CLI 세션을 VS Code에서 확인하는 방법 (Codex·Claude Code)

> 확인일: 2026-10-25 (KST)
>
> 대상: 다른 에이전트·스크립트가 `codex exec` 또는 `claude -p`로 실행한 작업을 VS Code의 **OpenAI Codex 확장** 또는 **Claude Code 확장** 대화 목록에서 찾는 경우. Claude Code가 Codex를 부르는 경우와 Codex가 Claude Code를 부르는 경우를 모두 다룹니다.

## 1. 요약

두 CLI 모두 비대화형으로 실행하면 기본값으로는 **자동화 출처**로 기록되어 VS Code 확장의 기본 대화 목록에서 빠집니다. 호출 측에서 출처를 대화형 분류로 지정하고, 확장과 같은 저장소·작업 경로를 사용해야 목록 조회 대상에 들어갑니다.

| 항목 | Codex CLI (→ OpenAI Codex 확장) | Claude Code CLI (→ Claude Code 확장) |
|---|---|---|
| 비대화형 기본 출처 | `exec` (목록 기본 필터 밖) | `entrypoint: "sdk-cli"` (목록 기본 필터 밖) |
| 목록에 포함시키는 방법 | `codex exec --thread-source cli` | 환경 변수 `CLAUDE_CODE_ENTRYPOINT=claude-vscode` |
| 세션 저장소 | `CODEX_HOME` (기본 `~/.codex`) | `CLAUDE_CONFIG_DIR` (기본 `~/.claude`)의 `projects/<인코딩된 cwd>/` |
| 작업 경로 지정 | `--cd <절대 경로>` | 프로세스 `cwd` (전용 옵션 없음) |
| 작업 경로와 목록의 관계 | 프로젝트 필터를 바꾸거나 전체 목록에서 확인 가능 | 확장은 **VS Code 첫 Workspace 폴더와 같은 cwd**의 세션만 조회. 하위 폴더·Worktree 세션은 기본 목록에 나오지 않음 |
| 세션 ID 수집 | `--json`의 `thread.started.thread_id` | `--session-id <UUID>`로 미리 지정하거나 `stream-json`의 `system/init.session_id` |
| 영속성을 끄는 옵션 (사용 금지) | `--ephemeral` | `--no-session-persistence` |
| 기존 자동화 세션 전환 | `codex exec fork <ID> --thread-source cli` | `claude -p --resume <ID> --fork-session` + 환경 변수 |

이 절차는 **대화 목록의 조회 대상에 포함시키기 위한 방법**입니다. 실행 중인 CLI 작업의 모든 진행 내용이 확장 화면에 실시간으로 반영되거나, 확장에서 그 프로세스를 제어할 수 있다는 의미는 아닙니다. 자동화 측에서는 JSONL 이벤트와 프로세스 종료 코드를 계속 수집합니다. 실행 중에는 CLI 호출자 한쪽에서만 작업을 지시하고, VS Code에서 후속 작업을 보낼 때는 기존 실행의 종료를 먼저 확인합니다.

## 2. 공통 원칙

1. VS Code에서 사용하는 확장의 CLI 실행 파일을 확인하고, 호출 에이전트에 그 절대 경로를 전달합니다. 버전별 디렉터리는 업데이트 때 바뀌므로 영구 고정하지 않습니다. 프로젝트의 CLI 탐지 원칙은 [Spec §7](DEVELOPMENT_v1.5.md)을 따릅니다.
2. CLI와 확장을 같은 Host·OS 사용자·세션 저장소(`CODEX_HOME` 또는 `CLAUDE_CONFIG_DIR`)에서 실행합니다. 터미널에서만 환경 변수를 바꾸면 이미 실행 중인 확장에는 반영되지 않습니다.
3. 세션 영속성을 끄는 옵션(`--ephemeral`, `--no-session-persistence`)을 사용하지 않습니다.
4. 셸 문자열을 조합하지 않고 실행 파일과 인자 배열을 분리해 실행하며, 프롬프트는 stdin으로 전달합니다.
5. 세션 ID를 호출자의 작업 ID와 연결해 기록하고, 후속 작업에는 `--last`·`--continue` 대신 그 ID를 사용합니다.
6. stdout JSONL과 stderr를 분리해 보관하고, 세션 시작 이벤트가 아니라 완료·실패 이벤트와 종료 코드로 작업 결과를 판정합니다.
7. 출처 지정은 표시 분류만 바꿉니다. 승인·Sandbox·권한·모델 설정은 해당 작업에 허용된 기존 정책을 유지합니다.
8. 검증 기록에는 **CLI 작업 성공 여부**, **VS Code 목록 표시 여부**, **대화 내용 열기 여부**, **실시간 진행 반영 여부**를 각각 남기고, 직접 확인하지 않은 항목은 미검증으로 기록합니다.

---

# A. Codex CLI

## A1. 확인한 사실과 아직 확인하지 않은 동작

| 항목 | 근거와 확인 결과 |
|---|---|
| 설치된 CLI | Windows의 `codex-cli 0.164.0-alpha.11.1` |
| 설치된 Codex 확장 | `openai.chatgpt` 버전 `26.1021.22027` |
| 출처 지정 옵션 | 설치된 `codex exec --help`와 `codex exec fork --help`에서 `--thread-source <SOURCE>` 확인. 새 세션 또는 fork한 세션의 출처를 지정하는 옵션 |
| 기본 목록 필터 | 공식 App Server 문서에서 `thread/list`의 `sourceKinds`가 생략되거나 `[]`이면 `cli`, `vscode`만 조회한다고 명시. `exec`, `appServer`는 별도 분류 |
| 설치된 확장의 조회 코드 | 확장 `out/extension.js`의 목록 조회에서 빈 배열인 `sourceKinds`와 `useStateDbOnly: true` 전달 확인 |
| 세션 영속성 | 설치된 CLI 도움말에서 `--ephemeral`은 세션 파일을 디스크에 저장하지 않는 옵션으로 확인 |
| 기존 Adapter 실행 방식 | 이 저장소의 `crates/adapters/src/codex.rs`는 `app-server --stdio`를 실행하고 Thread/Turn 요청을 보냄 |
| 실제 VS Code 화면 표시 | 이 문서 작성 과정에서 새 작업 세션 생성·fork·화면 표시 시험은 실행하지 않음 |
| Linux·Remote SSH 화면 표시 | 이 문서의 절차로 직접 시험하지 않음. 기존 Provider 실행 시험과 별개의 확인 항목 |

**판단:** 출처를 `cli`로 지정하면 확인한 기본 목록 필터의 대상에 들어갑니다. 실제 화면 표시는 동일 저장소 사용, 확장의 추가 필터, 목록 갱신과 색인 상태에도 영향을 받으므로 아래 확인 절차로 판정합니다. CLI 도움말에 옵션이 없거나 확장 버전이 바뀐 경우에는 이 판단을 그대로 적용하지 않고 다시 확인합니다.

공식 근거: [App Server의 Thread 목록 조회](https://learn.chatgpt.com/docs/app-server), [CLI 옵션·fork·resume](https://learn.chatgpt.com/docs/cli/reference), [비대화형 실행과 JSONL 출력](https://learn.chatgpt.com/docs/non-interactive-mode).

`--thread-source`의 구체적인 옵션 지원은 위 설치 버전의 도움말로 확인했습니다. 공식 CLI 문서에 같은 옵션이 항상 기재되어 있다고 가정하지 않습니다.

## A2. 호출 전에 실행 환경 확인

### Windows 로컬

VS Code 통합 터미널에서 실행 파일 후보와 옵션을 확인합니다.

```powershell
Get-Command codex -All | Select-Object CommandType, Source
codex --version
codex exec --help
codex exec fork --help
$env:CODEX_HOME
```

이 환경에서 확인한 실행 파일은 다음 경로입니다.

```text
C:\Users\Psyki-HOME\.vscode\extensions\openai.chatgpt-26.1021.22027-win32-x64\bin\windows-x86_64\codex.exe
```

`Get-Command` 결과가 여러 개이면 Codex 확장이 사용하는 실행 파일인지 확인한 뒤 선택합니다. 호출 에이전트에서도 선택한 절대 경로에 `--version`과 `exec --help`를 실행해 버전과 `--thread-source` 지원을 확인합니다.

`CODEX_HOME`을 별도로 지정하지 않았다면 기본 저장소는 `~/.codex`입니다. 이 Windows 사용자에서는 `C:\Users\Psyki-HOME\.codex`입니다. 사용자 지정 경로를 사용한다면 CLI 자식 프로세스와 Codex 확장의 실행 환경에 같은 절대 경로가 전달되어야 합니다.

### Linux·WSL·Remote SSH

아래 명령은 Codex 확장이 실행되는 Host의 터미널에서 확인합니다.

```sh
hostname
id -un
printf '%s\n' "${CODEX_HOME:-$HOME/.codex}"
command -v codex
codex --version
codex exec --help
```

확장 동봉 CLI가 `PATH`에 없으면 `command -v codex` 결과만으로 설치 여부를 판단하지 말고, 해당 Host에 설치된 확장에서 실행 파일을 찾습니다.

- Windows 로컬 확장과 WSL의 CLI는 기본 저장소가 다릅니다. 같은 프로젝트 파일에 접근할 수 있다는 사실만으로 대화 목록을 공유하지는 않습니다.
- Remote SSH 창에서는 확장이 실제로 로컬과 원격 중 어디서 실행되는지 먼저 확인합니다. 원격 확장이라면 호출 에이전트도 같은 원격 사용자·저장소에서 CLI를 실행합니다.
- 예를 들어 `psyki-nas`에서 CLI 작업을 실행했다면 해당 원격 환경의 확장에서 확인합니다. Windows 로컬 창에 자동 동기화된다고 가정하지 않습니다.
- 경로 표기나 심볼릭 링크 때문에 프로젝트 필터가 달라지지 않도록 확장에서 연 작업 경로와 CLI의 실제 `cwd`를 함께 기록합니다.

이 Host 구분은 [Claude Code CLI](#b-claude-code-cli)에도 같이 적용됩니다.

## A3. 새 작업을 위임하는 명령

### PowerShell

`$codexExe`는 앞 단계에서 확인한 실행 파일 경로입니다. 예시는 파일 수정이 없는 분석 요청입니다.

```powershell
$codexExe = 'C:\Users\Psyki-HOME\.vscode\extensions\openai.chatgpt-26.1021.22027-win32-x64\bin\windows-x86_64\codex.exe'
$workspace = (Resolve-Path 'D:\Downloads\Vibe_dev\TelosWeaver').Path
$prompt = 'AGENTS.md와 README.md를 읽고 프로젝트 구조를 한국어로 설명해 주세요. 파일은 수정하지 마세요.'
$OutputEncoding = [System.Text.UTF8Encoding]::new($false)

$prompt | & $codexExe exec --thread-source cli --cd $workspace --json -
$runExitCode = $LASTEXITCODE
```

실제 개발 작업에서는 `$workspace`에 [배정 규칙](phase-0/WORK_ALLOCATION.md)에 따른 Worktree 경로를 지정합니다. 대화 표시를 위해 작업 경로를 대표 저장소로 바꾸지는 않습니다. `--thread-source`는 실행 권한을 바꾸는 옵션이 아닙니다.

### 에이전트 코드에서 프로세스를 직접 실행하는 경우

다음 Python 예시는 호출 형태와 세션 ID 수집을 설명합니다. `subprocess.run`이 끝난 뒤 이벤트를 읽는 예시이므로 실시간 진행 표시가 필요하면 stdout을 줄 단위로 읽는 방식으로 구성합니다.

```python
import json
import os
import subprocess
from pathlib import Path

codex_exe = Path(
    r"C:\Users\Psyki-HOME\.vscode\extensions\openai.chatgpt-26.1021.22027-win32-x64"
    r"\bin\windows-x86_64\codex.exe"
)
workspace = Path(r"D:\Downloads\Vibe_dev\TelosWeaver").resolve(strict=True)
prompt = "AGENTS.md와 README.md를 읽고 프로젝트 구조를 설명해 주세요. 파일은 수정하지 마세요."

result = subprocess.run(
    [str(codex_exe), "exec", "--thread-source", "cli",
     "--cd", str(workspace), "--json", "-"],
    input=prompt,
    cwd=workspace,
    env=os.environ.copy(),
    encoding="utf-8",
    capture_output=True,
    shell=False,
)

thread_ids = []
for line in result.stdout.splitlines():
    event = json.loads(line)
    if event.get("type") == "thread.started":
        thread_ids.append(event["thread_id"])

print(json.dumps({"sessionIds": thread_ids, "exitCode": result.returncode}))
```

호출자는 CLI 절대 경로·버전, Host, Workspace, 실제 `CODEX_HOME`, 세션 ID, 종료 코드와 오류 정보를 작업 기록에 남깁니다. `thread.started`는 세션 시작 알림이며 작업 성공 판정이 아닙니다.

## A4. 같은 세션에 후속 작업 보내기

```powershell
& $codexExe exec resume '<SESSION_ID>' --json '앞선 분석에서 확인한 수정 사항을 적용해 주세요.'
```

여러 에이전트가 실행하는 환경에서는 `--last`로 대상을 고르지 않습니다. `resume`에 출처 옵션을 붙였다고 기존 `exec` 세션의 분류가 바뀐다고 가정하지 않습니다. 출처 지정 도움말의 적용 대상은 새 세션과 fork입니다.

## A5. 이미 만들어진 exec 세션을 목록 조회 대상에 포함하기

기존 실행의 종료를 확인한 뒤 원본 내용을 복사한 새 세션에 `cli` 출처를 지정합니다.

```powershell
& $codexExe exec fork '<SESSION_ID>' --thread-source cli --json
```

프롬프트를 생략하면 추가 Turn 없이 fork를 생성합니다. 새 세션 ID는 원본과 다르므로 원본 ID와 새 ID를 함께 기록하고 이후 작업을 보낼 대상을 명확히 합니다. 원본 세션은 유지됩니다.

세션을 찾기 위한 터미널의 대화형 목록은 `& $codexExe resume --all --include-non-interactive`로 엽니다. 이 명령은 VS Code의 표시 필터를 바꾸거나 세션 출처를 변환하지 않습니다. 저장되지 않은 `--ephemeral` 세션은 복구할 수 없습니다.

## A6. VS Code에서 확인하고 문제를 구분하는 순서

1. 호출 측에서 세션 ID가 반환되었는지 확인하고, 전체 CLI 명령이 성공했는지는 종료 코드와 완료·실패 이벤트로 별도 판정합니다.
2. 같은 Host·사용자·`CODEX_HOME`을 사용하는 Codex 확장에서 대화 목록을 다시 엽니다. 현재 프로젝트로 제한하는 필터가 있다면 실제 작업한 Worktree를 선택하거나 전체 목록에서 확인합니다.
3. 목록이 갱신되지 않으면 명령 팔레트의 `Developer: Reload Window`로 확장을 다시 로드하고 조회합니다.
4. 제목이 비슷한 다른 대화와 혼동하지 않도록 프롬프트·작업 경로와 가능한 경우 세션 ID로 대상을 확인합니다.
5. 대화를 열어 사용자 요청과 응답이 보이는지 확인합니다. 목록에 제목만 보이는 것과 대화 내용이 정상적으로 열리는 것은 따로 기록합니다.

| 증상 | 확인할 항목 |
|---|---|
| CLI에서는 찾지만 VS Code 목록에는 없음 | 출처가 기본 `exec`인지, `--thread-source cli`로 새로 만들었는지, 추가 필터·Archive 여부·목록 갱신 |
| CLI와 확장에서 서로 다른 목록이 보임 | Host·OS 사용자·`CODEX_HOME`, 로컬/WSL/Remote SSH 확장 실행 위치 |
| 특정 프로젝트에서만 안 보임 | `--cd`·실제 `cwd`, Worktree, 프로젝트 필터, 경로 표기 차이 |
| `--thread-source`를 인식하지 못함 | 실제 실행한 CLI 절대 경로·버전·도움말. 확장 동봉 CLI와 `PATH`의 다른 CLI를 혼동했는지 |
| 목록에는 있지만 실행 중 진행 내용이 안 바뀜 | 실시간 반영은 미검증. 자동화의 JSONL 이벤트를 기준으로 진행 상태 확인 |
| fork 후에도 안 보이거나 내용이 열리지 않음 | 새 세션 ID·저장소·CLI 오류와 확장 로그 확인. 화면 표시 문제를 작업 실패나 성공으로 단정하지 않음 |

확장에서는 `useStateDbOnly: true`로 목록을 조회하는 코드를 확인했습니다. JSONL 파일이 있다는 사실만으로 목록의 색인 상태까지 정상이라고 판단하지 않습니다. 세션 JSONL이나 내부 SQLite의 `source`를 직접 수정하는 절차는 이 가이드에 포함하지 않습니다.

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

# C. 서로 호출하는 경우

| 호출 방향 | 호출 측에서 지정할 것 | 확인할 곳 |
|---|---|---|
| Claude Code → Codex | Codex 확장 동봉 `codex.exe`, `exec --thread-source cli --cd <Workspace> --json -` | OpenAI Codex 확장 대화 목록 ([A](#a-codex-cli)) |
| Codex → Claude Code | Claude Code 확장 동봉 `claude.exe`, `CLAUDE_CODE_ENTRYPOINT=claude-vscode`, cwd = VS Code Workspace, `-p --session-id <UUID>` | Claude Code 확장 세션 목록 ([B](#b-claude-code-cli)) |
| Claude Code → Claude Code | 부모 세션 변수 제거 후 `CLAUDE_CODE_ENTRYPOINT` 명시 ([B2](#호출자가-claude-code-세션-안에서-실행-중인-경우)) | Claude Code 확장 세션 목록 |
| Codex → Codex | `--thread-source cli` | OpenAI Codex 확장 대화 목록 |

- 호출하는 에이전트의 Sandbox가 상대 CLI의 세션 저장소(`~/.codex`, `~/.claude`) 쓰기나 네트워크 접근을 막으면 세션이 저장되지 않거나 실행이 실패할 수 있습니다. 이 경우 허용 범위를 넓히지 말고 해당 작업의 승인 정책에 따라 처리합니다. Codex Sandbox 안에서 Claude Code를 실행하는 경우는 이 문서에서 시험하지 않았습니다.
- 호출 측의 환경 변수가 자식 프로세스로 그대로 상속되므로, 상대 CLI의 출처·저장소 변수는 호출 코드에서 명시적으로 설정합니다.
- 양쪽 확장에서 같은 작업을 확인하려면 두 CLI 모두 같은 Workspace 경로에서 실행하고, 작업 기록에 Codex 세션 ID와 Claude 세션 ID를 함께 남깁니다.

# D. TelosWeaver Adapter와의 관계

현재 TelosWeaver Codex Adapter는 `codex exec`가 아니라 `app-server --stdio`와 `thread/start`·`thread/resume`·`thread/fork`를 사용합니다. 따라서 `--thread-source cli`를 Adapter 실행 인자에 그대로 추가하는 변경은 적용 대상이 아닙니다. App Server에서 `thread/list.sourceKinds`는 **조회할 출처를 고르는 필터**이며, 지정한다고 생성된 Thread의 출처가 바뀌지는 않습니다.

현재 TelosWeaver Claude Adapter(`crates/adapters/src/claude.rs`)는 `claude -p`를 실행하고 `CLAUDE_CODE_ENTRYPOINT`를 지정하지 않으므로, 상위 환경에 값이 없으면 세션이 `sdk-cli`로 기록되어 Claude Code 확장 목록에서 제외됩니다.

Adapter가 시작한 Run도 확장 목록에 보이도록 하는 요구는 Spec v1.5.3의 DEC-08·[§7.6](DEVELOPMENT_v1.5.md#76-vs-code-확장-대화-목록-표시)으로 확정했고, 각 Adapter 담당에게 [작업 지시](phase-0/evidence/PH0-INTEGRATION/VSCODE_LISTING_ASSIGNMENTS.md)로 배정했습니다. 구현·판정 전까지는 위 설명이 현재 상태입니다.

이 문서는 외부 에이전트의 CLI 호출 절차를 정리한 운영 문서이며 Adapter 구현, 공통 계약, Phase 0의 완료 판정은 변경하지 않습니다. 기존 구현·시험 근거는 [PH0-CODEX 검증 기록](phase-0/evidence/PH0-CODEX/README.md)과 [PH0-CLAUDE 검증 기록](phase-0/evidence/PH0-CLAUDE/README.md)을 참고합니다.

# E. 위임 에이전트에 전달할 지시 예시

### Codex를 호출하게 할 때

```text
Codex 작업을 호출할 때 VS Code Codex 확장과 같은 Host, OS 사용자, CODEX_HOME을 사용해 주세요.
확장에서 사용하는 CLI의 절대 경로와 --version, exec --help를 확인해 주세요.
새 작업은 exec --thread-source cli --cd <배정된 Workspace 절대 경로> --json - 로 실행하고 프롬프트는 stdin으로 전달해 주세요.
기존 승인·Sandbox·모델 정책을 유지하고, --ephemeral은 사용하지 마세요.
thread.started의 thread_id를 작업 ID와 연결해 기록하고, 후속 작업에는 --last 대신 해당 ID를 사용해 주세요.
종료 코드와 완료·실패 이벤트를 확인하고, stdout JSONL과 stderr를 분리해 보관해 주세요.
CLI 실행 결과와 VS Code 목록·대화 표시 확인 결과를 구분하고, 확인하지 않은 화면 동작은 미검증으로 보고해 주세요.
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

# VS Code 확장 대화 목록 표시 작업 지시 (DEC-08)

| 항목 | 값 |
|---|---|
| 작성 | `claude-opus-5.5/session-be1c1774` (사용자 지시로 Spec 개정·배정 작성), 2026-10-25 |
| 기준 | `master` (Spec v1.5.3 반영 Commit), 계약 `7497877` (변경 없음) |
| 근거 | Spec DEC-08·[§7.6](../../../DEVELOPMENT_v1.5.md#76-vs-code-확장-대화-목록-표시)·AGENT-05·§19.1, 조사·수동 절차 [AGENT_CLI_VSCODE_SESSIONS.md](../../../AGENT_CLI_VSCODE_SESSIONS.md) |
| 배정 | PH0-CLAUDE = `claude-opus-5.5/session-c9336d2c`, PH0-CODEX = Codex, PH0-AGY = Gemini (각 Adapter 기존 담당). 설정 연결·기록 = PH0-INTEGRATION |
| 목표 | Adapter가 시작한 Run의 세션이 각 Provider의 VS Code 확장 대화 목록에 기본으로 나타나고, 운영자가 숨길 수 있으며, 지원 여부가 Capability `vscode_listing`으로 정직하게 표시됨 |

## 1. 공통 규칙

1. **소유권:** 각 담당은 자기 Adapter의 독점 경로(Spec §19.5.3)만 수정합니다. 공통 계약 변경은 필요 없도록 설계했습니다(`Capability` 키는 문자열, 환경 변수 제거는 기존 `CommandSpec.env`의 `None`으로 가능). 그래도 계약·공통 파일이 필요하면 배정표 §4에 변경 요청을 올립니다.
2. **설정 형태:** Adapter 설정에 표시 모드(`listed` 기본 / `hidden`)를 받는 필드를 추가하고, **필드를 지정하지 않으면 `listed`**로 동작하게 합니다. INTEGRATION이 운영자 정책과 연결하기 전에도 기본 표시가 되도록 하기 위함입니다. 타입 이름은 각 Adapter 모듈 안에서 정합니다(공통 타입을 만들지 않음).
3. **Capability:** `detect()`에 `vscode_listing`을 추가합니다. 자동 판정이 통과한 버전·Host만 `supported`/`environment-tested`, 그 전에는 `unverified`, 수단이 없으면 `unsupported`(사유)입니다. 판정 기준 확장 버전을 사유나 증거에 남깁니다.
4. **금지:** Provider 저장소(JSONL·SQLite·요약 DB)를 직접 수정해 출처를 바꾸지 않습니다. 세션 영속성을 끄는 옵션을 쓰지 않습니다. 표시를 위해 실행 cwd를 바꾸지 않습니다.
5. **시험:** Mock·Contract 시험으로 인자·환경·Protocol 필드를 확인하고, 실환경은 가벼운 모델·저장소 밖 임시 폴더에서 수행합니다(배정표 §6 "Claude 실환경 시험 조건"과 Provider별 기존 조건). 자동 판정 시험은 `#[ignore]` 실환경 시험으로 둡니다.
6. **화면 확인:** 확장 화면에서 보이고 열리는지는 사용자 VS Code 조작이 필요합니다. 필요하면 사용자에게 **열 폴더·세션 ID·확인할 화면**을 단계별로 요청하고, 하지 못했으면 미검증으로 기록합니다. 자동 판정과 화면 확인을 증거에서 다른 행으로 적습니다.
7. **시험 세션 정리:** 세션 ID를 증거에 남기고 병합 시점에 정리합니다(배정표 §6 "시험 세션 정리"). 화면 확인용으로 사용자 Workspace에서 만든 세션도 같은 규칙입니다.
8. **Branch·Worktree:** 최신 `master`에서 `D:\worktree\TelosWeaver\<작업>-listing`, Branch `ph0/<작업>-listing`. 배정표는 자기 행과 §7에 `master` 단독 커밋으로 착수·완료를 기록합니다(§7 표 행 사이에 빈 줄 금지).
9. **끝나면:** 검사(`cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`) 결과와 함께 INTEGRATION에 통합을 요청합니다. Linux(`psyki-nas`)는 이번 필수 범위가 아니며, 하면 Host 열을 나눠 기록합니다.

## 2. PH0-CLAUDE — `ph0/claude-listing`

| # | 작업 | 확인 방법 |
|---|---|---|
| L1 | 자식 환경에 `CLAUDE_CODE_ENTRYPOINT`를 명시: `listed` → `claude-vscode`, `hidden` → `sdk-cli`. 상속된 부모 세션 변수(`CLAUDECODE`, `CLAUDE_CODE_SESSION_ID`, `CLAUDE_CODE_CHILD_SESSION`, `CLAUDE_CODE_SESSION_ATTENDED`, `CLAUDE_CODE_MESSAGING_SOCKET`, `CLAUDE_CODE_MESSAGING_TOKEN`, `CLAUDE_PID`)는 `CommandSpec.env`에 `None`으로 넣어 제거 | Mock 시험: 두 모드의 `CommandSpec.env` 값, 부모 변수 제거 |
| L2 | `vscode_listing` Capability | Mock 시험: 실환경 판정 전 `unverified` |
| L3 | 자동 판정 실환경 시험: Fresh·Resume·NativeFork 각 1회 후 `~/.claude/projects/<cwd 인코딩>/<세션 ID>.jsonl`의 위치와 첫 줄 부근 `entrypoint`가 `claude-vscode`인지, `hidden`이면 `sdk-cli`인지 | 판정 규칙은 확장 2.1.310 `extension.js`(programmatic 판정: `sdk-cli`·`sdk-ts`·`sdk-py`, `sessionKind` `daemon*`; 목록 범위: 첫 Workspace 폴더와 같은 cwd, Worktree 제외). 확장 버전을 증거에 기록 |
| L4 | 화면 확인(사용자): 사용자가 연 Workspace에서 `listed` Run 1개 실행 → Claude Code 확장 과거 대화 목록에서 보이는지·열리는지 | 미실행 시 미검증 |
| L5 | 증거 `evidence/PH0-CLAUDE/VSCODE_LISTING.md` | 시험 세션 ID·확장/CLI 버전·판정 |

참고: 2026-10-25 수동 시험에서 `-p`(환경 변수 없음) → `sdk-cli`, `claude-vscode` 지정 → 전체 줄 `claude-vscode`, `--resume --fork-session` + 지정 → 새 세션 전체 `claude-vscode`, 지정 없이 Resume → 같은 파일에 `sdk-cli` 줄 추가를 확인했습니다([조사 문서](../../../AGENT_CLI_VSCODE_SESSIONS.md) B1). Resume에도 반드시 같은 값을 지정합니다.

## 3. PH0-CODEX — `ph0/codex-listing`

| # | 작업 | 확인 방법 |
|---|---|---|
| X1 | 수단 확정 실험: 설치 CLI의 `app-server generate-json-schema`로 `ThreadStartParams`·`ThreadForkParams`의 `threadSource`(문자열, "analytics source classification") 확인. `threadSource: "cli"`(및 필요하면 `"vscode"`)로 만든 Thread의 목록 분류가 바뀌는지, 바뀌지 않으면 다른 공개 Protocol 수단(`initialize`의 `clientInfo` 등)이 있는지 확인 | App Server `thread/list`를 `sourceKinds` 생략으로 호출해 포함 여부 확인. Thread 응답의 `source` 값 기록 |
| X2 | 확정한 수단을 `thread/start`·`thread/fork`에 적용(`listed`), `hidden`은 지정 생략. `ephemeral: false` 명시 | Mock 시험: 요청 JSON |
| X3 | `vscode_listing` Capability. 수단을 찾지 못하면 `unsupported`(사유: 확인한 필드·버전) | Mock 시험 |
| X4 | 자동 판정 실환경 시험: Fresh·Resume·NativeFork 후 기본 `thread/list`에 포함, `hidden`이면 제외. Thread의 cwd와 확장의 프로젝트 필터 관계 기록 | 확장 26.1021.22027 기준(빈 `sourceKinds`, `useStateDbOnly: true`) |
| X5 | 화면 확인(사용자): Codex 확장 대화 목록에서 보이는지·열리는지 | 미실행 시 미검증 |
| X6 | 증거 `evidence/PH0-CODEX/VSCODE_LISTING.md`. 시험 Thread는 `thread/archive` | |

## 4. PH0-AGY — `ph0/agy-listing`

| # | 작업 | 확인 방법 |
|---|---|---|
| A1 | 기존 `--app_data_dir=antigravity`(`ExtensionShared`)를 `listed`, CLI 기본 디렉터리를 `hidden`으로 대응시키는 설정 표현 정리(기존 동작 유지) | Mock 시험 |
| A2 | `vscode_listing` Capability. `hidden`일 때 로그인 상태가 디렉터리별이라 실패할 수 있음(AGY-DATA-02)을 사유에 기록 | Mock 시험 |
| A3 | 자동 판정 실환경 시험: 확장이 대화 목록에 쓰는 기록(`conversation_summaries.db` 등, 확장 코드로 확인)에 Run이 포함되는지, Workspace별 필터가 있는지 | 확장 1.9.0·`agy` 1.4.7 기준 기록 |
| A4 | 화면 확인(사용자): Antigravity 확장 대화 기록에서 보이는지·열리는지 | 미실행 시 미검증 |
| A5 | 증거 `evidence/PH0-AGY/VSCODE_LISTING.md`. 시험 대화는 `brain/<id>`·`conversations/<id>.db`(`-wal`·`-shm`) 정리, 요약 DB 잔여 여부 기록 | |

## 5. PH0-INTEGRATION — 담당 결과 통합 후

| # | 작업 |
|---|---|
| I1 | `crates/core/src/integration/policy.rs`의 Provider별 정책에 `vscodeListing`(`listed` 기본/`hidden`)을 추가하고 각 Adapter 설정으로 전달. Agy는 기존 `data_dir`과 충돌하지 않게 대응 규칙을 정함 |
| I2 | 실제 적용 값·방식을 Run의 승인 Snapshot 기록에 추가(§7.5 규칙 6과 같은 위치) |
| I3 | `phase0_g2` 계열 실환경 시험에 세 Provider `vscode_listing` 자동 판정 결과를 모아 G3 근거로 기록, §19.1 새 항목 판정 |
| I4 | 각 담당 Branch 검토·병합·Worktree 정리, 배정표 갱신 |

## 6. 범위 밖

- Extension(PH0-EXT)의 Run 상세 표시(Provider·Native 세션 ID·Workspace 경로)는 P0 Task Detail(§17) 범위로 후속 배정합니다. "해당 폴더 열기"는 P1 검토입니다.
- 실행 중 확장 쪽 입력의 기술적 차단은 하지 않습니다(§7.6 의미와 한계).
- Linux·Remote SSH 판정은 G3 Linux 작업과 함께 후속으로 합니다.

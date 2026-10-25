# TelosWeaver

서로 다른 AI 에이전트(Codex, Claude Code, Antigravity)를 사용하여 **목표 확인 → 계획 → 위임 → 실행 → 산출물 수집 → 검증 → 승인 판단 → 재작업 또는 완료**를 관리하는 로컬 우선 오케스트레이터입니다.

- 기준 설계: [docs/DEVELOPMENT_v1.5.md](docs/DEVELOPMENT_v1.5.md)
- 진행 현황: [docs/phase-0/WORK_ALLOCATION.md](docs/phase-0/WORK_ALLOCATION.md) (작업 배정·게이트·변경 요청의 단일 원본)
- 에이전트 작업 지침: [AGENTS.md](AGENTS.md)
- 외부 에이전트가 실행한 CLI 세션 표시(Codex·Claude Code): [VS Code에서 세션 확인](docs/AGENT_CLI_VSCODE_SESSIONS.md)

## 현재 상태 — Phase 0 (기술 검증)

| 게이트 | 상태 |
|---|---|
| G0 공통 계약 | 통과 |
| G1 대표 Provider 실행 | 통과 (Claude, Windows) |
| G2 전체 구성 통합 | 통과 확정 (Windows): 세 Provider·마스터 결정 검증·Extension·RN 연결, 각 담당 확인 완료 |
| G3 Phase 0 수용 | 대기: 사용자 Ubuntu 서버에서 공통 검사 통과, Linux·Remote SSH 실환경 시험 남음 |

**동작하는 것 (Windows 로컬에서 실제 CLI로 검증)**

- **Provider 실행:** 각 에이전트의 VS Code 확장이 포함·관리하는 CLI를 찾아(DEC-06) Managed 실행합니다. 시작·이벤트 스트림·완료·실패·취소·자식 프로세스 회수를 공통 이벤트로 보고합니다.

  | Provider | 확장 | 검증한 CLI | 비고 |
  |---|---|---|---|
  | Claude Code | `anthropic.claude-code` | 2.1.310 | Resume·Fork 동작 |
  | Codex | `openai.chatgpt` | codex-cli 0.164.0-alpha.11.1 | App Server, Resume·Fork 동작 |
  | Antigravity | `google.google-antigravity` (`~/.gemini/bin/agy`) | 1.4.7 | 확장 데이터 디렉터리 공유, Resume 동작, Fork 미지원 |

- **Core Daemon (`telos-weaver serve`):** 토큰 인증 Local API(Windows Named Pipe / Unix Socket, NDJSON), 운영자 정책 파일 기반 실행 승인, 실행 감독(멱등 시작·이벤트 보존·취소·Daemon 종료 시 활성 Run 정리).
- **VS Code Extension:** 지정한 Core 실행 파일을 명시적으로 시작하거나 실행 중인 Core에 연결해 인증·Health를 표시합니다(StatusBar·Explorer 상태 View·출력 창). UI를 닫아도 Core는 계속 실행되고, 다시 열면 같은 Core에 연결합니다. 알려진 Run ID의 이벤트를 읽기 전용으로 구독·재생합니다. 사용법은 [extension/README.md](extension/README.md).
- **마스터:** 마스터 에이전트(Phase 0 임시 Claude)가 구조화 결정을 내면 Core가 검증해 잘못된 결정·중복·권한 초과를 거부하고, 허용된 결정(`create_tasks`·`request_run`·`request_cancel`)을 집행해 결과를 다음 Turn에 돌려줍니다.

**아직 없는 것:** Reason Network의 제거 검증·Linux 확인(PH0-RN-08)과 메시지 기능의 외부 노출(Core는 RN을 띄워 `core.health`의 `rn`으로 알리고 내부 Rust API로만 씀), Extension의 전체 UI(Task 목록·마스터 대화·승인·Artifact), 영속 저장소·재시작 복구, Scheduler·Workspace 관리·검증·Artifact, Linux·Remote SSH 검증(사용자 Ubuntu 서버, [절차](docs/phase-0/LINUX_REMOTE_HOST.md)). 미구현 기능은 지원하는 것처럼 표시하지 않습니다.

## 저장소 구조 (Spec §3.4)

```
crates/
  schema/      telos-schema     공통 계약 타입, JSON Schema 생성 원본 (§4~§13)
  adapters/    telos-adapters   AgentAdapter Trait, 공용 프로세스 런타임(runtime), 확장 CLI 탐지(discovery),
                                Provider Adapter(claude·codex·agy) (§7)
  rn-client/   telos-rn-client  RN Fork Local RPC 클라이언트 — 계약 재노출만 (§12)
  core/        telos-core       Core Daemon, 실행 파일 telos-weaver (§3~§15)
                 api/           Local Core API 서버·인증
                 execution/     실행 감독(ExecutionSupervisor)
                 policy/        마스터 결정 검증, 신뢰된 정책 조회
                 master/        Master Controller
                 integration/   Phase 0 통합 구성: 운영자 정책 승인, Provider 탐지·등록, 종료 정리
schemas/       telos-schema에서 생성한 JSON Schema (커밋 대상)
extension/     VS Code Extension, TypeScript (§17) — 생성 타입만 사용, Core 연결·최소 상태 UI
rn-fork/       Reason Network Fork `telos-rn`(원본 v0.12.11, subtree) — DEC-07 정리 진행 중, 독립 Cargo Workspace
(docs/archive/의 이전 설계 문서는 로컬에만 보관하며 원격 저장소에 올리지 않습니다)
docs/          설계 문서, phase-0/에 배정표·WSL 절차·작업별 검증 증거
```

의존 방향은 `schema ← adapters, rn-client ← core`이며 Cargo 의존 선언으로 강제합니다. Extension은 Rust 코드에 의존하지 않고 `schemas/`에서 생성한 TypeScript 타입만 사용합니다.

```
telos-schema (Rust 타입) ──생성──▶ schemas/*.schema.json ──생성──▶ extension/src/generated/*.ts
```

## 실행

```sh
cargo run -p telos-core --bin telos-weaver -- --version

# 정책 파일 없이: Local API 인증·Health만 제공하고 Run은 모두 거부합니다.
cargo run -p telos-core --bin telos-weaver -- serve --project-root <path> --project-id <id> --host-id <id>

# 정책 파일로 실행 승인과 Provider 실행을 켭니다.
cargo run -p telos-core --bin telos-weaver -- serve --project-root <path> --project-id <id> --host-id <id> --policy policy.json
```

정책 파일은 Phase 0용 운영자 설정입니다. Local API 클라이언트가 보낸 경로·권한을 믿지 않고 이 파일에 승인된 Workspace·Provider·모델만 실행합니다. 정식 정책은 이후 `core/policy`가 대체합니다.

```json
{
  "revision": "local-1",
  "permissionProfiles": ["phase0"],
  "workspaces": [{ "id": "ws-1", "path": "D:/work/ws-1", "writable": true }],
  "claude": { "models": ["haiku"], "permissionMode": "acceptEdits", "allowedTools": ["Read", "Write"] },
  "codex": { "models": ["gpt-6-luna"], "effort": "low", "runTimeoutSecs": 180 },
  "agy": { "models": ["gemini-3.8-flash-low"], "effort": "low", "mode": "accept-edits" },
  "rn": { "executable": "D:/work/TelosWeaver/rn-fork/target/release/telos-rn.exe" }
}
```

Provider 절은 필요한 것만 둡니다. 시작 시 탐지 결과와 등록하지 못한 Provider의 사유를 출력합니다. `rn` 절이 있으면 Core가 그 `telos-rn`을 프로젝트별 상태 폴더(`<Core 상태>/rn`)로 띄우고 정지 시 함께 정지하며, 끊기면 최대 3회 다시 띄웁니다(`rn`은 직접 빌드: `cd rn-fork && cargo build --release`). RN이 없어도 Core와 Run은 동작하고 `core.health`의 `rn`이 Degraded가 됩니다. `bypassPermissions`·`--dangerously-skip-permissions` 같은 권한 우회는 거부합니다.

## 개발

### Rust

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

실제 CLI를 실행하는 실환경 시험은 사용량을 쓰므로 기본 실행에서 제외(`#[ignore]`)되어 있습니다. 명시적으로 실행합니다.

```sh
cargo test -p telos-core --test phase0_g1 -- --ignored --test-threads=1      # G1: Claude
cargo test -p telos-core --test phase0_g2 -- --ignored --test-threads=1      # G2: 세 Provider
cargo test -p telos-core --test phase0_master -- --ignored --test-threads=1  # 마스터
TELOS_RN_EXECUTABLE=<telos-rn 경로> cargo test -p telos-core --test phase0_rn -- --ignored --test-threads=1  # RN 연결
```

시험은 저장소 밖 임시 폴더에서 실행하며, 시험 세션은 코드 병합 시점에 정리합니다([배정표](docs/phase-0/WORK_ALLOCATION.md) §6).

`telos-schema`의 타입을 바꾸면 JSON Schema를 다시 생성합니다. 생성하지 않으면 `json_schemas` 테스트가 실패합니다.

```sh
UPDATE_SCHEMAS=1 cargo test -p telos-schema --test json_schemas
```

### Extension

```sh
cd extension
npm install
npm run build   # schemas/ → src/generated/ 타입 생성 후 tsc
npm test        # 단위 시험 (실제 Core·Extension Host 시험은 extension/README.md 참고)
```

### 병렬 작업

Phase 0은 작업 ID(PH0-*)별 독점 경로로 나누어 여러 에이전트가 병렬로 구현합니다(Spec §19.5). 착수 전 [배정표](docs/phase-0/WORK_ALLOCATION.md)에 자기 행을 기록하고, Worktree는 `D:\worktree\TelosWeaver\` 아래에 만듭니다. 공통 파일·계약 변경은 변경 요청(REQ-xxx)으로 처리합니다.

## 라이선스

다음 중 하나를 선택해 사용할 수 있습니다.

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))

별도로 밝히지 않는 한, 이 저장소에 제출한 기여는 Apache-2.0 정의에 따라 위 두 라이선스로 이중 라이선스되며 추가 조건은 붙지 않습니다.

`rn-fork/`의 Reason Network Fork는 원본의 LICENSE·NOTICE를 그대로 유지하고 수정한 파일에 변경 사실을 표시합니다.

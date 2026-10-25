# telos-core

Extension Host와 분리된 독립 프로세스로 실행되는 TelosWeaver Core Daemon입니다 (Spec §3.3). 실행 파일 이름은 `telos-weaver`입니다. Task/Run 상태, 정책·예산, 실행 감독, Workspace, 결과·검증·승인의 원본을 소유합니다.

## 모듈

새 모듈은 `src/<module>/`에 아래 이름으로 추가합니다. 소유 작업은 Spec §19.5.3의 Phase 0 독점 경로입니다.

### 구현됨 (Phase 0)

| 모듈 | 책임 | Spec | 소유 | 상태 |
|---|---|---|---|---|
| `api` | Local Core API 서버: Windows Named Pipe / Unix Socket, NDJSON(1 MiB 프레임), 시작마다 바뀌는 토큰 인증(사용자 전용 권한), `core.*`·`runs.*` 메서드, 구독·재생 | §3.3, §17 | HOST | Phase 0 최소 범위 |
| `execution` | Execution Supervisor: Run ID 발급, 멱등 시작, 이벤트 수집·검사·보존(메모리), 취소 접수와 실제 종료 구분, 신뢰된 `RunAuthorizer`로만 실행 | §7.2, §7.3, §9.1 | HOST | Phase 0 최소 범위 |
| `policy` | Decision Validator: 마스터 결정의 호출자·Plan·Epoch·Revision·Payload·kind·권한·한도·Task 트리·의존성 검사, 신뢰된 조회 `TrustedLookup` | §4.2, §13 | MASTER | Phase 0 최소 범위 |
| `master` | Master Controller: 마스터를 제어용 Task로 실행, JSON 결정 회수, 멱등 재생·충돌, `create_tasks`·`request_run`·`request_cancel` 집행, Resume 결과 왕복, `controller_epoch`, 결정 기록 | §4 | MASTER | Phase 0 최소 범위 |
| `integration` | Phase 0 통합 구성: 운영자 정책 파일 기반 Run 승인(`LocalRunPolicy`)과 Provider별 실행 조건, 확장 CLI 탐지·Adapter 등록, Daemon 종료 시 활성 Run 정리, RN Fork 시작·감시·재시작·정지(`RnService`, `core.health`의 `rn`) | §19.5.8, §12.3·§12.5 | INTEGRATION | 정식 정책·Snapshot 서비스가 대체할 임시 구성 |

OS 프로세스의 시작·입출력·종료·자식 회수(Windows Job Object, Unix Process Group)는 Core가 아니라 `telos-adapters::runtime`에 있습니다. Provider Adapter와 Core가 같은 런타임을 씁니다.

상태는 모두 메모리에 있으며 Daemon 재시작 후 복구하지 않습니다.

### 계획 (P0 이후)

| 모듈 | 책임 | Spec | 주요 요구사항 |
|---|---|---|---|
| `scheduler` | Task Registry, 의존성·순환 검사, 큐·동시성, 상태 전이 | §9 | TASK-01~06, DELEG-04·10 |
| `context` | Context Snapshot, Capsule, 입력 해석 | §6, §8 | CTX-01~04, SESS-06~07 |
| `workspace` | none/directory/git-worktree, Writer Lock, Diff | §10 | WS-01~02, GIT-01~05·08 |
| `artifact` | 불변 Artifact Version, Checksum, InputBinding 해석 | §5.3, §10 | ART-01~03 |
| `validation` | Validator Job, 승인 대상 고정, 재작업 | §11 | VAL-01, VERIFY-01~03·06~07 |
| `mcp` | Task MCP 여섯 도구, 연결 자격·Scope | §9.2 | DELEG-01~03, SAFE-08 |
| `persistence` | SQLite·Event Journal, 실행 의도, Migration | §14 | SAFE-06·13, OPS-03 |

`policy`의 정식 정책 서비스(경로·명령·Session 소유권·예산 잔액 조회)와 `master`의 미집행 kind 5종(검증·재작업·사용자 질문·완료 판정 등)도 P0 범위입니다.

## Phase 0에서 정한 항목

| 항목 | 결정 |
|---|---|
| 비동기 런타임 | Tokio |
| Local API 전송·접근 제어 | Windows Named Pipe(현재 사용자 SID만 허용하는 DACL), Unix Socket(`${XDG_RUNTIME_DIR}`), NDJSON. 토큰 파일은 프로젝트 밖 사용자 상태 디렉터리. 보안 설정에 실패하면 시작하지 않음 |
| 프로세스 감독 | Windows Job Object(생성 시점 연결), Unix Process Group. 구현은 `telos-adapters::runtime` |
| 마스터 결정 Payload 검증 | `telos-schema`의 kind별 타입으로 역직렬화(미정의 필드 거부) 후 `policy::validate`의 순서 검사 |
| Run 승인 | 신뢰된 `RunAuthorizer`만 허용. 정책 파일이 없으면 `DenyRuns`, 있으면 운영자 정책(`integration::LocalRunPolicy`) |

## 아직 정하지 않은 항목

- SQLite 접근 crate와 Migration 도구 (`persistence`)
- 정식 정책·Snapshot 서비스와 운영자 정책의 교체 방식

## 실행

```sh
telos-weaver --version
telos-weaver serve --project-root <path> --project-id <id> --host-id <id> [--policy <file>]
```

정책 파일 형식과 실환경 시험은 저장소 [README](../../README.md)를 참고합니다.

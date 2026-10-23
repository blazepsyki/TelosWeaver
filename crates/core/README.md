# telos-core

Extension Host와 분리된 독립 프로세스로 실행되는 TelosWeaver Core Daemon입니다 (Spec §3.3). 실행 파일 이름은 `telos-weaver`입니다. Task/Run 상태, 정책·예산, 실행 감독, Workspace, 결과·검증·승인의 원본을 소유합니다.

## 모듈 배치 계획

아직 구현된 모듈은 없습니다. 새 모듈은 `src/<module>/`에 아래 이름으로 추가합니다.

| 모듈 | 책임 | Spec | 주요 요구사항 |
|---|---|---|---|
| `api` | Local Core API (Named Pipe/Unix Socket), UI용 Snapshot·이벤트 커서 | §3.3, §17 | OPS-01, UI-07 |
| `master` | Master Controller — 마스터 세션, 결정 회수, `controller_epoch` | §4 | MASTER-01~05 |
| `policy` | Decision Validator — Schema·Revision·Epoch·권한·예산·Capability 검사 | §4.2, §13 | CFG-01~02, SAFE-01~02 |
| `scheduler` | Task Registry, 의존성·순환 검사, 큐·동시성, 상태 전이 | §9 | TASK-01~06, DELEG-04·10 |
| `execution` | Execution Supervisor — 프로세스 수명·취소·자식 회수, Adapter 이벤트 수집 | §7.3, §15 | SAFE-03, SESS-02 |
| `context` | Context Snapshot, Capsule, 입력 해석 | §6, §8 | CTX-01~04, SESS-06~07 |
| `workspace` | none/directory/git-worktree, Writer Lock, Diff | §10 | WS-01~02, GIT-01~05·08 |
| `artifact` | 불변 Artifact Version, Checksum, InputBinding 해석 | §5.3, §10 | ART-01~03 |
| `validation` | Validator Job, 승인 대상 고정, 재작업 | §11 | VAL-01, VERIFY-01~03·06~07 |
| `mcp` | Task MCP 여섯 도구, 연결 자격·Scope | §9.2 | DELEG-01~03, SAFE-08 |
| `persistence` | SQLite·Event Journal, 실행 의도, Migration | §14 | SAFE-06·13, OPS-03 |

## Phase 0에서 정할 항목

- 비동기 런타임과 Local API 전송 방식(Named Pipe/Unix Socket)·접근 제어
- SQLite 접근 crate와 Migration 도구
- MasterDecision `kind`별 Payload JSON Schema 검증 방식
- Windows Job Object / Unix Process Group 기반 프로세스 감독 구현

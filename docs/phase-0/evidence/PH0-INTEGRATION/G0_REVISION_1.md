# G0 개정 1 — 독립 검토 요청 판정과 처리

> **판정자:** PH0-INTEGRATION `claude-opus-5.5/session-c9336d2c`  
> **대상 요청:** REQ-002~009 ([요청서](G0_REVIEW_REQUESTS.md))  
> **기존 계약 기준:** `ae31eb2`  
> **판정일:** 2026-10-24

## 1. 판정 요약

| 요청 | 판정 | 처리 작업 | 요지 |
|---|---|---|---|
| REQ-002 | 채택 | INTEGRATION(인터페이스), HOST(구현·수용 시험) | `ProcessHandle`을 `&self` 기반 공유 Handle로 바꾸고 대기·종료의 취소 안전성·멱등성을 계약으로 명시 |
| REQ-003 | 채택 | INTEGRATION(인터페이스), HOST(구현·수용 시험) | 줄 자르기를 폐지하고 `LineTooLong`으로 보고, 스트림별 `ReadError` 종료, 버퍼 상한은 Backpressure |
| REQ-004 | 채택 | CONTRACT | `DecisionOutcome`을 `status` 태그 열거형으로 바꿔 거부 사유·효과 조합을 타입으로 강제 |
| REQ-005 | 채택 | CONTRACT | `SecretString`으로 토큰 Debug를 가리고 `ApiRequest` Debug의 `token` 값도 가림 |
| REQ-006 | 채택 | CONTRACT | Run 종료 이벤트 규칙 확정, 공통 검사 함수 `check_run_events`와 반례 Fixture 추가, 이벤트 Schema 0.2 |
| REQ-007 | 수정 채택 | CONTRACT | `projectKey` 정규화·시험 벡터, 인증 자료를 프로젝트 밖 사용자 디렉터리로 이동, 토큰 수명, 프레임 규칙·`frame_too_large`, API 0.2 |
| REQ-008 | 수정 채택 | CONTRACT(계약 경계), MASTER(신뢰된 조회 구현) | `DecisionContext` 검사 범위 명시, `project_id`·`allowed_host_ids`와 거부 코드 2종 추가. 조회 인터페이스는 MASTER가 `core/policy`에 정의 |
| REQ-009 | 채택 | CONTRACT | `DecisionPayload`에서 닿는 모든 구조체가 미정의 필드를 거부. 자유 형식 `JsonObject` 필드만 예외 |

## 2. 요청별 결정

### REQ-002 대기·입력·취소의 동시성

- `ProcessHandle`의 모든 메서드를 `&self`로 바꾸고 `Send + Sync`를 요구합니다. 구현은 내부 Actor 또는 동기화로 하며, 여러 Task가 같은 Handle을 `Arc`로 공유해 대기·입력·종료를 동시에 호출할 수 있습니다.
- `wait`는 취소 안전합니다. 대기 Future를 버려도 프로세스 추적에 영향이 없고 다시 호출할 수 있습니다. 종료 후에는 항상 같은 `ExitStatus`를 반환합니다.
- `terminate`는 멱등입니다. 동시·반복 호출은 하나의 종료 절차를 공유하고 같은 보고를 받습니다. 이미 끝난 프로세스에는 실제 종료 원인(`Exited`)을 보고합니다. 자연 종료와 경합하면 실제로 먼저 일어난 사건을 보고합니다.
- 종료 뒤 `write_stdin`·`close_stdin`은 `StdinUnavailable`입니다.
- 공개 인터페이스 변경은 G0 계약(공용 프로세스 경계) 개정이며 HOST가 미배정이라 INTEGRATION이 인터페이스만 바꿉니다. 구현과 요청서의 수용 시험은 HOST 담당입니다.

### REQ-003 출력 오류와 상한

- 상한을 넘은 줄은 **잘라서 전달하지 않습니다.** 줄 전체를 버리고 `LineTooLong { total_bytes }`를 보냅니다. Provider는 이를 Protocol 오류로 처리하며 정상 메시지로 해석하지 않습니다.
- 각 스트림은 정확히 한 번 `Closed`(EOF) 또는 `ReadError`(읽기 실패)로 끝납니다. 이후 그 스트림의 항목은 없습니다.
- `max_buffered_bytes`에 도달하면 읽기를 멈춥니다(Backpressure). 출력을 버리지 않으므로 기존 `Overflow`를 폐지합니다. 소비자가 멈추면 자식 프로세스도 쓰기에서 멈출 수 있으며 `terminate`는 계속 동작해야 합니다.
- 출력 상한(이 계층)과 Local API 프레임 상한(REQ-007)은 별개입니다.

### REQ-004 DecisionOutcome

- `status`를 태그로 하는 열거형으로 바꿉니다. `accepted`는 `effects`만, `rejected`는 `rejection`만 가지며 다른 조합과 미정의 필드는 역직렬화에서 거부합니다. Wire 필드 이름은 유지합니다.
- `replayed: true`는 같은 멱등 키·같은 Payload의 재요청에 처음 기록한 결과를 그대로 반환했다는 뜻이며 `effects`는 처음 집행한 효과입니다. 다시 집행하지 않습니다.
- 부분 집행은 Phase 0 계약에 두지 않습니다. 필요해지면 별도 상태로 정의합니다.

### REQ-005 토큰 보호

- `SecretString`(Wire는 일반 문자열)을 도입하고 Debug는 `"<redacted>"`로 출력합니다. 원문은 `expose()`로만 꺼냅니다.
- `ApiRequest`의 Debug는 `params` 최상위의 `token` 값을 가립니다. 로그에는 Debug 형식만 쓰고 `params` 원문을 직접 출력하지 않는 것을 HOST·EXT 규칙으로 둡니다.

### REQ-006 Run 이벤트 종료

- Run의 마지막 이벤트(종료 이벤트)는 다음 중 하나입니다. 종료 이벤트 뒤에는 어떤 이벤트도 없습니다.
  - `process_exited`: 그 Run 전용 프로세스가 끝났을 때. 두 출력 스트림이 `Closed`/`ReadError`에 도달하고 자식 회수를 시도한 뒤 보냅니다.
  - `turn_finished`: 프로세스가 Run보다 오래 사는 경우.
  - `error`(`terminal: true`): 프로세스가 시작되지 않았거나 추적을 잃어 종료를 확인할 수 없을 때. 존재하지 않은 프로세스의 `process_exited`를 만들지 않습니다.
- `start_run`이 오류를 반환한 Run은 이벤트를 남기지 않으며 `events`는 `UnknownRun`입니다.
- 전용 프로세스 Run에서 `turn_finished` 뒤에는 `usage`·`error`(비종료)·`process_exited`만 올 수 있습니다.
- 취소 접수 뒤의 늦은 이벤트도 순서대로 보존합니다. `sequence`는 재구독에서 다시 매기지 않습니다.
- 위 규칙을 `telos_schema::check_run_events`로 검사할 수 있게 하고, 시작 실패·취소·지속형·추적 유실 Fixture를 추가합니다. 의미가 바뀌므로 `PROVIDER_EVENT_SCHEMA_VERSION`을 `0.2`로 올립니다.

### REQ-007 Local API 주소·인증·프레임

- `projectKey`: 프로젝트 루트를 OS에서 정규화(심볼릭 링크 해석)한 절대 경로를 문자열 규칙(`normalize_project_root`)으로 바꾼 뒤 UTF-8 SHA-256의 앞 16자(소문자 hex). Windows는 `\\?\` 접두사 제거·`/` 구분자·소문자, 모든 OS에서 끝의 구분자 제거. Rust 구현과 공통 시험 벡터 `schemas/vectors/project-key.json`을 함께 둡니다.
- **수정:** 인증 토큰은 프로젝트 폴더 안에 두지 않습니다. Worker가 다루는 Workspace나 Git에 섞일 수 있기 때문입니다. Windows `%LOCALAPPDATA%\telos-weaver\<projectKey>\api-token`, Unix `${XDG_STATE_HOME:-~/.local/state}/telos-weaver/<projectKey>/api-token`.
- Unix Socket은 경로 길이 제한 때문에 `${XDG_RUNTIME_DIR}/telos-weaver/<projectKey>.sock`(없으면 `/tmp/telos-weaver-<uid>/`, 디렉터리 0700)에 둡니다. Windows Pipe 이름은 기존대로입니다.
- 토큰은 Core 시작마다 새로 만들고(32바이트 난수 hex) 사용자 전용 권한으로 쓴 뒤에만 연결을 받습니다. 권한 설정·ACL 적용에 실패하면 Core는 시작하지 않습니다. 재시작하면 기존 연결은 끊기며 Client는 토큰을 다시 읽습니다.
- 같은 OS 사용자로 실행되는 Worker도 사용자 디렉터리에 접근할 수 있습니다. 이를 막는 것은 권한 Profile·Sandbox(SAFE-01, §10)의 책임이며 이 계약의 한계로 기록합니다.
- 프레임: `MAX_FRAME_BYTES`는 종료 `\n`을 뺀 UTF-8 바이트 수입니다. JSON 안에 줄바꿈 문자를 그대로 넣지 않습니다. 초과 요청에는 `requestId: ""`, `frame_too_large` 응답 후 연결을 닫습니다. 서버는 상한을 넘는 프레임을 보내지 않습니다. 큰 텍스트는 이벤트를 나누고 큰 원본은 `rawEventRef` Artifact로 참조합니다. 공통 함수 `encode_frame`을 제공합니다.
- 오류 코드 추가와 규칙 변경이 있으므로 `API_PROTOCOL_VERSION`을 `0.2`로 올립니다.

### REQ-008 DecisionContext 범위

- `DecisionContext`는 **목록·상태 기반 검사**만 담습니다: Epoch·Revision, 한도, Task 트리, 허용 Provider·Workspace 모드·Profile·Host 목록, Project.
- 경로·명령·Session 소유권·호출자와 Run의 관계·예산 잔액처럼 이 Snapshot에 없는 검사는 MASTER가 `core/policy`에 정의할 신뢰된 조회로 수행합니다. 조회에 실패하면 `policy_unavailable`, 이 단계에서 수행할 수 없는 검사가 필요하면 `check_unsupported`로 거부하며 허용으로 간주하지 않습니다.
- `accepted`는 결정 단계의 검사 통과를 뜻하며 Run 시작 시의 권한 재검사(§9.1)를 대신하지 않습니다.

### REQ-009 중첩 Payload

- 마스터 입력은 신뢰하지 않으므로 `DecisionPayload`에서 닿는 모든 구조체(`TaskSpec`·`AgentSelection`·`WorkspaceRequest`·`InputBinding`·`OutputContract`·`ValidationContract`·`SessionRequest`·`ArtifactVersionRef`)에 미정의 필드 거부를 적용합니다. 같은 타입을 쓰는 Local API 요청에도 적용됩니다.
- 예외는 명시적 자유 형식 필드(`ValidationContract.parameters` 등 `JsonObject`)뿐입니다. 권한·제약을 뜻하는 값은 타입 필드로만 받습니다.
- 이전에 무시되던 입력이 거부되는 호환 변경이지만 아직 소비자가 없어 이전 처리 방침은 두지 않습니다.

## 3. 처리 순서

1. INTEGRATION(`main`): 판정 기록, `sha2` 의존성 추가, 공용 런타임 인터페이스 개정(REQ-002·003).
2. CONTRACT(`ph0/contract` Worktree): REQ-004~009.
3. INTEGRATION: 통합·재검증, 새 계약 기준 Commit 기록. G0 판정은 유지하되 기준을 개정합니다.

## 4. 처리 결과

**개정 계약 기준 Commit: `ba97e7b`** (2026-10-24). G0 통과 판정은 유지하고 기준만 `ae31eb2`에서 개정합니다.

| Commit | 내용 |
|---|---|
| `952bb26` | 검토 제출안 접수 |
| `7d296a8` | INTEGRATION: 판정 기록, REQ-002·003 런타임 인터페이스, `phase0_runtime_contract` 시험, `sha2 0.11.0` |
| `98177db` | CONTRACT: REQ-004~009 ([CONTRACT 기록 §5](../PH0-CONTRACT/README.md#5-g0-개정-1-req-004009)) |
| `ba97e7b` | `ph0/contract` 통합 |

통합 기준 재검증(`ba97e7b`, Windows):

| 명령 | 결과 |
|---|---|
| `cargo test --workspace --locked` | 통과 (27개) |
| `cargo clippy --workspace --all-targets --locked` | 경고 없음 |
| `cargo fmt --all --check` | 통과 |
| `cd extension && npm run build` | TypeScript 42개 생성, `tsc` 통과 |

요청별 상태:

| 요청 | 상태 | 남은 일 |
|---|---|---|
| REQ-002·003 | 계약 완료 | HOST가 구현하고 요청서의 수용 시험(대기 중 입력·취소, 반복 취소, 경합, 읽기 오류·긴 줄·느린 소비자 등)을 실행 |
| REQ-004·005·006·009 | 완료 | 소비 작업(MASTER·HOST·EXT·Provider)이 새 기준으로 사용 |
| REQ-007 | 완료(계약) | HOST·EXT가 주소·토큰 파일·ACL을 구현하고 벡터·프레임 경계 시험을 각자 실행 |
| REQ-008 | 완료(계약) | MASTER가 `core/policy`에 신뢰된 조회 인터페이스를 정의 |

모든 시험은 Contract 수준입니다. 판정자와 CONTRACT 처리자가 같은 담당자라는 한계는 G0 1차와 같으며, 이번 개정은 독립 검토 요청을 근거로 했습니다.

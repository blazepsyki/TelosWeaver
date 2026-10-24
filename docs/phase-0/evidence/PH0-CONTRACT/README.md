# PH0-CONTRACT 검증 기록

| 항목 | 값 |
|---|---|
| 작업 ID | PH0-CONTRACT |
| 담당 | `claude-opus-5.5/session-c9336d2c` |
| Branch / Worktree | `ph0/contract` / `D:\worktree\TelosWeaver\ph0-contract` |
| 기준 Commit | 1차 `2d0179a` (G0 준비), 개정 1 `00cf7ea` |
| 결과 Commit | 1차 `ade440e`, 개정 1은 이 기록을 포함한 `ph0/contract`의 마지막 Commit |
| 계약 기준 | 배정표 §1에 기록. §1~§4는 1차 기록이며 개정 1의 변경은 §5에 있습니다 |

## 1. 수정 파일

모두 PH0-CONTRACT 독점 경로입니다 (Spec §19.5.3).

- `crates/schema/src/{api,decision,events,failure,fixtures,rn}.rs` 신규, `crates/schema/src/lib.rs` 등록·Schema 목록
- `crates/schema/tests/contracts.rs` 신규
- `schemas/*.schema.json` 재생성 (7개 → 42개)
- `crates/adapters/src/lib.rs`, `crates/adapters/src/unimplemented.rs`, `crates/adapters/tests/unimplemented.rs`
- 이 문서

## 2. 정한 계약

| G0 계약 | 내용 |
|---|---|
| Adapter 실행 | Run ID는 Core 발급(`RunStarted.run_id == context.run_id`), 같은 멱등 키·같은 Run은 처음 결과 반환·프로세스 재생성 금지, 다른 Run이면 `IdempotencyConflict`. 실패 분류 `FailureKind`(§7.3 8종 + `unsupported_capability`·`unknown`). `AdapterError`에 `Failure`·`UnknownRun`·`IdempotencyConflict`·`EventsUnavailable` 추가 |
| 이벤트·취소 | kind별 정규 Payload 9종(`ProviderEventPayload`), `sequence`는 Run마다 1부터 연속, `after_sequence` 다음부터 재구독, 보존 범위 밖은 `EventsUnavailable`, 마지막 이벤트는 `process_exited`(프로세스가 더 오래 살면 `turn_finished`). `cancel_run`은 접수 여부(`false`=이미 종료), 실제 종료는 이벤트·`inspect_run`으로 확인. 미확인 사용량은 생략 |
| 마스터 결정 | kind 8종의 Payload 타입(`DecisionPayload`, 알 수 없는 필드 거부), 검증 입력 `DecisionContext`, 결과 `DecisionOutcome`(`accepted`/`rejected`, `RejectionCode` 14종, `DecisionEffect`, 멱등 재요청은 `replayed: true`) |
| Local Core API | `telos_schema::api`. Named Pipe `\\.\pipe\telos-weaver-<projectKey>` / Unix `<stateDir>/core.sock`, NDJSON(한 줄 1 MiB 이하), 사용자 전용 접근 + `<stateDir>/api-token`으로 `core.hello` 인증. 메서드 7종(`core.hello`·`core.health`·`runs.start/inspect/cancel/subscribe/unsubscribe`), 알림 2종, 오류 코드 9종 |
| RN RPC | `telos_schema::rn`. 메서드 9종(§12.1), Envelope·Receipt(§12.2), `EndpointRef`(Host·Provider·Native ID·Fingerprint), 커서 구독과 `resync_required`, 오류 코드 10종, `RN_PROTOCOL_VERSION = "0.1"` |
| 생성·시험 | 42개 JSON Schema, `fixtures` feature의 공통 Fixture(정상 Task·RunContext·이벤트 순서·결정, 공통 실패 결정 4종), `UnsupportedAdapter` 시험 대역 |

공용 프로세스 경계(`telos_adapters::runtime`)는 PH0-INTEGRATION이 G0 준비(`2d0179a`)에서 등록했으며 이 작업에서 바꾸지 않았습니다.

## 3. 시험

| 명령 | 결과 | 종류 |
|---|---|---|
| `cargo test --workspace` | 통과 (schema 14, adapters 4, rn-client 2, Schema 일치 1) | Contract |
| `cargo clippy --workspace --all-targets` | 경고 없음 | 정적 검사 |
| `cargo fmt --all --check` | 통과 | 포맷 |
| `UPDATE_SCHEMAS=1 cargo test -p telos-schema --test json_schemas` 후 재실행 | 생성물과 일치 | 생성 |
| `cd extension && npm ci && npm run build` | TypeScript 42개 생성, `tsc` 통과 | 생성 |

실행 환경: Windows 10.0.26300 (MINGW64 셸), rustc 1.94.0, cargo 1.94.0, Node 24.14.0, npm 11.9.0. 실행 일시 2026-10-24.

모든 시험은 Contract 수준입니다. 실제 Provider·RN·Core API 동작은 검증하지 않았습니다.

## 4. Stub·미검증·남은 문제

- 세 Provider Adapter는 여전히 `UnsupportedAdapter`에 위임하는 Stub입니다. 공통 시험은 더 이상 이들을 검사하지 않으므로 각 Provider 담당이 교체해도 공통 시험이 깨지지 않습니다.
- Local API의 `projectKey` 계산·토큰 파일 권한·Pipe ACL은 문서 계약이며 HOST가 구현·시험해야 합니다.
- RN RPC 메시지는 TelosWeaver 쪽 정의입니다. RN Fork 실제 RPC와의 호환은 PH0-RN이 Fork 도입 후 확인해야 합니다.
- `DecisionContext`의 권한 범위는 Provider·Workspace 모드·Profile ID 목록 수준의 Phase 0 최소입니다.
- `telos-rn-client`는 아직 자체 RPC 타입을 갖고 있습니다. `telos_schema::rn` 재노출로 바꾸는 등록 작업을 PH0-INTEGRATION에 요청합니다.

## 5. G0 개정 1 (REQ-004~009)

판정과 결정 근거는 [G0 개정 1](../PH0-INTEGRATION/G0_REVISION_1.md)에 있습니다. REQ-002·003(공용 런타임)은 PH0-INTEGRATION이 `00cf7ea`에서 처리했습니다.

### 5.1 수정 파일

모두 PH0-CONTRACT 독점 경로입니다.

- `crates/schema/src/{api,decision,events,fixtures,lib,provider,task}.rs`, 신규 `crates/schema/src/secret.rs`
- `crates/schema/tests/contracts.rs`
- `schemas/*.schema.json` 10개 재생성, 신규 `schemas/vectors/project-key.json`
- `crates/adapters/src/lib.rs` (문서 계약)
- 이 문서

### 5.2 처리 내용

| 요청 | 처리 |
|---|---|
| REQ-004 | `DecisionOutcome`을 `status` 태그 열거형(`Accepted { effects }` / `Rejected { rejection }`)으로 변경, 미정의 필드 거부. 접근 메서드 `status()`·`effects()` 등 추가. Wire 필드 이름 유지 |
| REQ-005 | `SecretString`(Wire는 문자열, Debug는 `<redacted>`) 추가, `HelloParams::token`에 적용. `ApiRequest`의 Debug가 `params.token`을 가림 |
| REQ-006 | 종료 규칙(`process_exited`·`turn_finished`·종료 `error`) 확정, `check_run_events`와 `RunTermination`·`RunEventViolation` 추가. 정상·시작 실패·취소·지속형·추적 유실 Fixture와 반례 6종. `PROVIDER_EVENT_SCHEMA_VERSION` 0.2 |
| REQ-007 | `normalize_project_root`·`project_key`·`encode_frame` 추가, 시험 벡터 9개. 주소·토큰 위치(프로젝트 밖 사용자 디렉터리)·토큰 수명·fail closed·프레임 규칙을 모듈 문서에 명시. `frame_too_large` 오류 추가. `API_PROTOCOL_VERSION` 0.2 |
| REQ-008 | `DecisionContext`에 `project_id`·`allowed_host_ids` 추가, 검사 범위 문서화, `policy_unavailable`·`check_unsupported` 거부 코드 추가 |
| REQ-009 | `TaskSpec`·`AgentSelection`·`WorkspaceRequest`·`InputBinding`·`OutputContract`·`ValidationContract`·`SessionRequest`·`ArtifactVersionRef`에 미정의 필드 거부. `parameters` 같은 `JsonObject`만 확장 허용. 중첩 실패 Fixture 5종 |

### 5.3 시험

| 명령 | 결과 | 종류 |
|---|---|---|
| `cargo test --workspace` | 통과 (27개: schema 21, adapters 4, core 2) | Contract |
| `cargo clippy --workspace --all-targets` | 경고 없음 | 정적 검사 |
| `cargo fmt --all --check` | 통과 | 포맷 |
| `UPDATE_SCHEMAS=1` 재생성 후 재실행 | Schema 42개·벡터 일치 | 생성 |
| `cd extension && npm ci && npm run build` | TypeScript 42개 생성, `tsc` 통과 | 생성 |

추가한 반례: 모순된 `DecisionOutcome` 3종, 중첩 미정의 필드 5종, 잘못된 이벤트 순서 6종, 토큰 Debug 노출, 프레임 상한 경계(한계·한계+1)와 이스케이프 줄바꿈. 실행 환경은 §3과 같습니다. 모두 Contract 수준이며 실제 프로세스·연결 시험은 없습니다.

### 5.4 호환 영향과 남은 일

- `HelloParams.token`의 JSON Schema가 `SecretString` 정의 참조로 바뀌었습니다. Wire 형식(문자열)은 같고 생성 TypeScript에서는 `string` 별칭입니다.
- `ArtifactVersionRef`에 미정의 필드 거부를 적용해 이를 포함한 `TaskResult`·`RnMessageEnvelope`의 Schema도 엄격해졌습니다.
- Local API 주소·토큰 파일·ACL의 실제 동작, 공용 런타임의 취소 안전성·출력 처리는 HOST가 구현하고 요청서의 수용 시험으로 확인해야 합니다.
- `DecisionContext` 밖의 신뢰된 조회 인터페이스는 MASTER가 `core/policy`에 정의합니다.

## 6. REQ-010 CLI 탐지 계약

> 2026-10-24 · `claude-opus-5.5/session-c9336d2c` · `ph0/contract` · 기준 `0094720` (HOST 1차 통합 `1d8734f` 이후)

PH0-HOST의 [REQ-010](../PH0-HOST/README.md#req-010-cli-탐색-계약과-모듈-등록) 요청을 처리했습니다. 공통 탐색 결과를 다른 작업이 같은 형태로 쓰도록 결과 DTO와 입력·Port를 확정합니다. 탐색 엔진 연결은 하지 않았습니다.

### 6.1 수정 파일

| 경로 | 내용 |
|---|---|
| `crates/schema/src/discovery.rs` (신규) | 결과 DTO: `CliDiscovery`, `CliDiscoveryOutcome`(`status` 태그 `found`/`unavailable`), `DiscoveredCli`, `ExtensionInstall`, `CliSource`, `ExtensionResolution`, `BinaryLocation`, `CliDiscoveryFailure` |
| `crates/schema/src/lib.rs` | 모듈 등록, `CliDiscovery` Schema 추가(43개) |
| `schemas/CliDiscovery.schema.json` (신규) | 생성물 |
| `crates/adapters/src/lib.rs` | 입력·Port: `ProviderCliRule`, `CliBinaryRule`(`InExtension`/`Managed`), `DiscoveryRequest`, `CliDiscoverer` trait |
| `crates/schema/tests/contracts.rs`, `crates/adapters/tests/unimplemented.rs` | Wire 형식·혼합/미정의 필드 거부·Port 객체 안전성 시험 |

### 6.2 정한 계약

- **결과는 오류가 아니라 값입니다.** `CliDiscoverer::discover`는 항상 `CliDiscovery`를 돌려주고, 실패는 `unavailable { failure, source?, reason }`으로 기록합니다. 명시 경로 확인이 실패하면 `source = explicit`이며 다른 출처로 넘어가지 않습니다(§7.5-4·5).
- **기록 항목(§7.5-6):** 출처(`explicit`/`extension`/`path`), 위치 결정 방법(`relative-path`/`managed-path`/`name-search`/`direct`), 실행 경로, CLI 버전, SHA-256, 확장 ID·버전·`targetPlatform`·설치 경로·폴더 결정 방법(`metadata`/`folder-scan`).
- **실패 분류:** `not_found`, `ambiguous`, `invalid_metadata`, `invalid_path`(비 UTF-8 경로 포함), `version_check_failed`, `changed_during_discovery`, `unsupported_platform`, `io_error`.
- **Provider 규칙:** 각 Provider 모듈이 `pub fn cli_rule(target_platform: &str) -> Option<ProviderCliRule>`를 제공합니다. Antigravity는 `CliBinaryRule::Managed { home_relative_path: ".gemini/bin/agy" }`처럼 확장 폴더 밖 관리 경로를 표현합니다.
- **요청:** 실행 Host ID, VS Code `targetPlatform`, 명시 경로, 우선순위 순서의 확장 폴더, Home, PATH 값(`None`이면 PATH 대체 안 함), `checked_at`. 시각은 Core가 넣어 엔진이 시계를 갖지 않습니다.
- 모든 결과 구조체는 미정의 필드를 거부합니다(REQ-009 정책과 같음).

### 6.3 HOST 인계 (엔진 연결)

- HOST는 `discovery` 모듈에 `CliDiscoverer` 구현(예: `NativeCliDiscoverer`, 공용 `ProcessRuntime` 주입)을 추가하고, 내부 `io::Error`를 위 실패 분류로 매핑합니다.
- 모듈 등록은 엔진 연결과 한 단위로 합니다. **이 연결에 한해 HOST가 `crates/adapters/src/lib.rs`에 `mod discovery;`(또는 `pub mod`)와 구현 타입의 `pub use` 줄을 추가하는 것을 허용합니다.** 다른 lib.rs 변경은 CONTRACT 요청으로 처리합니다.
- `discovery_*` 시험의 `#[path]` 소스 포함을 공개 API 호출로 바꿉니다.
- 엔진의 `SelectionSource`·`ExtensionInstall.from_metadata`는 각각 `CliSource`·`ExtensionResolution`에 대응합니다. 엔진에는 위치 결정 방법(`BinaryLocation`)과 `targetPlatform` 기록이 아직 없으므로 연결 시 추가합니다.

### 6.4 시험

| 명령 | 결과 | 종류 |
|---|---|---|
| `UPDATE_SCHEMAS=1 cargo test -p telos-schema --all-features` 후 재실행 | Schema 43개 일치, contracts 18개 통과 | Contract·생성 |
| `cargo test --workspace --locked` | 61개 통과 | Contract·HOST 기존 시험 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo fmt --all --check` | 통과 | 정적 검사 |
| `cd extension && npm ci && npm run build` | TypeScript 43개 생성, 빌드 통과 | 생성 |

실제 확장 폴더·CLI로 한 시험은 없습니다. 실제 탐색 결과는 HOST 연결 후 각 Provider 작업이 `environment-tested`로 기록합니다.

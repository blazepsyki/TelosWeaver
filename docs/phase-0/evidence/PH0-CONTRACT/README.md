# PH0-CONTRACT 검증 기록

| 항목 | 값 |
|---|---|
| 작업 ID | PH0-CONTRACT |
| 담당 | `claude-opus-5.5/session-c9336d2c` |
| Branch / Worktree | `ph0/contract` / `D:\worktree\TelosWeaver\ph0-contract` |
| 기준 Commit | `556e62f` (G0 준비) |
| 결과 Commit | 이 기록을 포함한 `ph0/contract`의 마지막 Commit |
| 계약 기준 | G0 확정 전 초안. 확정 기준 Commit은 배정표 §1에 기록 |

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

공용 프로세스 경계(`telos_adapters::runtime`)는 PH0-INTEGRATION이 G0 준비(`556e62f`)에서 등록했으며 이 작업에서 바꾸지 않았습니다.

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

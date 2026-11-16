# PH1-PERSIST 검증 기록

> Spec §19.5.8 형식입니다. 실행하지 않은 시험을 통과로 적지 않습니다.

| 항목 | 값 |
|---|---|
| 작업 ID | PH1-PERSIST |
| 담당 | `claude-opus-5.5/session-c9336d2c` (사용자 지정 2026-11-16, 통합 담당이 직접) |
| Branch / Worktree | `ph1/persist` / `D:\worktree\TelosWeaver\ph1-persist` |
| 기준 Commit | `59c0b51` |
| 결과 Commit | 배정표의 PH1-PERSIST 행에 기록 |
| 계약 기준 | `bac970e` + REQ1-001 |

## 1. 수정 파일

- 독점 경로: `crates/core/src/persistence/mod.rs`, `crates/core/src/persistence/sqlite.rs`, `crates/core/tests/persistence_store.rs`, `crates/core/tests/persistence_recovery.rs`, `crates/core/tests/fixtures/persistence/mod.rs`, 이 문서
- REQ1-001로 처리한 공통 경로: `crates/schema/src/journal.rs`, `schemas/JournalEntry.schema.json`, `schemas/ApiJournalEntryParams.schema.json`, `Cargo.toml`, `Cargo.lock`, `crates/core/Cargo.toml`, `docs/DEVELOPMENT_v1.5.md`(v1.5.12), `crates/core/README.md`

## 2. 구현한 기능

- **계약 확인:** H0 `Store` 인터페이스를 Spec §14와 대조해 공백을 찾았고 REQ1-001로 보강했습니다(§6).
- **`SqliteStore`:** 프로젝트 상태 디렉터리의 `core.db`. 연결 하나, `locking_mode=EXCLUSIVE`(다른 프로세스가 열면 `AlreadyOpen`), `journal_mode=WAL`, `synchronous=FULL`.
- **조건부 쓰기:** `apply(WriteBatch)` 한 번이 한 Transaction. 새 기록은 버전 1, 갱신은 읽은 버전과 같을 때만 +1. 하나라도 어긋나면 상태·Journal·번호 모두 남지 않습니다.
- **Journal:** 1부터 빈틈없는 `sequence`, Plan 범위 조회, 변경 뒤 기록을 `payload`에 포함, 오래된 항목 정리와 `cursor_expired` 판정.
- **실행 의도:** `recorded`로만 생성, Run마다 하나, 단계는 앞으로만, 요청 내용 변경 거부, 미완 의도 조회.
- **멱등 키:** 같은 키가 있으면 배치 전체 미적용, 같은 Digest는 `IdempotencyReplay`, 다른 Digest는 `IdempotencyConflict`.
- **잠금:** Writer 단독·Reader 공유, 소유 Epoch가 맞을 때만 해제, 인수는 해제와 획득을 한 배치로.
- **불변 기록:** Session·Context Snapshot·Artifact Version·사용량은 같은 내용의 재기록만 허용.
- **Provider 원본 이벤트:** Run별 `sequence`로 저장, 재전달 무시.
- **Schema 버전:** 현재 버전은 열고, 이전 버전은 백업(`core.db.v<버전>.<시각>.bak`) 뒤 Migration, 더 새 버전과 버전 없는 외부 파일은 열지 않습니다.
- **저장 실패:** DB 크기 상한(`SqliteOptions::max_pages`)이나 디스크 부족으로 쓰기가 실패하면 `StoreError::Io`.

## 3. Stub·미지원·미검증

| 항목 | 상태 | 내용 |
|---|---|---|
| Core Daemon 연결 | 미연결 | `SqliteStore`는 라이브러리로만 있습니다. `telos-weaver serve`는 아직 이 저장소를 열지 않으며 상태는 계속 메모리에 있습니다. 연결은 SCHED·HOST 통합에서 합니다. 그 전에는 재시작 복구를 지원한다고 표시하지 않습니다 |
| Linux | 미검증 | Windows에서만 실행했습니다. `psyki-nas` 공용 Clone이 없어 Linux 빌드(번들 SQLite의 C 컴파일)·파일 잠금·프로세스 강제 종료 시험을 실행하지 못했습니다. 재확인: 서버에서 `cargo test -p telos-core --test persistence_store --test persistence_recovery` |
| 전원 장애 내구성 | 미검증 | `synchronous=FULL` 설정에 따른 SQLite의 보장에 기대며, 실제 전원 차단 시험은 하지 않았습니다. 프로세스 강제 종료는 시험했습니다 |
| Journal 보존 정책 | 미구현 | 정리 수단(`prune_journal`)만 있습니다. 언제 얼마나 남길지는 HOST가 구독 구현 때 정합니다 |
| Provider 원본 이벤트 상한 | 미구현 | 저장·조회만 있습니다. Run별 보존 상한과 마스킹은 HOST 범위입니다(OPS-02, SAFE-04) |
| `provider_capabilities`·`messages` 등 | 범위 밖 | §14.1 표의 나머지 데이터는 P0 경로의 사용처가 정해질 때 해당 작업이 요청합니다. RN 메시지는 RN의 DB에 있습니다 |
| 실제 Migration | 해당 없음 | Schema 버전이 1뿐이라 운영 Migration은 없습니다. 백업·Migration 절차는 시험용 버전 2로 검증했습니다 |

## 4. 시험

실행 환경: Windows 11 Pro 10.0.26300(Host `Psyki-HOME`, Windows 로컬), rustc 1.100.0, Node 24.22.1, `rusqlite` 0.43.0(번들 SQLite, `libsqlite3-sys` 0.40.0). 외부 CLI·모델 호출이 없는 시험입니다.

| 명령 | 날짜 | OS/Host | 기대 결과 | 실제 결과 | 종료 상태 | 종류 |
|---|---|---|---|---|---|---|
| `cargo test -p telos-core --test persistence_store` | 2026-11-16 | Windows 로컬 | 12개 통과 | 12 passed | 0 | 통합(실제 SQLite 파일) |
| `cargo test -p telos-core --test persistence_recovery` | 2026-11-16 | Windows 로컬 | 6개 통과 | 6 passed | 0 | 통합(실제 SQLite 파일, 자식 프로세스 강제 종료 포함) |
| `cargo test -p telos-core --lib persistence` | 2026-11-16 | Windows 로컬 | 6개 통과 | 6 passed | 0 | 단위(Migration·백업) |
| `cargo test --workspace` | 2026-11-16 | Windows 로컬 | 전체 통과 | 249 passed, 0 failed, 42 ignored | 0 | 전체 회귀 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 2026-11-16 | Windows 로컬 | 경고 없음 | 경고 없음 | 0 | 정적 검사 |
| `cargo fmt --all --check` | 2026-11-16 | Windows 로컬 | 차이 없음 | 차이 없음 | 0 | 형식 |
| `UPDATE_SCHEMAS=1 cargo test -p telos-schema --test json_schemas` | 2026-11-16 | Windows 로컬 | Schema 갱신 | 2개 파일 갱신 | 0 | 생성 |
| `cd extension && npm test` | 2026-11-16 | Windows 로컬 | 32개 통과 | pass 32, fail 0 | 0 | 생성 타입 회귀 |

시험별 확인 내용:

| 시험 | 확인 | 요구사항 |
|---|---|---|
| `conditional_writes_follow_versions` | 새 기록 1, 갱신 +1, 오래된 버전·중복 생성·없는 기록 갱신 거부 | §9.5, SAFE-13 |
| `a_failed_write_cancels_the_whole_batch` | 실패한 배치의 Task·의도·Journal 번호가 남지 않음 | §14.1, SAFE-06 |
| `journal_is_gapless_and_scoped_by_plan` | 번호 연속, 실패 배치는 번호 미사용, Plan 범위, Run의 Plan 범위 추적 | §14.3 |
| `pruned_cursor_expires_and_numbering_continues` | 정리 뒤 이전 커서 만료, 전부 지워도 번호 이어짐 | §17 |
| `intents_start_recorded_and_only_advance` | 의도 단계 순서, Run당 하나, 요청 내용 변경 거부 | §14.3 |
| `duplicate_idempotency_key_never_applies_twice` | 재생·충돌 모두 상태 미변경, 처음 결과 조회 | SAFE-13 |
| `locks_exclude_writers_and_fence_previous_owners` | Writer 충돌, Reader 공유, 인수 뒤 이전 소유자 해제 거부 | GIT-02, WS-02 |
| `immutable_records_cannot_be_rewritten` | 불변 기록의 재기록·변경, 없는 Version 참조 거부 | ART-03, CTX-01 |
| `remaining_records_round_trip` | 검증·승인·Workspace·예산·Plan·Run Session 저장·갱신·조회와 Journal 종류 | §14.1 |
| `journal_only_writes_are_limited_to_decisions_and_control` | 기록 없는 상태 변경 항목을 만들 수 없음 | §4.2 |
| `run_events_are_kept_per_run_and_redelivery_is_ignored` | Run별 저장, 재전달 무시, Journal 번호 미사용 | §14.3 |
| `concurrent_writers_from_one_version_have_one_winner` | 8개 스레드가 같은 버전에서 갱신하면 하나만 적용(5회 반복) | §9.5 |
| `committed_records_survive_reopen` | 닫았다 열어도 기록·미완 의도·잠금·이벤트 보존, 번호 이어짐 | SESS-03, SAFE-06 |
| `second_open_is_rejected_while_in_use` | 사용 중인 DB의 두 번째 열기 거부, 닫은 뒤 열림 | OPS-01 |
| `newer_schema_is_not_opened_or_modified` | 더 새 Schema를 열지 않고 파일이 한 Byte도 바뀌지 않음 | §14.1 |
| `full_storage_fails_the_write_without_partial_state` | 상한 도달 시 `Io`, 실패 배치 흔적 없음, 이전 기록 조회 가능 | §15.1, E2E-08 |
| `killed_process_leaves_committed_records_and_frees_the_database` | 다른 프로세스가 쓰는 동안 열기 거부, 강제 종료 뒤 확정 기록·미완 의도·잠금이 남고 다음 프로세스가 엶 | §14.2, OPS-03 |
| `older_schema_is_backed_up_then_migrated` 외 3개(단위) | 백업 뒤 Migration, 백업 실패 시 미적용, 단계 누락 보고, 버전 없는 외부 파일 거부 | §14.1 |

- 로그·Fixture 위치: 시험은 `%TEMP%\telos-persist\`와 `%TEMP%\telos-persist-unit\` 아래 일회용 디렉터리를 쓰고 끝나면 지웁니다(단위 시험 디렉터리는 다음 실행 때 덮어씁니다).

## 5. 반례 결과

위 표의 거부·실패 시험이 반례입니다. 특히 다음을 확인했습니다.

- 오래된 버전의 쓰기, 뒤로 가는 의도 단계, 다른 Digest의 멱등 키, 충돌하는 잠금, 맞지 않는 Epoch의 해제, 불변 기록 변경은 모두 거부되고 배치 전체가 취소됩니다.
- 저장 공간 부족에서 부분 기록이 남지 않습니다.
- 강제 종료된 프로세스가 쥐고 있던 DB를 다음 프로세스가 열 수 있고, `dispatched` 의도가 그대로 보여 재시작 대조 대상이 됩니다.

## 6. 변경 요청과 인계

**REQ1-001 (채택·처리):**

1. `telos-schema` `JournalKind`에 `WorkspaceChanged`·`BudgetChanged`·`UsageRecorded` 추가. 기존 값은 그대로입니다.
2. `persistence::Store`:
   - `Versioned<T>`: `stateVersion` 필드가 없는 Plan·예산 장부의 버전을 조회 때 함께 반환
   - 쓰기 추가: `ContextSnapshot`, `Budget`, `Journal`(`DecisionApplied`·`Control` 전용)
   - 오류 추가: `IdempotencyReplay`, `Invalid`, `AlreadyOpen`
   - `AppliedBatch`를 `entries`·`last_sequence`로 변경(Journal 항목을 만들지 않는 배치 표현)
   - 조회 추가: `plans`, `run_intent`, `task_sessions`, `run_sessions`, `context_snapshot`, `task_workspaces`, `task_artifacts`, `validation`, `task_validations`, `plan_approvals`, `budget`, `plan_usage`
   - `prune_journal`, `append_run_events`, `run_events`
3. 의존성 `rusqlite` 0.43.0(`bundled`). 시스템 SQLite에 기대지 않고, 동기 API가 `Store`와 맞습니다.

**인계:**

- **SCHED:** 전이 규칙 검사는 저장소가 하지 않습니다. `VersionConflict`를 받으면 다시 읽고 판단합니다. 잠금의 Epoch 값은 호출자가 정합니다(인수 때 이전 값 + 1).
- **HOST:** `SqliteStore::open(&core_db_path(프로젝트 상태 디렉터리))`로 엽니다. `AlreadyOpen`은 같은 프로젝트의 Core가 이미 실행 중이라는 뜻입니다. `Store`는 동기이므로 비동기 문맥에서는 `spawn_blocking`으로 부릅니다. `StoreError::Io`를 받으면 후속 실행을 보류합니다.
- **MASTER:** 결정 적용은 `Idempotency`(범위 `decision`)와 `Journal`(`DecisionApplied`)을 상태 변경과 한 배치로 씁니다.
- **CONTEXT·ARTIFACT·VALIDATION·WORKSPACE:** 자기 기록의 쓰기·조회가 위 목록에 있습니다. 더 필요한 조회는 변경 요청으로 올립니다.
- **통합 담당:** Linux 시험(서버 Clone 재생성 뒤), 수용 대조표의 PERSIST 행은 H3에서 기록합니다.

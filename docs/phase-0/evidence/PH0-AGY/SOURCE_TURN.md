# PH0-AGY 후속: Turn 지정 Resume 거부 (`source_turn_id`)

| 항목 | 값 |
|---|---|
| 작업 | PH0-AGY 후속: Turn 지정 Resume 거부 (`source_turn_id`) |
| 담당 | `gemini-3.8-flash/session-c2e931ec` (PH0-AGY) |
| 기준 | `main` `9f062bf` (착수 기록 `a164b79`), Spec v1.5.7, 계약 `6a4f58a` |
| Branch / Worktree | `ph0/agy-source-turn` / `D:\worktree\TelosWeaver\ph0-agy-source-turn` |
| 지시서 | [AGY_SOURCE_TURN_FOLLOWUP.md](../PH0-INTEGRATION/AGY_SOURCE_TURN_FOLLOWUP.md) |
| 날짜 | 2026-10-26 |

## 1. 배경 및 문제

`RunContext.session.source_turn_id`는 특정 Turn 지점에서의 재개/분기를 요청합니다.
기존 `crates/adapters/src/agy.rs`의 `command_args`는 `SessionMode::Resume`에서 `source_turn_id`를 검사하지 않고 `source_session_id`만으로 `--conversation <id>`를 구성했습니다.
그 결과 특정 Turn을 지정한 재개 요청이 조용히 최신 지점에서 이어 쓰기로 처리되어 원본 세션에 의도하지 않은 Turn이 추가되는 결함이 있었습니다 (Spec §2.1 원칙 7, §7.1).
`agy` 1.4.7 CLI에는 특정 Turn 기준 재개 수단이 없으므로(`--conversation`만 제공), 프로세스를 시작하기 전에 명시적으로 거부해야 합니다.

## 2. 작업 내역

| # | 작업 | 내용 | 위치 |
|---|---|---|---|
| T1 | Turn 지정 Resume 프로세스 시작 전 거부 | `command_args`의 `SessionMode::Resume`에서 `context.session.source_turn_id.is_some()`이면 `AdapterError::UnsupportedCapability { capability: capability_keys::NATIVE_RESUME, reason }` 반환 | `crates/adapters/src/agy.rs` |
| T2 | `detect()`의 `native_resume` 사유 보강 | 사유에 `resuming at a specific turn is unsupported` 문구 추가. 상태(`unverified`) 및 근거 수준(`EnvironmentTested`) 유지, 모듈 주석 보강 | `crates/adapters/src/agy.rs` |
| T3 | 수정 전 동작 검출 및 시험 추가 | `agy_adapter.rs`의 `resume_builds_conversation_flag_and_native_fork_is_unsupported`에 `source_turn_id` 지정 Resume 거부 및 프로세스 미생성 단언 추가, `detect_reports_only_verified_capabilities_and_host_mismatch`에 사유 단언 추가 | `crates/adapters/tests/agy_adapter.rs` |

## 3. 수정 전 동작 검출 (T3)

T1 거부 로직을 적용하기 전에 시험 단언을 먼저 추가하고 실행한 결과, `start_run`이 조용히 성공하여 예상대로 정확히 실패함을 확인했습니다:

```text
running 1 test

thread 'resume_builds_conversation_flag_and_native_fork_is_unsupported' (15588) panicked at crates\adapters\tests\agy_adapter.rs:686:5:
assertion failed: matches!(adapter.start_run(&task, &at_turn).await,
    Err(AdapterError::UnsupportedCapability { ref capability, .. }) if
    capability == capability_keys::NATIVE_RESUME)
test resume_builds_conversation_flag_and_native_fork_is_unsupported ... FAILED
```

T1 거부 로직 적용 후 해당 시험 및 전체 단위 시험이 즉시 통과했습니다.

## 4. 검사 결과

| 검사 | 명령 | 결과 |
|---|---|---|
| 코드 포맷 | `cargo fmt --all --check` | 통과 |
| 엄격 Clippy | `cargo clippy --workspace --all-targets -- -D warnings` | 통과 (경고 0건) |
| 어댑터 단위 시험 | `cargo test -p telos-adapters --test agy_adapter` | 16개 전체 통과 |
| 전체 Workspace 시험 | `cargo test --workspace` | 175개 통과 (38개 제외) |

- 모델 API 호출: 없음 (프로세스 시작 전 거부 및 Mock 검증).
- 기존 파일 및 다른 Adapter 변경 없음.

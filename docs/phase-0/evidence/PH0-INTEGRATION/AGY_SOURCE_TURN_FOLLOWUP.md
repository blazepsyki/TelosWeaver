# PH0-AGY 후속: Turn 지정 Resume 거부 (`source_turn_id`)

| 항목 | 값 |
|---|---|
| 작성 | PH0-INTEGRATION (`claude-opus-5.5/session-c9336d2c`), 2026-10-26 사용자 지시 |
| 처리 | **PH0-AGY**, 기존 Adapter 담당 |
| 기준 | 이 지시서를 기록한 `master` Commit, Spec v1.5.7, 계약 `6a4f58a` |
| 근거 | [REQ-024](REQ-024.md) §7 발견, Spec §2.1 원칙 7("미지원·미검증 Capability를 지원하는 것처럼 대체하지 않음")·§7.1("세부 조건은 `reason`과 실행 전 검사로 표현"), 같은 결함의 Claude 수정 [`ffdc70a`](../PH0-CLAUDE/CAPABILITY.md) |
| 우선순위 | 비차단(G3 조건 아님) |

## 1. 문제

`RunContext.session.source_turn_id`는 "이 Turn 지점에서 재개·분기"를 요청합니다. `crates/adapters/src/agy.rs`의 `command_args`는 `SessionMode::Resume`에서 `source_session_id`만 보고 `--conversation <id>`를 붙이며, `source_turn_id`는 확인하지 않습니다. 그래서 특정 Turn을 지정한 Resume 요청이 **오류 없이 최신 지점에서 이어 쓰기**로 바뀝니다. 요청과 다른 동작이고, 원본 대화에 의도하지 않은 Turn이 추가됩니다.

`agy` 1.4.7 `--help`에는 특정 Turn·단계 기준 재개 옵션이 없습니다(`--conversation`·`--continue`만). `SessionMode::NativeFork`는 이미 `native_fork`로 거부하므로 이번 대상은 Resume입니다.

## 2. 작업

| # | 작업 | 확인 방법 |
|---|---|---|
| T1 | `command_args`의 `SessionMode::Resume`에서 `source_turn_id`가 있으면 프로세스 시작 전에 `AdapterError::UnsupportedCapability { capability: capability_keys::NATIVE_RESUME, reason }`로 거부합니다. 사유 예: "resume at a specific turn is not supported by this CLI (only --conversation, which continues from the latest point)" | Mock 시험: Turn 지정 Resume이 이 오류로 끝나고 프로세스를 만들지 않음. 지정하지 않은 Resume은 기존처럼 `--conversation` |
| T2 | `detect()`의 `native_resume` 사유에 "특정 Turn 기준 재개 미지원"을 더합니다. 상태(`unverified`)·근거 수준은 바꾸지 않습니다 | Mock 시험: 사유 문구 |
| T3 | 수정 전 동작 검출: T1의 거부만 임시로 빼면 새 시험이 실패하는지 확인하고 되돌립니다 | 증거에 기록 |

참고: 기존 시험 `resume_builds_conversation_flag_and_native_fork_is_unsupported`(`agy_adapter.rs`)에 경우를 추가하는 것이 자연스럽습니다. Claude의 같은 수정은 `claude.rs`의 `command_args`와 `claude_adapter.rs`의 `native_fork_passes_resume_and_fork_flags`를 참고하세요.

## 3. 범위와 금지

- 수정 경로는 PH0-AGY 경로(`crates/adapters/src/agy.rs`, `agy/**`, Antigravity 시험, 자기 증거)뿐입니다.
- Turn 기준 재개를 다른 방법(대화 파일 편집 등)으로 흉내 내지 않습니다.
- 모델 호출은 필요 없습니다(시작 전 거부라 Mock 시험으로 충분).

## 4. 작업 방식

1. 최신 `master`에서 Branch `ph0/agy-source-turn`, Worktree `D:\worktree\TelosWeaver\ph0-agy-source-turn`.
2. 착수·완료를 배정표 PH0-AGY 행과 §7에 `master` 단독 커밋으로 기록합니다(§7 표 행 사이 빈 줄 금지). 담당 Worktree에 커밋하지 않은 변경을 남기지 않습니다.
3. 검사: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p telos-adapters --test agy_adapter`, `cargo test --workspace`.
4. 결과를 `docs/phase-0/evidence/PH0-AGY/SOURCE_TURN.md`에 적고 INTEGRATION 검토·통합을 요청합니다.

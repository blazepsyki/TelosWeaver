# PH0-AGY 후속: 공통 Capability 키 전환 (REQ-024)

| 항목 | 값 |
|---|---|
| 작성 | PH0-INTEGRATION (`claude-opus-5.5/session-c9336d2c`), 2026-10-26 |
| 처리 | **PH0-AGY**, 기존 Adapter 담당 |
| 기준 | 이 지시서를 기록한 `master` Commit, Spec v1.5.7 §7.1·§7.2, 계약 기준 `6a4f58a` |
| 근거 | [REQ-024](REQ-024.md) §5·§6, [Codex 조사](../PH0-CODEX/REQ-024.md) §2 |
| 우선순위 | 비차단. REQ-024 완료 조건 |

## 1. 작업

| # | 작업 | 확인 방법 |
|---|---|---|
| K1 | `crates/adapters/src/agy.rs`의 `detect()` 키와 `unsupported(...)`·`UnsupportedCapability` 값을 문자열 리터럴 대신 `telos_schema::capability_keys` 상수로 씁니다 | 리터럴 검색 0건(아래 명령) |
| K2 | CLI 미탐지 시 `start_run`(현재 `agy.rs`의 `unsupported("start_run", …)`)을 `capability_keys::MANAGED_RUN`으로 바꿉니다 | Mock 시험: 미구성 Adapter의 `start_run` 오류 `capability == "managed_run"` |
| K3 | Antigravity 시험(`agy_adapter.rs`·`agy_environment.rs`)에서 Capability 키를 리터럴로 비교하는 곳을 상수로 맞춥니다 | 시험 통과 |

```powershell
rg -n '"(managed_run|event_stream|cancel|native_resume|native_fork|send_input|answer_approval|vscode_listing|start_run)"' crates/adapters/src/agy.rs crates/adapters/src/agy
```

## 2. 범위와 금지

- 키 이름만 바꿉니다. 상태·증거·사유 판정과 동작은 바꾸지 않습니다.
- 수정 경로는 PH0-AGY 경로(`crates/adapters/src/agy.rs`, `agy/**`, Antigravity 시험, 자기 증거)뿐입니다. 모델 호출은 필요 없습니다.

## 3. 작업 방식

1. 최신 `master`에서 Branch `ph0/agy-capability-keys`, Worktree `D:\worktree\TelosWeaver\ph0-agy-capability-keys`.
2. 착수·완료를 배정표 PH0-AGY 행과 §7에 `master` 단독 커밋으로 기록합니다(§7 표 행 사이 빈 줄 금지).
3. 검사: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p telos-adapters --test agy_adapter`, `cargo test --workspace`.
4. 결과를 `docs/phase-0/evidence/PH0-AGY/CAPABILITY_KEYS.md`에 적고 INTEGRATION 검토·통합을 요청합니다.

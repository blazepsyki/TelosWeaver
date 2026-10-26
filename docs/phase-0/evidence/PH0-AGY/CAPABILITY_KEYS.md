# PH0-AGY 공통 Capability 키 전환 (REQ-024)

| 항목 | 값 |
|---|---|
| 작업 | PH0-AGY 후속: 공통 Capability 키 전환 ([지시서](../PH0-INTEGRATION/AGY_CAPABILITY_KEYS.md)) |
| 담당 | `gemini-3.8-flash/session-c2e931ec` |
| 기준 | Commit `fc038d7`, 공통 계약 `6a4f58a`, Spec v1.5.7 §7.1·§7.2 |
| 날짜 | 2026-10-26 |

## 1. 수행 내역

Spec v1.5.7 및 REQ-024 공통 Capability 계약에 따라 Antigravity Adapter의 키 리터럴을 `telos_schema::capability_keys` 표준 상수로 전환했습니다.

| # | 항목 | 적용 내용 |
|---|---|---|
| K1 | 공통 키 상수 사용 | `crates/adapters/src/agy.rs`의 `detect()` 반환 맵 키 및 `unsupported(...)` / `UnsupportedCapability` 값의 문자열 리터럴을 `capability_keys::{MANAGED_RUN, EVENT_STREAM, CANCEL, NATIVE_RESUME, NATIVE_FORK, SEND_INPUT, ANSWER_APPROVAL, VSCODE_LISTING}` 상수로 치환 |
| K2 | CLI 미탐지 오류 키 통일 | CLI 미탐지 시 `start_run`의 오류를 `unsupported("start_run", …)`에서 `unsupported(capability_keys::MANAGED_RUN, …)`으로 정규화 (Spec §7.2) |
| K3 | 시험 리터럴 정규화 | `crates/adapters/tests/agy_adapter.rs` 및 `crates/adapters/tests/agy_environment.rs`에서 Capability 키를 비교·조회하는 모든 리터럴을 `capability_keys` 상수로 전환 |

### 리터럴 검증
지시서의 리터럴 검색 명령 검증 결과:
```powershell
Select-String -Path "crates/adapters/src/agy.rs" -Pattern '"(managed_run|event_stream|cancel|native_resume|native_fork|send_input|answer_approval|vscode_listing|start_run)"'
# 결과: 0건
```

## 2. 검사 결과

| 검사 | 결과 |
|---|---|
| `cargo fmt --all --check` | 통과 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 통과 (경고 0건) |
| `cargo test -p telos-adapters --test agy_adapter` | 16개 통과 (0 실패) |
| `cargo test --workspace` | 175개 통과 (0 실패, 38개 ignored) |

## 3. 범위 및 불변 사항

- 하드코딩된 문자열 리터럴을 표준 상수로 치환하고 미탐지 시 오류 키를 정규화한 것 외에, 런타임 동작 코드, Capability 판정 상태(`Supported`, `Unsupported`, `Unverified`), 사유(`reason`) 문자열, 증거 수준(`evidence_level`), 공개 API 시그니처는 일체 변경하지 않았습니다.
- 실환경 모델 호출 시험은 실행하지 않았습니다.

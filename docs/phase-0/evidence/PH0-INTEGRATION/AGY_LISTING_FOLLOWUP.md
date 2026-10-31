# PH0-AGY 비차단 후속: `hidden` 화면 확인 결과를 코드 주석에 반영

| 항목 | 값 |
|---|---|
| 작성 | PH0-INTEGRATION (`claude-opus-5.5/session-c9336d2c`), 2026-10-26 사용자 지시 |
| 처리 | **PH0-AGY (Gemini)**, 기존 Adapter 담당 |
| 기준 | 이 지시서를 기록한 `master` Commit, Spec v1.5.6, 계약 `7497877`(변경 없음) |
| 근거 | DEC-08 통합 기록 [README.md](README.md) §10, AGY 증거 [VSCODE_LISTING.md](../PH0-AGY/VSCODE_LISTING.md) §3.3·§4(사용자 화면 확인 `07b7210`), Spec §7.6 Phase 0 판정 |
| 우선순위 | 비차단. G3 판정 조건이 아님 |

## 1. 배경

DEC-08 제출(`17cb846`) 시점에는 `hidden` 대화가 확장 인덱스(`~/.gemini/antigravity/conversation_summaries.db`)에 `source="CORTEX_TRAJECTORY_SOURCE_CLI"`로 나타나 확장 UI에서 숨겨지는지가 미확정이었습니다. 이후 사용자 화면 확인(`07b7210`)에서 `listed`는 확장 대화 목록에 보이고 `hidden`은 보이지 않음을 확인했고, Spec §7.6도 "Claude·Antigravity의 `hidden`은 자동 판정·화면 확인 통과"로 기록했습니다.

코드 주석 두 곳은 아직 제출 시점의 "미검증"을 적고 있어 증거·Spec과 어긋납니다.

## 2. 작업

| # | 대상 | 작업 |
|---|---|---|
| F1 | `crates/adapters/src/agy.rs` `AgyVsCodeListing::Hidden` 문서 주석(현재 87~91행) | "확장 UI에서 실제로 숨겨지는지는 미검증" 문장을 화면 확인 결과로 바꿉니다. 확장 인덱스에 CLI 출처 행이 생길 수 있지만 확장 UI가 이를 목록에서 걸러 표시하지 않는다는 점, 확인 범위(Windows 로컬, 확장 1.9.0·`agy` 1.4.7, 사용자 화면 확인 `07b7210`), 근거 문서(§3.3·§4)를 적습니다. 로그인 분리(`AGY-DATA-02`) 문장은 유지합니다 |
| F2 | `crates/adapters/tests/agy_environment.rs` `env_vscode_listing`의 관찰 주석(현재 802~806행) | "확장 UI 표시 여부는 미검증" 부분을 화면 확인 결과로 바꿉니다. 이 시험이 자동으로 확인하는 것은 여전히 파일 위치와 인덱스 출처뿐이며 UI 숨김은 사용자 화면 확인에 근거한다는 점을 남깁니다(시험이 보장하는 범위를 넓혀 쓰지 않음). 허브 감시 원인은 추정으로 둡니다 |
| F3 | 같은 두 파일에서 같은 뜻의 다른 문장 | 검색(`미검증`·`미확정`·`unverified`)으로 남은 표현을 확인하고, `hidden` UI 숨김에 관한 것만 고칩니다. 미검증 CLI 버전의 `unverified` Capability처럼 다른 뜻은 그대로 둡니다 |

## 3. 범위와 금지

- 주석만 바꿉니다. 동작, 단언, Capability `reason` 문자열, 공개 API는 바꾸지 않습니다. 바꿔야 할 이유를 찾으면 고치지 말고 INTEGRATION에 알립니다.
- Linux·Remote SSH의 화면 숨김은 확인하지 않았습니다. 주석에 확인한 것처럼 쓰지 않습니다(§7.6 "Linux·Remote SSH는 미판정").
- 증거 문서(`docs/phase-0/evidence/PH0-AGY/`)는 이미 결과를 담고 있으므로 고칠 필요가 없습니다. 이번 작업 기록은 아래 §5의 짧은 절로 추가합니다.
- 실환경 시험(모델 호출)은 필요 없습니다. 실행했다면 세션 ID를 기록하고 정리합니다.

## 4. 작업 방식

1. 최신 `master`에서 Branch `ph0/agy-hidden-doc`, Worktree `D:\worktree\TelosWeaver\ph0-agy-hidden-doc`를 만듭니다.
2. 착수 전 배정표 PH0-AGY 행(`integrated → in-progress`, Branch·Worktree·기준 Commit)과 §7에 `master` 단독 커밋으로 기록합니다(§7 표 행 사이 빈 줄 금지).
3. 검사: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p telos-adapters --test agy_adapter`, `cargo test --workspace`(실환경 제외 기본 실행). 결과 숫자를 기록합니다.
4. 완료하면 PH0-AGY 행을 `review`로 바꾸고 결과 Commit과 함께 INTEGRATION 검토·통합을 요청합니다.

## 5. 기록

[VSCODE_LISTING.md](../PH0-AGY/VSCODE_LISTING.md) 끝에 "8. 후속: 주석 보정" 절을 추가해 바꾼 위치·검사 결과·Commit을 적습니다.

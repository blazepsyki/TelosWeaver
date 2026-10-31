# PH0-CODEX 후속: Capability 판정 보정과 미검증 기능 실환경 시험 (G3 재검토 C3)

| 항목 | 값 |
|---|---|
| 작성 | PH0-INTEGRATION (`claude-opus-5.5/session-c9336d2c`), 2026-10-26 사용자 지시 |
| 처리 | **PH0-CODEX (Codex)**, 기존 Adapter 담당 |
| 기준 | 이 지시서를 기록한 `master` Commit, Spec v1.5.6, 계약 `7497877`(변경 없음) |
| 근거 | [G3 대조표](G3_ACCEPTANCE.md) §2·§2.1 C3, [Codex Linux 기록](../PH0-CODEX/LINUX.md), [통합 증거](README.md) §9.3·§9.4 |
| 우선순위 | 비차단(G3 조건 아님) |

## 1. 문제

`codex.rs`의 `detect()`는 `target == "win32-x64"`이고 CLI가 `codex-cli 0.164.0-alpha.11.1`일 때만 `managed`·`stream`·`cancel`·`resume`·`native_fork`를 `supported`·`environment-tested`로 보고합니다. 그 밖의 Host는 모두 `unverified`·`mock-tested`입니다. `vscode_listing`도 Windows 확장 폴더 이름(`openai.chatgpt-26.1021.22027-win32-x64`)일 때만 검증으로 봅니다.

그런데 `psyki-nas`(Linux x64, Remote SSH 확장 26.1021.22027 동봉 CLI 0.164.0-alpha.11.1)에서 Codex 실환경 7개가 통과했고(§9.3), G2 Linux 재실행에서도 Codex `listed` 목록 기록이 확인됐습니다(§9.4). 결과적으로 Linux에서 지원을 낮게 보고합니다.

`send_input`(`turn/steer`)과 승인 응답(Allow/Deny)은 구현과 Mock 시험만 있고 실환경 시험이 없습니다.

## 2. 작업

| # | 작업 | 확인 방법 |
|---|---|---|
| X1 | 검증 판정을 Host별 실제 증거에 맞춥니다. 실환경 시험이 통과한 (플랫폼, CLI 버전) 조합 목록을 두고, 그 목록에 있으면 해당 항목을 `supported`·`environment-tested`로 보고합니다. 최소 `win32-x64`·`linux-x64` × `0.164.0-alpha.11.1`. 목록에 없는 조합은 지금처럼 `unverified` | Mock 시험: 두 플랫폼에서 검증 항목이 `supported`, 다른 버전·플랫폼은 `unverified` |
| X2 | `vscode_listing` 판정도 같은 방식으로 바꿉니다. Linux는 자동 목록 판정(기본 DB 목록 포함)만 확인됐고 화면 확인은 하지 않았으므로, 사유에 Host별 화면 확인 여부를 구분해 적습니다. 판정 근거가 되는 확장 버전 확인을 폴더 이름 문자열 대신 탐지 결과의 확장 ID·버전·플랫폼으로 할 수 있으면 그렇게 합니다 | Mock 시험: Windows·Linux 경로 각각 |
| X3 | `send_input` 실환경 시험(`#[ignore]`): 긴 Turn을 시작한 뒤 `turn/steer`로 지시를 추가하고, 추가 입력이 같은 Turn에 반영되는지, 이벤트 순서가 공통 규칙(Run당 Turn 1개, `turn_finished` 뒤 새 Turn 없음)을 지키는지 확인. 결과에 따라 `send_input`을 `supported` 또는 사유가 있는 `unsupported`로 판정 | Windows 실환경. Linux는 선택 |
| X4 | 승인 응답 실환경 시험(`#[ignore]`): 승인이 필요한 정책으로 파일 쓰기를 요청해 승인 요청 → Allow면 파일 생성, Deny면 미생성·거부 결과를 확인. Adapter 단위 시험이며 Core 정책(현재 `CodexApprovals::Deny` 고정)은 바꾸지 않음 | Windows 실환경. Linux는 선택 |
| X5 | 판정 사유(`reason`) 문자열을 위 결과에 맞게 고칩니다. "steering and approval need environment checks" 같은 문구는 X3·X4 결과로 대체 | Mock 시험에서 사유 확인 |

## 3. 범위와 금지

- 수정 경로는 PH0-CODEX 경로(`crates/adapters/src/codex.rs`, `codex/**`, Codex 시험, 자기 증거)뿐입니다. 공통 계약·Core·다른 Adapter는 바꾸지 않습니다.
- Capability **키 이름**은 이 작업에서 바꾸지 않습니다(REQ-024에서 처리). REQ-024가 먼저 통합되면 그 키를 따릅니다.
- 실행하지 않은 시험을 근거로 `supported`로 올리지 않습니다. X3·X4가 실패하거나 실행하지 못했으면 `unverified`를 유지하고 사유를 적습니다.
- 실환경 시험은 가벼운 모델(`gpt-6-luna`, effort low)과 저장소 밖 일회용 폴더에서 하고, 시험 Thread ID를 기록해 병합 시 공식 Archive합니다. 사용자 대화는 쓰지 않습니다.

## 4. 작업 방식

1. 최신 `master`에서 Branch `ph0/codex-capability`, Worktree `D:\worktree\TelosWeaver\ph0-codex-capability`.
2. 착수 전 배정표 PH0-CODEX 행(`integrated → in-progress`)과 §7에 `master` 단독 커밋으로 기록합니다(§7 표 행 사이 빈 줄 금지).
3. 검사: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`, X3·X4 실환경 시험. Linux를 실행했다면 Host 열을 나눠 적습니다.
4. 결과는 `docs/phase-0/evidence/PH0-CODEX/CAPABILITY.md`에 기록하고, PH0-CODEX 행을 `review`로 바꿔 INTEGRATION 검토·통합을 요청합니다.

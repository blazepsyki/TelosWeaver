# PH0-INTEGRATION 검증 기록

| 항목 | 값 |
|---|---|
| 작업 ID | PH0-INTEGRATION |
| 담당 | `claude-opus-5.5/session-c9336d2c` |
| Branch / Worktree | `main` / `D:\Downloads\Vibe_dev\TelosWeaver` |
| 기준 Commit | `fca1669` |
| G0 기준 Commit | `ae31eb2` |
| 상태 | G0 통과. G1~G3 진행 전 |

## 1. G0 판정

**판정: 통과 (2026-10-24).** Spec §19.5.8의 G0 조건인 배정·공통 인터페이스·시험 경계·최소 Stub 등록을 확정하고 기준 Commit을 기록했습니다.

| 조건 | 근거 |
|---|---|
| 배정 | 배정표 §2. INTEGRATION·CONTRACT 담당 기록, 나머지 7개 작업은 `unassigned` |
| 공통 인터페이스 | 배정표 §5의 7개 계약. 정의 내용은 [PH0-CONTRACT 기록](../PH0-CONTRACT/README.md) §2 |
| 시험 경계 | 공통 시험은 `UnsupportedAdapter` 시험 대역과 Schema·Wire 계약만 검사. Provider별 시험은 각 작업의 `tests/<provider>_*.rs` |
| 최소 Stub 등록 | `crates/adapters/src/runtime/mod.rs`, `crates/core/src/{api,execution,master,policy}/mod.rs`, `crates/rn-client/src/lib.rs`의 계약 재노출 |
| 기준 Commit | `ae31eb2` |

G0 통과는 계약이 정해졌다는 뜻이며 Provider·RN·Core API의 실행 지원을 뜻하지 않습니다.

## 2. 통합 이력

| Commit | 내용 |
|---|---|
| `556e62f` | G0 준비: `[workspace.dependencies]` 일원화, `fixtures` feature, Stub 경로 등록 |
| `cc142cf` | `ph0/contract`(`fd0b206`) 통합 |
| `ae31eb2` | `telos-rn-client`가 `telos_schema::rn`을 재노출하도록 등록 |

통합 시 `ph0/contract`의 변경 파일이 모두 PH0-CONTRACT 독점 경로(및 자기 증거 경로) 안에 있음을 확인했습니다.

## 3. 통합 기준 재검증 (`ae31eb2`)

| 명령 | 결과 |
|---|---|
| `cargo test --workspace --locked` | 통과 (18개) |
| `cargo clippy --workspace --all-targets --locked` | 경고 없음 |
| `cargo fmt --all --check` | 통과 |
| `cd extension && npm ci && npm run build` | TypeScript 42개 생성, `tsc` 통과 |

실행 환경: Windows 10.0.26300 (MINGW64 셸), rustc 1.94.0, cargo 1.94.0, Node 24.14.0, npm 11.9.0. 모두 Contract 수준이며 실환경 시험은 없습니다.

## 4. 한계와 남은 문제

- **자기 검토:** INTEGRATION과 CONTRACT를 같은 담당자가 맡아 G0 판정 주체(INTEGRATION, CONTRACT 확인)가 사실상 한 명입니다. 다른 작업 착수 전에 사용자 또는 다른 에이전트의 계약 검토를 권장합니다.
- **CI 없음:** 원격 저장소가 없어 `.github/workflows`를 두지 않았습니다. 통합 검사는 수동으로 실행합니다.
- **Linux 미검증:** 모든 검사는 Windows에서만 실행했습니다.
- **줄바꿈:** `core.autocrlf=true` 환경에서 새 Worktree가 CRLF로 체크아웃되어, Schema 재생성 시 내용 변경 없이 수정된 파일로 표시됩니다. `.gitattributes`는 Spec §19.5.3의 어느 작업 경로에도 없어 변경 요청 REQ-001로 등록했습니다.
- **미사용 의존성:** `telos-rn-client`의 `serde`·`serde_json`은 재노출 후 현재 쓰이지 않습니다. PH0-RN의 전송 구현에서 쓸 예정이라 유지합니다.

## 5. 다음 단계

- G1: 공용 런타임(HOST)과 대표 Provider Codex의 실제 경로 연결.
- 나머지 7개 작업은 배정 요청을 받아 기록한 뒤 `ae31eb2`를 계약 기준으로 착수합니다.

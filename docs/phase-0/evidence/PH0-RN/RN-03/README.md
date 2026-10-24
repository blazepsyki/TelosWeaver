# PH0-RN-03 Reason CLI·로그인 제거

| 항목 | 값 |
|---|---|
| 서브 태스크·담당 | PH0-RN-03 · `claude-opus-5.5/session-c9336d2c` |
| 작성일 | 2026-10-25 |
| Branch / Worktree | `ph0/rn-03` / `D:\worktree\TelosWeaver\ph0-rn` |
| 기준 | PH0-RN-00 처리표 §3.1·§3.3, Spec v1.5 DEC-07·§12.4~12.5 |

## 1. 제거한 것 — 클로즈드 소스 Reason CLI 연동 (§3.1)

| 대상 | 내용 | 파일 |
|---|---|---|
| Reason 세션 동기화·전달 | Daemon이 몇 초마다 `reason session list --json`을 실행해 Reason 세션을 목록에 넣고 `reason session send`로 전달하던 루프, CLI 관리 모드의 `reason network __relay` 자식 프로세스, 전달 실패 시 재시도 대기(`retry`)와 `--fields` 거부 대응(`plain_until`) | `src/daemon/reason.rs` 삭제, `src/daemon/mod.rs`, `src/daemon/mail.rs`, `src/daemon/sessions.rs` |
| `reason` 에이전트 | 실행 중인 에이전트를 `reason`으로 판별·표시, `setup`의 Reason 항목과 `cfg.reason` 설정 | `src/agent.rs`, `src/setup.rs`, `src/config.rs`, `src/wizard.rs` |
| CLI 관리 설치 모드 | `managed()`(실행 파일이 `~/.reason/bin` 아래면 `reason network`로 이름과 동작을 바꿈). `CLI`·`ENGINE`은 이제 항상 `reason-network` | `src/main.rs`, `src/mcp.rs` |
| Reason 에이전트용 회신 안내 | `render::for_agent`의 `agent` 인자(Reason 에이전트에게만 CLI 회신 안내) | `src/render.rs`, 호출부 4곳 |

## 2. 제거한 것 — Reason 계정 로그인 (§3.3)

| 대상 | 내용 | 파일 |
|---|---|---|
| 명령 | `proof <jws>`(증명 첨부), `endpoint`(증명 대상 id 출력), `init`·`join`의 `--proof` | `src/main.rs` |
| 로컬 API | `Request::Proof`, `CreateTeam`·`Join`의 `proof` 필드 | `src/proto.rs`, `src/client.rs`, `src/daemon/mod.rs` |
| 가입 프로토콜 | `JoinRequest.proof`·`owner_only`, 같은 계정의 다른 컴퓨터 가입 경로(`same_account_needs_owner`)와 거절 누적(`Refusals`). 이제 첫 거절이 최종 답 | `src/net.rs`, `src/daemon/teamops.rs` |
| 팀 상태 | `attestations`(기계별 증명), `attest`·`verified_people`·`has_proof`·`same_account`, 이름 점유 판단의 증명 경로 | `src/team.rs` |
| 표시 | `MachineInfo.verified`·`has_proof`, 이름 옆 "(verified)", `--json` 팀 출력의 두 필드 | `src/proto.rs`, `src/render.rs`, `src/main.rs`, `src/daemon/teamops.rs` |
| 안내 문구 | 에이전트 Skill의 "reasonmachines.com에서 로그인" 안내를 `link`·`join`·`init` 안내로 교체 | `src/skill.md` |

팀원 이름은 모두 미검증입니다. 가입은 초대 코드·팀 링크로만 통제되며, 이름 점유는 "그 사람의 기계가 보증하거나 아무도 그 이름을 쓰지 않을 때"만 허용됩니다(원본의 로그인 없는 경로와 같음).

## 3. RN-00 처리표 정정 — `ring`과 Relay 목록 서명

RN-00은 "`ring`은 로그인 증명 검증에서만 쓰인다"고 결론냈지만, `proof.rs`의 서명 검증 함수(`verify_signed`)는 **Relay 목록**(`relays.rs`, `reasonmachines.com/network/relays.json`)의 서명 검증에도 쓰입니다. Relay는 Spec v1.5에서 유지 대상이므로 다음과 같이 처리했습니다.

- `src/proof.rs`를 `src/signed.rs`로 바꾸고 일반 JWS 검증(RS256, 고정된 Reason 공개 키 1개와 그 해시 고정 시험, 헤더 필드 제한, 크기 제한)만 남겼습니다. 로그인 증명 형식(`Proof`, 발급자·이름·기계 확인, 만료 무시 검증)은 지웠습니다.
- `ring` 의존성은 그대로입니다(`Cargo.toml`·`Cargo.lock` 변경 없음). 이제 `ring`을 쓰는 곳은 `src/signed.rs` 하나입니다.
- 시험용 키는 `src/testdata/signer.pk8`·`forger.pk8`로 이름만 바꿨습니다. 시험 빌드에서만 신뢰하는 키이며 운영 키와 무관합니다.
- RN-00 문서의 해당 결론과 처리표 행에 정정 주석을 달았습니다.

## 4. 호환성

- 원본 RN이 보내는 `JoinRequest`의 `proof`·`owner_only`, 팀 상태의 `attestations`는 이 Fork에서 읽을 때 무시됩니다(`deny_unknown_fields` 없음). 반대로 이 Fork의 요청에는 증명이 없어 원본에서는 로그인 없는 가입으로 처리됩니다. 원본과 섞인 팀은 목표가 아니며 Phase 0은 로컬 전용입니다.
- 기존 `config.toml`의 `reason` 키는 무시됩니다.
- 로컬 API 응답의 `code`는 `name_taken`만 남았습니다.

## 5. 검증 (Windows 11, rustc 1.94.0, `rn-fork/`에서)

| 명령 | 결과 |
|---|---|
| `cargo build` | 성공, 경고 0건 |
| `cargo test --locked` (격리 환경, RN-01·02와 같은 방식) | 단위 시험 178개 통과. RN-02의 190개에서 `proof.rs` 10개·`team.rs` 증명 시험 8개·`net.rs` 1개·`teamops.rs` 1개를 지우고 `signed.rs` 8개를 더함(`reason.rs`의 시험 2개는 Unix 전용이라 원래 Windows에서 실행되지 않음). 시험 후 임시 홈·상태 폴더는 비어 있음 |
| `cargo clippy --locked --all-targets` | 경고 0건 |
| 잔존 검색 | `find_bin("reason")`, `~/.reason/bin`, `reason session`, `__relay`, `crate::proof`, `Request::Proof`, `attestations`, `managed()` 0건. `ring::` 사용처는 `src/signed.rs`뿐 |

**실행하지 않은 것:** `setup`, Daemon, 실제 에이전트, Python E2E 시험.

## 6. 후속으로 넘긴 것

| 항목 | 넘긴 곳 |
|---|---|
| 상태 폴더 `~/.reason/network`, `reason network …` 주석, `REASON_NETWORK_*` 환경 변수, 팀 링크 도메인 | PH0-RN-04(이름 변경) |
| 원본 `tests/e2e/*.py`의 증명 관련 시나리오(`proofs.py`는 Relay 목록 서명 도우미도 담고 있어 다른 스크립트가 import함), `README.md`·`DESIGN.md` | PH0-RN-04(문서·E2E 재작성) |
| Relay 목록 조회를 팀이 있을 때만 하도록 조정 | PH0-RN-04(RN-00 처리표대로) |

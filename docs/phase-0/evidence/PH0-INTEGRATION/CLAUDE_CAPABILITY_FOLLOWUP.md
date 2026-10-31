# PH0-CLAUDE 후속: Capability 사유 정정과 승인 경로 근거 시험 (G3 재검토 C2·C4)

| 항목 | 값 |
|---|---|
| 작성 | PH0-INTEGRATION (`claude-opus-5.5/session-c9336d2c`), 2026-10-26 사용자 결정 |
| 처리 | **PH0-CLAUDE** |
| 착수 조건 | **[REQ-024](REQ-024.md) 단계 B 통합 뒤**(사용자 결정 5-B: 공통 Capability 키 전환과 한 번에 처리) |
| 근거 | [G3 대조표](G3_ACCEPTANCE.md) §2·§2.1 C2·C4, [CLAUDE 기록](../PH0-CLAUDE/README.md) §3.2, [통합 증거](README.md) §9.4 |
| 우선순위 | 비차단(G3 조건 아님) |

## 1. 사용자 결정 (2026-10-26)

| # | 질문 | 결정 |
|---|---|---|
| 1 | Claude `native_resume`·`native_fork` 판정 수준 | **`unverified` 유지, 사유만 정정.** Phase 0 실환경(Windows·`psyki-nas`) 통과 사실과 남은 Phase 1 조건(세션 소유권·잘못된 세션 복구, Resume은 원본 세션에 Turn 추가, Turn 지정 Fork 없음)을 사유에 적음. Antigravity `native_resume`과 같은 기준 |
| 2 | `answer_approval` 상태 | **`unsupported` 유지, 사유 정정.** "Adapter 미구현. CLI는 `--permission-prompt-tool stdio`(+ `initialize` 제어 요청)로 실시간 승인 제공(2.1.310 확인)" 취지 |
| 3 | 실시간 승인 구현 | **Phase 0에서 하지 않음.** Phase 1 승인 정책·UI와 함께 |
| 4 | 근거 시험 | **`#[ignore]` 실환경 시험 추가.** 통과하면 `answer_approval` 사유의 근거 수준을 `environment-tested`로 |
| 5 | 순서 | **REQ-024 단계 B 뒤 한 번에.** 키 이름 전환을 함께 함 |

## 2. 작업

| # | 작업 | 확인 방법 |
|---|---|---|
| L1 | REQ-024에서 확정한 공통 키로 `claude.rs` `detect()`와 `UnsupportedCapability.capability` 값을 전환 | Mock 시험: 키 목록이 계약과 일치 |
| L2 | `native_resume`·`native_fork`: 상태 `unverified` 유지, 근거 수준은 검증 버전에서 `environment-tested`, 사유를 결정 1대로 정정("Phase 0 experiment pending" 제거) | Mock 시험: 상태·근거 수준·사유 |
| L3 | `answer_approval`: 상태 `unsupported` 유지, 사유를 결정 2대로 정정. L4가 통과한 버전에서 근거 수준 `environment-tested`, 그 외 `documented` | Mock 시험 |
| L4 | 실환경 시험 `env_permission_prompt_tool_offers_live_approval`(`claude_environment.rs`, `#[ignore]`): 일회용 폴더, `haiku`, 자식 환경은 기존 시험과 같이 부모 세션 변수 제거·`CLAUDE_CODE_ENTRYPOINT=sdk-cli`. `-p --input-format stream-json --output-format stream-json --verbose --permission-mode default --permission-prompt-tool stdio --tools Write`로 시작해 `initialize` 제어 요청을 보낸 뒤 Write를 요청. ① `control_request`(`can_use_tool`, `Write`)가 오는지 ② Allow 응답이면 파일 생성 ③ Deny 응답이면 미생성·`permission_denials` 1. 세션 ID를 출력하고 시험이 세션 폴더를 정리 | Windows 실환경 통과. Linux는 선택 |
| L5 | 증거 기록: `docs/phase-0/evidence/PH0-CLAUDE/CAPABILITY.md` | — |

참고: INTEGRATION이 2026-10-26 같은 절차를 Windows에서 직접 실험해 ①②③을 확인했습니다(세션 `b3192da1…`·`e3e50b98…`·`86a3873f…`, 삭제 완료). `--permission-prompts host`만 주고 응답자가 없으면 자동 거부됐습니다. L4는 이 결과를 재현 가능한 시험으로 남기는 것입니다.

## 3. 범위와 금지

- `answer_approval`·`send_input` 구현, Core 정책·승인 흐름 변경은 하지 않습니다(결정 3).
- 수정 경로는 PH0-CLAUDE 경로(`crates/adapters/src/claude.rs`, `claude/**`, Claude 시험·fixture, 자기 증거)뿐입니다.
- 실행하지 않은 시험을 근거로 근거 수준을 올리지 않습니다.

## 4. 작업 방식

1. REQ-024 단계 B 통합을 확인한 뒤 최신 `master`에서 Branch `ph0/claude-capability`, Worktree `D:\worktree\TelosWeaver\ph0-claude-capability`.
2. 착수·완료를 배정표 PH0-CLAUDE 행과 §7에 `master` 단독 커밋으로 기록합니다.
3. 검사: `cargo test --workspace`, 엄격 Clippy, fmt, `claude_adapter`, L4 실환경.
4. 독립 검토자 배정은 착수 시 사용자에게 확인합니다(PH0-CLAUDE와 INTEGRATION 담당이 같은 세션).

# PH0-CLAUDE 후속: 공통 Capability 키·사유 정정·실시간 승인 근거 (G3 재검토 C2·C4, REQ-024)

| 항목 | 값 |
|---|---|
| 담당 | `claude-opus-5.5/session-c9336d2c` (PH0-CLAUDE, 독립 검토자 없이 진행: 사용자 지시 2026-10-26) |
| 기준 | `main` `a4d544c`(착수 기록 `fc038d7`), Spec v1.5.7, 계약 `6a4f58a` |
| Branch / Worktree | `ph0/claude-capability` / `D:\worktree\TelosWeaver\ph0-claude-capability` |
| 지시서 | [CLAUDE_CAPABILITY_FOLLOWUP.md](../PH0-INTEGRATION/CLAUDE_CAPABILITY_FOLLOWUP.md) |
| 코드 Commit | `36c5379` |

## 1. 변경

| # | 내용 | 위치 |
|---|---|---|
| L1 | `detect()` 키와 `UnsupportedCapability.capability`를 `telos_schema::capability_keys` 상수로. CLI 미탐지 오류 `start_run` → `managed_run` | `crates/adapters/src/claude.rs` |
| L2 | `native_resume`·`native_fork`: `unverified` 유지(사용자 결정 1-B). 사유는 "Phase 0 environment tests passed on Windows and Linux"와 남은 조건(Resume은 원본 세션에 Turn 추가·Fork는 최신 지점에서 분기·특정 Turn 기준 미지원·세션 소유권과 잘못된 세션 복구는 Phase 1). 근거 수준은 검증 버전(2.1.310)에서 `environment-tested`, 그 외 `documented` | 같음 |
| L3 | `answer_approval`: `unsupported` 유지(결정 2-A). 사유를 "Adapter 미구현, CLI는 `--permission-prompt-tool stdio`로 실시간 승인 제공(2.1.310 확인), Phase 1 승인 정책·UI로 미룸"으로 정정. L4가 통과한 버전에서 `environment-tested`, 그 외 `documented`(이전 사유 "print mode has no live approval path"는 사실과 달라 제거). 메서드가 돌려주는 오류 사유도 같은 문구 | 같음 |
| 추가 | **`source_turn_id`가 있는 Resume·Fork를 프로세스 시작 전에 거부**(`native_resume`·`native_fork` 키). 이전에는 값을 무시하고 최신 지점에서 이어 써, 요청과 다른 동작을 조용히 했습니다(Spec §2.1 원칙 7, §7.1 "세부 조건은 `reason`과 실행 전 검사"). 지시서에 없던 변경이지만 L2 사유와 맞추려면 필요해 함께 고쳤습니다 | `command_args` |
| 문서 | 모듈 문서의 승인·Resume/Fork 설명을 위 판정으로 갱신 | `claude.rs` 머리 |
| L4 | 실환경 시험 `env_permission_prompt_tool_offers_live_approval`: Adapter를 거치지 않고 CLI를 직접 실행(`-p --input-format stream-json --output-format stream-json --verbose --model haiku --permission-mode default --permission-prompt-tool stdio --tools Write`), `initialize` 제어 요청 뒤 Write 요청. Allow 응답 → 파일 생성·거부 0, Deny 응답 → 미생성·거부 1, 둘 다 `can_use_tool`(`Write`) 요청 수신·종료 코드 0. 부모 세션 변수 제거·`CLAUDE_CODE_ENTRYPOINT=sdk-cli`, 120초가 지나면 이 시험의 프로세스만 종료 | `crates/adapters/tests/claude_environment.rs` |
| Mock | 공통 키 8개만 보고, 미구성 `start_run` 오류 키 `managed_run`, `send_input`·`answer_approval` 오류 키, Resume·Fork 근거 수준·사유, 승인 사유·근거 수준, 미검증 버전의 `documented`, Turn 지정 Resume·Fork 거부(프로세스 미생성) | `crates/adapters/tests/claude_adapter.rs` |

## 2. 검사

| 검사 | Windows `windows-local` | `psyki-nas` |
|---|---|---|
| `claude_adapter`(Mock) | 16개 통과 | 16개 통과 |
| `env_permission_prompt_tool_offers_live_approval` | 통과 12.8초 (세션 `84bd3c70-1b81-46e1-9f5d-4e9eb8549658`·`b4ed85d9-96e9-47f4-b19c-4048100a9362`) | 통과 11.6초 (세션 `9ee4f7ad-a752-4d1f-b912-26081e371de7`·`42cd2b6b-669d-4770-ad38-e1decc164463`) |
| `cargo test --workspace --locked` | 175개 통과·39개 제외(새 실환경 시험 1개 추가) | — |
| 엄격 Clippy·fmt | 통과 | — |

수정 전 동작 검출: `command_args`의 Turn 지정 거부만 임시로 빼고 `native_fork_passes_resume_and_fork_flags`를 돌리면 실패(`claude_adapter.rs:518`, 거부 대신 시작), 되돌리면 16개 통과를 확인했습니다. 한계: 기존 Resume·Fork 실환경 시험(`env_resume_and_fork`)은 이번 변경 경로(`source_turn_id` 없음)와 같아 다시 돌리지 않았습니다.

## 3. 시험 세션

위 네 세션은 각 Host의 `~/.claude/projects/<telos-claude-env/approval-{allow,deny} 인코딩>`에 있으며 병합 시 폴더째 삭제합니다.

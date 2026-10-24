# PH0-CODEX 통합 인계

작성일: 2026-10-24. 담당: `Codex / PH0-CODEX-20261024`. 코드: `a498976`, `0dcc1ca`. 통합 기준: main `04cc2c3`를 반영한 `83cf156`, 계약 `3a37919`. 검증·실제 버전·시험 세션 정리는 [README](README.md)를 따릅니다.

현재 상태: main 통합 `64f9674`, 배정표 integrated `86f580e`. REQ-012·REQ-014는 완료됐습니다. 이번 현황 재확인에서도 Workspace 118개·엄격 Clippy·fmt가 통과했습니다. 아래에서 Core 연결·추가 환경 검증만 남은 작업으로 구분합니다.

## 1. 공통 등록·정리 결과

배정표의 REQ-012·REQ-014는 CONTRACT·INTEGRATION이 `64f9674`에서 적용했습니다. [COMMON_CHANGES.patch](COMMON_CHANGES.patch)는 다음 두 변경을 제출했던 이력이며 현재 main에 재적용하지 않습니다.

1. `crates/adapters/src/lib.rs`: `pub mod codex;`로 구성·Resolver·탐색 규칙 공개.
2. `crates/adapters/src/unimplemented.rs`: 미사용 `unimplemented_adapter!` 매크로와 재수출 제거. `UnsupportedAdapter` 구현은 그대로 유지.

제출 시 패치의 `git apply --check`를 확인했고 실제 공통 적용은 담당자가 수행했습니다. 두 번째 변경으로 unused-macros·unused-imports가 해소됐습니다. 현재 엄격 Clippy는 경고 예외 없이 통과하며 공통 시험 대역·계약 시험도 유지됩니다.

공통 Cargo 의존성·Lock·Schema·AgentAdapter Trait 변경은 없었습니다. 기존 통합 담당이 integrated로 기록하고 구현 Branch·Worktree를 정리했습니다. 현황 문서 갱신은 최신 main의 별도 Branch·Worktree에서 진행하며 기존 게이트 판정을 변경하지 않습니다.

## 2. Core 연결 순서

`CodexAdapter::default()`는 실행 설정이 없으므로 Managed 시작을 거부합니다. 실제 연결은 `CodexAdapter::new(CodexConfig)`를 사용합니다.

| 구성 값 | Core에서 공급할 근거 |
|---|---|
| executable, cli_version | 실행 Host의 `NativeCliDiscoverer` 결과. `codex::cli_rule(target_platform)` 입력 사용. 검증한 경로·버전을 같은 Run에서 고정 |
| host_id | 실제 실행 Host 정체성. Task·RunContext의 Host와 일치해야 함 |
| runtime | HOST의 `Arc<dyn ProcessRuntime>`. 실제 환경은 `NativeRuntime` |
| launcher | 신뢰된 `Arc<dyn CodexLaunchResolver>`. TaskSpec·RunContext를 실제 Workspace·정책·Snapshot과 대조 |
| request_timeout, cancel_grace | Core 실행 정책의 Protocol 제한과 프로세스 종료 유예 |

`CodexLaunchResolver::resolve(task, context)`는 승인된 `cwd`, 조립된 `prompt`, 사용 가능한 정확한 `model`, `effort`, `sandbox`, `approvals`, `run_timeout`을 반환합니다. Task의 문자열 ID나 모델 출력만으로 경로·권한을 승인하지 마십시오. Phase 0 시험용 Resolver는 제품 정책 구현이 아닙니다.

- Fresh/Capsule은 새 Native Thread를 만듭니다. Capsule 본문·Snapshot·Profile 적용은 Core가 `prompt`에 조립해야 합니다. Adapter는 입력 바인딩을 대신 해석하지 않습니다.
- Resume는 source_session_id를 요구하고 원본 Thread ID 유지 여부를 검사합니다. source_turn_id를 지정한 Resume는 `resume_at_turn` 미지원으로 거부합니다.
- NativeFork는 source_session_id와 선택적 source_turn_id를 받아 공식 `thread/fork`를 호출합니다. 새 Thread ID를 확인합니다. 이 기능이 Workspace 파일 복제를 수행하지는 않습니다.
- 실행 모델·Sandbox 설정 변경은 해당 Native 세션에서 허용되는지 Core 정책이 판단해야 합니다. 실패 시 다른 모델이나 Fresh로 조용히 전환하지 않습니다. Capsule/Fresh 대체 선택·전환 사유 기록은 상위 오케스트레이션 범위입니다.

RunStarted 및 모든 ProviderEvent의 native_session_id/native_turn_id를 Core가 저장해야 합니다. Adapter의 Run Registry와 이벤트는 메모리에만 있으며 Daemon 재시작 후 재연결·DB 복구를 제공하지 않습니다. Daemon 종료 시 활성 Run에 cancel_run을 호출하고 최종 이벤트 또는 inspect_run의 종료 상태를 확인하는 연결이 필요합니다.

## 3. 입력·승인·취소 경계

- `send_input`은 현재 Turn에 `turn/steer`를 보내며 expectedTurnId를 명시합니다. Run별 입력 키는 동일 내용의 재시도를 중복 전송하지 않습니다. 종료된 Run의 후속 Turn 생성 API는 아닙니다. Mock 검증만 했습니다.
- 기본 `CodexApprovals::Deny`에서는 도구 승인·권한 상승 요청을 거부합니다. workspace-write Sandbox 자체로 허용한 cwd의 작업은 별도 대화형 승인 없이 실행될 수 있으므로 시작 전 정책 검사가 필요합니다.
- `CodexApprovals::Prompt`는 대상 Thread/Turn이 일치하는 native 승인 요청을 공통 ApprovalRequired로 전달합니다. `ApprovalAnswer::Allow`를 보내려면 `CodexLaunchResolver::authorize_approval(task, context, request)`가 **그 native action**을 실제 정책·Workspace·Run에 대조해 허용해야 합니다. 기본 구현은 미지원으로 거부합니다. permission escalation에는 권한을 부여하지 않으며 네트워크 승인도 거부합니다. 실제 대화형 Allow 성공은 미검증입니다.
- `cancel_run`의 true는 취소 요청 수락입니다. Turn의 실제 interrupted 결과와 프로세스 종료를 별도로 관찰해야 합니다. 정상 완료와 취소 요청 경합에서는 관찰된 실제 완료를 우선합니다.
- 정상 Turn 뒤에도 프로세스를 정리하므로 exit 0만으로 Turn 성공을 판단하지 마십시오. 잘못된 모델의 실환경 실패도 프로세스 exit 0으로 정리됐습니다.

## 4. 보존·제한·정리 API

현재 제한은 Adapter 내부 보호 상한입니다. 전체 프로젝트 동시성·예산 정책을 대체하지 않습니다.

| 대상 | 상한·동작 |
|---|---|
| Adapter Registry | 시작 키 128개. 완료 항목도 재시도 멱등성을 위해 유지하며 초과 시 ResourceLimit |
| App Server | 미완료 요청 32개, 알림 채널 64개, 한 줄 1 MiB, 공용 출력 버퍼 4 MiB |
| Run 이벤트 | 최대 4096개·8 MiB 안에서 종료 이벤트 공간 예약. 임의 이벤트 탈락 대신 명시적 실패 |
| 입력·승인 | Run별 입력 결과 키 32개, 대기 승인 16개, 입력 텍스트 64 KiB |
| 재구독 | sequence 1부터 메모리 보존. 보존된 마지막 sequence보다 큰 cursor는 EventsUnavailable |

실제 처리 시 연속 sequence·최종 이벤트를 확인하고 이벤트 유실·종료 불명을 성공으로 바꾸지 마십시오. 로그·UI로 보내는 stderr 원문은 이 Adapter가 제공하지 않습니다.

`owned_thread_ids()`는 이 Adapter가 새로 만든 Thread 중 아직 Archive하지 않은 ID만 반환합니다. `archive(thread_id, cwd)`는 해당 소유 목록 밖의 Thread를 거부하고 성공한 Archive 재시도를 멱등 처리합니다. Resume 원본이 외부에서 들어온 Thread라면 이 Adapter가 소유하는 것으로 바꾸지 않습니다. Archive는 대화 보관 처리이며 사용자 데이터 삭제 API가 아닙니다. 완료한 사용자 세션의 자동 Archive 정책은 Core에서 별도 결정해야 합니다.

## 5. 지원 판정과 후속 검증

`detect(host_id)`는 구성된 Host와 현재 실행 플랫폼을 확인합니다. 실제 Windows x64 `codex-cli 0.164.0-alpha.11.1`에서 확인한 managed·stream·cancel·resume·native_fork만 EnvironmentTested/Supported로 표시합니다. 다른 버전·플랫폼과 send_input·approval은 Unverified로 남습니다. CLI 업데이트 후 같은 이름의 메서드가 있다는 이유로 실환경 증거를 승계하지 마십시오.

통합 상태와 다음 검증 순서는 다음과 같습니다.

1. **공통 등록 완료:** REQ-012 공개가 반영됐고 병합본 Codex 탐지도 통합 담당이 재확인했습니다. 외부 Core 소비 경로의 실제 연결 시험은 아래 3번에 포함합니다.
2. **공통 검사 완료:** REQ-014 정리 후 `cargo test --workspace --locked --offline` 118개 통과, `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`, `cargo fmt --all -- --check` 통과를 이번 현황 갱신에서도 확인했습니다.
3. 신뢰된 Codex 실행 Resolver와 공통 탐색 결과를 Core에 연결하고 Local API를 통한 G2 실행·이벤트 저장·Daemon 종료 정리를 검증합니다. 대표 G1 Provider를 변경할 필요는 없습니다.
4. Linux·Remote SSH는 기존 사용자 대기 지시가 해제된 뒤 대상 환경에서 수행합니다. 파일 Worktree 격리·Attached·Fork 전체 비용 비교는 해당 단계에서 별도 검증합니다.

PH0-CODEX 자체 구현과 Windows 증거는 통합됐으며, 남은 일은 Core 연결과 명시한 미검증 기능·환경입니다. 전체 Phase 0 완료는 G2·G3를 포함한 별도 판정을 따라야 합니다.

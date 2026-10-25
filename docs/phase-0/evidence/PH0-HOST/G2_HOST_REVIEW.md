# G2 HOST 확인 및 REQ-019 사후 확인

| 항목 | 값 |
|---|---|
| 담당 | `Codex / G2-REVIEW-01a17034` · PH0-HOST |
| 날짜 | 2026-10-25 |
| 검토 기준 | main `08a4a13`, 착수 배정 `ef3d2f1`, Spec v1.5.1, 공통 계약 `7497877` |
| Branch / Worktree | `ph0/host-g2-review` / `D:\worktree\TelosWeaver\ph0-host-g2-review` |
| 대상 | G2 통과 판정의 HOST 확인, REQ-019(`aff3c52`, 통합 `8fdc8a9`) 사후 확인 |
| 결과 | **HOST 범위 G2 통과 판정에 동의하며 REQ-019 사후 확인 완료. 검증 범위에서 차단 결함 없음.** |

## 1. 최신 구현 상태

RN-06의 RPC(`cfba833`)와 RN-07의 Client·Core 연결(`8fdc8a9`)이 main에 통합되었습니다. Spec은 v1.5.1이며 REQ-018을 포함한 계약 기준은 `7497877`입니다. Core는 운영자 정책의 `rn.executable`로 프로젝트별 상태 폴더 아래 RN을 시작·감시·제한 재시작·정지합니다. Core 밖에는 `core.health`의 `rn`만 노출하며 메시지는 내부 Rust API로 사용합니다.

INTEGRATION은 G2를 통과 판정하고 CODEX·HOST·EXT·AGY의 확인을 요청했습니다. 이 문서는 HOST의 확인 결과이며 다른 담당의 확인이나 게이트의 최종 상태를 대신하지 않습니다. G3·PH0-RN-08의 Linux·Remote SSH·제거 수용 검증은 남아 있습니다.

## 2. REQ-019 검토

`ExecutionSupervisor`에 `HealthComponent`와 `add_health_component`가 추가됐고, `health()`가 기본 항목 뒤에 공급자 결과를 붙입니다. 실행 승인·Run 등록·이벤트 수집·취소의 경로는 바뀌지 않았습니다.

| 확인 항목 | 결과 |
|---|---|
| 기본 항목 | `execution`·`scheduler`·`persistence` 순서와 상태·detail 유지 |
| 전체 Health | 기존 `degraded`와 serverVersion 유지. 추가 항목의 상태로 전체 판정을 새로 계산하지 않음 |
| 공급자 평가 | 조회할 때마다 호출하므로 RN의 `ok → degraded → ok`가 반영됨. 등록 순서대로 추가 |
| 실제 RN 공급자 | `RnService::component()`가 상태를 잠금 아래 복사하며 RPC·프로세스 대기 같은 I/O는 수행하지 않음. 공급자는 동기·비차단이어야 한다는 연결점 계약과 일치 |
| API·Wire | 기존 `HealthResult.components`로 전달, Schema·API 버전 변경 없음. 인증된 Named Pipe API에서 동적 상태 반영 확인 |
| 실행 독립성 | 공급자 등록·조회는 Run Registry·권한 판정에 연결되지 않음. RN Degraded 대역에서 시작·Running 조회·취소·Cancelled·이벤트 재생 확인 |

사후 확인을 위해 HOST 소유 시험에 다음 3개를 추가했습니다. 제품 코드는 수정하지 않았습니다.

- `host_api::registered_health_components_preserve_builtins_and_refresh_their_status`: 여러 공급자의 순서·반복 조회·상태 및 detail 갱신, 기본 항목·전체 상태·버전 보존.
- `host_api::local_api_refreshes_an_added_rn_component_without_changing_default_health`: 인증된 실제 Local API에서 RN 상태 변화와 기존 Health 보존.
- `host_execution::degraded_rn_health_does_not_gate_run_start_tracking_replay_or_cancel`: RN Degraded와 Managed Run 제어의 독립성. 이 시험의 Adapter는 대역이며 실제 Provider 실행 증거가 아닙니다.

## 3. 실제 RN 재확인

명령: `TELOS_RN_EXECUTABLE`을 실제 `rn-fork/target/debug/telos-rn.exe`로 지정하고 `cargo test -p telos-core --test phase0_rn rn_round_trip_restart_and_stop_with_core --locked -- --ignored --exact --nocapture --test-threads=1`을 실행했습니다. Cargo 산출물은 대표 저장소 `target`을 사용했습니다.

`TEMP`·`TMP`를 새 시험 전용 폴더 `%TEMP%/telos-g2-review-705dff271f9445d9b8ed62bfab40756a`로 지정했습니다. 기존 시험 함수가 정리·생성하는 `telos-phase0-rn/real`은 이 폴더 안에만 만들어졌습니다. 정책·Core 상태·RN DB도 이 시험 폴더 안에 있습니다.

| 단계 | 결과 |
|---|---|
| 시작 | RPC 0.1, Fork 빌드 Commit `ef3d2f1`, RN PID `45256`, `started by Core` |
| 메일함 왕복 | 시험 메일함 `review`에 `rn07-m1` 전송, Receipt `queued → delivered`, Core 메일함의 답장 `message` 이벤트 확인 |
| 강제 종료 | PID `45256` 종료 후 `stopped; restarting (1/3)` Degraded 확인 |
| 재시작 | 새 PID `46172`, RN `ok`. 저장된 Receipt 유지와 이전 커서 `resync_required` 확인 |
| 정지 | `shutdown()` 후 RN RPC 접속 실패. 시험 PID 종료 확인 |

이 시험은 실제 RN·공용 런타임·실제 Pipe를 사용하며 Provider·모델은 호출하지 않습니다.

## 4. 검증 결과

환경: Windows 로컬, Node v24.14.0, VS Code 1.143.1. Rust 검사는 Worktree에서 `--target-dir D:/Downloads/Vibe_dev/TelosWeaver/target`으로 실행했습니다.

| 명령 | 결과 |
|---|---|
| `cargo test -p telos-core --test host_api --test host_execution --locked` | 14개 통과(기존 11개 + 사후 확인 3개) |
| `cargo test --workspace --locked --quiet` | 155개 통과, 실환경 32개 기본 제외 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 통과 |
| `cargo fmt --all -- --check` | 통과 |
| 실제 RN 왕복·재시작·정지 시험 | 1개 통과(기본 제외 시험에서 이 항목만 명시 실행) |
| `rn-fork/`의 `cargo build --locked` | 성공 |
| Extension 타입 생성·빌드·시험 | 29개 통과. EXT 확인 문서 참고 |
| 실제 Core 연결·격리 Extension Host | 각각 통과. EXT 확인 문서 참고 |

## 5. G2 확인 범위와 남은 조건

- HOST의 기존 G1 실행 감독·프로세스 회수·API와 새 RN 연결의 정합성을 코드 및 위 시험으로 확인했습니다. INTEGRATION의 G2 통과 판정에 동의합니다.
- 실제 Claude Run 중 RN 장애에도 추적·취소가 유지된 증거는 [RN-07 기록](../PH0-RN/RN-07/README.md) §3.2를 검토했습니다. 이번에는 유료 Provider 시험을 재실행하지 않았으며 대역 시험과 구분합니다.
- RN 장애 중 Provider 출력 이벤트가 계속 수집됐다는 주장은 하지 않습니다. 기존 실환경 기록도 추적·취소·실제 종료·자식 회수 확인으로 한정합니다.
- 기존 RN에 연결만 하는 경로, 재시작 상한의 실제 연속 실패, 재시작 도중 Core 정지, Linux·Remote SSH는 이번 재검증에 포함하지 않았습니다. 이 경로까지 검증 완료했다는 의미가 아닙니다.
- 작업 중 main `2fff559`에 PH0-AGY의 G2 확인 완료 기록이 추가됐음을 확인했습니다. 이 변경은 배정표뿐이며 이번 검증 대상 제품 코드와 겹치지 않습니다. 이 문서와 CODEX·EXT 확인을 합치면 요청된 각 담당 확인이 갖춰집니다. G2 최종 상태·REQ-019 확인 상태 반영은 기존 INTEGRATION 담당에게 인계합니다. Linux 대기·G3·RN-08의 조건은 변경하지 않습니다.

# PH0-HOST 구현 및 검증 기록

| 항목 | 값 |
|---|---|
| 작업 ID·담당 | PH0-HOST · Codex / `PH0-HOST-20261024` |
| 갱신·검증일 | 2026-10-24, Windows 로컬 |
| 현재 상태 | in-progress. 기존 요청 1~3은 master 통합 완료(`685bd96`). 후속 G1 HOST 확인·REQ-013 ③ 구현·검증 완료 제출. Linux·Remote SSH는 사용자 지시로 대기 |
| Branch / Worktree | `ph0/host` / `D:\worktree\TelosWeaver\ph0-host` |
| 이번 기준 | `3fe9153` 및 착수 `66e9cb9`, Spec 개정 1.4.3, 공통 계약 `3a37919`. §1~§6은 이전 `eb642fd` 기준 REQ-010 연결 이력 |
| 이번 코드 Commit | `4791c5d` (G1 검토·REQ-013 ③), 이전 REQ-010 연결 `a3f196a`(통합 `685bd96`) |
| 이전 결과 | 구현 `366d524`·`d1d818f`, 인계 `29295c9`, master 1차 통합·REQ-011 `1d8734f` |
| 기준 문서 | [Spec](../../../DEVELOPMENT_v1.5.md), [대표 배정표](../../WORK_ALLOCATION.md), [CONTRACT 인계 §6.3](../PH0-CONTRACT/README.md#63-host-인계-엔진-연결) |

현재 제출은 [G1 HOST 확인·REQ-013 ③ 처리 결과](G1_HOST_REVIEW.md)를 따릅니다. 아래 §1~§6은 이전 REQ-010 연결 당시의 기록이며, 당시 테스트 수와 미완료 표기를 현재 상태로 사용하지 않습니다.

이전 제출은 기존 탐색 엔진을 확정된 REQ-010 계약에 연결한 결과입니다. 대표 저장소의 의존성·진입점 통합과 계약 확정을 반영해 HOST Branch를 `master`까지 fast-forward한 뒤 작업했습니다. G1~G3 판정이나 PH0-HOST 전체 verified 전환은 하지 않습니다.

## 1. 이번 수정 파일과 소유권

코드 변경은 `git diff --name-only eb642fd..a3f196a`로 확인합니다.

| 경로 | 변경 내용 |
|---|---|
| `crates/adapters/src/discovery/mod.rs` | `NativeCliDiscoverer`의 공개 Port 구현, 공통 DTO·실패 분류·위치·플랫폼 기록 연결 |
| `crates/adapters/src/lib.rs` | CONTRACT 인계의 명시적 예외에 따라 `mod discovery;`, `pub use discovery::NativeCliDiscoverer;` 두 줄만 추가 |
| `crates/adapters/tests/discovery_extensions.rs` | 내부 소스 포함을 제거하고 공개 API를 통한 확장 선택·기록·실패 시험 8개로 전환 |
| `crates/adapters/tests/discovery_resolution.rs` | 공개 API를 통한 우선순위·관리 경로·버전·변경 감지·실제 로컬 프로세스 시험 9개 |
| `crates/adapters/tests/support/runtime/discovery.rs` | 공개 ProcessRuntime 시험 대역과 임시 파일 Fixture. 제품 내부 모듈을 포함하지 않음 |
| `crates/adapters/tests/fixtures/runtime/fixture.rs` | 로컬 실제 프로세스 시험용 `--version` 응답 추가 |
| 이 증거 디렉터리 | 현재 검증 기록과 [인계 문서](HANDOFF.md) 갱신 |

Cargo.toml·Cargo.lock·Schema·Provider별 모듈·Core 진입점은 이번에 변경하지 않았습니다. 배정표는 대표 저장소 master에서 자기 행·변경 기록만 별도 Commit으로 갱신합니다.

## 2. 연결한 공개 계약

### 2.1 호출 경계

`telos_adapters::NativeCliDiscoverer::new(Arc<dyn ProcessRuntime>)`로 런타임을 주입하고 `CliDiscoverer::discover(provider_id, rule, request)`를 호출합니다. 입력은 기존 `ProviderCliRule`, `DiscoveryRequest`이며 출력은 기존 `telos_schema::CliDiscovery`입니다. 별도의 공통 타입이나 Schema를 만들지 않았습니다.

- 결과에는 요청의 `provider_id`, `host_id`, `checked_at`을 보존합니다. 엔진에서 확인 시각을 새로 만들지 않습니다.
- `found`에는 절대 실행 경로·출처·CLI 버전·소문자 SHA-256을 기록합니다. 확장 출처에는 확장 ID·버전·설치 경로·폴더 결정 방법·해당 targetPlatform을 기록합니다.
- `BinaryLocation`은 명시/PATH의 `Direct`, 확장 상대 경로의 `RelativePath`, 제한 파일 이름 검색의 `NameSearch`, 확장 밖 관리 경로의 `ManagedPath`로 구분합니다.
- 플랫폼 공용·접미사 없는 확장은 `target_platform`을 생략합니다. 활성 메타데이터 또는 선택 폴더의 플랫폼 접미사를 기록합니다.
- 실패는 반환 오류가 아니라 `unavailable` 값입니다. 실패한 출처를 보존하고 내부 오류 문자열의 비교로 분류하지 않습니다. Parser·OS·프로세스 오류의 원문이나 파일 내용은 reason으로 복사하지 않습니다.

### 2.2 실패·선택 규칙

명시 경로 → 확장이 포함·관리하는 실행 파일 → PATH 순서를 유지합니다. 명시 경로 확인 실패와 선택한 파일의 버전 확인 실패는 다른 출처로 대체하지 않습니다. 관리 경로 출처는 해당 확장이 설치된 경우에만 선택합니다.

| 공통 실패 분류 | 연결·시험한 경우 |
|---|---|
| `not_found` | 후보 없음, 다른 플랫폼 폴더, 상대 PATH 항목만 존재 |
| `ambiguous` | 활성 메타데이터 여러 개, 같은 버전의 적용 가능 폴더 여러 개, 이름 검색 파일 여러 개 |
| `invalid_metadata` | 깨진 JSON·잘못된 필드 형식·폐기된 활성 확장·확장 경로 탈출 |
| `invalid_path` | 잘못된 명시 경로·일반 실행 파일이 아닌 경로·관리 경로 탈출·비 UTF-8 Windows 경로 |
| `version_check_failed` | 시작 실패, 출력 I/O 오류·길이/총량/UTF-8 오류·빈 출력, 비정상/불명/외부 종료, 5초 타임아웃 |
| `changed_during_discovery` | 버전 확인 전후 실행 파일 SHA-256 불일치 |
| `unsupported_platform` | 엔진이 처리하지 않는 실행 targetPlatform |
| `io_error` | 나머지 파일시스템 I/O 실패 |

기존 제한을 유지합니다. 메타데이터는 4 MiB, 확장 이름 검색은 깊이 8·항목 10,000개, 버전 명령은 5초·한 줄 16 KiB·총 출력 64 KiB입니다. 비정상 버전 확인은 프로세스 정리를 요청합니다. `found`는 Provider 기능이나 로그인 지원의 확인을 뜻하지 않습니다.

## 3. 시험과 환경

검증은 `D:\worktree\TelosWeaver\ph0-host`의 공식 Cargo Workspace에서 수행했습니다. rustc/cargo 1.94.0, Windows 로컬 환경이며 루트 Cargo.lock을 `--locked --offline`으로 사용했습니다. 현재 Workspace의 sha2는 0.11.0, libc는 0.2.190입니다. 이전 독립 Manifest의 버전과 구분합니다.

| 명령 | 실제 결과 | 종류 |
|---|---|---|
| `cargo test -p telos-adapters --test discovery_extensions --test discovery_resolution --locked --offline` | 17개 통과, 종료 코드 0 | 공개 API·Fixture·ProcessRuntime 시험 대역·로컬 실행 프로세스 |
| `cargo test --workspace --locked --offline` | 71개 통과, 종료 코드 0 | Contract·HOST 전용·공통 시험 |
| `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | 경고 없음, 종료 코드 0 | 정적 검사 |
| `cargo fmt --all -- --check` | 통과, 종료 코드 0 | 형식 검사 |
| `git diff --check` | 통과, 종료 코드 0 | 공백 검사 |

전체 71개는 HOST 전용 40개(런타임·I/O 11, 탐색 17, API 8, 실행 감독 4)와 공통 31개입니다. 탐색 시험은 이전 7개에서 17개로 전환·확장했습니다.

모든 탐색 시험은 외부에서 공개하는 `CliDiscoverer`/`NativeCliDiscoverer`를 호출합니다. `#[path = "../src/discovery/mod.rs"]`를 제거했습니다. `#[path]`로 포함하는 파일은 테스트 전용 지원 코드뿐입니다.

실제 로컬 프로세스 시험은 보관한 Rust Fixture를 임시 디렉터리에 빌드하고 `NativeRuntime`으로 `--version`을 실행했습니다. 실제 AI CLI·인증·네트워크 호출은 없습니다. 탐색 타임아웃 시험은 5초가 지난 뒤 정리 요청을 확인합니다. Windows 파일시스템에서는 비 UTF-8 경로가 실행 전에 거부되는 것을 확인했습니다.

## 4. 현재 구현 범위와 남은 일

| 영역 | 현재 상태 |
|---|---|
| 공용 런타임·Local API·실행 감독 | 1차 구현이 `1d8734f`로 master에 통합됨. 이번에 전체 회귀 시험 통과 |
| REQ-011 | 완료. 공식 Workspace 의존성과 최소 Daemon 진입점이 반영됨 |
| REQ-010 계약 | `3a37919`로 확정·통합됨 |
| REQ-010 엔진 연결·공개 시험 | 이번 Branch에서 구현·검증 완료. INTEGRATION의 후속 통합·판정 대기 |
| Provider별 `cli_rule`·실제 CLI | 각 Provider 작업 범위. 이번에는 시험 규칙과 로컬 Fixture만 사용 |
| 실제 Run 실행 | 제품 진입점은 여전히 빈 Provider 목록·`DenyRuns`. Provider와 신뢰된 승인 서비스 연결 필요 |
| Linux·Remote SSH 검증 | 사용자 지시로 대기. 이번에는 WSL 설치·접속·Linux 실행·교차 검사를 수행하지 않음 |
| 영속화·Snapshot·Workspace·Scheduler·RN·UI | 이번 연결 범위 밖. 기존 구현/미구현 상태를 확대해 판정하지 않음 |

공식 Workspace 시험 통과는 실제 Provider G1이나 Phase 0 수용 통과를 뜻하지 않습니다. 통합 담당자는 이 제출물을 master에 반영한 뒤 다시 검증하고 배정표의 판정을 갱신해야 합니다.

## 5. 이전 증거와 Artifact

1차 구현의 Windows 로컬 30개 시험과 독립 Manifest·Linux 대상 정적 검사 결과는 `29295c9`의 이 문서에 있습니다. 당시에는 REQ-010·011이 대기였으며 현재 상태의 판정에 그대로 쓰지 않습니다.

`REQ-011-INTEGRATION.patch`, `daemon_entrypoint.rs`, `Validate-Host.ps1`, `VALIDATION-Cargo.lock`은 당시 제출·검증 Artifact로 보존합니다. REQ-011은 이미 master에 반영되었으므로 패치를 다시 적용하지 않습니다. 현재 검증의 기준은 §3의 공식 Workspace 명령과 루트 Cargo.lock입니다.

## 6. 변경 요청과 인계

### REQ-010 CLI 탐색 계약과 모듈 등록

계약은 `3a37919`로 확정되었습니다. 이번 제출이 공개 엔진·DTO 매핑·실패 분류·기록 필드·정식 API 시험을 연결합니다. lib.rs 변경은 CONTRACT의 §6.3에서 허용한 모듈 등록·구현 타입 재노출 두 줄뿐입니다. 새 계약 변경 요청은 필요하지 않았습니다. 후속 통합은 [인계 문서](HANDOFF.md)를 따릅니다.

### REQ-011 실행 의존성과 Daemon 진입점 연결

`1d8734f`에서 완료되었습니다. 통합 담당자가 libc를 0.2.190으로 정한 기준을 유지했고 이번에는 Cargo·진입점을 변경하지 않았습니다. 기존 최소 진입점의 실행 승인·활성 Run 종료 연결은 MASTER·Provider·INTEGRATION의 후속 작업입니다.

## 7. G1 HOST 확인·REQ-013 ③ 후속 결과

- 코드 `4791c5d`: undefined 메타데이터를 universal과 동일하게 처리하고, 취소 수락이 실제 완료 결과·종료 불명을 덮어쓰지 않도록 실행 감독을 수정했습니다.
- 기존 Windows G1 실행 관찰을 코드와 대조하고 HOST 로컬 시험으로 수명·상태 처리를 확인했습니다. 실제 Claude G1 5종을 이번에 다시 실행한 것은 아닙니다.
- 현재 Branch 검증: 탐색 19개·실행 감독 6개·Windows 런타임 11개, Workspace 123개 통과(실환경 24개 기본 제외), 엄격 Clippy·fmt 통과.
- 실제 Antigravity 1.9.0/CLI 1.4.7의 --version 탐지에서 resolution=metadata와 플랫폼 필드 생략을 확인했습니다. 모델 호출·세션 생성은 없습니다.
- INTEGRATION 소유 G1 시험의 약한 상태·취소 검사를 강화할 REQ-015를 제출합니다. G1 게이트와 REQ-013 완료 판정은 통합 담당이 반영합니다.
- 상세 근거·재현·한계·변경 요청은 [G1_HOST_REVIEW.md](G1_HOST_REVIEW.md)를 따릅니다. Linux·Remote SSH는 계속 대기합니다.

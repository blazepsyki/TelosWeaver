# PH0-HOST 탐색 계약 연결 인계

> 2026-10-24 · Codex / PH0-HOST-20261024 · `ph0/host` · 코드 `e844791` · 기준 `2ac1497` · 계약 `271e7fd`

사용자 요청 1~3을 수행했습니다. 최신 main을 반영하고 REQ-010 탐색 엔진을 공개 계약에 연결했으며 내부 소스 포함 시험을 공개 API 시험으로 바꿨습니다. 사용자 요청 4의 Linux·Remote SSH 검증은 대기입니다. PH0-HOST 전체 verified와 G1~G3 판정은 하지 않습니다.

## 1. 이번 제출과 통합

- 1차 HOST 구현과 REQ-011은 이미 `d8e8725`로 통합되어 있습니다. 기존 REQ-011 패치를 재적용하지 마십시오.
- 이번 코드 변경은 `git diff 2ac1497..e844791`로 확인합니다. 공개 `NativeCliDiscoverer`가 `CliDiscoverer`를 구현하고 공통 DTO를 반환합니다.
- lib.rs는 CONTRACT 인계 §6.3의 예외에 따라 모듈 등록·구현 타입 재노출 두 줄만 추가했습니다. Schema·Trait·Cargo·Provider별 모듈은 변경하지 않았습니다.
- INTEGRATION은 후속 Branch를 통합한 뒤 §3의 공식 명령으로 재검증하고 REQ-010의 엔진 연결 상태를 갱신해 주십시오. 대표 배정표의 자기 행·변경 기록은 HOST가 main의 별도 Commit으로 갱신합니다.

## 2. Provider 담당 사용 경계

`NativeCliDiscoverer::new(Arc<dyn ProcessRuntime>)`에 공용 런타임을 주입하고 `CliDiscoverer::discover(provider_id, &rule, &request)`를 호출합니다. Provider별 `ProviderCliRule`과 Host별 `DiscoveryRequest`는 각 호출자가 공급합니다. 반환된 `CliDiscovery`는 별도 변환 없이 같은 공통 타입으로 기록·표시할 수 있습니다.

- 요청의 Host·확인 시각을 보존합니다. 엔진이 인증 상태나 지원 Capability를 추정하지 않습니다.
- `found`는 실행 경로·버전·해시 확인을 뜻합니다. 같은 Run에서는 확인한 실행 경로를 고정하십시오.
- `unavailable`의 실패 분류·출처·사유를 사용하십시오. 명시 경로나 버전 확인 실패를 임의 PATH 대체로 우회하지 않습니다.
- 확장 출처에는 위치 결정 방법·폴더 결정 방법·실제 플랫폼 기록을 포함합니다. 관리 경로는 해당 확장이 설치되었을 때만 선택합니다.
- 실제 Provider 규칙·로그인·프로토콜·Run 실행 시험은 각 Provider 담당이 수행해야 합니다. 시험용 `Probe`와 `sample.agent` 규칙은 제품 구현이 아닙니다.

## 3. 검증 결과와 재현

```powershell
cargo test --workspace --locked --offline
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo fmt --all -- --check
```

Windows에서 71개 시험과 정적 검사가 통과했습니다. 그중 탐색 공개 API 시험은 17개이며 실제 로컬 Fixture 실행·5초 타임아웃·해시 변경·관리 경로·실패 분류를 포함합니다. 상세 반례·환경은 [검증 기록](README.md)에 있습니다. Cache가 없는 환경에서는 `--offline`만 생략합니다.

## 4. 이번 이후 남는 경계

1. Linux·Remote SSH는 사용자 지시로 대기합니다. 본 제출에서 환경 구성이나 검증을 시작하지 않았습니다.
2. G1은 대표 배정표에서 정한 Provider Adapter와 신뢰된 `RunAuthorizer` 연결이 필요합니다. 제품 진입점의 `DenyRuns`를 시험용 승인으로 대체하지 마십시오.
3. Run을 허용하는 최종 Daemon은 활성 Run 취소와 실제 종료 확인까지 조합해야 합니다. API 연결 종료를 Run 취소로 취급하지 않습니다.
4. 현재 실행 감독은 메모리 보존입니다. 영속화·재시작 복구·Writer Lock·전체 Workspace 서비스를 구현했다고 판단하지 않습니다.

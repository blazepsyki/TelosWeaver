# PH0-HOST 통합 인계

> 2026-10-24 · Codex / PH0-HOST-20261024 · `ph0/host` · 구현 `159617e` · 최종 코드 `53ad1de`

HOST 소유 경로의 구현과 독립 검증 결과를 제출합니다. 공통 계약 기준은 `ba97e7b`입니다. 공식 Workspace 통합·G1 판정·Linux 실행 검증이 남아 있어 상태는 in-progress로 유지합니다. 대표 배정표의 결과 Commit·인계 문구 갱신과 변경 요청 판정은 기존 INTEGRATION 담당자가 처리해 주십시오.

## 1. 통합 순서

1. 구현 Commit과 이 문서를 검토합니다. 공유 의존성이 없어 코드만 main에 합치면 공식 Workspace 빌드가 되지 않습니다. REQ-011 적용과 한 통합 단위로 처리하십시오.
2. REQ-011의 [패치](REQ-011-INTEGRATION.patch)를 검토하고 공통 Cargo.toml·Core main.rs에 적용합니다. `git apply --check`는 통과했습니다. Cargo.lock은 INTEGRATION이 갱신하십시오. 제안 진입점은 기본 `DenyRuns`로 인증·Health만 확인하며 임의 실행을 허용하지 않습니다.
3. REQ-010의 공통 탐색 DTO와 공개 Port를 CONTRACT가 확정합니다. HOST가 내부 엔진의 매핑·공개 경계를 조정하고 시험의 소스 경로 포함을 정식 API 호출로 바꿉니다.
4. MASTER·Snapshot·Workspace 서비스에서 `RunAuthorizer`가 승인된 값만 반환하도록 연결합니다. Provider에는 `NativeRuntime`을 주입합니다. 시험의 `AllowFixture`를 제품에 연결하지 마십시오.
5. 실제 Codex Adapter로 시작·실패·이벤트·취소·실제 종료·출력/Artifact를 확인해 G1을 판정합니다. 현재 Mock·로컬 프로세스 결과는 실제 CLI PoC의 증거가 아닙니다.
6. 기존 WSL 절차로 Linux·Socket 권한·자식 회수·Remote SSH를 검증합니다. Linux 대상 Clippy는 통과했지만 실제 실행 증거는 아직 없습니다.

## 2. 사용 경계

- `telos_adapters::runtime::NativeRuntime`은 G0 Trait을 구현합니다. 탐색한 Native 실행 파일을 전달하고 `LineTooLong`·`ReadError`는 Provider가 명시적 실패로 연결해야 합니다.
- `ExecutionSupervisor::new(project_id, host_id, adapters, policy)`에 신뢰된 `RunAuthorizer`를 주입합니다. `RunPermit`은 승인된 Workspace·Context Snapshot·Policy Revision·Controller Epoch의 참조입니다. 클라이언트의 임의 참조를 승인하면 안 됩니다.
- API 서버 종료는 연결·구독을 닫습니다. Run을 허용하는 최종 Daemon은 활성 Run 취소와 실제 종료 확인도 조합해야 합니다. 현재 제안 진입점은 실행을 허용하지 않습니다.
- 이벤트·멱등 결과는 메모리 보존입니다. 전체 이벤트 32 MiB 등 구체 상한은 [검증 기록](README.md)에 있습니다. 재시작 복구나 Writer Lock을 구현했다고 취급하지 마십시오.

## 3. 공식 확인 명령

공유 변경 반영 후 저장소 루트에서 실행합니다. 이 제출 시점에는 공식 Workspace에서 수행한 결과가 아닙니다.

```powershell
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
cargo run -p telos-core --bin telos-weaver -- --version
```

독립 검증은 [Validate-Host.ps1](Validate-Host.ps1)로 재현합니다. 공통 Manifest를 변경하지 않고 보관한 검증 Lock으로 실행합니다. Linux 실제 시험은 별도로 수행해 OS·Host·CLI 버전·결과를 기록해야 합니다.

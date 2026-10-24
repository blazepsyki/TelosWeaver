# TelosWeaver 에이전트 작업 지침

## 기준 문서

- [docs/DEVELOPMENT_v1.4.md](docs/DEVELOPMENT_v1.4.md)가 유일한 구현 기준입니다. `docs/archive/`는 로컬에만 보관하는 이력 참고용이며 원격 저장소에 올리지 않습니다(`.gitignore`).
- Codex·Claude Code·Antigravity 등 이 저장소에서 작업하는 모든 에이전트가 이 문서를 따릅니다. `CLAUDE.md`는 이 문서를 불러오기만 합니다.
- Phase 0 작업은 착수 전에 Spec §19.5와 [작업 배정표](docs/phase-0/WORK_ALLOCATION.md)를 읽고, 자기 작업 행을 기록한 뒤 독점 경로만 수정합니다.
- 코드와 Spec이 어긋나면 Spec을 따르고, Spec 변경이 필요하면 코드보다 먼저 사용자와 합의합니다.
- 코드 주석에 해당 Spec 절(§)과 요구사항 ID(TASK-01, SAFE-13 등)를 남겨 추적성을 유지합니다.

## 구조 규칙 (Spec §3.4)

- Crate 의존 방향은 `telos-schema ← telos-adapters, telos-rn-client ← telos-core`입니다. 역방향·순환 의존을 만들지 않습니다.
- Spec에 정의된 계약 타입은 `telos-schema`에 둡니다. JSON 필드는 camelCase, 열거 값은 Spec 문자열 원문을 씁니다. 선택 필드는 `#[serde(default, skip_serializing_if = "Option::is_none")]`를 붙입니다.
- `telos-schema`를 바꾸면 `UPDATE_SCHEMAS=1 cargo test -p telos-schema --test json_schemas`로 `schemas/`를 갱신해 함께 커밋합니다.
- Extension(`extension/`)은 Rust 코드를 포함하거나 호출하지 않습니다. Core와의 타입은 `extension/src/generated/`의 생성물만 쓰고 손으로 복제하지 않습니다.
- 새 Core 모듈은 [crates/core/README.md](crates/core/README.md)의 모듈 배치표를 따릅니다.
- RN Fork는 Core와 별도 프로세스·DB입니다. `rn-fork/`를 Core Workspace에 합치지 않습니다.

## 구현 원칙 (Spec §2.1 요약)

- 미구현·미검증 기능을 지원하는 것처럼 표시하지 않습니다. 미지원 제어는 `AdapterError::UnsupportedCapability`로 거부합니다.
- 실행 성공·검증 통과·Task 완료·메시지 전달을 서로 다른 상태로 다룹니다.
- 실행하지 않은 시험을 통과로 기록하지 않습니다. Mock 통과와 실환경 통과를 구분합니다.

## 명령

- `cargo test --workspace`, `cargo clippy --workspace --all-targets`, `cargo fmt --all`
- `cd extension && npm run build`

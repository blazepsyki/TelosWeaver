# TelosWeaver

서로 다른 AI 에이전트(Codex, Claude Code, Agy)를 사용하여 **목표 확인 → 계획 → 위임 → 실행 → 산출물 수집 → 검증 → 승인 판단 → 재작업 또는 완료**를 관리하는 로컬 우선 오케스트레이터입니다.

- 기준 설계: [docs/DEVELOPMENT_v1.3.md](docs/DEVELOPMENT_v1.3.md)
- 현재 상태: **구현 착수 전 뼈대.** 공통 계약 타입과 구성 단위만 있으며 Provider 실행·Core 런타임은 구현되지 않았습니다.

## 저장소 구조 (Spec §3.4)

```
crates/
  schema/      telos-schema     공통 계약 타입, JSON Schema 생성 원본 (§4~§13)
  adapters/    telos-adapters   AgentAdapter Trait과 Codex·Claude·Agy 구현 (§7)
  rn-client/   telos-rn-client  RN Fork Local RPC 클라이언트 (§12)
  core/        telos-core       Core Daemon, 실행 파일 telos-weaver (§3~§15)
schemas/       telos-schema에서 생성한 JSON Schema (커밋 대상)
extension/     VS Code Extension, TypeScript (§17)
rn-fork/       Reason Network Fork 자리 — 아직 비어 있음
docs/          설계 문서 (archive/에 이전 버전)
```

의존 방향은 `schema ← adapters, rn-client ← core`이며 Cargo 의존 선언으로 강제합니다. Extension은 Rust 코드에 의존하지 않고 `schemas/`에서 생성한 TypeScript 타입만 사용합니다.

```
telos-schema (Rust 타입) ──생성──▶ schemas/*.schema.json ──생성──▶ extension/src/generated/*.ts
```

## 개발

### Rust

```sh
cargo test --workspace
cargo clippy --workspace --all-targets
cargo run -p telos-core -- --version
```

`telos-schema`의 타입을 바꾸면 JSON Schema를 다시 생성합니다. 생성하지 않으면 `json_schemas` 테스트가 실패합니다.

```sh
UPDATE_SCHEMAS=1 cargo test -p telos-schema --test json_schemas
```

### Extension

```sh
cd extension
npm install
npm run build   # schemas/ → src/generated/ 타입 생성 후 tsc
```

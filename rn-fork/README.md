# rn-fork

[Reason Network](https://github.com/reason-machines/reason-network) Fork(Rust Daemon)가 들어갈 자리입니다 (Spec §12). 아직 비어 있습니다.

Core와 같은 Rust지만 별도 프로세스·DB로 유지합니다 (DEC-05). Core Workspace(`crates/*`)에 포함하지 않습니다.

Phase 0에서 정할 항목:

- 저장 방식: Git submodule, subtree, 또는 별도 저장소
- 기준 Upstream Tag/SHA와 Patch Inventory 기록 위치 (§12.3, RN-08)
- Fork RPC 버전(`rpc_schema_version`)과 `crates/rn-client`의 Contract Test 연결 (RN-02)
- RPC 메시지 타입을 공유 Crate로 둘지 여부 (§3.4)
- Windows/Linux Build·설치·제거 재현 절차 (RN-01)

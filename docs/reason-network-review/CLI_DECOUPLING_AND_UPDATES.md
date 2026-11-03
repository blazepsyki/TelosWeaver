# 비공개 CLI 제거 가능성과 업데이트 구조

작성일: 2026-10-22 (Asia/Seoul)

## 검토 기준과 결론

기준 소스는 `reason-machines/reason-network`의 commit `f447dd3b2aee58887e16f628bb40b6e0d8012271`, package version `0.12.6`입니다. 이번 검토는 비공개 `reason` CLI 연동을 소스에서 제거할 수 있는지와 엔진의 업데이트 방식을 확인하는 데 한정했습니다. 일반 취약점 평가는 수행하지 않았습니다.

**소스 구조상 비공개 CLI 연동을 제거하고 공개 엔진만 유지할 수 있다고 판단합니다.** P2P 통신, 팀 생성·초대·가입, 로컬 IPC, MCP, 메시지 저장·전달은 공개 소스에 구현되어 있습니다. 비공개 CLI 호출은 별도 모듈과 adapter 분기에 모여 있습니다. 다만 아래는 제거 범위를 추적한 결과이며, 제거 patch를 적용하거나 제거 후 빌드·실행을 검증한 결과는 아닙니다. 원본 엔진 소스는 변경하지 않았습니다.

공개 엔진의 실행 파일인 `reason-network.exe`와 비공개 제품의 실행 파일인 `reason`을 구분해야 합니다. 공개 엔진에도 자체 CLI 진입점이 있지만, 이것까지 비공개인 것은 아닙니다. 현재 Cargo package는 실행 파일을 정의하고 library target을 정의하지 않습니다. 비공개 CLI 연동 제거와 Rust library로 재구성하는 작업은 별개입니다.

## 제거가 가능한 근거

- `Cargo.toml`에 비공개 Reason SDK나 `reason` binary를 필수로 요구하는 의존성은 없습니다. 해당 프로그램을 실행할 때는 `agent::find_bin("reason")`으로 외부 실행 파일을 찾습니다.
- [src/daemon/reason.rs](../reason-network/src/daemon/reason.rs)는 `reason session list`, `reason session send`에 연결되는 cloud 세션 동기화와 `reason network __relay`의 유지 실행을 담당합니다. 엔진 P2P의 `src/net.rs`와 구분됩니다.
- [src/daemon/mod.rs:67](../reason-network/src/daemon/mod.rs)는 `cfg.reason`일 때 세션 동기화를, `managed()`일 때 cloud relay loop를 시작합니다. 이 두 호출을 제거해도 공개 네트워크 시작 경로는 별도로 남습니다.
- [src/daemon/teamops.rs:211](../reason-network/src/daemon/teamops.rs)의 팀 생성과 가입에서 Reason 로그인 proof는 선택값입니다. proof 없이 로컬 machine key와 초대를 이용하는 경로가 있습니다.
- [src/main.rs:47](../reason-network/src/main.rs)의 `managed()`는 외부 라이선스 확인이 아니라 실행 파일이 `~/.reason/bin` 아래에 있는지 판정합니다. 이 판정에 따른 분기들을 독립 엔진 동작으로 정리할 수 있습니다.

## 소스에서 제거·정리할 범위

| 구분 | 대상 | 제거 또는 수정할 내용 |
|---|---|---|
| cloud 세션·relay 실행 | `src/daemon/reason.rs`, `src/daemon/mod.rs` | `mod reason`, `reason_loop`, `relay_loop`, loop 시작 분기와 전용 `plain_until` 상태를 제거합니다. 모듈 내부 테스트도 함께 정리합니다. |
| cloud 메시지 전달 | `src/daemon/mail.rs` | `Channel::Command("reason", ...)` 분기와 Reason 전용 재시도 처리를 제거합니다. `codex queue`, Claude socket, hook, inbox 전달은 유지합니다. |
| 전용 재시도 상태 | `src/daemon/mod.rs`, `src/daemon/sessions.rs` | `Live.retry`와 초기화를 정리합니다. 네트워크 응답 상태의 문자열 `retry`는 별개이므로 유지합니다. |
| 자동 감지·설정 | `src/setup.rs` | `reason()` adapter, `AGENTS`의 Reason 항목, legacy 감지의 Reason 전용 항목, 서비스에 `REASON_NETWORK_REASON_BIN`을 전달하는 코드를 제거합니다. |
| 설정·agent 분류 | `src/config.rs`, `src/agent.rs`, `src/daemon/sessions.rs` | `Config.reason`, Reason agent 이름 감지·표시·push tier 분기를 제거하거나 지원하지 않는 agent로 명시합니다. 기존 config의 해당 키 처리도 확인합니다. |
| 관리 설치 판정 | `src/main.rs`, `src/mcp.rs`, `src/migrate.rs`, `src/daemon/update.rs` | `managed()`에 따른 CLI 표기·가입 안내·upgrade 거부·migration 및 업데이트 예외를 독립 엔진 정책으로 정리합니다. `managed()`를 무조건 false로 바꾸는 것만으로는 호출 소스가 없어지지 않습니다. |
| 출력·오류 안내 | `src/render.rs`, `src/daemon/mail.rs`, 관련 help 문구 | Reason cloud 전용 회신 안내와 `reason update` 안내를 제거합니다. 공개 엔진의 자체 명령 안내는 유지할 수 있습니다. |
| 설치·사용 안내 | `install.sh`, `src/skill.md`, `README.md`, `DESIGN.md` | 비공개 Reason installer 실행 선택지와 계정·CLI 설치 안내를 독립 엔진 안내로 바꿉니다. `install.ps1`은 현재 공개 엔진을 직접 설치하는 경로지만 배포 정책에 따라 따로 채택 여부를 정합니다. |

이 표는 호출 및 참조를 기준으로 정리한 수정 범위입니다. 단순 문자열 일괄 삭제로 처리하면 안 됩니다. 예를 들어 `src/hook.rs`의 JSON `reason` 필드는 hook 결과 형식이며, `<reason-message>`는 메시지 표시 형식입니다. 이들은 비공개 CLI 호출이 아닙니다.

제거 후 유지되는 기능은 로컬 daemon, IPC, MCP 도구, 팀·초대, iroh 기반 P2P, 메시지·첨부파일, Codex·Claude 등 다른 agent adapter입니다. 없어지는 기능은 Reason cloud 세션 탐색·메시지 전달, Reason CLI를 통한 cloud bridge와 관리 설치 연동입니다. 이 구분은 구조 분석에 따른 예상이며 제거 후 동작 검증은 필요합니다.

## CLI 제거와 별도로 결정할 외부 의존성

비공개 CLI를 제거해도 Reason relay, relay 목록 URL, n0 주소 탐색과 선택적인 Reason 신원 proof 검증이 자동으로 제거되지는 않습니다.

특히 `src/proof.rs`는 계정 증명과 relay 목록의 서명 검증에 함께 사용됩니다. 비공개 CLI를 실행하는 모듈이 아닙니다. CLI 제거만을 위해 통째로 삭제할 필요는 없습니다. 자체 relay·자체 신원 체계까지 원한다면 별도 범위로 설계해야 합니다.

`reason network __relay`는 비공개 CLI가 제공하는 cloud bridge이고, `src/net.rs`의 iroh relay는 공개 엔진의 P2P 통신 기반입니다. 이름에 relay가 있다고 함께 제거하면 P2P 통신 구성까지 바뀝니다.

## 현재 자동 업데이트 흐름

근거: [src/daemon/update.rs](../reason-network/src/daemon/update.rs), [src/main.rs:88](../reason-network/src/main.rs), [src/daemon/mod.rs:349](../reason-network/src/daemon/mod.rs).

1. daemon 시작 시 `auto_update = true`, `from_source() = false`, `managed() = false`, `REASON_NETWORK_NO_UPDATE` 및 legacy 대응 환경변수 부재를 모두 만족하면 업데이트 loop를 시작합니다.
2. 최초 90초를 기다리고 GitHub의 `reason-machines/reason-network/releases/latest`에 `curl -fsSLI` 요청을 보냅니다. redirect 최종 URL의 `/tag/v...`에서 version을 얻습니다.
3. 이 값을 바이너리의 `CARGO_PKG_VERSION`과 숫자열로 비교합니다. 최신 version이 더 크지 않으면 설치하지 않습니다.
4. 새 version이 있으면 현재 실행 파일의 디렉터리를 설치 대상으로 지정하고, GitHub latest release의 `install.ps1` 또는 `install.sh`를 받아 실행합니다. 소스를 받아 Cargo로 다시 빌드하는 업데이트가 아닙니다.
5. installer에는 `REASON_NETWORK_BIN_DIR`과 `REASON_NETWORK_NO_PROMPT=1`을 전달합니다. 따라서 현재 Unix script의 최초 대화형 Reason CLI 설치 선택지는 이 호출에서는 건너뜁니다. 독립 엔진 자동 업데이트가 매번 비공개 CLI를 설치하는 구조는 아닙니다.
6. installer가 배포 archive를 다운로드·검사·설치하면, 업데이트 코드는 설치된 실행 파일의 `--version`을 다시 확인합니다. 실제 version이 기존보다 큰 경우에만 daemon 재시작 절차로 넘어갑니다.
7. daemon은 네트워크를 닫고 종료합니다. OS 서비스, Windows의 후속 daemon 시작, 재접속하는 MCP client 또는 다음 명령이 새 daemon을 시작합니다. 모든 기존 agent/MCP 프로세스를 새 실행 파일로 교체하는 절차는 아니므로, 도구 프로세스를 갱신하려면 세션 재시작이 필요할 수 있습니다.
8. 업데이트가 없거나 실패한 경우 이후 6시간 간격으로 다시 확인합니다. installer 실행에는 300초 timeout이 있습니다.

`from_source()`는 실행 파일 경로 요소에 `target`이 있는지 확인합니다. 소스 빌드 이력을 확인하지 않습니다. 따라서 자체 빌드 파일도 다른 디렉터리에 복사하면 자동 업데이트 대상이 될 수 있습니다. 근거: [src/util.rs:267](../reason-network/src/util.rs).

## installer가 실제로 하는 일

근거: [install.ps1](../reason-network/install.ps1), [install.sh](../reason-network/install.sh).

| 단계 | Windows | macOS·Linux |
|---|---|---|
| 배포 파일 선택 | CPU에 맞는 `reason-network-<target>.zip` | OS·CPU에 맞는 `reason-network-<target>.tar.gz` |
| 배포 경로 | 기본값은 GitHub latest release | 기본값은 GitHub latest release |
| 확인 | 같은 경로의 `.sha256`과 다운로드 파일 hash 비교 | 같은 경로의 `.sha256`과 다운로드 파일 hash 비교 |
| 실행 확인 | 압축을 풀고 새 파일의 `--version` 실행 | 압축을 풀고 새 파일의 `--version` 실행 |
| 파일 교체 | 기존 실행 파일을 임의 suffix의 `.old`로 옮기고 새 파일 배치. 새 파일 배치 실패 시 기존 파일명 복구 시도 | `.reason-network.new`로 설치한 뒤 최종 파일명으로 rename |
| 추가 동작 | 필요하면 사용자 PATH에 설치 디렉터리 추가 | PATH에 없으면 안내 출력. macOS 기존 서명본의 TeamIdentifier와 새 파일의 일치 확인 분기 존재 |

archive와 `.sha256`을 함께 다운로드하는 방식입니다. 이번 검토에서 실제 배포 파일과 서명을 실행 검증하지는 않았습니다. Windows의 파일명 복구 분기는 전체 업데이트 이후의 rollback 시스템과 구분해야 합니다.

두 installer에는 `REASON_NETWORK_VERSION`, `REASON_NETWORK_REPO`, `REASON_NETWORK_DOWNLOAD_BASE`로 artifact 선택을 바꾸는 경로가 있습니다. 그러나 엔진의 최신 version 조회와 installer 자체를 받는 URL은 `src/main.rs`의 `REPO` 상수로 구성합니다. 환경변수 하나만 바꾸면 업데이트 전체가 자체 배포처로 전환된다고 가정하면 안 됩니다.

## 수동 upgrade와 Reason CLI 관리 설치

공개 엔진의 `reason-network upgrade`는 같은 `installer()`를 직접 실행합니다. 자동 업데이트의 `auto_update`, `NO_UPDATE`, `from_source()` 판정을 거치지 않습니다. 따라서 자동 업데이트를 중지해도 이 명령으로 공식 배포본이 설치될 수 있습니다. installer 성공 뒤 daemon에 종료를 요청하고, agent 세션을 다시 시작하라는 안내를 표시합니다. 근거: [src/main.rs:670](../reason-network/src/main.rs).

`managed()`가 참인 설치에서는 엔진의 자동 업데이트를 시작하지 않고, 엔진 `upgrade`도 거부하며 `reason` CLI로 업데이트하라고 안내합니다. 그 이후 비공개 CLI가 version을 선택하고 파일을 교체하는 내부 과정은 이 저장소에 없습니다. 공개 소스로 확인한 것은 위임 경계까지입니다. workflow 주석에는 CLI가 특정 macOS/Linux archive의 SHA-256을 고정한다고 적혀 있지만, 비공개 CLI의 실제 구현을 검증한 결과로 취급하지 않았습니다.

별도로 `relays.json` 갱신도 있습니다. 이는 relay 주소 목록을 갱신하는 처리이며 실행 파일 업데이트가 아닙니다. `REASON_NETWORK_NO_UPDATE=1`만으로 이 목록 요청까지 중지하지는 않습니다. Cargo.lock이나 Rust 의존성을 실행 중 자동 갱신하는 경로도 이번 업데이트 구현에는 없습니다.

## 비공개 CLI를 제거한 fork의 업데이트 방침

**공식 업데이트를 그대로 사용하면 fork의 제거 사항이 유지되지 않습니다.** 공식 installer는 현재 디렉터리의 실행 파일을 upstream 배포본으로 교체합니다. patch를 유지한 채 merge하거나 재빌드하는 절차가 없습니다.

현재 목적에는 다음 방식이 적합하다고 판단합니다.

1. 비공개 CLI 연동과 함께 엔진 내부 자동 업데이트·수동 upgrade·공식 installer 실행 경로를 배포본에서 제거하거나 명시적으로 지원하지 않도록 처리합니다. 설정의 기본값만 false로 바꾸는 것보다 적용 범위가 명확합니다.
2. upstream 변경을 별도로 가져와 기준 commit과 diff를 검토합니다. CLI 연동이 다시 들어왔는지, 새로운 외부 실행·전송 경로가 생겼는지 확인합니다.
3. fork의 제거 사항을 유지한 소스와 Cargo.lock으로 직접 빌드·검증합니다. version, upstream commit, fork commit과 바이너리 hash를 함께 기록합니다.
4. harness가 엔진을 종료·교체·시작하도록 관리합니다. 실패 시 이전 바이너리와 해당 상태 파일로 되돌릴 방침은 실제 연동 구현에서 정합니다.

자동 배포가 필요하면 자체 release와 installer를 만들고 최신 version 조회·script 취득·artifact 취득을 모두 그 배포 체계로 전환할 수 있습니다. 다만 현재 목적에는 우선 수동 소스 갱신·검토·재빌드가 더 단순합니다.

## 구현 시 검증할 내용

아직 코드 제거는 하지 않았습니다. 구현할 때는 제거한 전용 상태·참조가 남지 않았는지 확인하고, 비공개 `reason` 실행 파일이 없는 환경에서 빌드 및 로컬 mock 기반 검증을 수행해야 합니다. 일반적인 P2P/MCP 경로가 유지되는지와 어떤 배치 경로에서도 비공개 CLI·공식 installer가 시작되지 않는지를 확인하는 것이 목적입니다. 이는 취약점 탐색과 별개인 기능 제거 검증입니다.

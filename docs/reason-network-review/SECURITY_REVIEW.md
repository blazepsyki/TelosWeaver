# Reason Network 엔진의 악성 동작·백도어 검토

작성일: 2026-10-22 (Asia/Seoul)

## 목적과 결론

목적은 공개 엔진을 직접 빌드해 `rust-multi-CLI-harness`의 구성요소로 사용할 때, 숨겨진 백도어·정보 탈취·관계없는 외부 전송·임의 명령 실행·은밀한 지속 실행이 포함되어 있는지 판단하는 것입니다. 비공개 Reason CLI의 사용은 전제하지 않습니다. 일반적인 보안 취약점, 공격 가능성, CVE 및 권한 격리의 견고함은 이번 분석 대상에서 제외했습니다.

**검토한 공개 소스와 호출 경로에서는 의도적인 백도어 또는 설명된 기능과 무관한 정보 탈취 동작을 발견하지 못했습니다. 공개 엔진을 고정 commit에서 직접 빌드하고 업데이트·자동 설정 경로를 통제하는 방식으로 채용을 검토할 수 있다는 것이 현재 판단입니다.**

이 판단은 악성 코드가 없다는 보증이 아닙니다. 전체 445개 외부 package의 소스, 배포 바이너리, 비공개 Reason CLI와 외부 서버 구현은 검토하지 않았습니다. 프로젝트를 실행하거나 패킷을 관찰하지도 않았습니다. 아래에서 코드로 확인한 사실과 그 사실에 대한 판단을 구분합니다.

이전 문서에 포함했던 IPC 취약점 재현, prompt injection 공격, 정책 우회, 디스크 고갈 및 RustSec 취약점 감사 계획은 사용자 요청에 맞춰 제외했습니다. 설정과 네트워크 동작은 숨겨진 기능 또는 외부 의존 여부를 판별하는 데 필요한 범위에서만 확인했습니다.

## 분석 기준과 수행 범위

| 항목 | 값 |
|---|---|
| 저장소 | https://github.com/reason-machines/reason-network |
| 소스 위치 | `D:\Downloads\Vibe_dev\reason-network` |
| 분석 자료 위치 | `D:\Downloads\Vibe_dev\reason-network-review` |
| 기준 commit | `f447dd3b2aee58887e16f628bb40b6e0d8012271` |
| package version | `0.12.6` |
| branch | `main` |
| 확보한 추적 파일 | 67개 |
| Rust/TypeScript 소스 | 38개, 17,154줄. 테스트 코드 포함 |
| Cargo.lock | 자체 package 1개와 crates.io package 445개 |

추적된 소스 전체에서 네트워크 주소·외부 프로세스·파일 접근·환경변수·내장 자산·동적 실행 관련 지점을 검색했습니다. 검색 결과를 바탕으로 진입점, daemon 시작, 팀 가입·인증, 네트워크 송수신, 세션 정보 수집, 파일 첨부, 메시지 전달, 알림, 설정, migration, 자동 업데이트의 주요 호출 경로를 읽었습니다. 모든 줄을 독립적으로 검증하거나 전체 의존성을 감사한 것은 아닙니다.

네트워크 관련 의존성 7개의 정확한 version을 static.crates.io에서 받아 Cargo.lock의 SHA-256과 대조했습니다. 그중 기본 네트워크 설정, 주소 게시, port mapping, metrics, 일부 build script에 해당하는 24개 파일을 원문 그대로 복사해 확인했습니다. 획득한 package 전체를 정밀 검토한 것은 아닙니다.

분석 대상 목록·원본 hash·의존성 archive는 [SOURCE_INVENTORY.md](SOURCE_INVENTORY.md)에 기록했습니다. 원본 엔진 소스와 harness 파일은 수정하지 않았습니다. 빌드·설치·setup·로그인·팀 가입·daemon·테스트·다른 에이전트로의 메시지 전송은 실행하지 않았습니다.

비공개 CLI 연동을 소스에서 제거할 구체적인 범위와 자동·수동 업데이트 절차는 [CLI_DECOUPLING_AND_UPDATES.md](CLI_DECOUPLING_AND_UPDATES.md)에 추가로 정리했습니다. 연동 제거와 제거 후 빌드 검증은 아직 수행하지 않았습니다.

## 비공개 Reason CLI 없이 사용할 수 있는가

**확인된 사실:** 이 저장소는 `reason-network`라는 실행 파일의 소스와 명령행 진입점까지 공개합니다. 사용하지 않으려는 별도 `reason` CLI와 구분해야 합니다. `reason-network.exe` 자체는 이 소스에서 빌드하는 공개 엔진입니다. 현재 package는 library가 아닌 바이너리를 정의합니다.

- [src/main.rs:511](../reason-network/src/main.rs)의 `Init`과 `Join`에는 `proof: Option<String>`이 전달됩니다.
- [src/daemon/teamops.rs:211](../reason-network/src/daemon/teamops.rs)의 팀 생성은 proof가 없으면 사용자가 지정한 이름과 로컬에서 생성한 machine key를 사용합니다. Reason 계정 로그인 요청을 보내는 코드가 아닙니다.
- [src/daemon/teamops.rs:299](../reason-network/src/daemon/teamops.rs)의 가입은 초대 코드 또는 ticket에서 팀·secret·peer·relay 정보를 읽어 peer와 통신합니다.
- [src/config.rs:57](../reason-network/src/config.rs)의 기본 설정에서 `reason = false`입니다. `reason session list`를 주기적으로 호출하는 통합은 이 설정이 켜져야 시작됩니다.
- [src/daemon/mod.rs:70](../reason-network/src/daemon/mod.rs)의 `reason network __relay` 시작은 `managed()`가 참일 때입니다. [src/main.rs:47](../reason-network/src/main.rs)의 `managed()`는 실행 파일의 실제 경로가 사용자 홈의 `.reason/bin` 아래인지 검사합니다. 자체 빌드 파일을 이 경로에 넣으면 이 분기에 들어갈 수 있습니다.

**판단:** 비공개 Reason CLI는 독립 엔진의 필수 실행 의존성이 아닙니다. 다만 README와 skill의 가입 안내, Unix 설치 스크립트의 선택 경로는 Reason 계정과 별도 CLI로 안내합니다. 소스 빌드·독립 팀 생성 경로를 사용한다는 전제가 필요합니다. 실제 빌드와 팀 생성은 이번 분석에서 실행하지 않았습니다.

## 외부로 무엇을 보내는가

| 경로 | 코드에서 확인한 동작 | 이번 목적에 따른 판단 |
|---|---|---|
| 팀 peer | 세션 presence, 팀 상태, 사용자가 또는 에이전트가 요청한 메시지·첨부파일을 주고받습니다. 연결 대상은 팀 구성원의 endpoint에서 나옵니다. | 프로젝트의 본래 기능입니다. 별도의 숨겨진 수집 대상은 확인하지 못했습니다. |
| Reason relay | 기본 fallback은 `https://relay-sfo1.compute.reasonmachines.com`입니다. 실제 기본 relay 집합은 서명된 목록에 따라 달라질 수 있습니다. | 통신 인프라 의존성입니다. 서버 측 구현·접속 기록은 검토하지 않았습니다. |
| relay 목록 | `https://reasonmachines.com/network/relays.json`을 시작 시 및 6시간 간격으로 가져옵니다. 고정 URL에 대한 GET이며 코드상 작업 내용·인증정보를 요청 body로 보내지 않습니다. | 콘텐츠 업로드 경로는 발견하지 못했지만 요청 상대는 접속 IP 등 일반 HTTP 메타데이터를 알 수 있습니다. |
| n0 주소 탐색 | 기본 설정은 `presets::N0`를 사용합니다. 의존성에서 `https://dns.iroh.link/pkarr` 주소 게시·조회와 DNS 조회가 추가됩니다. | peer를 찾기 위한 주소 게시입니다. 메시지 내용 전송과 구분해야 하며, 완전한 로컬 전용 실행은 아닙니다. |
| LAN | mDNS 탐색과 iroh의 portmapper가 관련됩니다. iroh 기본 feature에 portmapper가 포함되고 기본 설정은 활성화입니다. | 자동 네트워크 탐색과 라우터 port mapping 시도 경로입니다. 은밀한 C2로 볼 근거는 발견하지 못했으며 실제 동작은 미관찰입니다. |
| GitHub | 자동 또는 명시적 upgrade에서 최신 version 조회, 설치 스크립트와 바이너리를 내려받습니다. | 검토한 코드가 나중에 바뀔 수 있는 정상 업데이트 기능입니다. 고정 소스 사용 시 제외해야 할 경로입니다. |
| Reason cloud | 조건이 맞으면 외부 `reason` 프로그램을 실행해 세션 조회·전달·cloud relay를 맡깁니다. | 독립 엔진 사용 시 제외할 선택 기능입니다. 해당 CLI 내부 통신은 이번 분석에서 판단하지 않았습니다. |

근거: [src/net.rs:251](../reason-network/src/net.rs), [src/net.rs:635](../reason-network/src/net.rs), [src/net.rs:850](../reason-network/src/net.rs), [src/net.rs:926](../reason-network/src/net.rs), [src/relays.rs:184](../reason-network/src/relays.rs), [src/daemon/mod.rs:338](../reason-network/src/daemon/mod.rs), [src/daemon/reason.rs](../reason-network/src/daemon/reason.rs), [iroh N0 preset:117](dependency-excerpts/iroh-1.4.1/src/endpoint/presets.rs), [pkarr 기본 URL:127](dependency-excerpts/iroh-1.4.1/src/address_lookup/pkarr.rs), [portmapper 기본 설정:35](dependency-excerpts/iroh-1.4.1/src/portmapper.rs).

자체 relay를 지정한 팀에서는 `presets::Minimal`을 사용하고 기본 relay 목록 갱신을 건너뛰는 분기가 있습니다. 그러나 **팀에 가입하기 전 daemon도 relay 목록을 갱신**하므로, 최초 실행부터 외부 서비스에 전혀 접속하지 않는다고 단정하면 안 됩니다. `lan = false` 역시 업데이트·relay 목록·인터넷 peer 통신 전체를 끄는 설정은 아닙니다.

## 읽는 정보와 공유되는 정보

**확인된 사실:** 엔진은 사용자가 직접 전송한 메시지만 처리하는 최소 message bus보다 넓은 범위를 읽습니다.

| 읽는 대상 | 사용 목적과 전달 범위 |
|---|---|
| agent 세션 registry, PID, 작업 폴더 | 실행 중인 세션 식별과 상태 표시 |
| Git root, branch, origin URL | `git rev-parse` 및 `git config --get remote.origin.url`로 읽고 repo 식별자와 branch로 정리 |
| Claude transcript와 tasks, Codex session index, hook이 알려준 transcript | 세션 제목과 진행 중인 todo를 추출해 `doing`으로 공유 |
| Claude messaging token 또는 `.key` 파일 | 로컬 Claude messaging socket에 인증하기 위한 token. 확인한 경로에서는 로컬 auth frame에 사용 |
| 사용자가 지정한 첨부파일 | 메시지에 포함해 수신자에게 전송. 자동으로 전체 저장소를 첨부하는 경로는 발견하지 못함 |
| 자체 machine key, 팀 상태, 메시지 DB | 로컬 identity·팀 상태·송수신 큐 유지 |
| agent 설정 파일 | MCP/hook 통합 상태 확인과 setup 시 설정 변경 |

근거: [src/agent.rs:211](../reason-network/src/agent.rs), [src/claude.rs:23](../reason-network/src/claude.rs), [src/claude.rs:122](../reason-network/src/claude.rs), [src/git.rs:11](../reason-network/src/git.rs), [src/doing.rs:30](../reason-network/src/doing.rs), [src/doing.rs:108](../reason-network/src/doing.rs), [src/doing.rs:176](../reason-network/src/doing.rs), [src/doing.rs:188](../reason-network/src/doing.rs), [src/files.rs:116](../reason-network/src/files.rs).

transcript는 파싱을 위해 실제 내용을 읽습니다. Claude/Gemini/Codex 등의 제목·todo를 추출하며, `doing::of`의 최종 문자열은 120자로 제한됩니다. 전체 대화 내용을 그대로 제3자에게 업로드하는 경로는 확인하지 못했습니다. repo·branch·상태·작업 제목은 presence로 팀 peer에 자동 공유되므로 이 정보까지 비공개로 유지해야 하는 환경에는 기본 동작이 맞지 않을 수 있습니다.

`share_paths = false`는 작업 폴더를 마지막 경로 요소로 줄이는 동작입니다. repo·branch·doing 공유 전체를 끄지는 않습니다. 또한 Claude MCP가 설정된 환경에서는 기존 Claude 세션도 자동 등록하는 분기가 있으므로 `share_unmanaged = false`만으로 사용자가 명시적으로 등록한 세션만 노출된다고 해석하면 안 됩니다. 근거: [src/daemon/directory.rs:67](../reason-network/src/daemon/directory.rs), [src/daemon/directory.rs:207](../reason-network/src/daemon/directory.rs).

**판단:** 확인한 정보 수집은 README에서 설명하는 팀 세션 검색 기능과 연결됩니다. 설명과 관계없는 credential 수집·전체 환경변수 전송·전체 저장소 자동 업로드·clipboard 읽기 경로는 발견하지 못했습니다. 로컬 Claude token을 읽는 부분은 존재하지만, 확인한 사용처는 로컬 세션 인증이며 token 자체를 팀 메시지나 HTTP 요청에 실어 보내는 경로는 확인하지 못했습니다.

## 외부 명령 실행과 지속 실행

외부 실행 지점을 호출 목적별로 확인했습니다.

| 지점 | 실행하는 대상 | 해석 |
|---|---|---|
| 로컬 daemon 시작 | 자기 자신의 `daemon` 명령 | client가 없던 daemon을 별도 background 프로세스로 시작합니다. Windows에서는 창을 숨깁니다. 정상 구조에 연결되며, 단발 명령 종료 뒤에도 daemon은 남을 수 있습니다. |
| peer 메시지 전달 | `codex queue --thread ... --message ...`, 선택적 `reason session send`, 로컬 Claude socket, hook 및 공개 TypeScript plugin | 메시지를 기존 agent 세션에 넣습니다. 확인한 네트워크 명령 분기에 수신 문자열을 shell 명령으로 직접 실행하는 일반 명령은 없습니다. agent 자체가 메시지에 따라 작업을 수행하는 것은 본래 기능입니다. |
| 사용자 알림·질문 | 고정된 PowerShell/osascript UI 코드 또는 notify-send | 제목·본문은 별도 인자나 환경변수로 전달됩니다. 확인한 스크립트는 notification/dialog 구성입니다. |
| setup | agent MCP·hook 설정, plugin 설치, OS 자동 시작 등록 | Windows HKCU Run, macOS launchd, Linux user systemd를 사용합니다. Linux에는 enable-linger 호출도 있습니다. |
| migration | 기존 tincan 설치·상태·설정 전환 | daemon에서 자동 호출되지만 기존 이름의 파일이 현재 바이너리와 동일한 copy인지 등 전제 조건이 있습니다. 일반적인 새 clone에서 무조건 설정을 바꾸는 분기는 아닙니다. |
| clipboard | clip/pbcopy/wl-copy/xclip | 초대 링크·주소를 clipboard에 씁니다. clipboard 내용을 읽어 수집하는 경로는 발견하지 못했습니다. |
| 업데이트 | curl, shell 또는 PowerShell, 내려받은 바이너리 | 검토 밖의 새 코드를 실행할 수 있는 명시적 경로입니다. 다음 항목에서 따로 다룹니다. |

근거: [src/client.rs:217](../reason-network/src/client.rs), [src/daemon/mail.rs:310](../reason-network/src/daemon/mail.rs), [src/daemon/mail.rs:421](../reason-network/src/daemon/mail.rs), [src/daemon/notify.rs:139](../reason-network/src/daemon/notify.rs), [src/daemon/ask.rs:478](../reason-network/src/daemon/ask.rs), [src/setup.rs:957](../reason-network/src/setup.rs), [src/setup.rs:1097](../reason-network/src/setup.rs), [src/migrate.rs:44](../reason-network/src/migrate.rs), [src/wizard.rs:274](../reason-network/src/wizard.rs).

setup은 연결 정보만 추가하는 처리가 아닙니다. Claude 설정의 allow에는 `mcp__reason-network__ask_user`를 추가하고, Codex MCP 설정에는 `default_tools_approval_mode = "approve"`를 기록합니다. 이는 코드에서 확인한 설정값입니다. 이용하는 제품의 version별 해석과 실제 동작은 이번에 검증하지 않았습니다. 근거: [src/setup.rs:290](../reason-network/src/setup.rs), [src/setup.rs:356](../reason-network/src/setup.rs).

**판단:** background 실행·설정 변경·자동 시작은 존재하지만 기능과 연결된 명시적 구현입니다. 확인한 코드와 검색 대상에서 숨겨진 별도 상주 프로그램, 관계없는 scheduled task, SSH 키 추가, 범용 reverse shell, 마이닝 처리는 발견하지 못했습니다. harness에서 관리한다면 필요한 연결만 직접 구성하는 편이 동작 범위를 파악하기 쉽습니다.

## 업데이트와 고정 소스 사용

**가장 중요한 사용 조건은 직접 빌드한 파일도 배치 위치에 따라 자동 업데이트 대상이 될 수 있다는 점입니다.**

- `auto_update` 기본값은 `true`입니다.
- 자동 업데이트에서 제외하는 `from_source()`는 실행 파일 경로의 구성 요소에 `target`이 있는지 확인합니다. 빌드에 사용한 commit을 식별하는 방식이 아닙니다.
- `target/release`에서 다른 폴더로 복사하면 이 제외 조건이 사라집니다. 별도 설정이나 환경값으로 중지하지 않으면 업데이트 대상이 될 수 있습니다.
- 최초 확인은 90초 뒤이며 이후 6시간마다 확인합니다. 새 version이 있으면 latest 설치 스크립트를 shell/PowerShell로 실행하고 바이너리를 교체합니다.
- `REASON_NETWORK_NO_UPDATE` 환경변수가 존재하거나 설정이 `auto_update = false`이면 자동 업데이트 loop를 시작하지 않습니다.
- 명시적인 `upgrade` 명령은 이 자동 업데이트 판정과 별도로 installer를 직접 호출합니다. 자동 업데이트 중지는 수동 upgrade까지 차단하지 않습니다.
- Unix `install.sh`는 최초 설치·대화형 단말 등의 조건에서 Reason 계정 사용 여부를 질문합니다. 긍정하면 별도 Reason installer를 받아 실행합니다.

근거: [src/config.rs:65](../reason-network/src/config.rs), [src/util.rs:267](../reason-network/src/util.rs), [src/daemon/update.rs:10](../reason-network/src/daemon/update.rs), [src/main.rs:88](../reason-network/src/main.rs), [src/main.rs:670](../reason-network/src/main.rs), [install.sh:38](../reason-network/install.sh).

**판단:** 업데이트 기능을 악의의 증거로 판단하지는 않습니다. 다만 검토한 공개 엔진만 사용하려면 최신 installer와 배포 바이너리로 전환하는 경로를 중지해야 합니다. 실행 환경에 `REASON_NETWORK_NO_UPDATE=1`을 전달하고 전용 config에도 `auto_update = false`를 설정하는 조건을 권장합니다. 이번에는 실제 환경의 설정을 변경하지 않았습니다.

## 고정 키·숨겨진 payload·빌드 구성

코드와 파일 목록에서 확인한 사실은 다음과 같습니다.

- machine key는 기존 로컬 `machine.key`를 읽거나 `SecretKey::generate()`로 생성합니다. 모든 사용자에게 공통된 비밀키를 사용하는 초기화는 확인하지 못했습니다.
- `src/proof.rs`의 긴 base64url 문자열은 Reason의 RSA 공개키입니다. 선택적으로 받는 계정 증명과 relay 목록의 서명 검증에 사용됩니다. 이 키로 서명되었다는 이유만으로 외부 peer의 팀 가입을 무조건 허용하는 master key 분기는 확인하지 못했습니다.
- `REASON_NETWORK_TRUST_KEYS`로 추가 공개키를 받는 부분은 test/debug 조건에 있습니다. 테스트용 `.pk8` 비밀키의 include는 `#[cfg(test)]` module 안에 있습니다. 통상적인 release build의 숨겨진 인증키라고 판단할 근거는 발견하지 못했습니다.
- base64/hex는 주로 공개키·서명·ticket·invite에 사용합니다. 디코딩한 payload를 임의 코드로 평가하는 경로는 발견하지 못했습니다.
- 원본 저장소에는 root `build.rs`, 추적된 `.cargo` 설정, submodule 정의가 없습니다. Cargo.lock의 source는 crates.io registry뿐이며 git 의존성은 없습니다.
- 확보한 iroh 관련 자료 중 `iroh`, `iroh-dns`, `iroh-relay`의 build script는 `cfg_aliases!`로 조건을 설정합니다. 이 세 파일에는 추가 바이너리 다운로드·외부 명령 실행·credential 읽기가 없습니다.
- iroh에는 metrics 기능이 있습니다. 확인한 `iroh-metrics`에는 지정한 gateway로 push하는 선택적 service 구현도 있습니다. 그러나 엔진 본체에서 이를 시작하거나 송신 목적지를 설정하는 호출은 발견하지 못했습니다. `metrics`라는 이름만으로 제3자 telemetry 송신으로 판단하지 않았습니다.
- CI와 release workflow는 Cargo format/lint/test/build, archive 생성과 GitHub Release 등록을 정의합니다. workflow 주석에는 macOS archive를 나중에 서명된 배포본으로 교체한다는 설명도 있습니다. 공개 소스 검토 결과를 배포 바이너리에 그대로 적용하지 않았습니다.

근거: [src/daemon/mod.rs:168](../reason-network/src/daemon/mod.rs), [src/proof.rs:37](../reason-network/src/proof.rs), [src/proof.rs:103](../reason-network/src/proof.rs), [src/proof.rs:190](../reason-network/src/proof.rs), [src/team.rs:337](../reason-network/src/team.rs), [iroh build.rs](dependency-excerpts/iroh-1.4.1/build.rs), [iroh-dns build.rs](dependency-excerpts/iroh-dns-1.4.1/build.rs), [iroh-relay build.rs](dependency-excerpts/iroh-relay-1.4.1/build.rs), [metrics service](dependency-excerpts/iroh-metrics-1.0.2/src/service.rs), [.github/workflows/release.yml](../reason-network/.github/workflows/release.yml).

## 채용 판단의 조건

다음은 취약점 대응 목록이 아니라 검토한 코드·기능과 실제 사용하는 코드·기능을 일치시키기 위한 조건입니다.

1. 기준 commit과 Cargo.lock을 고정하고 통상적인 release 설정으로 직접 빌드합니다. `cargo build --release --locked`는 후속 작업의 후보 명령이며 이번에는 실행하지 않았습니다.
2. 설정과 실행 환경에서 자동 업데이트를 중지하고 명시적 upgrade도 사용하지 않습니다. 빌드 후 분석 대상 바이너리의 hash를 기록합니다.
3. 비공개 `reason` CLI를 설치하지 않고 독립 엔진을 `.reason/bin` 밖에 배치하며 `reason = false`를 유지합니다.
4. 최초 구성에는 `init`/`join`의 `--no-setup`을 사용하거나 harness adapter에서 필요한 등록만 수행합니다. `--no-setup`도 daemon 시작과 통상적인 네트워크 처리를 중지하지는 않습니다.
5. 필요한 세션만 harness에서 등록합니다. agent 전체의 setup·hook 추가·OS 자동 시작은 각각 사용 여부를 정합니다. 세션 메타데이터 공유 사양을 받아들일 수 있는지도 확인합니다.
6. 외부 서비스를 배제해야 한다면 relay 목록 취득, N0 preset, mDNS, portmapper, 업데이트·CLI 통합을 별도로 제외하고 실행 관측으로 확인해야 합니다. 현재 엔진을 수정 없이 완전 offline이라고 판단하지 않습니다.

## 미확인 범위

이번 결론은 공개 소스에 대한 정적 악성 동작 검토입니다. 확보하지 않은 대다수 의존성, proc macro, native build dependency를 포함하는 전체 소스 감사는 아닙니다. Rust 사용, checksum 일치, Git 이력, 기존 테스트의 존재만으로 무해함을 증명하지 않았습니다.

최종 채용 전에 추가한다면 취약점 탐색보다 고정 소스를 격리 환경에서 빌드하고, 시작·팀 생성·메시지 송수신·종료 과정의 실제 접속 대상, 송신 내용, 시작 프로세스, 파일·레지스트리 변경이 위 설명과 일치하는지 확인하는 작업이 이번 목적에 맞습니다. 이 실행 검증은 아직 수행하지 않았습니다.

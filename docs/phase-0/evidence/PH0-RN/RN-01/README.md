# PH0-RN-01 원본 도입·기준

| 항목 | 값 |
|---|---|
| 서브 태스크·담당 | PH0-RN-01 · `claude-opus-5.5/session-c9336d2c` |
| 작성일 | 2026-10-25 |
| Branch / Worktree | `ph0/rn-01` / `D:\worktree\TelosWeaver\ph0-rn` |
| 기준 | PH0-RN-00 처리표 §5-1, Spec v1.5 §12.3·§12.5·§19.5.10 |

## 1. 도입

| 항목 | 값 |
|---|---|
| 원본 | `https://github.com/reason-machines/reason-network` |
| 기준 태그·커밋 | `v0.12.11` = `b3557ff454cd26a04c2a18804ba83ae5db8b119d` |
| 방법 | `git subtree add --prefix=rn-fork <원본> v0.12.11 --squash` |
| 생성 커밋 | `a510619`(squash, `git-subtree-split: b3557ff…`), 병합 `5d77e02` |
| 원본 수정 | 없음. `rn-fork/TELOSWEAVER.md`(출처·정책·Patch Inventory)만 추가 |
| 고지 | `LICENSE-MIT`, `LICENSE-APACHE`, `NOTICE`를 원본 그대로 유지 |
| 기존 자리 표시 | `rn-fork/README.md`(TelosWeaver 자리 표시)를 subtree 도입 전에 제거. 이제 `rn-fork/README.md`는 원본 README |

`rn-fork/.github/workflows`도 함께 들어왔지만 저장소 루트가 아니어서 GitHub Actions로 실행되지 않습니다.

## 2. 빌드 구조

- RN Fork는 독립 Cargo Workspace입니다. 루트 `Cargo.toml`에 `exclude = ["rn-fork"]`를 추가해, `rn-fork` 안에서 빌드할 때 TelosWeaver Workspace로 잡히지 않게 했습니다(INTEGRATION 경로).
- 원본 `Cargo.lock`(패키지 446개)을 그대로 쓰며 `--locked`로 빌드합니다. RN Fork의 `Cargo.toml`·`Cargo.lock` 변경은 INTEGRATION 소유입니다(§19.5.10). PH0-RN은 같은 담당자가 맡고 있어 이후 서브 태스크에서 함께 처리합니다.
- 빌드 산출물은 `rn-fork/target/`이며 원본 `.gitignore`(`/target`)로 제외됩니다.

## 3. 수정 전 빌드·시험 (Windows 11, rustc 1.94.0)

| 명령 (`rn-fork/`에서) | 결과 |
|---|---|
| `cargo build --locked` | 성공, 1분 10초. `target/debug/reason-network.exe --version` → `reason-network 0.12.11` |
| `cargo test --locked` (격리 환경, 아래) | 단위 시험 191개 통과. `tests/install_script.rs`는 Windows에서 대상 시험 0개 |
| `cargo clippy --locked --all-targets` | 경고 0건 |

**격리:** 시험이 사용자 폴더나 에이전트 설정을 건드리지 않도록 `USERPROFILE`·`HOME`·`APPDATA`·`LOCALAPPDATA`·`REASON_NETWORK_HOME`을 임시 폴더로 돌리고, `REASON_NETWORK_NO_SERVICE=1`·`NO_UPDATE=1`을 설정했습니다(툴체인 위치는 `RUSTUP_HOME`·`CARGO_HOME`으로 지정). 시험 후 임시 홈에는 rustup 설정 파일 외에 생긴 것이 없고, 실제 `~/.reason`도 생기지 않았습니다.

**실행하지 않은 것:** 원본 `install.sh`·`install.ps1`, `setup`(에이전트 설정 변경), Daemon, Python E2E 시험(`tests/e2e/*.py`, 두 대 이상의 기계·실제 에이전트 필요). 실행 파일은 버전 확인(`--version`)만 했습니다.

## 4. 이후 서브 태스크의 파일 소유권

| 경로 | Writer |
|---|---|
| `rn-fork/**` (원본 코드·시험·문서, `TELOSWEAVER.md`) | PH0-RN (`claude-opus-5.5/session-c9336d2c`), 단일 Writer |
| `rn-fork/Cargo.toml`, `rn-fork/Cargo.lock` | INTEGRATION(같은 담당자) |
| `crates/rn-client/**` | PH0-RN |
| 루트 `Cargo.toml`·`Cargo.lock`, `crates/core/src/integration/**` | INTEGRATION |
| `telos_schema::rn` 계약 변경 | CONTRACT(같은 담당자, 변경 요청으로 기록) |

병렬 Writer는 두지 않습니다. 서브 태스크마다 `ph0/rn-0N` Branch를 만들고 병합 후 삭제합니다.

## 5. 다음 단계

PH0-RN-02(업데이트·설치 제거)부터 PH0-RN-00 처리표 §3.2·§5-3대로 진행합니다. 각 단계는 빌드·시험이 통과하는 중간 상태로 끝냅니다.

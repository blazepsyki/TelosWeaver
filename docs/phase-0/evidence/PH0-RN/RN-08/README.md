# PH0-RN-08 제거 검증 및 수용

| 항목 | 값 |
|---|---|
| 서브 태스크·담당 | PH0-RN-08 · `gemini-3.8-flash/session-f1b89d3a` (독립 검토자, Spec §19.5.10) |
| 작성일 | 2026-10-26 |
| Branch / Worktree | `ph0/rn-08` / `D:\worktree\TelosWeaver\ph0-rn-08` |
| 기준 | Spec v1.5 DEC-07·§12.3~§12.5·§19.5.10, RN-POC-13~15, 계약 기준 `7497877` |
| 사용자 결정 (2026-10-26) | ① 원본 Python E2E 시험 폐기(Rust 시험과 Core 관리자 연동 시험으로 대체) ② 실제 설정 미변경 격리 임시 폴더 방식 및 모델 쿼터 미소모 격리 Daemon 검증 ③ Windows 로컬 및 `Psyki-NAS` Linux 순차 진행 |

## 1. 정적 코드 및 의존성 감사 (RN-POC-13)

### 1.1 제거 대상 호출 및 진입점 부재 검증
`rn-fork/src` 전체 소스 트리를 대상으로 정적 검색 및 호출 추적을 전수 수행했습니다:
- **`reason` CLI 호출 (`find_bin("reason")`, `Command::new("reason")`, `"reason session"` 등)**: **0건**
- **자동 업데이트 및 설치 스크립트 (`update.rs`, `auto_update`, `install.sh`, `install.ps1`, `upgrade` 서브커맨드)**: **0건**
- **Reason 계정 로그인 증명 (`proof.rs`, `Request::Proof`, `verified_people`, `JoinRequest.proof`)**: **0건**
- **구 `~/.reason` 상태 경로 참조**: **0건** (코드 내 설명 주석 1건 외 실제 경로 접근 없음)

### 1.2 의존성 감사 (`Cargo.toml`, `Cargo.lock`)
- **`ring` 의존성 검토**: `ring` crate는 로그인 증명 제거 후, 유지 대상인 Reason 중계 서버 목록(Relay)의 서명 검증 모듈(`src/signed.rs`)에서만 고정 공개 키 검증용(`verify_signed`)으로 한정 사용됨을 확인했습니다 (`ring::` 호출 5건 모두 `src/signed.rs` 내부).
- **불필요한 외부 네트워크/설치 의존성 부재**: 자체 업데이터 및 다운로더 의존성이 완전히 제거되었습니다.

### 1.3 컴파일 바이너리 리터럴 검사
컴파일된 바이너리(`telos-rn.exe`)의 바이트 스트림에서 제거 대상 문자열을 검색했습니다:
- `reason session`, `install.sh`, `install.ps1`, `auto_update`, `REASON_NETWORK_BIN_DIR`, `Request::Proof`, `verified_people`: **모두 미검출 (NOT FOUND)**
- 서브커맨드 목록: `who`, `send`, `status`, `init`, `invite`, `link`, `join`, `team`, `remove`, `leave`, `relay`, `folder`, `held`, `approve`, `reject`, `setup`, `doctor`, `daemon`, `stop`, `listen`, `mcp`만 존재 (`upgrade`, `proof`, `endpoint` 부재 확인).

---

## 2. 원본 Python E2E 시험 폐기 및 정리 (결정 ①)
- `rn-fork/tests/e2e/*.py`의 9개 스크립트는 원본 제품명(`reason-network`)과 제거된 기능(자동 업데이트, CLI 관리 모드, 로그인 등)을 호출하여 TelosWeaver Fork와 호환되지 않음.
- 사용자 결정에 따라 9개 Python 스크립트를 공식 폐기(삭제)하고, `rn-fork/tests/e2e/README.md` 및 `rn-fork/TELOSWEAVER.md`(#30)에 폐기 사유와 대체 근거를 명시했습니다.

---

## 3. 대표 실행 및 격리 관찰 (RN-POC-14)

### 3.1 에이전트 설정 변경(`setup`) 격리 검증 (Windows 11)
사용자의 실제 설정(`~/.claude/settings.json`, `~/.codex/config.toml` 등)을 전혀 변경하지 않도록, 임시 홈 디렉터리(`$testDir\home`) 및 격리 상태 폴더(`TELOS_RN_HOME=$testDir\rn_home`)를 구성하여 검증했습니다.
- `telos-rn setup --dry-run`: 변경 예정 사항(Codex, Copilot CLI Hook) 정상 안내.
- `telos-rn setup`: 격리된 Codex 설정(`config.toml`, `hooks.json`) 및 Copilot CLI 설정 정상 업데이트.
- `telos-rn setup --remove`: 업데이트된 Hook 및 설정이 깨끗하게 복원·제거됨 확인.

### 3.2 격리 Daemon 실행 관찰 (Windows & Linux 공통)
임시 디렉터리에서 `telos-rn daemon`을 구동하고 2~3초간 실행 상태를 관찰했습니다:
- **자식 프로세스 감사**: `reason` 등 자식 프로세스 생성 **0건** (Parent PID 기준 자식 프로세스 수 0).
- **외부 통신 감사**: 팀 미가입 상태에서 외부 HTTP/HTTPS/P2P 네트워크 연결 **0건** (`netstat`/`ss` 연결 수 0).
- **보안 및 권한 경계**:
  - Windows: 현재 OS 사용자 SID 전용 DACL이 적용된 Named Pipe (`\\.\pipe\telos-rn-<hash>`, `\\.\pipe\telos-rn-rpc-<hash>`) 생성 확인.
  - Linux (`psyki-nas`): 소유자 전용 권한 `0600`의 Unix Socket (`network.sock`, `rpc-network.sock`) 및 `rpc-token` (`0600`) 생성 확인.
- 종료: 프로세스 종료(`kill`/`Stop-Process`) 시 남은 프로세스 없이 클린 종료 및 리소스 회수 확인.

---

## 4. 기능 및 계약 연동 검증 (RN-POC-15)

### 4.1 Windows 검증 결과
| 검사 | 명령 | 결과 |
|---|---|---|
| `rn-fork` 단위/통합 시험 | `cargo test` (in `rn-fork`) | **204개 통과** (0 failed) |
| `rn-fork` 정적 분석 | `cargo clippy --all-targets -- -D warnings` | 경고 0건 |
| `rn-fork` 코드 포맷 | `cargo fmt --all -- --check` | 통과 |
| `telos-rn-client` 계약 시험 | `cargo test -p telos-rn-client` | **6개 통과** |
| Core RN 오프라인 시험 | `cargo test -p telos-core --test phase0_rn` | **3개 통과** (실환경 2개 기본 제외) |
| 실제 RN 왕복·재시작·정지 | `phase0_rn` 실환경 1 (`rn_round_trip_restart_and_stop_with_core`) | **통과** (Core가 실제 RN 바이너리 구동 → `listen`/`send` 메일함 왕복 → Receipt `queued`→`delivered` 전이 → 강제 종료 감지 후 1/3 재시작 복구 → Core 종료 시 함께 정지) |

### 4.2 Linux 검증 결과 (`Psyki-NAS`, Ubuntu 26.04.1 LTS)
서버의 전용 Worktree(`~/TelosWeaver-wt/rn-08`)에서 검증을 수행했습니다:
| 검사 | 명령 | 결과 |
|---|---|---|
| Linux Build | `cargo build` (in `rn-fork`) | 성공 (17.99초) |
| `rn-fork` 단위/통합 시험 | `cargo test` (in `rn-fork`) | **212개 통과** (0 failed, Unix 전용 시험 포함) |
| `rn-fork` 정적 분석 | `cargo clippy --all-targets -- -D warnings` | 경고 0건 |
| `rn-fork` 코드 포맷 | `cargo fmt --all -- --check` | 통과 |
| 격리 Daemon 권한 감사 | `stat -c "%a" rpc-token` 및 `*.sock` | 토큰 `600`, 소켓 `600` 확인, 자식 프로세스 0건, 네트워크 연결 0건 |

검증 후 서버의 `~/TelosWeaver-wt/rn-08` 워크트리는 깨끗하게 정리되었습니다.

---

## 5. 검증 한계 및 인계

1. **Phase 0 범위 한정**: P2P 팀 간 다자간 네트워크 기능은 DEC-07 및 Spec §12.4에 따라 Phase 0에서는 로컬 전용으로 한정하여 검증했습니다.
2. **모델 쿼터 미소모**: Claude haiku 등 실제 모델을 통한 메시지 전달 및 RN 장애 독립성은 앞선 RN-05~07에서 실환경 검증이 완료되었으므로, 본 RN-08에서는 사용자 권장안 승인에 따라 모델 쿼터 소모 없이 격리 Daemon 및 Core 연동 경로 위주로 감사했습니다.
3. **인계 판정**:
   - 업스트림 Reason CLI 연동, 자동 업데이트, Reason 로그인 증명의 제거가 완벽히 입증되었으며(RN-POC-13),
   - 대표 실행 중 파일·프로세스·외부 통신 부재(RN-POC-14)와
   - TelosWeaver 계약과의 정합성 및 Windows/Linux 공통 동작(RN-POC-15)이 확인되었습니다.
   - PH0-RN-08 수용 완료 결과를 INTEGRATION(G3 수용 판정)에 인계합니다.

# DEC-08 Codex VS Code 목록 표시 구현·검증

| 항목 | 값 |
|---|---|
| 담당·날짜 | PH0-CODEX · `Codex / PH0-CODEX-LISTING-01a17034` · 2026-10-25 |
| 기준 | main `269a771`, 착수 배정 `025a8f0`, Spec v1.5.3 DEC-08·§7.6, 계약 `7497877` |
| Branch / Worktree | `ph0/codex-listing` / `D:\worktree\TelosWeaver\ph0-codex-listing` |
| 환경 | Windows x64, `openai.chatgpt` 26.1021.22027, 확장 동봉 `codex-cli 0.164.0-alpha.11.1`, Rust 1.99.1 |
| 결과 | `listed` 구현·Fresh/Resume/NativeFork 자동 판정 통과. `hidden`은 현재 공개 수단을 찾지 못해 명시적 거부. 실제 확장 목록 표시·대화 열기는 사용자 확인 대기 |

## 1. X1: 실제 App Server에서 확인한 사실

사용한 실행 파일은 `~/.vscode/extensions/openai.chatgpt-26.1021.22027-win32-x64/bin/windows-x86_64/codex.exe`입니다. `app-server generate-json-schema --out <임시 폴더>`로 설치 CLI 자체의 Schema를 생성했습니다.

| 계약 | 확인 |
|---|---|
| `ThreadStartParams.threadSource`·`ThreadForkParams.threadSource` | 존재. 설명은 analytics source classification이며 문자열을 받음 |
| `ThreadResumeParams` | `threadSource` 없음 |
| `ThreadListParams.sourceKinds` | 생략·빈 배열이면 기본 interactive 출처만 조회 |
| `ThreadListParams.useStateDbOnly` | 확장 26.1021.22027과 같은 DB 전용 목록 조회 가능 |
| `InitializeParams.clientInfo` | `name`·`title`·`version`. 다른 공개 생성 출처 제어 필드는 생성 Schema에서 찾지 못함 |

모든 실험은 저장소 밖 새 임시 폴더에서 수행했습니다. 생성·조회·Archive·Unarchive는 공식 App Server 요청으로만 했으며 JSONL·SQLite를 직접 수정하지 않았습니다.

### 1.1 출처 지정과 실제 목록 분류는 같지 않았음

| 실험 | Native `thread.source` | Turn 후 기본 DB 목록 |
|---|---|---|
| `clientInfo.name=telosweaver`, `threadSource` 생략 | `vscode` | 포함 |
| 같은 이름, `threadSource="cli"` | `vscode` | 포함 |
| `clientInfo.name=codex_vscode`, 필드 생략 | `vscode` | 포함 |
| `clientInfo.name=telosweaver`, `threadSource="exec"` | 생성 직후와 저장 후 모두 `vscode` | 포함 |
| 같은 이름, `threadSource="appServer"` | 생성 직후와 저장 후 모두 `vscode` | 포함 |

위 Turn 실험은 `CODEX_INTERNAL_ORIGINATOR_OVERRIDE`·`CODEX_THREAD_ID`·`CODEX_SESSION_ID` 등 부모 Codex 변수를 제거한 환경에서도 확인했습니다. `codex_exec`·`codex_cli_rs` 같은 다른 초기화 이름으로 생성한 무 Turn Thread도 `vscode`를 반환했습니다. 제품 코드는 초기화 이름 `telosweaver`를 유지합니다.

처음에는 Turn 없는 Thread가 DB 전용 목록에 없었습니다. 이는 `hidden` 성공 증거가 아닙니다. 짧은 Turn을 완료해 저장된 뒤 기본 목록에 들어오는지 다시 시험했습니다. `threadSource="exec"`·`"appServer"`도 Turn 후 `thread/read`가 `vscode`를 반환하며 목록에 들어왔습니다.

**판정:** 현재 버전에서는 `threadSource` 생략을 `hidden` 구현으로 취급할 수 없습니다. 필드는 analytics 분류이며, 시험한 공개 필드와 초기화 이름으로 실제 목록 분류를 바꾸는 수단을 찾지 못했습니다. 모든 가능한 인터페이스에 그런 수단이 없다는 일반적 단정은 하지 않습니다.

실험 원본은 다음 임시 폴더에 있습니다.

- Schema: `%TEMP%/telos-codex-listing-schema-24d829b37b9644c9a5318a689507e172`
- 무 Turn 최초 실험: `%TEMP%/telos-codex-listing-probe-djmr6qii/results.json`
- 부모 변수 제거·완료 Turn 실험: `%TEMP%/telos-codex-listing-origin-yot08iq7/results.json`
- 다른 초기화 이름의 무 Turn 실험: `%TEMP%/telos-codex-listing-client-y38bzber`
- `exec`·`appServer` 완료 Turn 실험: `%TEMP%/telos-codex-hidden-probe-u7haertr/results.json`

## 2. 구현

| 파일 | 변경 |
|---|---|
| `crates/adapters/src/codex.rs` | `CodexVscodeListing::{Listed, Hidden}`와 기본 `Listed`, 명시적 설정 생성자, Capability, 영속 생성·Fork, 응답 출처 검사, 미지원 Hidden 거부 |
| `crates/adapters/src/codex/tests.rs` | 4개 회귀 시험과 Native 응답 대역의 source 필드 |
| `crates/adapters/src/codex/tests/environment.rs` | 명시 실행하는 Fresh·Resume·NativeFork DB 목록 판정 시험 |

### 2.1 설정과 동작

기존 `CodexAdapter::new(config)`는 `Listed`를 기본으로 사용합니다. 명시적으로 선택하려면 다음 생성자를 씁니다. 기존 `CodexConfig` 구조체 리터럴을 바꾸지 않아 INTEGRATION 소유 파일을 수정하지 않았습니다.

```rust
CodexAdapter::with_vscode_listing(config, CodexVscodeListing::Listed)
```

- Fresh·Capsule·NativeFork에 `ephemeral: false`, `threadSource: "vscode"`를 지정합니다. `threadSource`를 가시성 전환 수단으로 주장하지 않으며 현재 Native 출처·기본 목록 실험과 함께 판정합니다.
- Resume에는 지원하지 않는 `threadSource`·`ephemeral` 필드를 넣지 않고 기존 세션을 사용합니다.
- Native 응답이 `cli`·`vscode` 외의 출처면 Turn을 시작하지 않고 `UnsupportedCapability`를 반환합니다. 이번 호출로 만든 Thread는 공식 Archive로 정리하며 Resume 원본은 Archive하지 않습니다.
- `Hidden`은 프로세스 생성 전에 `UnsupportedCapability`로 거부합니다. 목록에 보이는 Thread를 만들어 놓고 숨김 성공으로 보고하지 않습니다. Archive를 숨김 출처의 대체 수단으로 사용하지 않습니다.
- cwd·Sandbox·승인·모델·도구 정책은 기존 값을 유지합니다.

### 2.2 Capability

`detect()`의 `vscode_listing`은 확인한 Windows x64·CLI 버전·확장 디렉터리 조합의 `Listed`에서만 `supported`·`environment-tested`입니다. 다른 버전·플랫폼은 `unverified`, `Hidden`은 `unsupported`입니다. 사유에 대상 버전, Hidden 미지원, 화면 확인 미완료를 명시합니다.

이는 DB 기본 목록 자동 판정의 지원 상태입니다. 실제 확장 화면에서 목록·대화를 확인했다는 의미는 아닙니다. Linux는 이번 판정 범위가 아닙니다.

## 3. X4: 실제 Adapter 자동 판정

`TELOS_CODEX_CLI`·`TELOS_CODEX_MODEL=gpt-6-luna`·`TELOS_CODEX_VERSION=codex-cli 0.164.0-alpha.11.1`·`TELOS_CODEX_HOST_ID=windows-local`을 지정했습니다. 각 Turn은 low effort로 `Reply only PONG. Do not use tools.`를 요청했습니다.

명령: `cargo test -p telos-adapters --lib env_vscode_listing_fresh_resume_and_fork --locked -- --ignored --nocapture --test-threads=1`. 산출물은 `--target-dir D:/Downloads/Vibe_dev/TelosWeaver/target`을 사용했습니다.

| 단계 | Native Thread | 결과 |
|---|---|---|
| Fresh | `01a17371-a72f-76c2-95e9-75b63e52ad61` | 완료, source `vscode`, 기본 DB 목록 포함 |
| Resume | 같은 Thread | 완료, 같은 ID·cwd·source 유지, 기본 DB 목록 포함 |
| NativeFork | `01a17371-cefe-7ff1-a3a0-7c48bf645990` | 완료, 새 ID·같은 cwd·source 유지, 기본 DB 목록 포함 |

세 단계 모두 `sourceKinds` 생략과 빈 배열을 각각 검사했고 `useStateDbOnly: true`였습니다. 최종 시험은 **`thread/read`를 호출하기 전에 DB 전용 목록을 검사**하므로 Native 읽기의 색인 보정에 기대지 않습니다. 목록 필터는 실행 cwd에서 Windows의 `\\?\` 표현만 제외하고, 이후 `thread/read`의 cwd와 실행 cwd가 같은 폴더인지 canonical 경로로 확인했습니다. 실행 cwd는 바꾸지 않았습니다. 각 Run은 완료 이벤트와 실제 프로세스 종료·자식 회수 이벤트를 보고했습니다.

초기 실행도 3개 모드가 통과했으며, 조회 순서를 강화한 뒤 위 ID로 한 번 더 통과했습니다. 최초 Fresh·Resume Thread `01a17369-a6d5-7001-8733-3f4eadd8f931`은 §5의 화면 확인용입니다.

## 4. 검사 결과

| 검사 | 결과 |
|---|---|
| `cargo test -p telos-adapters --lib --locked` | 실환경 추가 전 19개 통과, 7개 기본 제외 |
| `cargo test --workspace --locked --quiet` | 159개 통과, 실환경 34개 기본 제외 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 통과 |
| `cargo fmt --all -- --check` | 통과 |
| `git diff --check` | 통과 |
| 실제 Adapter 목록 시험 | 초기 1개 통과, DB 조회 순서 보강 후 1개 재통과. 실행마다 Fresh·Resume·NativeFork 3개 Turn 확인 |

위 Rust 명령은 Worktree에서 공용 산출물 디렉터리 `--target-dir D:/Downloads/Vibe_dev/TelosWeaver/target`을 사용했습니다. 새 회귀 시험 4개는 기본 표시·영속 필드, Hidden의 프로세스 생성 전 거부, 비대화형 Native 출처의 생성 전 거부·원본 소유권 보존, 미확인 버전의 Capability를 검사합니다.

## 5. X5: 화면 확인

**미검증·사용자 확인 대기입니다.** 자동 시험 후 Fresh Thread 하나를 공식 `thread/unarchive`로 복원해 기본 DB 목록 포함을 재확인했습니다. 사용자에게 아래 폴더를 VS Code 새 창으로 열고 Codex 대화 목록에서 해당 대화를 찾아 열어 달라고 요청했습니다.

- 폴더: `C:\Users\Psyki-HOME\AppData\Local\Temp\telos-codex-live-22132-1792937666091924000`
- Thread: `01a17369-a6d5-7001-8733-3f4eadd8f931`
- 내용: Fresh와 Resume의 `PONG` 응답
- 별도 판정: 목록 표시 / 대화 열기 / 실시간 반영. 실시간 반영은 시험하지 않습니다.

실행은 종료됐으며 화면 확인 대상 하나만 의도적으로 남겼습니다. 사용자 확인 또는 통합 시점에 이 Thread도 공식 Archive로 정리합니다. 사용자의 다른 Thread는 수정하지 않습니다.

## 6. X6: 시험 Thread 정리

| 실험 | 생성한 Thread | 처리 |
|---|---|---|
| 무 Turn 최초 | `01a17362-48ed-7330-8435-8e5dff3cbcaa`, `01a17362-4930-7701-b2ca-0750cb4fbfc8`, `01a17362-4950-7712-8199-be3a116bea6b`, `01a17362-4ac6-7120-946e-53572d8d4901` | 공식 Archive 응답 성공 |
| 완료 Turn·초기화 이름 | `01a17363-02ae-75b1-a236-f43dc033f8bf`, `01a17363-104b-7323-8d23-59ce7ade5989`, `01a17363-20ff-7692-975c-d39305f6ae04` | 공식 Archive 응답 성공 |
| 다른 이름·출처, 무 Turn | `01a17365-3b5e-72e1-8b65-8fc45d815e46`, `01a17365-3ceb-7fd0-b4e2-555b13a8c1cd`, `01a17365-3f01-7350-92fd-35a2af3bec37` | 공식 Archive 응답 성공 |
| 비대화형 출처 완료 Turn | `01a1736c-6c96-74a2-bad6-33fd519cc28a`, `01a1736c-7a15-71c1-b629-720dff3fece9` | 공식 Archive 응답 성공 |
| Adapter Fork | `01a17369-d017-7c11-9cf0-f692417a099b` | 공식 Archive 응답 성공 |
| Adapter Fresh·Resume | `01a17369-a6d5-7001-8733-3f4eadd8f931` | Archive 후 화면 확인용으로 공식 Unarchive. 확인·병합 후 정리 대상 |
| Adapter 재검증 Fresh·Resume·Fork | `01a17371-a72f-76c2-95e9-75b63e52ad61`, `01a17371-cefe-7ff1-a3a0-7c48bf645990` | 공식 Archive 응답 성공 |

마지막에 여섯 시험 폴더를 대상으로 `thread/list`를 `archived: false`·`useStateDbOnly: true`로 조회했습니다. 화면 확인용 Thread 하나만 활성 목록에 남았으며 나머지 시험 폴더의 활성 목록은 비어 있었습니다.

## 7. INTEGRATION 인계

1. 운영자 정책의 `vscodeListing`을 명시적 생성자에 연결하고 실제 설정·방식을 Snapshot에 기록하는 I1·I2는 INTEGRATION 범위입니다.
2. 현재 Codex 버전에서 `hidden` 생략 구현 가설은 반증됐습니다. 기본 표시만 지원하고 Hidden을 명시 거부하는 구현을 검토 요청하며, 이 제한에 대한 사용자 선택을 질문했습니다. 아직 답변을 승인으로 기록하지 않았습니다.
3. §7.6·작업 지시의 출처 설명을 실제 관찰과 구분할 필요가 있습니다. 공통 문서·정책 변경은 기존 담당에게 요청하며 이 Branch에서 수정하지 않습니다.
4. **DEC-08 전체 완료로 선언하지 않습니다.** 기본 목록 자동 판정은 통과했지만 Hidden과 실제 화면 확인은 위 제한·미검증 상태입니다. Linux·통합 정책·최종 G3 판정은 별도입니다.

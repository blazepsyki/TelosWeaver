# G2 EXT 담당 확인

| 항목 | 값 |
|---|---|
| 담당·날짜 | `Codex / G2-REVIEW-01a17034` · 2026-10-25 |
| 기준 | master `08a4a13`, Spec v1.5.1, 계약 `7497877`, 착수 배정 `ef3d2f1` |
| Branch / Worktree | `ph0/host-g2-review` / `D:\worktree\TelosWeaver\ph0-host-g2-review` |
| 결과 | **EXT 범위에서 INTEGRATION의 G2 통과 판정에 동의합니다. REQ-019의 추가 Health 항목과 호환됩니다.** |

## 1. 확인 내용

Extension 연결은 `43ce639`, REQ-016은 `2331c7c`로 통합됐고 REQ-017의 구독 순서 보장도 계약 문서에 반영됐습니다. REQ-018의 RN 계약이 생성 타입에 포함되며 새 생성물은 50개입니다. `9d8a9db` 이후 Extension 제품 코드는 변경되지 않았습니다.

REQ-019로 Core의 `components`에 `rn`이 추가되어도 기존 클라이언트는 이 목록을 해석하고 TreeView에 표시합니다. 새 UI 회귀 시험은 Core 연결과 전체 `degraded` 상태를 유지한 채 RN의 `ok → degraded → ok`가 해당 행에 반영되는지 확인합니다. RN 항목의 변화만으로 Core 연결을 다시 시도하거나 동일한 연결 상태 문구를 반복 출력하지 않습니다.

기존 실제 Core 연결 시험에는 정책 없는 Core의 `rn` 항목이 `degraded`·`not configured`인지 검사하는 조건을 추가했습니다. 제품 코드는 수정하지 않았습니다.

## 2. 검증

Windows 로컬, Node v24.14.0, npm 11.9.0, VS Code 1.143.1입니다. 실제 Core는 검토 Worktree 소스로 `cargo build -p telos-core --bin telos-weaver --locked` 후 대표 저장소 `target/debug/telos-weaver.exe`에 생성한 바이너리를 사용했습니다.

| 명령 | 결과 |
|---|---|
| `npm ci --ignore-scripts` | 성공, 취약점 0건 |
| `npm test` | 생성 타입 50개·TypeScript 빌드 성공, 29개 통과(기존 28개 + RN 행 갱신 1개) |
| `TELOS_CORE_EXECUTABLE` 지정 후 `node --test test/live-core.cjs` | 1개 통과. 인증·Degraded·RN 설정 없음·DenyRuns·연결 해제 후 생존·동일 PID 재사용. 시험 Core PID `19392` |
| Core·VS Code 실행 파일 지정 후 `node scripts/test-host.cjs` | 종료 코드 0, 5항목 통과, `coreSurvivedHostExit=true`. Core PID `18156`, 결과 `%TEMP%/telos-ext-host-CMFjrS/result.json` |

격리 Extension Host는 별도 사용자 프로필·Extension 디렉터리·Core 상태 폴더를 사용했습니다. Provider·모델을 호출하거나 사용자 설정을 변경하지 않았습니다. 시험 Core PID는 종료 후 부재를 확인했습니다.

## 3. 남은 범위

실제 RN 왕복·재시작은 [HOST 확인](../PH0-HOST/G2_HOST_REVIEW.md)에서 별도로 재현했습니다. UI의 RN 상태 변화는 대역 시험이고, 실제 Extension Host에는 정책 없는 Core를 연결했으므로 실제 RN 재시작을 UI에서 관찰한 증거로 확대하지 않습니다.

전체 Task·마스터 대화·승인·Artifact UI, Linux·Remote SSH, G3는 후속 범위입니다. G2의 EXT 담당 확인을 제출하며 최종 게이트 상태 반영은 기존 INTEGRATION 담당이 수행합니다.

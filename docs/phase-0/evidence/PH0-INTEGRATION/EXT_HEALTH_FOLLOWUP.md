# PH0-EXT 비차단 후속: 연결 오류 중 이전 Health를 현재 상태처럼 표시하지 않기 (G3 발견 3)

| 항목 | 값 |
|---|---|
| 작성 | PH0-INTEGRATION (`claude-opus-5.5/session-c9336d2c`), 2026-10-26 사용자 지시 |
| 처리 | **PH0-EXT (Codex)**, 기존 Extension 담당 |
| 기준 | 이 지시서를 기록한 `master` Commit, Spec v1.5.6, 계약 `7497877`(변경 없음) |
| 근거 | [통합 증거](README.md) §9.5 발견 3(Remote SSH Host 유실 재현, 사용자 VS Code), Spec §2.1(미검증·불명 상태를 정상처럼 표시하지 않음)·§17 |
| 우선순위 | 비차단. G3 판정 조건이 아니며 G3 대조표에는 "처리됨" 또는 "비차단 후속"으로 기록 |

## 1. 관찰

`psyki-nas`에서 Core를 `kill -9`로 끝내자 패널이 `연결 오류`와 진단 "실행 중인 Core에 연결할 수 없습니다"를 표시했습니다. 그런데 같은 패널에 `execution ok` 등 마지막으로 받은 Health 구성 요소 값이 그대로 남아, 죽은 Core의 상태가 현재 값처럼 보였습니다.

## 2. 원인 (INTEGRATION 코드 확인, 실행 재현은 담당이 확인)

- `extension/src/core.ts`의 두 오류 전환이 이전 상태를 펼쳐 씁니다. 연결 종료 처리(현재 132행 `this.update({ ...this.state, phase: "error", … })`)와 재연결 실패 처리(현재 253~257행)에서 `health`·`hello`가 `error` 상태로 넘어갑니다.
- `extension/src/extension.ts`의 `StatusProvider`(현재 40행)는 `phase`와 상관없이 `state.health.components`를 행으로 그립니다. StatusBar 문구(`phaseName`)는 `연결 오류`라서 올바르고, 상태 View만 문제입니다.

## 3. 작업

| # | 작업 | 확인 방법 |
|---|---|---|
| X1 | `connected`가 아닌 상태(`error`·`connecting`·`disconnected`)에서 Health 구성 요소를 현재 값으로 표시하지 않습니다. 방식은 담당이 정합니다. (a) 오류 전환 때 `health`를 버리기, (b) 값을 두되 View에서 "마지막 확인 값"과 확인 시각으로 구분해 표시하기 중 하나를 고르고, 고른 이유를 증거에 적습니다 | Node 시험: `connected`(Health 있음) → 연결 종료 → `error`에서 View 행에 구성 요소 상태가 현재 값으로 나오지 않음 |
| X2 | 재연결 실패 경로(`scheduleReconnect`)와 Heartbeat 실패 뒤 연결 종료 경로 모두 X1과 같은 결과가 되게 합니다. 재연결에 성공하면 새 Health로 정상 표시가 돌아옵니다 | Node 시험: 실패 → 재연결 성공 시 새 Health 표시, 이전 값이 섞이지 않음 |
| X3 | `hello`(Core 버전·Project·Host)를 오류 중에 남길지 정합니다. 남긴다면 "마지막으로 연결한 Core"임이 드러나게 표시합니다. Extension 공개 API `state()`를 쓰는 기존 시험(`src/test/host.ts` 등)의 기대와 맞는지 확인합니다 | 기존 시험 통과, 바뀐 기대는 증거에 기록 |
| X4 | 출력 창 로그는 상태 전환만 기록하는 REQ-016 동작을 유지합니다 | 기존 `ui.test.cjs` 통과 |

## 4. 범위와 금지

- 수정 경로는 PH0-EXT 독점 경로(`extension/`, 자기 증거)만입니다. Core·`telos-schema`·생성 타입은 바꾸지 않습니다. 필요하면 배정표 §4에 요청합니다.
- Core 프로세스 수명, 연결·재연결 정책(횟수·간격), 구독(`runFeed.ts`) 동작은 바꾸지 않습니다.
- 화면 확인은 선택입니다. 하면 사용자에게 열 창·누를 명령·볼 위치를 단계별로 요청하고, 하지 못했으면 미검증으로 기록합니다. Linux·Remote SSH 재확인은 필수가 아니며, 하면 Host 열을 나눠 적습니다.
- 모델 호출은 필요 없습니다.

## 5. 작업 방식

1. 최신 `master`에서 Branch `ph0/ext-health`, Worktree `D:\worktree\TelosWeaver\ph0-ext-health`를 만듭니다.
2. 착수 전 배정표 PH0-EXT 행(`integrated → in-progress`, 담당자 ID·Branch·Worktree·기준 Commit)과 §7에 `master` 단독 커밋으로 기록합니다(§7 표 행 사이 빈 줄 금지).
3. 수정 전 빌드에서 새 시험이 실패하는 것을 먼저 확인합니다(REQ-016과 같은 방식).
4. 검사: `cd extension && npm test`, `npm run test:core`(실제 DenyRuns Core), `npm run test:host`(격리 VS Code Host). Rust는 바꾸지 않으므로 Workspace 시험은 필요 없습니다. 결과 숫자를 기록합니다.
5. 결과는 `docs/phase-0/evidence/PH0-EXT/HEALTH_STALE.md`에 적고, PH0-EXT 행을 `review`로 바꿔 결과 Commit과 함께 INTEGRATION 검토·통합을 요청합니다.

# Phase 0 작업 배정표

> **프로젝트:** TelosWeaver  
> **갱신일:** 2026-10-24  
> **범위 원본:** [Spec §19.5](../DEVELOPMENT_v1.4.md#195-phase-0-병렬-구현--작업-분할과-소유권)  
> **현재 상태:** G0 통과, 개정 1 반영(계약 기준 `077bd78`). PH0-HOST 1차 구현을 `1d8734f`로 통합했고(REQ-011 포함), REQ-010 탐지 계약을 `3a37919`로 추가했습니다. HOST 탐지 엔진 연결(`685bd96`)과 PH0-CLAUDE Adapter(`3bdb808`, `b8e3fa9`)를 통합했습니다. G1 Core 연결을 `cfe1b19`로 통합하고 실제 Claude 실행으로 G1을 판정했고, HOST 확인과 REQ-015 재검증으로 G1 통과를 확정했습니다(`399992d`). Codex·Agy Core 연결을 `aef3297`로 통합해 세 Provider가 한 Daemon에서 실제 실행됩니다(G2 일부). PH0-MASTER를 `e14de42`로 통합해 마스터 결정 검증 경로도 갖췄습니다. 나머지 작업은 배정 요청을 받아 착수할 수 있습니다. G2·G3는 대기입니다.

## 1. 단일 원본과 갱신 주체

대표 저장소의 이 파일을 단일 원본으로 사용합니다. 대표 저장소의 위치는 최초 통합 담당이 착수 시 기록합니다. 다른 Worktree·Clone의 복사본은 참고용이며 그것만 바꿔 소유권을 확보하지 않습니다.

수정 범위는 Spec §19.5.5를 따릅니다.

| 누가 | 고칠 수 있는 곳 |
|---|---|
| 담당 에이전트 | 자기 작업 행(상태·Branch/Worktree·기준·결과 Commit·인계), 자기가 제출하는 §4 변경 요청 행, §7 변경 기록 추가 |
| 통합 담당만 | §1 관리 항목(계약 기준 Commit 등), §3 게이트 판정, §4 요청의 채택·완료 판정, §5·§6, 다른 작업의 행, `integrated`·`verified` 전환 |
| 공통 조건 | 배정표 수정은 대표 저장소 `main`에서 단독 커밋으로 합니다(코드 Branch에 섞지 않음). 배정 시작(`unassigned → reserved`)은 커밋 직전에 그 행이 아직 `unassigned`인지 확인합니다 |

작업 에이전트는 Spec의 작업 ID를 선택해 자기 행에 담당·Branch/Worktree·기준 Commit을 기록한 뒤 해당 경로를 수정합니다. 통합 담당이 정해지지 않았다면 최초 착수를 총괄하도록 지정된 진행 담당이 인수 사실을 기록합니다. 서로 다른 에이전트가 독립적으로 같은 역할을 차지하지 않습니다.

한 작업 ID에 Writer 한 명만 배정합니다. 같은 담당자가 여러 ID를 맡을 수 있으며, 담당자 ID는 동명이인이나 재시작 세션을 구분할 수 있는 세션·작업 식별자를 포함합니다. 작업 범위와 파일 예외는 Spec을 따르고 이 표에서 임의 확대하지 않습니다.

| 관리 항목 | 현재 값 |
|---|---|
| 대표 저장소 | `D:\Downloads\Vibe_dev\TelosWeaver`, Branch `main`. 공개 원격 `origin` = https://github.com/blazepsyki/TelosWeaver (`main`만 push, 작업 Branch는 로컬에서 병합 후 삭제) |
| 통합 담당자 ID | `claude-opus-5.5/session-c9336d2c` (2026-10-24 사용자 지정) |
| 공통 계약 기준 Commit | `3a37919` (2026-10-24 REQ-010 CLI 탐지 계약 추가). G0 개정 1 `077bd78`, 1차 기준 `9710402` |
| 공통 계약 기준 설명 | §5의 7개 계약. 변경은 §4 변경 요청으로만 처리합니다. 판정 근거: `evidence/PH0-INTEGRATION/README.md`, 개정: `evidence/PH0-INTEGRATION/G0_REVISION_1.md` |

## 2. 배정 상태

| 작업 ID | 담당자 ID | 상태 | Branch / Worktree | 기준 Commit | 결과 Commit | 인계·요청 |
|---|---|---|---|---|---|---|
| PH0-INTEGRATION | `claude-opus-5.5/session-c9336d2c` | in-progress | `main` / 대표 저장소 작업 트리 | `3f050e9` | `9710402` (G0), `077bd78` (G0 개정 1), `3a37919` (REQ-010) | HOST 1차 통합·REQ-011(`1d8734f`), REQ-010 계약(`3a37919`)·엔진 연결(`685bd96`), CLAUDE 통합(`b8e3fa9`), G1 Core 연결(`7156435`, 통합 `cfe1b19`), AGY 통합·REQ-013 ①②(`286844a`), CODEX 통합·REQ-012·014(`64f9674`). CODEX 증거 갱신(`e55c0b2`), HOST G1 확인 통합(`b561da2`), REQ-015·G1 확정(`399992d`). Codex·Agy Core 연결(`8a5e64b`, 통합 `aef3297`), MASTER 통합(`e14de42`). 다음: RN·EXT 배정 후 통합(G2), WSL 구성(사용자 수행 필요) |
| PH0-CONTRACT | `claude-opus-5.5/session-c9336d2c` | verified | `ph0/contract` / Worktree 정리됨 | `0094720` | 1차 `ade440e`, 개정 1 `4c14d2a` (통합 `077bd78`), REQ-010 `b1e7473` (통합 `3a37919`) | 이후 계약 변경은 §4 요청으로 처리 |
| PH0-HOST | `Codex / PH0-HOST-20261024` | in-progress | `ph0/host` / 병합 후 삭제 | 후속 기준 `3fe9153`·착수 `66e9cb9` (Spec 개정 1.4.3), 계약 `3a37919` | 1차 `29295c9`(통합 `1d8734f`), REQ-010 `fa7facf`(통합 `685bd96`), G1 확인·REQ-013 ③ `4791c5d`·`cbb1b4f`(통합 `b561da2`) | G1 HOST 확인·REQ-013 ③ 완료·통합(`b561da2`, main 123개·clippy·fmt, Agy `metadata` 재확인). 취소 경합 판정 보정 반영. REQ-015는 INTEGRATION이 처리(`399992d`). [확인·인계](evidence/PH0-HOST/G1_HOST_REVIEW.md). 참고(INTEGRATION 증거 §8.4): Codex 토큰 스트림에서 연결별 송신 대기열(8)이 차 `slow_consumer`로 구독이 닫힌 사례가 있음(클라이언트 재구독으로 처리). 현황 재확인(`089d0b3`): G1 통과 확정·REQ-015 완료, Workspace 142개·엄격 Clippy·fmt 통과. 남은 일: WSL Linux·Remote SSH 실행 검증(사용자 지시로 대기). 후속 작업은 최신 `main`에서 새 Branch·Worktree로 시작 |
| PH0-CODEX | `Codex / PH0-CODEX-20261024` | integrated | `ph0/codex` / 병합 후 삭제(증거 갱신 `1aa8d67` 통합 `e55c0b2`) | 착수 `7a9d4b3`, 계약 `3a37919`, main 반영 `83cf156` | 코드 `0dcc1ca`(초기 `a498976`), 인계 `72a4679`·`e8dbe84` (통합 `64f9674`) | 통합 완료. [검증·인계](evidence/PH0-CODEX/README.md): Mock 12개·Windows 실환경 7개 통과, 시험 Thread 8개 Archive. 통합본 118개·엄격 Clippy·fmt 통과, 병합본 탐지 실환경 재확인. 통합 후 현황 재확인(`86f580e`): Workspace 118개·엄격 Clippy·fmt 통과. 자기 검증·인계 문서 갱신 `1aa8d67`는 `e55c0b2`로 통합 완료. Core 연결 완료(`aef3297`, G2 실환경 통과). 정리 누락 시험 Thread 4개+1개는 Archive 미처리(INTEGRATION 증거 §8.5). 현황 재확인(`089d0b3`): Workspace 142개·엄격 Clippy·fmt 통과(실환경 30개 기본 제외). 남은 일: `send_input`·승인 Allow 실환경 미검증, Linux 검증 |
| PH0-CLAUDE | `claude-opus-5.5/session-c9336d2c` | integrated | `ph0/claude` / 병합 후 삭제 | `689d43e`, 계약 `3a37919` | `a3027c3`·`62c86af` (통합 `3bdb808`·`b8e3fa9`) | G1 대표 Provider(§6). [검증 기록](evidence/PH0-CLAUDE/README.md): Mock 13개, 실환경 6개(haiku) 통과 — PROV-01~08, Resume·Fork 동작 확인. 남은 일: G1 Core 연결, 시험 세션 Archive(§5 목록), Linux 검증 |
| PH0-AGY | `gemini-4-argon/session-9e6137eb` | integrated | `ph0/agy` / 병합 후 삭제 | `611032a`, 계약 `3a37919` | `aa21611` (통합 `286844a`) | 통합 완료(main 106개 통과, 병합본에서 탐지 실환경 재확인). **Core 연결 완료(`aef3297`).** **전달(INTEGRATION 증거 §8.4):** ① Workspace 경로가 `\\?\` 형식이면 Antigravity가 Workspace 대신 `~/.gemini/antigravity/scratch/`에 씀 — INTEGRATION은 경로 정규화와 `--add-dir`로 해결, AGY 시험은 쓰기 위치를 검사하지 않음. ② agy 1.4.7 대화 파일은 `conversations/<id>.db`(`-wal`·`-shm`)라 `cleanup_session`의 `.pb` 삭제가 맞지 않음. 참고: 시험 정리는 `brain/<id>`·`conversations/<id>.pb`만 지우므로 `conversation_summaries.db`의 요약 기록은 남을 수 있음. 현황 재확인(`86ef048`): REQ-013 ①②③·Core 연결(`aef3297`)·Workspace 142개·엄격 Clippy·fmt 통과 확인, 앞선 PH0-AGY 시험 세션의 잔여 `conversations/<id>.db` 16개 삭제 완료. 제출 내용: 구현·검증 완료([검증 기록](evidence/PH0-AGY/README.md)): Mock·Contract 14개, 실환경 6개(`gemini-3.8-flash-low`, `--effort low`) 통과 — AGY-DATA-01~05(`--app_data_dir=antigravity` 기본 채택·버전 명령 검증·잠금 충돌 없음), PROV-01~09, Resume(`--conversation`) 확인, NativeFork 미지원 거부(FORK-POC-01), 시험 세션 즉시 삭제 완료. REQ-013 제출(`pub mod agy;` 및 Spec §7.5 반영). 남은 일: Linux 검증 |
| PH0-MASTER | `claude-opus-5.5/session-c9336d2c` | integrated | `ph0/master` / 병합 후 삭제 | `74a2a83`, 계약 `3a37919` | `fcf6def` (통합 `e14de42`) | [검증 기록](evidence/PH0-MASTER/README.md): 결정 검증(호출자·Plan·Epoch·Revision·Payload·kind·권한·한도·트리·의존성·신뢰된 조회)·Controller(멱등·집행·Resume 결과 왕복·Epoch·기록). 검증기 8개·Controller 6개, 실제 Claude 마스터(haiku) 실환경 2개 통과: 위임→Worker 실행→결과 왕복, 권한 밖 Provider `permission_denied`. 남은 일: 미집행 kind 5종·Local API·영속성·정식 정책은 P0 |
| PH0-RN | 미배정 | unassigned | 미지정 | 미지정 | 없음 | 착수 가능. Fork 도입 방식·기준 SHA 기록, `telos_schema::rn` 계약 구현 |
| PH0-EXT | `Codex / PH0-EXT-20261024` | reserved | `ph0/ext` / `D:\worktree\TelosWeaver\ph0-ext` (준비용) | `207c125` (Spec 개정 1.4.4), 계약 `3a37919` | 없음(배정·사전 준비만) | 사용자 직접 배정. Core 실행 파일 탐지·명시적 시작·연결·Health/상태 표시·생성 타입 사용 범위의 사전 결정·계약 점검. 구현 미착수. `runs.subscriptionClosed`와 lastSequence 재구독 전달 사항 확인. Windows 우선, Linux·Remote SSH는 기존 대기 지시 유지. 준비 기록은 `evidence/PH0-EXT/README.md`에 작성 예정 |

상태는 `unassigned → reserved → in-progress → review → integrated → verified`를 사용합니다. `blocked`는 기존 담당자를 유지하며 차단 사유·필요한 계약·재개 조건을 기록합니다. 통합 전 `verified`로 표시하지 않습니다. 개별 `verified`는 Phase 0 전체 통과와 다릅니다.

## 3. 통합 게이트

| 게이트 | 상태 | 근거 |
|---|---|---|
| G0 공통 계약 확정 | 통과 (2026-10-24), 개정 1 반영 | 1차 `9710402`. 독립 검토 REQ-002~009 처리 후 기준 `077bd78`. 근거: `evidence/PH0-INTEGRATION/README.md` §1, `G0_REVISION_1.md` §4 |
| G1 대표 Provider 실행 | 통과 확정 (2026-10-24, INTEGRATION·CLAUDE 판정, HOST 확인 완료) | 대표 Provider Claude(§6). Local API → 운영자 정책 승인 → 확장 CLI 탐지 → Claude Adapter → 공용 런타임 경로로 시작·이벤트·완료·실패·취소·결과 파일·Daemon 종료 정리를 실제 실행. HOST 확인 `G1_HOST_REVIEW.md`(취소 경합 보정 `4791c5d`, 통합 `b561da2`), REQ-015 강화 시험 실환경 5개 재통과(`399992d`). 근거: `evidence/PH0-INTEGRATION/README.md` §7 |
| G2 전체 구성 통합 | 대기 | 세 Provider·마스터·RN·Extension 연결 증거 필요. 현황: **세 Provider Core 연결 완료**(`aef3297`): 한 Daemon에서 Claude·Codex·Antigravity의 완료·결과 파일·실패·취소·Daemon 종료 정리를 실제 실행(G2 실환경 4개). **마스터 결정 검증 경로 완료**(`e14de42`): 실제 Claude 마스터의 결정 회수·검증·거부·집행·결과 왕복(`evidence/PH0-MASTER/README.md`). RN·EXT 미배정. 근거: `evidence/PH0-INTEGRATION/README.md` §8.4 |
| G3 Phase 0 수용 | 대기 | Spec §19.1 필수 조건과 대상 환경 증거 필요. Linux·Remote SSH Host는 [WSL 절차](WSL_LINUX_HOST.md)로 확보 예정(미구성) |

기존 타입·빌드·단위 테스트가 존재한다는 이유만으로 게이트를 통과시키지 않습니다. 증거는 Spec §19.5.8의 형식으로 `docs/phase-0/evidence/<작업 ID>/README.md`에 기록합니다.

## 4. 계약·공통 파일 변경 요청

| 요청 ID | 요청 작업 | 변경 대상·이유 | 영향 작업 | 처리 주체 | 상태·결과 Commit |
|---|---|---|---|---|---|
| REQ-001 | PH0-INTEGRATION | `.gitattributes` 신설(LF 고정). `core.autocrlf=true` 환경의 새 Worktree가 CRLF로 체크아웃되어 생성물이 내용 변경 없이 수정으로 표시됨. 현재 Spec §19.5.3에 소유 작업이 없어 Spec에 INTEGRATION 경로로 추가 필요 | 전체 | 사용자 확인 후 INTEGRATION | 완료. 사용자가 LF로 결정, `.gitattributes` 추가·Spec v1.3.4에 경로 등록. Index 내용 변경 없음 |
| REQ-002 | G0 독립 검토 / PH0-HOST | 공용 ProcessHandle의 대기·입력·취소 동시성 및 wait 취소 안전성 계약 확정 | HOST, CODEX, CLAUDE, AGY, INTEGRATION | INTEGRATION 조정·HOST 처리, CONTRACT 확인 | 계약 완료(`00cf7ea`, 통합 `077bd78`). 구현·수용 시험은 PH0-HOST 대기. [판정](evidence/PH0-INTEGRATION/G0_REVISION_1.md) · [요청](evidence/PH0-INTEGRATION/G0_REVIEW_REQUESTS.md#req-002-대기입력취소의-동시성-계약) |
| REQ-003 | G0 독립 검토 / PH0-HOST | 출력 스트림의 I/O 오류·긴 Protocol 줄·버퍼 초과를 명시적 실패/복구로 연결 | HOST, 세 Provider, INTEGRATION | INTEGRATION 조정·HOST 처리, CONTRACT 확인 | 계약 완료(`00cf7ea`, 통합 `077bd78`). 구현·수용 시험은 PH0-HOST 대기. [판정](evidence/PH0-INTEGRATION/G0_REVISION_1.md) · [요청](evidence/PH0-INTEGRATION/G0_REVIEW_REQUESTS.md#req-003-출력-오류와-상한-초과-처리) |
| REQ-004 | G0 독립 검토 / PH0-CONTRACT | DecisionOutcome의 승인/거부·사유·효과 불변조건과 모순된 공통 시험 수정 | CONTRACT, MASTER, HOST, EXT | CONTRACT 처리·INTEGRATION 통합, MASTER 확인 | 완료(`4c14d2a`, 통합 `077bd78`). [판정](evidence/PH0-INTEGRATION/G0_REVISION_1.md) · [요청](evidence/PH0-INTEGRATION/G0_REVIEW_REQUESTS.md#req-004-decisionoutcome의-불변조건) |
| REQ-005 | G0 독립 검토 / PH0-CONTRACT | HelloParams 및 원시 인증 요청의 Debug·로그 경로에서 토큰 원문 보호 | CONTRACT, HOST, EXT | CONTRACT 처리·INTEGRATION 통합, HOST·EXT 확인 | 완료(`4c14d2a`, 통합 `077bd78`). [판정](evidence/PH0-INTEGRATION/G0_REVISION_1.md) · [요청](evidence/PH0-INTEGRATION/G0_REVIEW_REQUESTS.md#req-005-인증-토큰의-debug로그-보호) |
| REQ-006 | G0 독립 검토 / PH0-CONTRACT | Run 이벤트의 시작 실패·지속형 프로세스·취소·유실·종료 불명 경계 확정 | CONTRACT, HOST, 세 Provider, EXT | CONTRACT·HOST 협의·INTEGRATION 통합 | 완료(`4c14d2a`, 통합 `077bd78`). [판정](evidence/PH0-INTEGRATION/G0_REVISION_1.md) · [요청](evidence/PH0-INTEGRATION/G0_REVIEW_REQUESTS.md#req-006-run-이벤트-종료-규칙) |
| REQ-007 | G0 독립 검토 / PH0-CONTRACT | Local API 주소·상태 디렉터리·토큰 수명·1 MiB 프레임 경계와 상호운용 시험 명세 | CONTRACT, HOST, EXT, INTEGRATION | CONTRACT·HOST·EXT 협의·INTEGRATION 통합 | 완료(`4c14d2a`, 통합 `077bd78`). [판정](evidence/PH0-INTEGRATION/G0_REVISION_1.md) · [요청](evidence/PH0-INTEGRATION/G0_REVIEW_REQUESTS.md#req-007-local-api-주소인증프레임-규칙) |
| REQ-008 | G0 독립 검토 / PH0-CONTRACT | DecisionContext의 최소 목록 검사와 실제 권한 검사의 경계·신뢰된 조회 계약 명시 | CONTRACT, MASTER, HOST | CONTRACT·MASTER 협의·INTEGRATION 통합 | 완료(`4c14d2a`, 통합 `077bd78`). [판정](evidence/PH0-INTEGRATION/G0_REVISION_1.md) · [요청](evidence/PH0-INTEGRATION/G0_REVIEW_REQUESTS.md#req-008-decisioncontext의-권한-검사-범위) |
| REQ-009 | G0 독립 검토 / PH0-CONTRACT | 알 수 없는 Payload 필드의 최상위/중첩 허용 정책과 Schema·Parser·Fixture 일치 | CONTRACT, MASTER, HOST | CONTRACT 처리·INTEGRATION 통합, MASTER 확인 | 완료(`4c14d2a`, 통합 `077bd78`). [판정](evidence/PH0-INTEGRATION/G0_REVISION_1.md) · [요청](evidence/PH0-INTEGRATION/G0_REVIEW_REQUESTS.md#req-009-중첩-payload의-알-수-없는-필드) |
| REQ-010 | PH0-HOST | CLI 공통 탐색의 결과 계약·Provider 규칙 입력·discovery 모듈 등록. Spec §7.5의 경로·출처·버전·해시를 다른 담당자가 같은 형태로 사용하도록 확정 | CONTRACT, HOST, 세 Provider, EXT | CONTRACT 계약·등록, INTEGRATION 조정 | 완료. 계약 `b1e7473`(통합 `3a37919`), HOST 엔진 연결·모듈 등록 `a3f196a`(통합 `685bd96`), 실제 확장 탐지 확인(PH0-CLAUDE). [상세](evidence/PH0-HOST/README.md#req-010-cli-탐색-계약과-모듈-등록) |
| REQ-011 | PH0-HOST | HOST 실행용 Tokio 기능·플랫폼/보안 의존성과 Core Daemon 진입점 연결 준비. 공통 Cargo·Lock·진입점은 INTEGRATION 소유 | HOST, INTEGRATION, 세 Provider, EXT | INTEGRATION 의존성·등록, HOST 구현 | 완료(`1d8734f`). 적용안대로 반영하되 `libc`는 배포 2주 기준으로 0.2.191 대신 0.2.190 고정. 공식 Workspace test 57개·clippy·fmt 통과. [상세](evidence/PH0-HOST/README.md#req-011-실행-의존성과-daemon-진입점-연결) |
| REQ-012 | PH0-CODEX | Codex 전용 구성·실행 승인 Port·cli_rule을 소비자가 사용할 수 있도록 Adapter lib.rs의 `mod codex;`를 `pub mod codex;`로 변경. 기존 Claude 공개 방식과 동일하며 공통 DTO·Trait 변경 없음 | CODEX, INTEGRATION, EXT | CONTRACT 등록·INTEGRATION 조정 | 완료(`64f9674`). `telos_adapters::codex::{CodexConfig, CodexLaunchResolver, cli_rule}` 공개 |
| REQ-013 | PH0-AGY | ① `crates/adapters/src/lib.rs`의 `mod agy;`를 `pub mod agy;`로 변경(Claude·Codex와 동일). ② Spec §7.5에 AGY-DATA-01~05 실환경 검증 결과(기본 `--app_data_dir=antigravity` 사용 + 실행 전 버전 명령으로 옵션 수용 확인, 미수용 시 조용한 대체 없이 `unsupported` 처리) 반영 및 비현실적 '확장 끔' 전제 정리. ③ (참고) VS Code `extensions.json`에서 universal 확장이 `"targetPlatform": "undefined"`로 기록되므로 `discovery/mod.rs`에서 `"universal"`과 함께 허용 검토 | AGY, INTEGRATION, HOST | CONTRACT 등록·INTEGRATION 조정 | 완료. ①② `286844a`(Spec v1.4.2 §7.5), ③ HOST `4791c5d`(통합 `b561da2`): `targetPlatform: "undefined"`를 universal로 처리, 실제 Antigravity가 `metadata`로 해석됨을 통합본에서 재확인. [근거](evidence/PH0-AGY/README.md) §3·§5 |
| REQ-014 | PH0-CODEX | 세 Provider가 Stub을 교체한 뒤 `crates/adapters/src/unimplemented.rs`에 남은 미사용 `unimplemented_adapter!` 매크로와 재수출 제거. `UnsupportedAdapter`·공통 계약 시험은 유지. 두 unused lint가 Codex 작업 Branch의 엄격 Clippy를 차단 | CONTRACT, CODEX, INTEGRATION | CONTRACT 정리·INTEGRATION 조정 | 완료(`64f9674`). 매크로·재수출 제거, `UnsupportedAdapter`·계약 시험 유지. 엄격 Clippy 통과 |
| REQ-015 | PH0-HOST | INTEGRATION 소유 `crates/core/tests/phase0_g1.rs`의 자동 판정 강화: 소문자 running 비교를 계약 RunStatus의 정확한 최종 상태 검사로 변경, 취소 accepted·실제 cancelled, Daemon 종료 전 Running·취소 수 1·종료 후 Cancelled 검사. 현재 약한 조건은 미종료/취소 미재현도 통과시킬 수 있음 | INTEGRATION, HOST, CLAUDE | INTEGRATION 시험 수정·실환경 재검증 | 완료(`399992d`). `RunStatus` 계약 값 정확 비교·제한 시간 재조회, 취소 accepted·실제 cancelled·종료 원인·회수, Daemon 종료 전 Running·취소 1·종료 후 Cancelled. 실환경 5개(haiku) 재통과, 세션 4개는 Archive 목록에 추가(INTEGRATION 증거 §7.5) |

요청에는 필요한 타입·메서드·의존성과 호환·시험 계획을 포함합니다. 통합 담당과 공통 계약 담당이 먼저 기준 변경을 통합하고 소비 작업에 전달합니다. 소비 작업에서 임시 공통 타입을 만들거나 같은 파일을 각자 수정하여 우회하지 않습니다.

REQ-002~009는 독립 검토의 제출안이며 `dcad086`으로 접수했습니다. 판정과 처리 방식은 [G0 개정 1](evidence/PH0-INTEGRATION/G0_REVISION_1.md)에 기록합니다. 요청자와 확인 사실·설계 제안·호환·시험 계획은 [G0 독립 검토 변경 요청서](evidence/PH0-INTEGRATION/G0_REVIEW_REQUESTS.md)에 기록합니다. 처리 결과 계약 기준을 `077bd78`로 개정했습니다. 소비 작업은 이 기준으로 착수·갱신합니다.

## 5. G0 계약 범위와 등록 경로

Spec §19.5.2의 계약을 아래 위치에 정의했으며 `9710402`로 확정하고 `077bd78`(G0 개정 1)로 개정했습니다. 이후 변경은 §4의 변경 요청으로만 처리합니다.

| 계약 | 정의 위치 | 작성 | 사용 작업 |
|---|---|---|---|
| Adapter 실행 | `telos_schema::{TaskSpec, RunContext, RunStarted, FailureKind}`, `telos_adapters::{AgentAdapter, AdapterError}` | CONTRACT | HOST, 세 Provider, MASTER |
| 이벤트·취소 | `telos_schema::ProviderEvent`와 kind별 Payload, `AgentAdapter::{events, cancel_run, inspect_run}` 문서 계약 | CONTRACT | HOST, 세 Provider, INTEGRATION |
| 공용 프로세스 경계 | `telos_adapters::runtime` 공개 인터페이스 (G0 Stub: `UnimplementedRuntime`, 구현 `NativeRuntime`) | INTEGRATION 등록, 이후 HOST 구현 | HOST, 세 Provider |
| CLI 탐지 (REQ-010) | `telos_schema::{CliDiscovery, CliDiscoveryOutcome, DiscoveredCli, ...}`, `telos_adapters::{ProviderCliRule, CliBinaryRule, DiscoveryRequest, CliDiscoverer}` | CONTRACT, 구현 HOST | HOST, 세 Provider, EXT |
| 마스터 결정 | `telos_schema::{MasterDecision, DecisionPayload, DecisionContext, DecisionOutcome}` | CONTRACT | MASTER, HOST |
| Local Core API | `telos_schema::api` (전송·인증 규칙은 모듈 문서) | CONTRACT | HOST, EXT |
| RN RPC | `telos_schema::rn`, `telos-rn-client`가 재노출 | CONTRACT 정의, INTEGRATION 등록 | RN, INTEGRATION |
| 생성·시험 | `schemas/*.schema.json`, `telos_schema::fixtures`(`fixtures` feature), `telos_adapters::UnsupportedAdapter` 시험 대역, 증거 형식 `evidence/PH0-INTEGRATION/EVIDENCE_TEMPLATE.md` | CONTRACT, INTEGRATION | 전체 |

G0에서 등록한 Stub 경로입니다. 등록된 파일은 이제 해당 작업의 독점 경로입니다.

| 경로 | 이후 소유 | 내용 |
|---|---|---|
| `crates/adapters/src/runtime/mod.rs` | HOST | 공용 프로세스 런타임 공개 인터페이스, `UnimplementedRuntime` |
| `crates/core/src/api/mod.rs`, `crates/core/src/execution/mod.rs` | HOST | Local API 서버·실행 감독 (`1d8734f`) |
| `crates/core/src/master/mod.rs`, `crates/core/src/policy/mod.rs` | MASTER | 모듈 문서만 |
| `crates/rn-client/src/lib.rs` | RN | `telos_schema::rn` 재노출 |

의존성 버전은 루트 `Cargo.toml`의 `[workspace.dependencies]`에서만 정합니다.

## 6. 환경·운영 결정

| 항목 | 결정 | 상태 | 결정일 |
|---|---|---|---|
| 줄바꿈 | 저장소 전체 LF 고정, `.cmd`·`.bat`만 CRLF (`.gitattributes`) | 적용 | 2026-10-24 |
| Linux·Remote SSH 실행 Host | 같은 PC의 WSL2 Ubuntu 26.04를 Linux Host 겸 Remote SSH Host로 사용. 절차는 [WSL_LINUX_HOST.md](WSL_LINUX_HOST.md). G3 필수 조건이므로 미루지 않음 | 절차 수립, 배포판 미설치. 구성 시점: PH0-HOST가 공용 런타임 구현에 착수할 때(늦어도 G2 전) | 2026-10-24 |
| CI | 원격 저장소는 생겼으나(2026-10-24) 사용자 결정으로 계속 보류. 도입 전까지는 통합 담당이 Spec §19.5.8의 검사 명령을 수동 실행하고 증거에 기록 | 보류 | 2026-10-24 |
| 대상 OS 우선순위 | 구현·일상 검증은 Windows 로컬 우선. Linux·Remote SSH는 G3 전에 WSL로 확보 | 적용 | 2026-10-24 |
| G1 대표 Provider | Claude Code. Spec §19.5.6의 기본값(Codex)에서 변경. 이유: PH0-CODEX 미배정, 통합 담당이 PH0-CLAUDE를 함께 맡음. 마스터 Provider 선택(MASTER-01)과는 무관 | 적용 | 2026-10-24 |
| Claude 실환경 시험 조건 | 모델 `haiku`, 짧은 프롬프트. 설정 디렉터리는 기본 `~/.claude`(확장과 공유, 토큰 복사·별도 로그인 없음). 실행은 저장소 밖 일회용 폴더에서만, 허용 도구 명시(`--allowedTools`/`--disallowedTools`, `--permission-mode`), `--dangerously-skip-permissions` 금지. 시험 세션은 "시험 세션 정리" 규칙을 따름. 기존 대화는 재개·수정하지 않음 | 적용 | 2026-10-24 |
| 마스터 Provider (Phase 0) | Claude Code. 임시 선택이며 MASTER-01에 따라 Provider를 고정하지 않고 교체 가능하게 구현. 시험 조건은 Claude 실환경 시험 조건을 따름 | 적용 | 2026-10-24 |
| 마스터 Phase 0 범위 | 시험 모델 `haiku`. 실행: `create_tasks`·`request_run`·`request_cancel`. 나머지 5종은 Payload 검사 후 `unsupported_kind`. 결과 왕복(Resume) 시험 1개. 마스터 Run은 도구 없음·읽기 전용 | 적용 | 2026-10-24 |
| 시험 세션 정리 | 특별한 사유가 없으면 실환경 시험 세션은 해당 코드를 `main`에 병합하는 시점에 삭제합니다(통합 담당, 병합 후 Branch·Worktree 정리와 함께). Claude: `~/.claude/projects/<시험 작업 폴더>` 삭제. Codex: 내부 SQLite가 Thread를 색인하므로 파일 삭제 대신 App Server `thread/archive`. Antigravity: `brain/<id>`, `conversations/<id>.db`(`-wal`·`-shm`). 시험은 저장소 밖 임시 폴더를 작업 폴더로 써서 사용자 대화 목록과 섞이지 않게 하고, 세션 ID는 증거에 남깁니다. 시험이 아닌 세션은 건드리지 않습니다 | 적용 | 2026-10-24 |

## 7. 변경 기록

| 일시 (KST) | 기록자 | 변경 | 기준 Commit |
|---|---|---|---|
| 2026-10-24 01:05 | `claude-opus-5.5/session-c9336d2c` | 사용자 지정으로 통합 담당 인수. PH0-INTEGRATION `unassigned → in-progress`, PH0-CONTRACT `unassigned → reserved`, G0 `대기 → 준비 중`. 대표 저장소 기록 | `3f050e9` |
| 2026-10-24 01:23 | `claude-opus-5.5/session-c9336d2c` | G0 준비: 의존성 버전을 `[workspace.dependencies]`로 일원화, `telos-schema` `fixtures` feature 추가, §5의 Stub 경로 등록, G0 계약 범위 기록 | `1dfde9b` |
| 2026-10-24 01:24 | `claude-opus-5.5/session-c9336d2c` | PH0-CONTRACT `reserved → in-progress`. Worktree `D:\worktree\TelosWeaver\ph0-contract`, Branch `ph0/contract` 생성 | `2d0179a` |
| 2026-10-24 01:34 | `claude-opus-5.5/session-c9336d2c` | `ph0/contract`(`ade440e`)를 `f1ee458`로 통합, RN Client 재노출 등록(`9710402`), `main`에서 재검증. PH0-CONTRACT `in-progress → verified`, G0 `준비 중 → 통과`, 기준 Commit `9710402` 확정. 나머지 작업 인계 갱신, REQ-001 등록, Worktree 정리 | `9710402` |
| 2026-10-24 12:25 | `claude-opus-5.5/session-c9336d2c` | 사용자 결정 반영: REQ-001 완료(LF 고정 `.gitattributes`), Linux·Remote SSH Host WSL 절차 수립, CI 보류. §6 환경·운영 결정 추가. Spec v1.3.4로 경로 등록. 공통 계약 변경 없음 | `9710402` |
| 2026-10-24 12:29 | `claude-opus-5.5/session-c9336d2c` | 사용자 결정: WSL 배포판을 Ubuntu 24.04에서 26.04(`Ubuntu-26.04`)로 변경, Host ID `wsl-ubuntu-2604`. 배포판 미설치 상태 유지 | `9710402` |
| 2026-10-24 12:32 | `claude-opus-5.5/session-c9336d2c` | 사용자 결정: WSL 구성 시점을 PH0-HOST 공용 런타임 구현 착수 시(늦어도 G2 전)로 지정. G0 계약은 다른 에이전트가 검토 중이며 결과는 §4 변경 요청으로 받음 | `9710402` |
| 2026-10-24 (시각 미기록, 제출자 기록) | `Codex / G0-REVIEW-20261024` | 사용자 요청으로 G0 독립 검토의 REQ-002~009 등록안·상세 요청서 작성. 별도 검토 Branch에서 제출하며 대표 저장소 반영·채택·구현·기준 변경은 기존 통합 담당이 처리 | `9710402` (검토 대상), `cfc6a2e` (제출안 기반) |
| 2026-10-24 12:59 | `claude-opus-5.5/session-c9336d2c` | 검토 제출안을 `dcad086`으로 접수. REQ-002~006·009 채택, REQ-007·008 수정 채택([G0 개정 1](evidence/PH0-INTEGRATION/G0_REVISION_1.md)). 처리 순서: INTEGRATION 런타임 인터페이스 → CONTRACT → 재통합 | `9710402` |
| 2026-10-24 13:02 | `claude-opus-5.5/session-c9336d2c` | G0 개정 1 INTEGRATION 부분 완료(`00cf7ea`, REQ-002·003 인터페이스). PH0-CONTRACT `verified → in-progress`, Worktree `D:\worktree\TelosWeaver\ph0-contract` 재생성. 개정 기준 확정 전까지 `9710402`가 유효 기준 | `00cf7ea` |
| 2026-10-24 13:11 | `claude-opus-5.5/session-c9336d2c` | `ph0/contract`(`4c14d2a`)를 `077bd78`로 통합, `main`에서 재검증(27개 통과). 계약 기준 `9710402 → 077bd78`, PH0-CONTRACT `in-progress → verified`, REQ-004~009 완료, REQ-002·003 계약 완료(HOST 구현 대기). Worktree 정리 | `077bd78` |
| 2026-10-24 13:21 | `claude-opus-5.5/session-c9336d2c` | 사용자 결정 DEC-06 반영(Spec v1.3.5): Provider CLI를 VS Code 확장에서 탐지. HOST에 공통 탐색 경로 추가, 세 Provider 인계에 확장 실행 파일 기록, WSL 절차 §2.9를 원격 확장 기준으로 수정. 공통 계약 코드 변경 없음 | `077bd78` |
| 2026-10-24 13:38 | `claude-opus-5.5/session-c9336d2c` | Spec v1.3.6: 확장 탐지를 버전 비의존 규칙으로 변경, Antigravity 실행 파일을 `~/.gemini/bin/agy`로 확정(사용자의 중복 설치 정리 후 재확인). AGY 인계·WSL 절차 갱신 | `077bd78` |
| 2026-10-24 13:42 | `claude-opus-5.5/session-c9336d2c` | Spec v1.3.7: Antigravity 데이터 디렉터리 검증(AGY-DATA-01~05)을 §7.5에 추가하고 PH0-AGY 첫 실험으로 인계. PH0-AGY는 다른 에이전트가 맡을 예정(사용자 지정, 아직 미배정) | `077bd78` |
| 2026-10-24 (시각 미기록) | `Codex / PH0-HOST-20261024` | 사용자 직접 배정으로 PH0-HOST `unassigned → reserved` 기록. Branch/Worktree와 준비·범위 기록, REQ-010·011 제출. 코드 구현·공통 계약 변경·WSL 구성은 아직 시작하지 않음. 기존 통합 담당과 G0 기준은 유지 | `f939abb` (준비 기반), `077bd78` (계약) |
| 2026-10-24 (시각 미기록) | `Codex / PH0-HOST-20261024` | 사용자 착수 지시로 PH0-HOST `reserved → in-progress`. 공용 런타임과 탐색 엔진을 소유 경로에 구현하며 REQ-010·011 공통 변경은 별도 적용안으로 준비. 공식 Workspace 연결 전 독립 Manifest로 검증하며 G0 기준·타 담당 소유권 유지 | `5d7f470` (착수 기반), `077bd78` (계약) |
| 2026-10-24 16:48 | `claude-opus-5.5/session-c9336d2c` | `ph0/host`(`29295c9`)와 REQ-011을 한 통합 단위로 `1d8734f`에 반영. `libc` 0.2.190 고정(2주 기준), Core `serve` 진입점(기본 `DenyRuns`). 공식 Workspace test 57개·clippy·fmt 통과. PH0-HOST는 REQ-010 연결·Linux 검증이 남아 `in-progress` 유지. REQ-010 채택, PH0-CONTRACT `verified → in-progress` | `1d8734f` |
| 2026-10-24 17:10 | `claude-opus-5.5/session-c9336d2c` | `ph0/contract`(`b1e7473`)를 `3a37919`로 통합, `main`에서 재검증(61개 통과, clippy·fmt). 계약 기준 `077bd78 → 3a37919`, REQ-010 계약 완료(HOST 엔진 연결 대기), PH0-CONTRACT `in-progress → verified`, Worktree 정리. G1은 PH0-CODEX 배정과 신뢰된 `RunAuthorizer` 연결(MASTER·INTEGRATION)이 필요하고, Linux 검증은 WSL 배포판 설치(사용자 수행)가 필요 | `3a37919` |
| 2026-10-24 17:20 | `claude-opus-5.5/session-c9336d2c` | 사용자 결정(Spec v1.3.8): 담당 에이전트가 자기 작업 행·변경 요청 제출·변경 기록을 직접 갱신. 기준·게이트·판정·`verified` 전환은 통합 담당 유지. §1 규칙 갱신. 이전 HOST의 직접 기록(`5d7f470`, `4b84405`)은 이 규칙에 부합 | `3a37919` |
| 2026-10-24 17:30 | `claude-opus-5.5/session-c9336d2c` | 사용자 지시로 Spec을 v1.4로 승격: `docs/DEVELOPMENT_v1.3.md` → `docs/DEVELOPMENT_v1.4.md`, 개정 1.3.8 원문을 `docs/archive/DEVELOPMENT_v1.3.md`로 보관. 저장소 안 링크 갱신(PH0-HOST·G0 검토 증거는 링크 경로만 수정). 계약 변경 없음 | `3a37919` |
| 2026-10-24 (시각 미기록) | `Codex / PH0-HOST-20261024` | 사용자 지시로 후속 작업 1~3 착수: 최신 main 반영, REQ-010 엔진 연결, 공개 API 시험 전환. HOST 기준을 Spec v1.4·계약 `3a37919`로 갱신. WSL Linux·Remote SSH 실행 검증은 사용자 지시로 대기하며 게이트·다른 작업 상태는 변경하지 않음 | `3e4a9b7` |
| 2026-10-24 17:27 | `claude-opus-5.5/session-c9336d2c` | 사용자 배정으로 PH0-CLAUDE `unassigned → in-progress`(Worktree `D:\worktree\TelosWeaver\ph0-claude`, Branch `ph0/claude`). 사용자 결정: G1 대표 Provider를 Codex에서 Claude로 변경(이유: PH0-CODEX 미배정, 통합 담당이 CLAUDE를 함께 맡아 G1을 바로 판정 가능). §6에 Claude 실환경 시험 조건 추가. 배정표 표 중간의 빈 줄 제거 | `eb642fd` |
| 2026-10-24 17:26 | `gemini-4-argon/session-9e6137eb` | 사용자 직접 배정으로 PH0-AGY `unassigned → reserved` 기록. Branch `ph0/agy`·Worktree `D:\worktree\TelosWeaver\ph0-agy` 예정, 기준 Commit `eb642fd`·계약 기준 `3a37919` 기록. 코드 구현 및 AGY-DATA-01~05 실험 착수 전 사전 결정 사항 확인 대기 | `eb642fd` (준비 기반), `3a37919` (계약) |
| 2026-10-24 17:33 | `gemini-4-argon/session-9e6137eb` | 사용자 착수 지시로 PH0-AGY `reserved → in-progress`(Worktree `D:\worktree\TelosWeaver\ph0-agy`, Branch `ph0/agy`). 사용자 결정: 확장 활성 상태에서 AGY-DATA-01~05 바로 검증, 경량 Flash 모델·`--effort low`·일회용 임시 폴더 사용, 테스트 세션은 실험 후 정리(기존 대화 보존) | `689d43e` (착수 기반), `3a37919` (계약) |
| 2026-10-24 (시각 미기록) | `Codex / PH0-HOST-20261024` | 요청 1~3 구현·검증 완료 제출. 코드 `a3f196a`, 검증·인계 `fa7facf`. 공개 CliDiscoverer 연결·DTO/실패 분류·기록 필드 연결, 공개 API 탐색 시험 17개·공식 Workspace 71개·clippy·fmt 통과. HOST 전체는 후속 통합·Linux 검증이 남아 in-progress 유지. 4번은 사용자 지시로 대기하며 다른 담당의 변경과 게이트 판정은 유지 | `eb642fd` (구현 기준), `3a37919` (계약) |
| 2026-10-24 17:42 | `claude-opus-5.5/session-c9336d2c` | `ph0/claude`(`a3027c3`)를 `3bdb808`, `ph0/host`(`fa7facf`)를 `685bd96`, 탐지 실환경 시험(`62c86af`)을 `b8e3fa9`로 통합. `main`에서 85개 통과(실환경 6개 무시), clippy·fmt. REQ-010 완료, PH0-CLAUDE `in-progress → integrated`. G1은 Core 연결 후 판정 | `b8e3fa9` |
| 2026-10-24 17:54 | `claude-opus-5.5/session-c9336d2c` | G1 Core 연결 착수(INTEGRATION). Worktree `D:\worktree\TelosWeaver\ph0-integration`, Branch `ph0/integration-g1`. 범위: 운영자 설정 기반 `RunAuthorizer`(Phase 0, 정식 정책은 MASTER), `ClaudeLaunchResolver` Core 구현, 탐지 결과로 Claude Adapter 구성, Daemon 종료 시 활성 Run 취소·종료 확인, Local API 경유 G1 실환경 시험. Spec v1.4.1로 `crates/core/src/integration/**` 등록 | `611032a` |
| 2026-10-24 (시각 미기록) | `Codex / PH0-CODEX-20261024` | 사용자 직접 배정으로 PH0-CODEX `unassigned → reserved`. 전용 Branch/Worktree와 기준 기록. 현재 설치된 Codex CLI의 버전·App Server 도움말만 확인했으며 코드 구현·실계정 모델 호출은 시작하지 않음. 실환경 시험 조건과 준비 사항을 정리하고 G1 대표·HOST·게이트 판정은 유지 | `0e46f49` (준비 기준), `3a37919` (계약) |
| 2026-10-24 18:03 | `claude-opus-5.5/session-c9336d2c` | G1 Core 연결 `ph0/integration-g1`(`7156435`)을 `cfe1b19`로 통합. `main`에서 91개 통과(실환경 11개 무시), clippy·fmt. G1 실환경 5개(haiku) 통과로 G1 `대기 → 통과 판정(HOST 확인 대기)`. HOST 행에 확인 요청 기록 | `cfe1b19` |
| 2026-10-24 (시각 미기록) | `Codex / PH0-CODEX-20261024` | 사용자 권장안 승인으로 PH0-CODEX reserved → in-progress. 실환경 범위·경량 모델/low·시험 세션 Archive 조건 확정. REQ-012 모듈 공개 요청을 제출하고 Codex 전용 구현·검증 착수. 기존 HOST·게이트 판정과 Linux 대기 지시는 유지 | `7a9d4b3`, 계약 `3a37919` |
| 2026-10-24 18:16 | `gemini-4-argon/session-9e6137eb` | PH0-AGY 구현·검증 완료로 `in-progress → review` 전환(`ph0/agy` 커밋 `aa21611`). AGY-DATA-01~05 및 PROV-01~09·FORK-POC-01 실환경 검증 완료(Mock 14개·실환경 6개 통과, 시험 세션 정리 완료). REQ-013(`pub mod agy;` 및 Spec §7.5 확정 규칙 반영) 제출 | `611032a`, 계약 `3a37919` |
| 2026-10-24 18:25 | `claude-opus-5.5/session-c9336d2c` | `ph0/agy`(`aa21611`) 검토·통합(`286844a`): 소유 경로만 변경, 권한 우회 차단·stderr 비복사·소프트 거부 실패 판정 확인. REQ-013 ① `pub mod agy`, ② Spec v1.4.2 §7.5 반영, ③ HOST 전달. `main` 106개 통과(실환경 17개 무시), clippy·fmt, 병합본 AGY 탐지 재확인. PH0-AGY `review → integrated` | `286844a` |
| 2026-10-24 18:28 | `claude-opus-5.5/session-c9336d2c` | 사용자 결정(Spec v1.4.3): 병합 직후 Branch·Worktree 삭제를 기본 동작으로 지정. 병합된 `ph0/agy`·`ph0/claude`·`ph0/host`·`ph0/integration-g1`·`ph0/contract`·`review/g0-contract-requests-20261024` Branch와 Worktree 삭제(AGY Worktree의 미커밋 변경은 파일 끝 빈 줄뿐이라 폐기). HOST·AGY가 후속 작업을 하면 최신 `main`에서 새로 생성. `ph0/codex`는 미병합으로 유지 | `2891f47` |
| 2026-10-24 (시각 미기록) | `Codex / PH0-CODEX-20261024` | 사용자 삭제 후 남아 있던 ph0/codex(`83cf156`)를 원래 Worktree에 복구. 구현 `0dcc1ca` 보존 확인, 인계·공통 변경 적용안 `72a4679` 제출로 PH0-CODEX in-progress → review. Mock 12개·실환경 7개 통과와 시험 세션 Archive 완료 기록. 복구 후 Workspace 118개 통과·fmt 통과, 엄격 Clippy는 공통 미사용 Stub 매크로 2건으로 실패하여 REQ-014 제출. REQ-012 공개·Core 연결·통합 판정은 기존 담당 유지 | `72a4679`, 계약 `3a37919` |
| 2026-10-24 19:01 | `claude-opus-5.5/session-c9336d2c` | `ph0/codex`(`e8dbe84`) 검토·통합(`64f9674`): 소유 경로만 변경, Sandbox read-only/workspace-write·네트워크 차단·기본 승인 거부·소유 Thread만 Archive 확인. REQ-012·REQ-014 적용(`COMMON_CHANGES.patch`). `main` 118개 통과(실환경 24개 무시), clippy `-D warnings`·fmt, 병합본 Codex 탐지 재확인. PH0-CODEX `review → integrated`. `ph0/codex` Branch·Worktree 삭제 | `64f9674` |
| 2026-10-24 (시각 미기록) | `Codex / PH0-CODEX-20261024` | 사용자 현황 최신화 요청으로 main `86f580e`의 통합 상태와 실제 코드를 재확인. Workspace 118개·엄격 Clippy·fmt 재통과. PH0-CODEX integrated·REQ-012/014 완료 판정은 유지하고, 자기 검증·인계 문서의 통합 전 현재 상태 표기를 갱신할 전용 Branch/Worktree를 새로 생성. 타 작업·게이트 판정은 변경하지 않음 | `86f580e` |
| 2026-10-24 (시각 미기록) | `Codex / PH0-CODEX-20261024` | 자기 증거 README·HANDOFF를 통합 완료 상태로 갱신한 `1aa8d67` 제출. 통합 전 Clippy 실패·패치 제출은 이력으로 보존하고 현재 엄격 검사 통과를 구분. PH0-CODEX 구현 integrated 상태와 타 작업·게이트 판정 유지. 문서 갱신 Branch/Worktree는 통합 담당 반영 전까지 보존 | `1aa8d67` |
| 2026-10-24 (시각 미기록) | `Codex / PH0-HOST-20261024` | 사용자 지시로 G1 HOST 검토와 REQ-013 ③ 처리 착수. 최신 main `3fe9153`에서 ph0/host·전용 Worktree를 새로 만들고 G1 취소·자식 회수·실행 감독 상태를 검토하며 universal 확장의 undefined 메타데이터를 시험·수정. Linux·Remote SSH 대기와 통합 담당의 게이트·요청 판정 권한 유지 | `3fe9153`, 계약 `3a37919` |
| 2026-10-24 19:13 | `claude-opus-5.5/session-c9336d2c` | 문서 보정: PH0-CODEX 증거 갱신 `1aa8d67`를 `e55c0b2`로 통합하고 `ph0/codex` Branch·Worktree 삭제. INTEGRATION 증거 §5(G0 시점 계획, 대표 Codex)를 시점 기록으로 표시하고 §8에 G2 현황·Codex·Agy Core 연결 계획 추가. G2 행에 Provider별 Core 연결 현황, INTEGRATION 행 다음 작업 정리 | `e55c0b2` |
| 2026-10-24 (시각 미기록) | `Codex / PH0-HOST-20261024` | G1 HOST 확인·REQ-013 ③ 구현·검증 완료 제출(코드 `4791c5d`, 인계 `b956448`). 실제 Agy metadata 탐지, Windows EOF/Job 강제 종료·자식 핸들 확인, 취소 경합·종료 불명 회귀 시험 추가. Workspace 123개·엄격 Clippy·fmt 통과. G1 실환경 시험의 자동 판정 강화 REQ-015 제출. 게이트·REQ-013 완료 반영은 통합 담당에게 인계하며 Linux·Remote SSH 대기 유지 | `b956448`, 계약 `3a37919` |
| 2026-10-24 19:30 | `claude-opus-5.5/session-c9336d2c` | `ph0/host`(`cbb1b4f`, 코드 `4791c5d`) 검토·통합(`b561da2`): 취소 수락이 실제 완료·종료 불명을 덮어쓰던 Supervisor 판정 보정, `undefined` = universal. REQ-015로 G1 시험 강화 후 실환경 5개 재통과(`399992d`). G1 `통과 판정(HOST 확인 대기) → 통과 확정`, REQ-013·REQ-015 완료. `ph0/host`·`ph0/integration-req015` Branch·Worktree 삭제 | `399992d` |
| 2026-10-24 19:44 | `claude-opus-5.5/session-c9336d2c` | G2 Codex·Agy Core 연결 착수(INTEGRATION, 사용자 지시). Worktree `D:\worktree\TelosWeaver\ph0-integration`, Branch `ph0/integration-g2`. 범위: 운영자 정책 `codex`·`agy` 절, `CodexLauncher`·`AgyLauncher`, Provider별 탐지·구성, 종료 정리, Local API 경유 G2 실환경 시험(Codex `gpt-6-luna`/low, Agy `gemini-3.8-flash-low`/low, Claude haiku). 계획: INTEGRATION 증거 §8 | `ce23c3e` |
| 2026-10-24 20:12 | `claude-opus-5.5/session-c9336d2c` | G2 Codex·Agy Core 연결 `ph0/integration-g2`(`8a5e64b`)을 `aef3297`로 통합. 정책 Provider별 절·`CodexLauncher`·`AgyLauncher`·세 Provider 탐지·등록. G2 실환경 4개·G1 실환경 5개 통과, Workspace 127개(실환경 28개 무시)·clippy·fmt. 발견: Antigravity가 확장 길이 경로를 작업 영역으로 인식하지 못해 scratch에 씀(정규화·`--add-dir`로 해결), Codex 스트림의 `slow_consumer`(재구독), agy 대화 파일 `.db`. AGY·HOST·EXT 행에 전달. Branch·Worktree 삭제 | `aef3297` |
| 2026-10-24 20:31 | `claude-opus-5.5/session-c9336d2c` | 사용자 배정으로 PH0-MASTER `unassigned → reserved`. 사용자 결정: Phase 0 마스터 Provider는 임시로 Claude(§6, 교체 가능). 나머지 착수 결정 확인 후 `in-progress` 전환 | `a9bc7c2` |
| 2026-10-24 20:38 | `claude-opus-5.5/session-c9336d2c` | PH0-MASTER `reserved → in-progress`. 사용자 결정(추천안 승인): 시험 모델 haiku, 실행 kind 3종, 결과 왕복 시험 1개, 마스터 Run 도구 없음. §6 기록 | `4b6f430` |
| 2026-10-24 20:50 | `claude-opus-5.5/session-c9336d2c` | `ph0/master`(`fcf6def`)를 `e14de42`로 통합. `main` 142개 통과(실환경 30개 무시)·clippy·fmt, 실제 Claude 마스터 실환경 2개 통과. PH0-MASTER `in-progress → integrated`. G2 남은 조건은 RN·EXT. Branch·Worktree 삭제 | `e14de42` |
| 2026-10-24 (시각 미기록) | `Codex / PH0-HOST-20261024 · PH0-CODEX-20261024` | 사용자 현황 최신화 요청으로 main `089d0b3`의 HOST·REQ-013/015 완료, G1 확정, 세 Provider Core 연결과 MASTER 통합을 코드·증거로 재확인. Workspace 142개·엄격 Clippy·fmt 재통과(실환경 30개 기본 제외, 모델 호출 재실행 없음). 자기 HOST·CODEX 행에 확인 결과를 추가하고 CODEX 증거 갱신의 과거 반영 대기 표현 정리. 타 작업·게이트 판정과 Linux 대기는 유지 | `089d0b3` |
| 2026-10-24 21:21 | `claude-opus-5.5/session-c9336d2c` | 사용자 지시: 지금까지의 시험 세션 삭제(Claude 33개·Antigravity 1개 삭제, Codex 잔여 2개 Archive), 앞으로는 병합 시점에 삭제하는 규칙을 §6에 추가. PH0-CLAUDE·MASTER·INTEGRATION 증거의 세션 목록을 삭제 완료로 표기 | `089d0b3` |
| 2026-10-24 21:51 | `gemini-4-argon/session-9e6137eb` | 사용자 현황 최신화 요청으로 `main`(`86ef048`)의 PH0-AGY 통합(`286844a`), REQ-013 ①②③ 완료(`286844a`·`b561da2`), Core 연결(`aef3297`), G1 확정(`399992d`), CODEX·MASTER 통합을 확인하고 Workspace 142개·엄격 Clippy·fmt 재통과 확인. INTEGRATION §8.4 전달 사항에 따라 PH0-AGY 시험이 남긴 `conversations/<id>.db` 16개 삭제 완료 및 PH0-AGY 행 갱신 | `86ef048` |
| 2026-10-24 22:48 | `claude-opus-5.5/session-c9336d2c` | 공개 저장소 준비(사용자 지시, push 전): `git filter-repo`로 전체 이력에서 `docs/archive/` 제거, 작성자·커미터 이메일과 파일 내용의 이메일을 GitHub 비공개 주소로 변경. **모든 Commit 해시가 바뀌었으며** 배정표·증거 문서의 해시 354곳을 대응표로 갱신. `docs/archive/`는 로컬 보관(`.gitignore`), Spec·AGENTS·README의 archive 링크를 로컬 보관 표기로 변경. 재작성 전 원본 백업 `D:\worktree\backup\TelosWeaver-pre-public-20261024.git`은 2026-10-24 23:15 사용자 지시로 삭제(복구 불가) | `f100687`(재작성 후) |
| 2026-10-24 22:58 | `claude-opus-5.5/session-c9336d2c` | 사용자 결정: `MIT OR Apache-2.0` 이중 라이선스 추가(`eadbd95`), 공개 저장소 https://github.com/blazepsyki/TelosWeaver 생성·`main` push. 대표 저장소 관리 항목·CI 결정(계속 보류)·WSL 동기화 절차 갱신. 앞으로 통합 담당이 병합 후 `main`을 push | `eadbd95` |
| 2026-10-24 23:17 | `claude-opus-5.5/session-c9336d2c` | 사용자 지시: 이력 재작성 전 원본 백업 `D:\worktree\backup\TelosWeaver-pre-public-20261024.git` 삭제(개인 이메일이 담긴 마지막 로컬 사본). 22:48 행의 백업 경로를 삭제 완료로 표기. `D:\worktree\backup\docs-archive-20261024`는 유지 | `3757f7e` |
| 2026-10-24 (시각 미기록) | `Codex / PH0-EXT-20261024` | 사용자 직접 배정으로 PH0-EXT unassigned → reserved. Branch ph0/ext·전용 Worktree와 기준 main 207c125·계약 3a37919를 기록. 구현 전 Core 실행 파일·시작 정책·인증/주소·UI·시험 조건을 확인하며 코드 구현은 시작하지 않음. RN·HOST·게이트 판정과 Linux 대기 지시는 유지 | `207c125` |

PH0-INTEGRATION은 배정표 관리 자체가 작업이므로 G3까지 `in-progress`로 유지합니다. PH0-CONTRACT는 같은 담당자가 별도 Worktree의 `ph0/contract`에서 진행했고, 통합 후 Worktree를 정리했습니다. 이후 계약 변경 요청이 오면 새 Worktree를 만들어 같은 규칙으로 처리합니다.

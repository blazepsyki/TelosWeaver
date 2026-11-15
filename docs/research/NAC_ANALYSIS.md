# NAC 프로젝트 분석과 TelosWeaver 적용 검토

- 검토일: 2026-11-15 (KST)
- 분석 대상: [arcee-ai/nac](https://github.com/arcee-ai/nac)
- NAC 기준: 기본 브랜치 `dev`, Commit `7d8b9696d0fab130e120647ca6fabf001e673908`
- TelosWeaver 기준: Commit `29077ee366b5b8e5cc74ebcec636387b40bd9955`
- 구현 기준: [DEVELOPMENT_v1.5.md](../DEVELOPMENT_v1.5.md) v1.5.11
- 문서 성격: 연구 검토와 적용 제안입니다. 기준 Spec, H0 판정, 작업 배정, 지원 기능을 변경하지 않습니다.
- 표기: **소스 확인**은 해당 Revision의 코드·문서에서 확인한 사실, **적용 제안**은 이를 바탕으로 한 설계 판단입니다. 소스에 있는 시험을 읽었으며 빌드·시험 실행이나 실제 Provider 검증은 하지 않았습니다.

## 1. 판단 요약

NAC에서 우선 참고할 부분은 **결과의 영속 확정, 응답 유실 후 재전달, 오래된 실행 결과 차단, 문맥 선별, 장애 시점별 시험**입니다.

TelosWeaver에는 Master와 Core의 책임 분리, TaskResult, CapsuleContent, ContextSnapshot, RunIntent, JournalEntry, 멱등 처리 등의 계약이 이미 있습니다. NAC의 개념을 별도 도메인으로 추가하기보다 기존 Phase 1 계약의 구현과 수용 시험을 구체화하는 데 활용하는 것이 적합합니다.

권장 순서는 다음과 같습니다.

1. 결과·멱등 기록·Journal을 확정하는 경계와 응답 유실 처리를 구체화합니다.
2. 확정 결과와 근거에서 Capsule을 구성하고 입력 Version을 고정합니다.
3. DB 단일 소유권과 저장 작업 수의 상한, 종료 시 처리 순서를 정합니다.
4. 사용자 후속 메시지의 접수와 마스터 입력 전달을 구분합니다.
5. 커밋 전후·응답 전후·취소 경합의 장애 시험을 구현합니다.

DAG 병렬화는 기존 P1 범위를 유지합니다. NAC의 직접 모델 호출, 자체 파일·터미널 도구, Direct 모드, Managed 배포를 TelosWeaver에 함께 도입할 필요성은 이번 검토에서 확인하지 못했습니다.

## 2. 분석 범위와 근거

NAC는 별도 임시 디렉터리에 소스를 받아 Revision을 확인하고 읽었습니다. TelosWeaver의 임시 경로 의존성이나 빌드 입력으로 추가하지 않았습니다. 문서의 NAC 링크는 모두 분석한 Commit에 고정했습니다.

| 구분 | 확인한 대상 |
|---|---|
| NAC 제품 구조 | README, Cargo Workspace, 세션 동작 문서, 아키텍처 결정 |
| 실행·문맥 | 오케스트레이터 프롬프트, Worker 실행 코드, Thread·Episode 조회, DAG |
| 영속화·복구 | dispatch 등록, 결과 확정, ACK, DB 소유권, StoreCoordinator, 복구 코드 |
| 사용자 입력 | 영속 Inbox, 실행 중 개입과 후속 실행 입력 |
| 시험 | 결과 재전달, 프로세스 종료, 세대 교체, 취소 경합, checkpoint 유효성 |
| TelosWeaver | Spec v1.5.11, Phase 1 배정표·H0 수용 대조표, schema, Core 서비스 인터페이스와 기존 Master 구현 |

소스에 기능이나 시험이 있다는 사실은 현재 환경에서 정상 동작함을 재검증했다는 뜻이 아닙니다. 성능·토큰 절감·실환경 안정성에 대한 비교 실험은 하지 않았습니다. 이후 Revision에서는 동작과 기본값이 바뀔 수 있습니다.

## 3. NAC의 목적과 구조

### 3.1 기본 실행 모델

**소스 확인:** NAC는 Rust 기반 에이전트 실행 환경과 React 웹 UI를 갖춘 프로젝트입니다. 오케스트레이터 방식에서는 중앙 에이전트가 작업을 Thread에 맡기고, 완료 결과를 Episode로 저장해 다음 dispatch의 입력으로 사용합니다.

| 구성 | 역할 | TelosWeaver와의 대응 |
|---|---|---|
| Orchestrator | 작업 분해·위임·결과 취합. 전용 프롬프트는 직접 파일 작업을 하지 않도록 제한 | Master |
| Thread | 이름을 가진 작업 흐름. dispatch마다 한 작업을 수행하고 이전 결과를 재사용 | Task·Session·Run의 일부 역할 |
| Episode | dispatch의 요청, 결과 본문, 상태, 식별자 저장 | TaskResult와 인계용 Artifact |
| Workset | 목표·범위·의존성·수용 조건·검증 방법을 담은 계획. 실행 큐 자체는 아님 | Plan·TaskSpec·ValidationContract |
| Store·SessionService | 영속 기록·실행 소유권·결과 확정·취소·복구 | PERSIST·SCHED·HOST |

Thread와 TelosWeaver Task는 일대일 대응하지 않습니다. NAC의 Thread 재사용에는 결과 이력 재사용이 포함되지만, TelosWeaver는 Task 계약·실행 시도·Native Session의 수명을 구분합니다.

근거: [NAC README](https://github.com/arcee-ai/nac/blob/7d8b9696d0fab130e120647ca6fabf001e673908/README.md), [Workspace 구성](https://github.com/arcee-ai/nac/blob/7d8b9696d0fab130e120647ca6fabf001e673908/Cargo.toml), [오케스트레이터 프롬프트](https://github.com/arcee-ai/nac/blob/7d8b9696d0fab130e120647ca6fabf001e673908/crates/nac-core/src/agent/prompts/nac_orchestrator.md), [Episode·Workset 타입](https://github.com/arcee-ai/nac/blob/7d8b9696d0fab130e120647ca6fabf001e673908/crates/nac-core/src/store/mod.rs).

### 3.2 README 소개와 현재 제품 동작의 차이

**소스 확인:** NAC는 `orchestrator`, `direct`, `direct-with-orchestrator` 세 가지 세션 동작을 지원하며 생성 후 동작을 변경하지 않습니다. 기본 웹 UI는 `direct`로 시작하고, `NAC_ORCHESTRATION=1`일 때 오케스트레이션 선택과 관련 화면을 표시합니다. 이 설정은 UI 표시 범위이며 권한 부여나 실행 backend 변경이 아닙니다.

또한 Episode의 본문은 `content: String`입니다. 식별자·상태·완료 프로토콜은 구조화돼 있지만, 본문까지 정형화된 검증 결과는 아닙니다. Episode 저장 성공은 작업 내용의 정확성이나 검증 통과를 증명하지 않습니다.

**적용 제안:** TelosWeaver는 실행 성공·검증 통과·Task 완료를 분리하는 기존 계약을 유지합니다. NAC의 세 가지 세션 모드나 Episode 타입을 그대로 추가하지 않습니다.

근거: [세션 동작](https://github.com/arcee-ai/nac/blob/7d8b9696d0fab130e120647ca6fabf001e673908/docs/usage/session-behaviors.md), [완료 프로토콜](https://github.com/arcee-ai/nac/blob/7d8b9696d0fab130e120647ca6fabf001e673908/crates/nac-core/src/worker_protocol.rs).

## 4. TelosWeaver의 기존 설계와 구현 상태

**소스 확인:** 검토 기준의 Phase 1 배정표는 H0가 사용자 판정 대기라고 기록합니다. PERSIST·SCHED·CONTEXT 등은 미배정이며, 관련 Core 모듈에는 H0 공개 Trait이 정의돼 있습니다. 계약의 존재와 기능 구현 완료를 구분해야 합니다.

| NAC에서 참고할 내용 | TelosWeaver의 기존 계약·설계 | 판단 |
|---|---|---|
| 중앙 판단과 집행 분리 | Master 구조화 결정, Core 검증·집행 (§4) | 기존 방향 유지 |
| 결과를 후속 작업 입력으로 재사용 | TaskResult, CapsuleContent, ContextSnapshot (§5·6·8) | 문맥 선택·구성 방식을 참고 |
| 실행 의도와 결과의 영속 확정 | RunIntent, WriteBatch, JournalEntry, 멱등 기록 (§14) | 구현 우선순위 높음 |
| 오래된 실행 결과 차단 | Task Revision, Run ID, controllerEpoch, stateVersion (§4·9·14) | 경합 시험 구체화 |
| DB 단일 소유권·제한된 저장 큐 | 프로젝트별 SQLite, 단일 Scheduler 소유권, 동기 Store Trait | 구현 세부 보완 후보 |
| 사용자 후속 입력의 영속 큐 | plans.message, 메시지·Receipt 계약 (§12·17) | 마스터 입력 전달 규칙 보완 후보 |
| 의존성 기반 실행 | P0 의존성 검사·상한, P1 DAG 병렬화 (§9) | 기존 단계 구분 유지 |

근거: [Phase 1 배정표](../phase-1/WORK_ALLOCATION.md), [H0 수용 대조표](../phase-1/H0_ACCEPTANCE_MAP.md), [Store 인터페이스](../../crates/core/src/persistence/mod.rs), [Scheduler 인터페이스](../../crates/core/src/scheduler/mod.rs), [ContextBuilder 인터페이스](../../crates/core/src/context/mod.rs).

## 5. 적용 제안

### 5.1 결과의 영속 확정과 응답 전달 분리

**소스 확인:** NAC는 Worker 실행 전에 session·thread·dispatch UUID·상위 run·단조 증가 generation을 기록합니다. 완료를 받으면 등록된 실행의 결과인지 검사하고, Episode와 dispatch 종료 기록을 같은 Transaction에서 확정한 뒤 Worker에 ACK를 보냅니다.

허용된 동일 결과의 재전달은 기존 Episode를 반환합니다. 내용 충돌, 오래된 generation, 교체되거나 종료된 상위 run의 미확정 결과는 거부합니다. 커밋 후 ACK 유실이 발생해도 저장한 결과는 남습니다.

근거: [dispatch 등록·결과 확정](https://github.com/arcee-ai/nac/blob/7d8b9696d0fab130e120647ca6fabf001e673908/crates/nac-core/src/store/worker_dispatches.rs), [완료·ACK 프로토콜](https://github.com/arcee-ai/nac/blob/7d8b9696d0fab130e120647ca6fabf001e673908/crates/nac-core/src/worker_protocol.rs), [영속화 소유권 설계](https://github.com/arcee-ai/nac/blob/7d8b9696d0fab130e120647ca6fabf001e673908/docs/architecture/0004-persistence-ownership.md).

**적용 제안:** TelosWeaver의 결과 보고 경로는 다음 순서를 명확히 합니다.

1. Core가 보고자의 Run 자격, Task Revision, Artifact 소유권·출력 계약을 검사합니다.
2. Artifact 내용을 불변 저장소에 확보합니다.
3. 결과와 Artifact 참조, 필요한 상태 변경, 멱등 기록, Journal을 정한 DB Transaction 경계에서 확정합니다.
4. 확정 후 성공 응답을 반환하고 관련 알림을 전달합니다.
5. 동일 요청이 다시 오면 확정 결과를 반환합니다. 같은 멱등 키에 다른 내용은 거부합니다.
6. UI 알림 실패나 응답 유실만으로 결과를 폐기하거나 외부 작업을 재실행하지 않습니다.

Artifact 파일 저장과 DB Transaction은 하나의 원자적 연산이 아닙니다. 내용 확보와 DB 참조 공개의 순서, 중간 장애 후 참조 없는 내용의 정리 조건을 별도로 정해야 합니다. 보고 성공도 Provider 종료·검증 통과·Task 완료와 구분합니다.

NAC는 자체 Worker 프로토콜을 통제하지만 TelosWeaver는 외부 CLI를 관리합니다. NAC의 ACK를 Codex·Claude Code·Antigravity 종료 프로토콜로 그대로 옮길 수는 없습니다. `report_task_result` 응답과 Core 결과 확정에 같은 원칙을 적용하고 Provider 종료는 별도로 추적합니다.

- 관련 작업: PH1-PERSIST·SCHED·MCP·ARTIFACT, 연결은 HOST·Provider.
- 기존 기준: Spec §5.3·§9.2·§9.5·§14.1·§14.3.
- 계약 검토: 현재 Store 공개 인터페이스로 필요한 결과·멱등 기록을 함께 확정하고 조회할 수 있는지 점검합니다. 부족하면 H0 변경 요청으로 처리합니다.

### 5.2 Episode 방식의 문맥 선별과 Capsule

**소스 확인:** NAC는 같은 Thread의 이전 성공 Episode들을 읽고, 명시한 다른 Thread에서는 최신 성공 Episode 하나를 읽습니다. 전체 Worker 대화를 무조건 공유하지 않습니다.

근거: [Worker 문맥 조회](https://github.com/arcee-ai/nac/blob/7d8b9696d0fab130e120647ca6fabf001e673908/crates/nac-core/src/store/threads.rs).

**적용 제안:** TelosWeaver에서는 TaskResult를 확정 결과로, CapsuleContent를 후속 실행에 필요한 선별 문맥으로 사용합니다.

| 전달할 내용 | 기존 계약 |
|---|---|
| 실행 결과와 산출물 | TaskResult.summary·artifacts·executionOutcome |
| 미해결 사항 | TaskResult.unresolved |
| 결정·제약·다음 작업의 질문 | CapsuleContent.decisions·constraints·openQuestions |
| 검증에 필요한 근거 | 고정 ArtifactVersionRef와 ValidationContract |
| 출처·선택 이유·생략 범위 | sourceSummary, ContextSnapshot.selected·exclusions |

다음 차이를 반영해야 합니다.

- 같은 Thread의 성공 Episode가 누적되므로 Episode 방식 자체가 문맥 크기 상한을 보장하지는 않습니다. 전달량과 선택 이유를 관리해야 합니다.
- NAC의 기본 문맥 조회는 실패 Episode를 제외합니다. TelosWeaver 재작업에는 이전 오류·검증 실패를 필요한 범위에서 명시적으로 전달해야 합니다.
- 다른 Thread의 최신 성공 결과를 읽는 방식은 입력 Version 고정과 다릅니다. TelosWeaver는 Run 시작 시 실제 Artifact Version을 Snapshot에 고정해야 합니다.
- 독립 Reviewer에는 대상 Artifact와 검증 계약을 우선 전달합니다. 구현자의 전체 이력·결론 상속은 기존 §8.2 기준을 따릅니다.

근거: [TaskResult](../../crates/schema/src/artifact.rs), [ContextSnapshot·CapsuleContent](../../crates/schema/src/context.rs), Spec §6·§8.

### 5.3 요약의 출처와 유효성 확인

**소스 확인:** NAC compaction은 원본 이력과 모델에 전달할 요약된 문맥을 구분합니다. 요약 대상 경계, 원본 Prefix 해시, 정책 해시 등을 관리하고, checkpoint 유효성을 검사합니다. 최신 checkpoint가 유효하지 않으면 이전 유효 checkpoint를 사용하는 시험도 있습니다.

근거: [compaction 계획·문맥 구성](https://github.com/arcee-ai/nac/blob/7d8b9696d0fab130e120647ca6fabf001e673908/crates/nac-core/src/agent/compaction/planning.rs), [checkpoint 시험](https://github.com/arcee-ai/nac/blob/7d8b9696d0fab130e120647ca6fabf001e673908/crates/nac-core/src/agent/compaction/tests/checkpoints.rs).

**적용 제안:** Core가 소유한 TaskResult·결정·근거에서 Capsule을 생성하고, 요약 원본과 생략 범위를 추적합니다. 입력이나 적용 정책이 달라지면 Capsule을 그대로 재사용할 수 있는지 다시 확인합니다.

외부 CLI 내부 대화를 Core가 완전히 읽거나 압축할 수 있다고 가정하지 않습니다. Capsule의 출처·유효성 관리부터 구현하고, 토큰 절감 효과는 실제 사용량·품질 측정으로 판단합니다. 구조화된 참조나 해시 검사는 요약 내용의 사실 정확성 검증을 대신하지 않습니다.

- 관련 작업: PH1-CONTEXT·ARTIFACT·MASTER.
- 기존 기준: Spec §6·§8, H0 ContextSnapshot·CapsuleContent.
- 계약 검토: 추가적인 요약 정책 Version이나 원본 범위 메타데이터가 필요하면 기존 Artifact 내용으로 표현할지, 공개 계약을 확장할지 먼저 검토합니다.

### 5.4 DB 단일 소유권과 제한된 저장 작업 처리

**소스 확인:** NAC는 정규화한 DB 경로에 대해 OS 파일 잠금을 획득하고 serving 프로세스가 소유권을 유지합니다. 저장 작업은 전용 persistence thread의 제한된 큐로 전달합니다. 과부하, 실행 전 취소, 실행 후 ACK 유실, 종료 시 접수 중단·큐 배출을 구분합니다.

근거: [StoreProcessLease](https://github.com/arcee-ai/nac/blob/7d8b9696d0fab130e120647ca6fabf001e673908/crates/nac-core/src/sessions/store_process_lease.rs), [StoreCoordinator](https://github.com/arcee-ai/nac/blob/7d8b9696d0fab130e120647ca6fabf001e673908/crates/nac-core/src/store/coordinator.rs).

**적용 제안:** TelosWeaver에서는 다음을 구현 세부로 검토합니다.

- DB 열기·Migration 전에 단일 serving 소유권을 확보합니다. API endpoint 중복 방지와 DB 소유권은 구분합니다.
- async 경로에서 동기 Store를 호출할 때 동시 호출·대기 작업 수에 상한을 둡니다. 무제한 spawn_blocking으로 대체하지 않습니다.
- DB 작업 중 Scheduler·구독자·프로세스 관리 잠금을 함께 오래 보유하지 않도록 합니다.
- 종료 시 새 접수를 중단하고 접수한 기록 처리를 마친 뒤 소유권을 해제합니다.
- 큐 포화 시 미접수 작업과 접수 후 결과가 불명확한 작업을 구분합니다.

NAC의 기본 큐 크기 128은 TelosWeaver의 검증된 기본값이 아닙니다. 예상 동시 Run 수·이벤트량에 맞춰 정해야 합니다. Windows와 Linux에서 같은 물리 DB의 경로 별칭과 비정상 종료 후 잠금 해제를 확인해야 합니다.

- 관련 작업: PH1-PERSIST·HOST·SCHED.
- 기존 기준: Spec §14, 동기 Store Trait.
- 계약 검토: 내부 구현으로 처리할 수 있는 부분은 기존 Trait을 유지합니다. 과부하·종료 오류를 모듈 밖에 노출해야 하면 공개 오류 계약 변경을 검토합니다.

### 5.5 사용자 후속 메시지의 접수와 실제 전달 구분

**소스 확인:** NAC는 영속 Inbox에서 실행 중 개입하는 steer와 다음 실행에 사용하는 queue를 구분합니다. 항목에 상태·대상 Run·실제 전달 Run·Version을 기록하며, 마지막 모델 입력 경계에 늦게 도착한 steer를 후속 실행으로 넘기는 동작을 문서화합니다.

근거: [Inbox 구현](https://github.com/arcee-ai/nac/blob/7d8b9696d0fab130e120647ca6fabf001e673908/crates/nac-core/src/store/session_inbox.rs), [입력 전달 동작](https://github.com/arcee-ai/nac/blob/7d8b9696d0fab130e120647ca6fabf001e673908/docs/usage/session-behaviors.md).

**소스 확인:** TelosWeaver의 PlansMessageParams에는 멱등 키·Plan ID·본문이 있고 결과로 messageId를 반환합니다. 이 타입만으로는 어느 마스터 Turn에 전달됐는지, 실행 종료와 겹친 입력을 어떻게 처리할지까지 정해지지 않습니다.

**적용 제안:** 메시지를 영속 저장해 접수한 사실과 마스터 Turn 입력에 포함한 사실을 구분합니다. 재시작 시 미전달 메시지를 조회하고, 전달하지 못한 입력을 버리거나 임의의 Run에 적용하지 않습니다. 실행 중 입력을 지원하지 않는 Provider는 다음 마스터 Turn으로 전달하는 경로부터 구현합니다.

RN Receipt, 마스터 입력 반영, 실제 계획 변경은 서로 다른 사실입니다. 마스터의 결정 전달은 기존 H0 D3의 Turn 종료 JSON 방식을 유지합니다.

- 관련 작업: PH1-MASTER·PERSIST·HOST·EXT. RN 경계는 PH1-RN과 확인합니다.
- 기존 기준: Spec §4.2·§12·§17, [Local API 계약](../../crates/schema/src/api.rs), [Receipt 상태](../../crates/schema/src/messaging.rs).
- 계약 검토: 전달 모드·입력 Version·전달 Turn 등의 공개 필드가 필요하면 H0 변경 요청 대상입니다. 이 문서에서 새 wire 값을 확정하지 않습니다.

### 5.6 실행 단계별 장애 진단

**소스 확인:** NAC Worker 감독에는 시작 전, 모델 API 호출 중, 도구 호출 중, 도구 완료 후 다음 모델 호출 준비 중, 최종 응답 후 종료 중을 구분하는 timeout 추적이 있습니다. 활성 도구 호출의 이름·ID·인자 정보도 활용합니다.

근거: [WorkerTimeoutTrace](https://github.com/arcee-ai/nac/blob/7d8b9696d0fab130e120647ca6fabf001e673908/crates/nac-core/src/tools/thread/worker.rs).

**적용 제안:** TelosWeaver의 기존 ProviderEvent를 이용해 마지막 확인 단계와 활성 toolCall ID를 진단에 포함합니다. Provider가 해당 이벤트를 제공하지 않으면 Unknown으로 남깁니다. 관측된 마지막 이벤트와 실제 정지 원인을 구분하고, 인자·출력은 기존 비밀 제거와 로그 보존 정책을 적용합니다.

- 관련 작업: PH1-HOST·각 Provider·EXT.
- 기존 기준: Spec §7·§15, [Provider 이벤트 계약](../../crates/schema/src/events.rs).

## 6. H1·H3에 참고할 장애 시험

**소스 확인:** NAC에는 실제 자식 프로세스를 사용해 커밋 전·커밋 후 ACK 전·ACK 후에 종료하는 시험이 있습니다. 결과 재전달, 내용 충돌, 오래된 generation, 상위 run 교체, 잘못된 프레임, 취소·timeout 경합도 다룹니다. 시험 코드의 존재와 내용을 확인했으며 이번 검토에서 실행하지 않았습니다.

근거: [Worker 결과·복구 시험](https://github.com/arcee-ai/nac/blob/7d8b9696d0fab130e120647ca6fabf001e673908/crates/nac-core/src/store/worker_dispatches/tests.rs), [완료 프로토콜 시험](https://github.com/arcee-ai/nac/blob/7d8b9696d0fab130e120647ca6fabf001e673908/crates/nac-core/src/tools/thread/completion_tests.rs), [저장 처리 프로세스 종료 시험](https://github.com/arcee-ai/nac/blob/7d8b9696d0fab130e120647ca6fabf001e673908/crates/nac-core/src/store/coordinator_crash_tests.rs).

**적용 제안:** 다음 항목을 기존 H0 수용 대조표의 요구사항과 연결해 구체적인 시험으로 만듭니다.

| 장애·경합 시점 | 확인할 결과 | 주요 관련 작업 |
|---|---|---|
| 실행 의도 저장 후 Adapter 전달 전 Core 종료 | 외부 실행이 없었다는 근거로 재전달 또는 폐기 판단 | PERSIST·SCHED·HOST |
| Adapter 전달 후 시작 응답 전 Core 종료 | 자동 중복 실행 없이 실제 상태 대조·안전 보류 | SCHED·HOST·Provider |
| 결과 DB 커밋 전 Core 종료 | 부분 참조·종료 상태를 성공으로 공개하지 않음 | PERSIST·ARTIFACT·MCP |
| 결과 저장 후 보고 응답 전 연결 종료 | 같은 요청 재전달 시 기존 결과 반환 | PERSIST·MCP |
| 같은 멱등 키에 다른 결과 재전달 | 충돌 거부, 기존 결과 유지 | PERSIST·MCP |
| 취소 확정 뒤 늦은 완료 도착 | 결과 이력 보존, Task 자동 완료 금지 | SCHED·HOST |
| 새 Run 시작 후 이전 Run 결과 도착 | 새 Run 결과로 채택하지 않음 | SCHED·MCP |
| 결과 확정 후 UI 알림 실패 | Snapshot·Journal 재접속으로 결과 확인 | HOST·EXT |
| 같은 물리 DB를 다른 경로 표기로 중복 실행 | 동일 DB 소유권 충돌 감지 | PERSIST·HOST |
| 저장 큐 포화 | 접수 실패 명시, 미접수 작업 실행 금지 | PERSIST·호출 모듈 |
| 사용자 메시지 접수 후 Core 재시작 | 미전달 메시지 보존, 전달한 입력의 중복 반영 방지 | MASTER·PERSIST |

DB·프로토콜 시험은 자격 증명 없는 Fixture부터 작성할 수 있습니다. 실제 CLI의 시작·종료·취소·Resume 동작은 별도 실환경 시험이 필요하며, Fixture 통과를 Provider 검증으로 기록하지 않습니다. H1은 수직 경로와 안전 보류, H3는 해당 요구사항의 통합 수용 기준에 맞춰 배치합니다.

## 7. 그대로 도입하지 않을 부분

### 7.1 DAG와 Workset의 중복 도입

**소스 확인:** NAC의 해당 DAG는 같은 tool-call 묶음의 Thread 참조를 의존성으로 정렬하고, 순환을 거부하며 선행 실패 시 후속 dispatch를 생략합니다. Workset은 영속 계획이며 실행 큐 자체가 아닙니다.

**적용 제안:** 순환 검사와 실패 전파 시험은 참고하되, TelosWeaver의 영속 Task 의존성·검증 완료 조건·Writer Lock을 이 DAG로 대체하지 않습니다. Workset을 별도 도메인으로 추가할 필요도 없습니다. P0 의존성 검사와 P1 병렬화의 기존 구분을 유지합니다.

근거: [DAG 구현](https://github.com/arcee-ai/nac/blob/7d8b9696d0fab130e120647ca6fabf001e673908/crates/nac-core/src/agent/dag.rs), Spec §4.1·§9.

### 7.2 실행 계층과 플랫폼 가정

**소스 확인:** NAC는 자체 ModelClient와 Worker 실행 경로를 갖습니다. retained worktree 구현 중 저장소 정체성 확인은 Unix의 장치·inode 정보를 사용하며, 비 Unix 경로에서는 미지원 오류를 반환합니다.

**적용 제안:** TelosWeaver는 외부 CLI Adapter 경계를 유지합니다. Windows를 포함한 실행·파일 정체성·잠금 처리는 기존 플랫폼 추상화에 맞춰 설계하고 직접 검증해야 합니다. 이 특정 경로의 미지원을 NAC 전체의 Windows 지원 여부로 일반화하지 않습니다.

근거: [Worker 실행](https://github.com/arcee-ai/nac/blob/7d8b9696d0fab130e120647ca6fabf001e673908/crates/nac-core/src/tools/thread/worker.rs), [retained worktree 구현](https://github.com/arcee-ai/nac/blob/7d8b9696d0fab130e120647ca6fabf001e673908/crates/nac-core/src/workspace/task_worktree.rs).

NAC 소스를 복사하거나 의존성으로 추가한 것은 아닙니다. 코드 재사용을 별도로 추진한다면 라이선스·고지와 플랫폼 의존성을 검토해야 합니다.

## 8. 적용 순서와 계약 변경 후보

| 순서 | 적용 내용 | 관련 작업 | 기존 계약과의 관계 |
|---|---|---|---|
| 1 | 결과·멱등 기록·Journal의 확정 경계와 응답 유실 처리 | PERSIST·SCHED·MCP·ARTIFACT | 기존 원칙 구현, 저장·조회 인터페이스 충족 여부 확인 |
| 2 | 확정 결과 기반 Capsule, 입력 Version 고정, 실패 근거 선별 | CONTEXT·ARTIFACT·MASTER | 기존 H0 계약 우선 사용 |
| 3 | DB 단일 소유권, 제한된 저장 작업, 종료 시 배출 | PERSIST·HOST·SCHED | 내부 구현 우선, 공개 오류 확장 필요 시 요청 |
| 4 | 마스터 후속 입력의 접수·전달·재시작 복구 | MASTER·PERSIST·HOST·EXT | 전달 모드·메타데이터는 계약 검토 후보 |
| 5 | 장애 시점별 시험·실행 단계 진단 | 각 담당·INTEGRATION | 기존 수용 기준과 연결 |
| P1 | DAG 병렬화·측정 기반 문맥량 최적화 | SCHED·CONTEXT | 기존 후속 범위 유지 |

이 순서는 연구 결과에 따른 우선순위 제안이며 H0 확정 전 기능 구현을 허가하거나 작업을 배정한 기록이 아닙니다. 실제 착수는 [Phase 1 배정표](../phase-1/WORK_ALLOCATION.md)와 Spec §19.6을 따릅니다. 모듈 공개 Trait·schema·wire 계약을 변경해야 한다면 기존 변경 요청 절차를 적용합니다.

가장 먼저 검토할 대상은 **PH1-PERSIST·SCHED·MCP의 결과 확정 경계와 H1 장애 시험 조건**입니다. Capsule은 기존 계약으로 구현할 수 있는 부분부터 진행하고, 공개 전달 모드·요약 메타데이터·과부하 오류 등은 필요성을 확인한 뒤 별도 계약 변경 후보로 다룹니다.

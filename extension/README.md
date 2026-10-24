# TelosWeaver Extension — Phase 0

Core의 연결·Health·오류·실행 Host를 표시하는 VS Code Workspace Extension입니다. Core는 별도 프로세스이며 Extension을 닫아도 계속 실행됩니다. 전체 마스터 대화·Task 목록·Artifact·승인 UI는 후속 범위입니다.

## 준비

저장소 루트에서 Core를 빌드합니다.

```powershell
cargo build -p telos-core --bin telos-weaver --locked
```

Extension 폴더에서 의존성과 생성 타입·TypeScript를 준비합니다.

```powershell
npm ci --ignore-scripts
npm run build
```

개발 중에는 이 `extension` 폴더를 VS Code로 열고 **TelosWeaver Extension** 디버그 구성을 실행합니다(F5). `.vscode/launch.json`의 사전 작업이 Extension을 빌드합니다. Core 바이너리는 별도로 준비해야 합니다. 상위 Workspace에서 디버그하려면 이 폴더를 Workspace에 추가할 수 있습니다.

## 연결과 설정

**TelosWeaver: Core 실행 파일 선택**으로 빌드한 실행 파일을 지정합니다. 또는 사용자/Host 설정에 절대 경로를 넣습니다.

```json
{
  "telosWeaver.corePath": "D:/path/to/telos-weaver.exe",
  "telosWeaver.projectId": "",
  "telosWeaver.hostId": ""
}
```

- **Core 시작 / 연결:** 먼저 선택한 프로젝트의 기존 Core에 연결하고, 없으면 지정한 바이너리를 검증해 시작합니다. 정책 파일은 전달하지 않아 새 Core는 DenyRuns입니다.
- **Core 연결:** 기존 Core에만 연결합니다. Core가 없으면 오류를 표시합니다.
- **연결 해제 (Core 유지):** 연결·Heartbeat·재시도를 닫습니다. Core는 종료하지 않습니다.
- **상태 새로 고침:** 실제 core.health를 조회합니다. 연결됨과 Degraded를 구분합니다.
- **Run 출력 구독:** 이미 알고 있는 Run ID의 정규화 이벤트를 읽기 전용으로 구독합니다. Run 생성 명령은 제공하지 않습니다.
- **출력 보기:** 크기 상한이 있는 OutputChannel을 엽니다.

Explorer의 **TelosWeaver** 패널과 상태 표시줄에서 연결·Core 버전·Project·Host·컴포넌트 상태를 확인할 수 있습니다. Extension 활성화 자체는 Core를 시작하지 않습니다.

다중 루트에서는 연결할 프로젝트를 선택합니다. 빈 창·가상 Workspace의 Core 시작과 신뢰하지 않은 Workspace의 연결·시작은 제한합니다. 실행 파일 경로와 Host ID는 machine 범위 설정입니다. projectId를 비우면 새 Core에 `project-<projectKey>`를 사용합니다. 기존 Core 연결은 반환된 ID를 표시하며, 명시적으로 지정한 ID와 다르면 거부합니다. 원격 Core 시작에는 명시적인 hostId가 필요합니다.

API 토큰은 계약상 사용자 파일에서 읽어 인증에만 사용합니다. 설정·로그·UI에는 저장하지 않습니다. Core가 재시작되면 다시 읽습니다. 오류·원격 진단의 원문 대신 제한된 메시지를 표시합니다.

## 재연결과 제한

- API 버전 0.2, UTF-8 NDJSON 한 줄 1 MiB, 대기 요청 32개, 요청/연결 기본 제한 5초입니다.
- 일시적 연결 장애에는 지수 Backoff로 최대 5회 재연결을 시도합니다. Core를 자동으로 재시작하지 않으며 이후에는 명시적으로 다시 연결합니다.
- 알려진 Run은 현재 Extension 세션에서 수신한 sequence를 유지하고 같은 프로젝트 재연결 시 재구독합니다. slow_consumer는 실제 수신 위치부터 최대 5회 재구독합니다. 이벤트 순서 유실·커서 유실은 명시적으로 중단합니다.
- UI 재로드 뒤 전체 Run 목록 복원은 지원하지 않습니다. Run ID를 다시 입력할 수 있습니다. Core가 재시작돼 잃은 Run을 자동 재실행하지 않습니다.
- 출력은 256 KiB에서 초기화하고 한 줄은 4096자로 제한합니다. 전체 로그 보관소를 대신하지 않습니다.
- Extension의 새 Core 시작에는 자동 설치·다운로드·Cargo 빌드·정책 파일·권한 우회를 넣지 않습니다. 실행 중인 Core에 연결했을 때의 정책은 그 Core가 관리합니다.
- Windows 실환경을 검증했습니다. Unix 경로 벡터는 확인했지만 Linux·Remote SSH 실행 검증은 대기 상태입니다.

## 시험

```powershell
npm test
```

생성·TypeScript 빌드와 로컬 Socket 시험 대역·UI 시험 18개입니다. 실제 Provider나 모델은 호출하지 않습니다.

실제 Core 및 설치된 VS Code를 사용하는 시험은 명시적으로 실행합니다.

```powershell
$env:TELOS_CORE_EXECUTABLE = '<빌드한 Core의 절대 경로>'
npm run test:core

$env:TELOS_VSCODE_EXECUTABLE = '<Code.exe의 절대 경로>'
npm run test:host
```

실제 시험은 임시 프로젝트·상태 디렉터리와 별도 VS Code 프로필·확장 디렉터리를 사용합니다. 실제 Core의 Run 거부와 연결 해제 후 생존, Extension Host 종료 후 생존을 확인하고 시험이 시작한 Core만 정리합니다. Host 시험 결과는 출력된 임시 디렉터리에 보존합니다. Core·VS Code 바이너리를 자동 다운로드하지 않습니다.

계약 원본은 `../schemas/`와 `../crates/schema/src/api.rs`입니다. 생성물은 직접 편집하거나 커밋하지 않습니다. 실제 범위·검증 증거는 [PH0-EXT 기록](../docs/phase-0/evidence/PH0-EXT/README.md)을 참고합니다.

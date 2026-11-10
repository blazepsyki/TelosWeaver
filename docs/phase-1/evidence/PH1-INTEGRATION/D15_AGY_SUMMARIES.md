# H0 D15: Antigravity 요약 인덱스의 시험 기록 정리

| 항목 | 값 |
|---|---|
| 담당 | PH1-INTEGRATION (`claude-opus-5.5/session-c9336d2c`) |
| 날짜 | 2026-11-10, Windows 로컬, Antigravity 1.5.3 |
| 결정 | [H0_DECISIONS.md](../../H0_DECISIONS.md) D15 (b): 요약 원본까지 조사 |
| 도구 | [agy_summary_cleanup.py](agy_summary_cleanup.py), [agy_pbwire.py](agy_pbwire.py) |

## 1. 원인

`~/.gemini/<dir>/conversation_summaries.db`(SQLite)는 원본이 아니라 색인입니다. Antigravity는 같은 디렉터리의 `jetbox_summaries_proto.pb`에서 이 DB를 다시 만듭니다. 2026-10-26에 SQLite 행만 지웠기 때문에, 이후 CLI 세션이 DB를 다시 만들면서 행이 되살아났습니다.

| 파일 | 역할 | 시험 기록 |
|---|---|---|
| `jetbox_summaries_proto.pb` | CLI 세션의 요약 원본. 최상위가 `(1: 대화 ID, 2: 요약)` 항목의 반복 | 남은 시험 행 52개(확장 쪽 32, CLI 쪽 20)가 모두 여기에 있었음 |
| `conversation_summaries.db` | 위 원본과 확장 허브 정보로 만든 색인 | 같은 52개 |
| `cache/last_conversations.json` | 작업 폴더 → 마지막 대화 ID(`--continue`용) | 시험 폴더 항목 17개 |
| `agyhub_summaries_proto.pb` | 확장 허브의 요약 원본 | 시험 기록 없음(대화 파일이 있는 대화만) |

## 2. 정리 방법

정리 대상은 대화 파일(`brain/<id>`, `conversations/<id>.db`)이 없고, 작업 폴더가 `telos-*` 시험 폴더이거나 프로젝트가 `default-cli-project`인 항목입니다. 같은 ID를 세 파일에서 함께 지웁니다.
- protobuf: 대상 항목만 빼고 나머지 항목은 원래 바이트 그대로 다시 조립합니다. Schema가 없어도 구조 해석과 재조립이 원본과 바이트 단위로 일치함을 확인했습니다.
- SQLite: `conversation_summaries`(와 있을 경우 `conversations_fts`)의 같은 ID 행.
- JSON: 그 ID를 가리키는 항목.

적용 전에 세 파일을 백업합니다. 사용자 대화와 다른 프로젝트 항목은 건드리지 않습니다.

## 3. 결과

| 단계 | 확장 쪽(`antigravity`) | CLI 쪽(`antigravity-cli`) |
|---|---|---|
| 적용 | 원본 52 → 20항목, DB 32행 삭제, JSON 12항목 삭제, 무결성 `ok` | 원본 38 → 18항목, DB 20행 삭제, JSON 5항목 삭제, 무결성 `ok` |
| `agy models`로 CLI 시작 뒤 | 시험 항목 0 | 시험 항목 0 |
| 실제 세션 1회(Flash low, 일회용 폴더) 뒤 | 시험 항목 0(되살아나지 않음) | 새 세션만 원본에 기록됨 |
| 새 세션의 대화 파일 삭제 → 도구 적용 → CLI 재시작 | 0 | 0 |

새 세션 `2d53a62d-a4a3-41f9-a437-16c09141718a`는 확장 쪽 DB에도 복제돼 있었고 함께 지워졌습니다. 백업은 INTEGRATION 세션 임시 폴더에 두었습니다(저장소 밖).

## 4. 후속

- 실환경 시험 정리 규칙에 이 도구 실행을 추가합니다. 시험이 대화 파일을 지운 뒤 `agy_summary_cleanup.py --apply`를 실행합니다.
- Antigravity 실환경 시험의 정리 함수(`cleanup_session`)에 요약 정리를 넣는 일은 PH1-AGY 범위입니다.
- 확장 UI 화면에서 빈 항목이 사라졌는지는 사용자 확인이 필요합니다(미확인).
- 원본 파일 형식은 Antigravity 업데이트로 바뀔 수 있습니다. 도구는 디렉터리마다 세 파일을 모두 해석한 뒤에 쓰므로, 해석에 실패하면 그 디렉터리는 바꾸지 않고 멈춥니다. 적용 전 시험 실행(인자 없음)으로 대상 수를 먼저 확인합니다.

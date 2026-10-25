# PH0-RN 검증 기록

PH0-RN은 Spec v1.5 DEC-07·§12·§19.5.10에 따라 Reason Network를 Fork해 클로즈드 소스 Reason CLI 연동, 자동 업데이트·받아서 실행하는 설치, Reason 계정 로그인을 제거하고 TelosWeaver 통신 서비스로 재구성하는 작업입니다. 담당은 `claude-opus-5.5/session-c9336d2c`이며 서브 태스크를 순차로 진행합니다.

| 서브 태스크 | 상태 | 기록 |
|---|---|---|
| PH0-RN-00 범위·제거 정책 | 완료 (2026-10-25) | [RN-00](RN-00/README.md): 원본 조사, 유지·제거 처리표, 로그인 의존성 결론, 외부 접속 목록, 후속 경계 |
| PH0-RN-01 원본 도입·기준 | 완료 (2026-10-25) | [RN-01](RN-01/README.md): v0.12.11 subtree 도입, 독립 Workspace, 수정 전 빌드·시험 191개 통과, 파일 소유권 |
| PH0-RN-02 업데이트·설치 제거 | 완료 (2026-10-25) | [RN-02](RN-02/README.md): 자동 업데이트·`upgrade`·`migrate`·설치 스크립트·시작 서비스 등록 제거, 시험 190개 통과, 후속(RN-04) 목록 |
| PH0-RN-03 Reason CLI·로그인 제거 | 완료 (2026-10-25) | [RN-03](RN-03/README.md): `reason.rs`·`reason` 에이전트·`managed()`·로그인 증명 제거, Relay 목록 서명 검증은 `signed.rs`로 분리해 유지(RN-00 정정), 시험 178개 통과 |
| PH0-RN-04 Daemon 기반 정리 | 완료 (2026-10-25) | [RN-04](RN-04/README.md): `telos-rn` 이름·`~/.telos-rn` 상태·`TELOS_RN_*` 분리, tincan 이전 처리 제거, 팀 링크 `rn1-…` 문자열, `init`·`join` 자동 연결 해제, Relay 목록은 팀이 있을 때만, 시험 179개·격리 실행 확인 |
| PH0-RN-05 메시지·정체성·저장소 | 완료 (2026-10-25) | [RN-05](RN-05/README.md): Endpoint(Workspace 제외 Fingerprint)·Receipt(`delivered`=실제로 봄)·재바인딩 보류·멱등 키·만료·보존 7일·Migration 백업 3개·Codex CLI는 Core 전달만, 시험 196개, 메일함·실제 Claude(haiku) 왕복 |
| PH0-RN-06 Local RPC 서버 | 대기 | — |
| PH0-RN-07 Client·Core 연결 | 대기 | — |
| PH0-RN-08 제거 검증·수용 | 대기 | — |

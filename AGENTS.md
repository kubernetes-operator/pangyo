# 개발 에이전트 공통 계약 (Claude Code / 기타 에이전트)

1. 작업 전 `README.md`, `docs/01_REQUIREMENTS.md`, `docs/07_LOCK_APPROVAL.md`, `docs/08_ROADMAP.md`, `docs/decisions/ADR-0002.md`(기술 스택)를 읽는다.
2. 모든 새 기능에서 **변수 / 기본 설정을 소스 상단**에 모으고 한국어 설명을 추가한다. 자격증명은 저장하지 않는다.
3. 제안 → 영향 영역 검토 → 소규모 구현 → 테스트(`make check` 통과 필수) → 문서/상태 업데이트의 **반복 루프**를 수행한다.
4. 기존 API/스키마를 임의로 파괴하지 않는다. 바뀌면 프로토콜 버전과 migration을 함께 추가한다.
5. 모든 새로운 개발에서 웹 오프라인 동작과 Markdown 내보내기 회귀 여부를 확인한다.
6. OS별 제한 사항을 실제 검증 전에는 지원 완료라고 주장하지 않는다.
7. **동시 작성 잠금은 승인 전 코드/서버 기능 활성화 금지.** 서버 미연결 상태에서 무손실 동기화 보장은 불가능하다.
8. Git remote, kube context, API 키, 개인 데이터는 설정 파일에 하드코딩하지 않는다.
9. 각 변경 완료 시 `docs/CHANGELOG.md`에 변경 파일, 테스트, 위험, 후속 과제를 남긴다.
10. 분기 기준: `feature/<요구ID 또는 로드맵 단계>-<기능명>`; 커밋 `feat|fix|docs|test|refactor|chore|ci: 요약`.

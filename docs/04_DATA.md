# 04. 데이터 모델 / 파일 정책

## 파일 경로
```
Notes/
  Inbox.md                    # 루트
  Work/
    Plan.md                   # 한 단계
    Project/
      Meeting.md              # 두 단계
```
- 폴더 깊이는 `0~2`; 폴더의 이동/개명 시 UUID·중복 이름 검사 필수.
- 사용자 파일명과 내부 ID를 분리. 외부 동기화의 안정적인 참조는 UUID.
- `.md` 또는 `.txt`만 허용, UTF-8, NFC 정규화 및 경로 탐색(`../`) 차단.
- 메모 제목 변경 시 경로도 재배치하지만 파일 내용 보존.

## 저장 테이블
- `notes`: note_id, owner_id(서버), folder, path, format, content_hash, version, timestamps.
- `edit_sessions`: 시작/종료, 누적 활성 시간, 장비 ID, 최초/재개 구분.
- `local_history`: note_id, 순번, 변경 내용(델타), redo cursor, 저장 시각.
- `sync_events`: append-only 변경 기록, 서버 수신 상태, 일관성 토큰.
- `devices`: 계정에 속한 등록된 장비, 공개키, 마지막 접속.

## 작성 시간
- 앱을 열어둔 시간 전체가 아니라 **키보드 활동이 있는 시간**만 측정.
- 마지막 입력 60초 후 세션을 일시 중단하고 재입력 시 신규 세션 기록.
- 강제 종료 시 마지막 저장 시각까지만 보장. 향후 monotonic clock + journal로 복구.

## 유사 문서
- MVP: title/path/content의 길이 제한이 있는 검색. 온라인 서버에서 검색 실행.
- 1차 서버: PostgreSQL `pg_trgm`, 요약 인덱스, 사용자별 범위 제한과 증분 업데이트.
- 2차: 한국어 검색 품질 검증 후 형태소 분석기/embedding 벡터 인덱스 추가 고려.
- 브라우저/저사양 BSD 클라이언트는 서버에서 받은 상위 K건만 표시.
- 트리와 방사형 그래프는 최대 표시 노드 수 제한.

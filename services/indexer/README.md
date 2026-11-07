# Indexer — 미래 서비스 영역

- 사용자가 승인된 메모 변경만 서버에서 색인.
- 전용 DB tenant filter (계정별 권한) 필수.
- 1차 `pg_trgm`·검색 캐시, 2차 선택적 embedding 계산.
- 서버 성능과 비용을 제한하기 위해 batch, debounce, user quota 적용.
- **v0.1 구현하지 않음**; 폴더만 예약.

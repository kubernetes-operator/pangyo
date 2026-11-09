-- === 004: 메모별 동기화 상태 (로드맵 06). 웹 Note.sync 와 같은 JSON. 데스크톱 동기화 연결 전까지는 보관만 ===
ALTER TABLE notes ADD COLUMN sync_json TEXT;

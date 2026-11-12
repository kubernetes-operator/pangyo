-- === 005: 앱 밖에서 지워진 동기화 메모 (로드맵 06 데스크톱 연결, ADR-0004) ===
-- 서버에 올라간 메모 파일이 바깥에서 사라지면 바로 서버에서 지우지 않고 여기에 보관합니다.
-- 사용자가 '서버에서도 삭제' 또는 '다시 받기'를 고르면 행을 지웁니다. 같은 ID로 다시 저장되어도 지웁니다.
CREATE TABLE IF NOT EXISTS external_deletions (
  id TEXT PRIMARY KEY,
  path TEXT NOT NULL,
  format TEXT NOT NULL,
  sync_json TEXT NOT NULL,
  detected_at TEXT NOT NULL
);

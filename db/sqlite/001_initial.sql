-- === SQLite native client 초안: 브라우저 IndexedDB는 별도 어댑터 사용 ===
PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;
CREATE TABLE IF NOT EXISTS notes (
  id TEXT PRIMARY KEY, -- UUID; 경로가 바뀌어도 불변
  path TEXT NOT NULL UNIQUE, -- root 또는 최대 두 단계 folder + file
  format TEXT NOT NULL CHECK (format IN ('md', 'txt')),
  content_hash TEXT NOT NULL, -- 실제 본문은 외부 Markdown/TXT 파일에 저장
  version INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL,
  modified_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS edit_sessions (
  id TEXT PRIMARY KEY,
  note_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
  started_at TEXT NOT NULL,
  ended_at TEXT,
  active_ms INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS local_history (
  note_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
  seq INTEGER NOT NULL,
  delta_blob BLOB NOT NULL, -- 정식판: 압축 델타, 암호화 정책 검토
  PRIMARY KEY(note_id, seq)
);

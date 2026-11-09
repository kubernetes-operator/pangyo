-- === 003: 데스크톱 작업공간 (로드맵 03단계, 2026-11-04) ===
-- 화면 설정(언어, 마지막 선택 메모)과 Undo/Redo 스택을 보조 인덱스에 보관합니다.
-- Undo/Redo는 웹과 같은 "전체 스냅샷 JSON" 프로토타입 방식. 델타(local_history.delta_blob)는 후속.
CREATE TABLE IF NOT EXISTS meta (
  key TEXT PRIMARY KEY,
  value_json TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS history_stacks (
  note_id TEXT PRIMARY KEY REFERENCES notes(id) ON DELETE CASCADE,
  undo_json TEXT NOT NULL DEFAULT '[]',
  redo_json TEXT NOT NULL DEFAULT '[]'
);

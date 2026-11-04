-- === 002: 편집 세션에 '열어둔 시간' 추가 (요구 23, 2026-11-04) ===
-- active_ms = 작성 시간(키 입력 간격 합), open_ms = 메모가 화면에 보이던 시간.
-- 기존 행은 열어둔 시간을 알 수 없으므로 0.
ALTER TABLE edit_sessions ADD COLUMN open_ms INTEGER NOT NULL DEFAULT 0;

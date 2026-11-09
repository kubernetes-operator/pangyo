-- === 004: 서버 검색·연관 메모 (로드맵 07, 요구 20) ===
-- pg_trgm: 3글자 조각 유사도. 한글도 글자 단위로 동작. 삭제된 메모(묘비)는 색인하지 않음.
CREATE EXTENSION IF NOT EXISTS pg_trgm;
CREATE INDEX notes_title_trgm ON notes USING gin (title gin_trgm_ops) WHERE deleted_at IS NULL;
CREATE INDEX notes_content_trgm ON notes USING gin (content gin_trgm_ops) WHERE deleted_at IS NULL;

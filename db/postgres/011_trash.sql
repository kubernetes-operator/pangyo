-- === 011: 휴지통 1주일 보관 (2026-11-13 사용 의견) ===
-- 메모를 지우면 묘비(deleted_at, content '')는 그대로 두고, 본문을 trashed_content에 보관합니다.
-- 보관 기간(기본 7일, TRASH_DAYS) 안에는 되살릴 수 있고, 지나면 서버가 trashed_content를 비웁니다(묘비는 동기화용으로 남김).
-- 이 마이그레이션 전에 지운 메모는 본문이 없어 휴지통에 나오지 않습니다.
ALTER TABLE notes ADD COLUMN trashed_content TEXT;
ALTER TABLE notes ADD COLUMN trashed_size BIGINT NOT NULL DEFAULT 0;
CREATE INDEX notes_trash ON notes (owner_id, deleted_at) WHERE trashed_content IS NOT NULL;

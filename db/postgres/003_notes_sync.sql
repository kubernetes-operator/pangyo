-- === 003: 메모 동기화 + 편집 임대 (로드맵 06, ADR-0003 A안, 2026-11-09) ===
-- 본문은 S3 도입 전까지 notes.content에 저장 (계정 used_bytes에 반영).
-- 삭제는 행을 지우지 않고 deleted_at(묘비)으로 남겨 다른 기기가 변경 목록에서 알 수 있게 함.
ALTER TABLE notes DROP CONSTRAINT IF EXISTS notes_owner_id_path_key;
ALTER TABLE notes ALTER COLUMN object_key DROP NOT NULL;
ALTER TABLE notes ADD COLUMN title TEXT NOT NULL DEFAULT '';
ALTER TABLE notes ADD COLUMN folder TEXT NOT NULL DEFAULT '';
ALTER TABLE notes ADD COLUMN content TEXT NOT NULL DEFAULT '';
ALTER TABLE notes ADD COLUMN size_bytes BIGINT NOT NULL DEFAULT 0 CHECK (size_bytes >= 0);
ALTER TABLE notes ADD COLUMN created_at TIMESTAMPTZ NOT NULL DEFAULT NOW();
ALTER TABLE notes ADD COLUMN deleted_at TIMESTAMPTZ;
-- 변경 목록 커서: 저장/삭제할 때마다 전역 시퀀스 값으로 갱신
CREATE SEQUENCE note_change_seq;
ALTER TABLE notes ADD COLUMN change_seq BIGINT NOT NULL DEFAULT nextval('note_change_seq');
-- 임대를 새로 줄 때마다 1 증가 = fencing token
ALTER TABLE notes ADD COLUMN lease_epoch BIGINT NOT NULL DEFAULT 0;
-- 같은 계정 안에서 살아 있는 메모의 경로는 대소문자 구분 없이 하나 (Windows/macOS 파일시스템과 일치)
CREATE UNIQUE INDEX notes_owner_live_path ON notes (owner_id, lower(path)) WHERE deleted_at IS NULL;
CREATE INDEX notes_owner_change ON notes (owner_id, change_seq);

CREATE TABLE note_leases (
  note_id UUID PRIMARY KEY REFERENCES notes(id) ON DELETE CASCADE,
  account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  holder TEXT NOT NULL,              -- 서버가 인증 정보로 만든 식별자 (세션/장비 + 클라이언트 ID)
  holder_label TEXT NOT NULL DEFAULT '', -- 화면 표시용 (예: "Chrome on Linux")
  fencing_token BIGINT NOT NULL,
  acquired_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  expires_at TIMESTAMPTZ NOT NULL
);

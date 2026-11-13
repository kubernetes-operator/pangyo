-- === 009: 프로젝트 (ADR-0005, 2026-11-13) ===
-- 프로젝트 = 폴더 위 단계. 메모의 project_id가 NULL이면 "기본" 프로젝트 → 기존 메모는 옮기지 않음.
-- ID는 클라이언트가 정함(메모와 같음, 오프라인 생성). 삭제는 묘비(deleted_at)로 남겨 다른 기기에 전달.
-- change_seq는 메모와 같은 시퀀스 → 변경 목록 하나의 커서로 메모·프로젝트를 함께 받음.
CREATE TABLE projects (
  id UUID PRIMARY KEY,
  owner_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  modified_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  deleted_at TIMESTAMPTZ,
  change_seq BIGINT NOT NULL DEFAULT nextval('note_change_seq')
);
CREATE UNIQUE INDEX projects_owner_live_name ON projects (owner_id, lower(name)) WHERE deleted_at IS NULL;
CREATE INDEX projects_owner_change ON projects (owner_id, change_seq);

ALTER TABLE notes ADD COLUMN project_id UUID REFERENCES projects(id);
-- 같은 경로라도 프로젝트가 다르면 허용
DROP INDEX notes_owner_live_path;
CREATE UNIQUE INDEX notes_owner_live_path ON notes
  (owner_id, COALESCE(project_id, '00000000-0000-0000-0000-000000000000'::uuid), lower(path))
  WHERE deleted_at IS NULL;
CREATE INDEX notes_project ON notes (project_id) WHERE project_id IS NOT NULL AND deleted_at IS NULL;

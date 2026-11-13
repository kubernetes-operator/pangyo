-- === 006: 프로젝트 ID ↔ 이름 (ADR-0005) ===
-- 다른 프로젝트의 메모 파일은 <루트>/@<이름>/… 에 있으므로, 화면·서버가 쓰는 프로젝트 ID와 폴더 이름을 연결합니다.
-- 목록의 정본은 화면이 저장하는 meta 'projects'(웹과 같은 JSON). 저장할 때마다 이 표와 폴더 이름을 맞춥니다.
CREATE TABLE IF NOT EXISTS projects (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS projects_name ON projects (lower(name));

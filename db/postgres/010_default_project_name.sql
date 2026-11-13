-- === 010: 기본 프로젝트 표시 이름 (2026-11-13 사용자 확인: 기본 프로젝트 이름 바꾸기) ===
-- 기본 프로젝트는 행이 없으므로(메모 project_id NULL) 계정에 표시 이름만 둡니다. '' = 화면 기본값("기본").
ALTER TABLE accounts ADD COLUMN default_project_name TEXT NOT NULL DEFAULT '';

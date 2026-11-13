-- === 008: 관리자 설정 — 계정당 저장공간 (2026-11-13 사용 의견 6번) ===
-- system_settings: 관리자가 화면에서 바꾸는 서버 설정. 'default_quota_mb'가 있으면 FREE_STORAGE_MB(환경변수)보다 우선.
-- accounts.quota_custom: 관리자가 계정별로 따로 정한 용량이면 true → 기본값을 바꿔도 그대로 유지.
CREATE TABLE system_settings (
  key TEXT PRIMARY KEY,
  value JSONB NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
ALTER TABLE accounts ADD COLUMN quota_custom BOOLEAN NOT NULL DEFAULT FALSE;

-- === Postgres 서버 데이터 모델 초안; 운영 마이그레이션 도구 연결 전 ===
CREATE TABLE IF NOT EXISTS accounts (
  id UUID PRIMARY KEY,
  oidc_subject TEXT NOT NULL UNIQUE,
  tier TEXT NOT NULL DEFAULT 'free' CHECK (tier IN ('free', 'plus', 'pro')),
  quota_bytes BIGINT NOT NULL CHECK (quota_bytes >= 0),
  used_bytes BIGINT NOT NULL DEFAULT 0 CHECK (used_bytes >= 0),
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE TABLE IF NOT EXISTS notes (
  id UUID PRIMARY KEY,
  owner_id UUID NOT NULL REFERENCES accounts(id),
  path TEXT NOT NULL,
  format TEXT NOT NULL CHECK (format IN ('md','txt')),
  object_key TEXT NOT NULL, -- 본문 객체 키, 경로가 아님
  content_sha256 TEXT NOT NULL,
  version BIGINT NOT NULL CHECK (version > 0),
  modified_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  UNIQUE (owner_id,path)
);
CREATE INDEX IF NOT EXISTS notes_by_owner_modified ON notes(owner_id, modified_at);
-- 주의: 이 파일은 배포 후 수정 금지(sqlx 체크섬). 변경은 새 번호 파일로. RLS·할당량 예약은 후속.

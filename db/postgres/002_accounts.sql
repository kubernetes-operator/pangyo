-- === 002: 로컬 계정(이메일+비밀번호), 로그인 세션, 장비 등록 (로드맵 05, 2026-11-07) ===
-- 로그인 방식: 지금은 'local'(DB). Google(OIDC)은 auth_provider='google' + oidc_subject로 후속 추가.
-- 비밀번호는 Argon2id 해시만, 세션/장비/등록 코드 토큰은 SHA-256 해시만 저장 (원문 저장 안 함).
ALTER TABLE accounts ALTER COLUMN oidc_subject DROP NOT NULL;
ALTER TABLE accounts ADD COLUMN email TEXT;
ALTER TABLE accounts ADD COLUMN display_name TEXT NOT NULL DEFAULT '';
ALTER TABLE accounts ADD COLUMN password_hash TEXT;
ALTER TABLE accounts ADD COLUMN auth_provider TEXT NOT NULL DEFAULT 'local'
  CHECK (auth_provider IN ('local', 'google'));
CREATE UNIQUE INDEX accounts_email_unique ON accounts (lower(email)) WHERE email IS NOT NULL;

CREATE TABLE sessions (
  id UUID PRIMARY KEY,
  account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  token_hash TEXT NOT NULL UNIQUE,
  user_agent TEXT NOT NULL DEFAULT '',
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  expires_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX sessions_by_account ON sessions(account_id);

CREATE TABLE devices (
  id UUID PRIMARY KEY,
  account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  platform TEXT NOT NULL,
  token_hash TEXT NOT NULL UNIQUE,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  last_seen_at TIMESTAMPTZ,
  revoked_at TIMESTAMPTZ
);
CREATE INDEX devices_by_account ON devices(account_id);

CREATE TABLE pairing_codes (
  code_hash TEXT PRIMARY KEY,
  account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  expires_at TIMESTAMPTZ NOT NULL,
  used_at TIMESTAMPTZ
);

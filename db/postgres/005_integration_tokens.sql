-- === 005: 자동화(n8n 등) 개인 토큰 (로드맵 07, 요구 19) ===
-- 범위(scopes)와 만료가 있는 토큰. 원문은 발급 응답에서 한 번만 보여주고 SHA-256만 저장.
CREATE TABLE integration_tokens (
  id UUID PRIMARY KEY,
  account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  token_hash TEXT NOT NULL UNIQUE,
  scopes TEXT[] NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  expires_at TIMESTAMPTZ NOT NULL,
  last_used_at TIMESTAMPTZ,
  revoked_at TIMESTAMPTZ
);
CREATE INDEX integration_tokens_by_account ON integration_tokens(account_id);

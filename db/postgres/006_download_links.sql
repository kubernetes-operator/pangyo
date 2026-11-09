-- === 006: 설치 파일 다운로드 링크 (2026-11-09) ===
-- 로그인 사용자가 요청하면 짧게 유효한 임의 토큰 주소(/dl/<토큰>)를 발급. 실제 저장 경로는 노출하지 않음.
-- 토큰 원문은 저장하지 않고 SHA-256만 저장.
CREATE TABLE download_links (
  token_hash TEXT PRIMARY KEY,
  account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  file_id TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  expires_at TIMESTAMPTZ NOT NULL,
  first_used_at TIMESTAMPTZ
);
CREATE INDEX download_links_expires ON download_links (expires_at);

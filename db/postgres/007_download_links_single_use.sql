-- === 007: 다운로드 링크 일회용 (로드맵 10, 2026-11-12) ===
-- claimed_at: 받는 중(다른 요청은 거절). 끝까지 보내면 completed_at 기록 → 그 뒤로는 사용 불가.
-- 중간에 끊기면 claimed_at을 비워 유효 시간(5분) 안에서 다시 받을 수 있게 함.
ALTER TABLE download_links
  ADD COLUMN claimed_at TIMESTAMPTZ,
  ADD COLUMN completed_at TIMESTAMPTZ;

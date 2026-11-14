-- === 012: 같은 기기 다시 등록 = 기존 장비 기록 이어 쓰기 (2026-11-13 사용 의견) ===
-- device_key_hash: 앱이 보낸 기기 고유값(Android ID, PC 머신 ID를 앱에서 한 번 해시)을 계정과 함께 다시 해시한 값.
--   원래 값은 서버에 남지 않고, 계정이 다르면 같은 기기라도 값이 다름.
-- pairing_codes.device_id: 웹 장비 목록에서 고른 장비의 다시 연결용 코드 (연결하면 그 장비 기록을 씀)
ALTER TABLE devices ADD COLUMN device_key_hash TEXT;
CREATE UNIQUE INDEX devices_account_key ON devices (account_id, device_key_hash) WHERE device_key_hash IS NOT NULL;
ALTER TABLE pairing_codes ADD COLUMN device_id UUID REFERENCES devices(id) ON DELETE CASCADE;

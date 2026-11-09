//! 비밀번호 해시, 토큰 생성/해시, 요청 제한, 입력 검증
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::http::HeaderMap;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::{Rng, RngCore};
use sha2::{Digest, Sha256};

// === 0. 보안 정책 변수 ===
/// 비밀번호 최소/최대 길이 (문자 수)
pub const PASSWORD_MIN: usize = 10;
pub const PASSWORD_MAX: usize = 256;
/// 장비 등록 코드: 헷갈리는 문자(0/O, 1/I/L) 제외, 8자리
const PAIRING_ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
const PAIRING_CODE_LEN: usize = 8;
/// 상태를 바꾸는 요청(POST/PUT/DELETE)에 반드시 있어야 하는 헤더 (CSRF 방지: 다른 사이트는 이 헤더를 붙일 수 없음)
pub const CSRF_HEADER: &str = "x-memonest-request";

pub fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    Ok(Argon2::default()
        .hash_password(password.as_bytes(), &salt)?
        .to_string())
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .map(|parsed| {
            Argon2::default()
                .verify_password(password.as_bytes(), &parsed)
                .is_ok()
        })
        .unwrap_or(false)
}

/// 없는 이메일로 로그인할 때도 비슷한 시간이 걸리도록 비교용 해시를 한 번 계산합니다.
pub fn burn_password_check(password: &str) {
    let _ = hash_password(password);
}

/// 32바이트 무작위 토큰 (URL-safe base64)
pub fn new_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn new_pairing_code() -> String {
    let mut rng = rand::rngs::OsRng;
    (0..PAIRING_CODE_LEN)
        .map(|_| PAIRING_ALPHABET[rng.gen_range(0..PAIRING_ALPHABET.len())] as char)
        .collect()
}

/// DB에는 토큰 원문 대신 SHA-256만 저장합니다.
pub fn token_hash(token: &str) -> String {
    Sha256::digest(token.trim().as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub fn normalize_email(raw: &str) -> Option<String> {
    let email = raw.trim().to_lowercase();
    let (local, domain) = email.split_once('@')?;
    let valid = !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && email.len() <= 254
        && !email.chars().any(|c| c.is_whitespace() || c.is_control());
    valid.then_some(email)
}

pub fn password_policy_error(password: &str) -> Option<String> {
    let n = password.chars().count();
    if n < PASSWORD_MIN {
        Some(format!("비밀번호는 {PASSWORD_MIN}자 이상이어야 합니다"))
    } else if n > PASSWORD_MAX {
        Some(format!("비밀번호는 {PASSWORD_MAX}자 이하여야 합니다"))
    } else {
        None
    }
}

/// Cloudflare/게이트웨이 뒤에서 실제 접속 IP (요청 제한 키). 직접 접속 시 위조 가능하므로 제한 용도로만 사용.
pub fn client_key(headers: &HeaderMap) -> String {
    headers
        .get("cf-connecting-ip")
        .or_else(|| headers.get("x-real-ip"))
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
        .or_else(|| {
            headers
                .get("x-forwarded-for")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.split(',').next())
                .map(|v| v.trim().to_owned())
        })
        .unwrap_or_else(|| "unknown".to_owned())
}

/// 고정 창 방식의 단순 요청 제한 (서버 1대 기준. 여러 대로 늘리면 Redis 등 공유 저장소 필요)
pub struct RateLimiter {
    window: Duration,
    limit: u32,
    hits: Mutex<HashMap<String, (Instant, u32)>>,
}

impl RateLimiter {
    pub fn new(window: Duration, limit: u32) -> Self {
        Self {
            window,
            limit,
            hits: Mutex::new(HashMap::new()),
        }
    }

    /// 허용되면 true
    pub fn check(&self, key: &str) -> bool {
        let now = Instant::now();
        let mut hits = self.hits.lock().unwrap_or_else(|p| p.into_inner());
        if hits.len() > 10_000 {
            hits.retain(|_, (start, _)| now.duration_since(*start) < self.window);
        }
        let entry = hits.entry(key.to_owned()).or_insert((now, 0));
        if now.duration_since(entry.0) >= self.window {
            *entry = (now, 0);
        }
        entry.1 += 1;
        entry.1 <= self.limit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_roundtrip() {
        let h = hash_password("correct horse battery").unwrap();
        assert!(h.starts_with("$argon2id$"));
        assert!(verify_password("correct horse battery", &h));
        assert!(!verify_password("wrong password!!", &h));
        assert!(!verify_password("x", "not-a-hash"));
    }

    #[test]
    fn email_normalization() {
        assert_eq!(
            normalize_email("  User@Example.COM "),
            Some("user@example.com".into())
        );
        for bad in [
            "",
            "user",
            "@example.com",
            "a@b",
            "a@.com",
            "a b@example.com",
        ] {
            assert_eq!(normalize_email(bad), None, "{bad}");
        }
    }

    #[test]
    fn tokens_and_codes() {
        let t = new_token();
        assert_eq!(t.len(), 43);
        assert_ne!(t, new_token());
        assert_eq!(token_hash(&t).len(), 64);
        let c = new_pairing_code();
        assert_eq!(c.len(), 8);
        assert!(c.bytes().all(|b| PAIRING_ALPHABET.contains(&b)));
    }

    #[test]
    fn rate_limiter_blocks_after_limit() {
        let l = RateLimiter::new(Duration::from_secs(60), 2);
        assert!(l.check("a") && l.check("a"));
        assert!(!l.check("a"));
        assert!(l.check("b"));
    }
}

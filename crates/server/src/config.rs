//! 서버 설정: 모두 환경변수로 주입합니다 (Helm values / .env). 비밀값은 Kubernetes Secret에서만.
use std::{env, path::PathBuf, time::Duration};

// === 0. 설정 기본값 ===
/// 총 계정 수 상한 (요구 17: 1000명 미만)
pub const DEFAULT_MAX_ACCOUNTS: i64 = 999;
/// 등급별 저장 용량 기본값(MB). 상품 정책 확정 전 임시값 (docs/06_SECURITY.md)
pub const DEFAULT_FREE_STORAGE_MB: i64 = 100;
/// 로그인 세션 유지 기간
pub const DEFAULT_SESSION_DAYS: i64 = 30;
/// 장비 등록 코드 유효 시간
pub const PAIRING_CODE_TTL: Duration = Duration::from_secs(10 * 60);
/// 편집 임대 기본 유지 시간 (ADR-0003). 클라이언트는 이보다 짧은 주기로 갱신
pub const DEFAULT_LEASE_TTL_SECS: i64 = 90;
/// 메모 한 개 본문 최대 크기 (요청 본문 제한 1MB 안에서)
pub const NOTE_CONTENT_MAX_BYTES: usize = 900 * 1024;
/// 변경 목록 한 번에 돌려줄 최대 개수
pub const CHANGES_PAGE_MAX: i64 = 200;
/// 설치 파일 다운로드 링크 유효 시간
pub const DOWNLOAD_LINK_TTL_SECS: i64 = 300;
/// 휴지통 보관 기간(일) 기본값 (2026-11-13 사용 의견: 1주일)
pub const DEFAULT_TRASH_DAYS: i64 = 7;
/// 로그인/가입/장비 등록 요청 제한: 같은 IP에서 이 시간 동안 이 횟수까지
pub const AUTH_RATE_WINDOW: Duration = Duration::from_secs(5 * 60);
pub const AUTH_RATE_LIMIT: u32 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Registration {
    /// 누구나 가입 (계정 수 상한까지)
    Open,
    /// 가입 차단 (기존 계정 로그인만)
    Closed,
}

#[derive(Debug, Clone)]
pub struct Config {
    /// `DATABASE_URL` — 없으면 계정 기능 없이 health/info만 동작
    pub database_url: Option<String>,
    /// `MAX_ACCOUNTS`
    pub max_accounts: i64,
    /// `FREE_STORAGE_MB`
    pub free_storage_mb: i64,
    /// `REGISTRATION` = open | closed
    pub registration: Registration,
    /// `SESSION_DAYS`
    pub session_days: i64,
    /// `COOKIE_SECURE` — 로컬 http 개발에서만 false
    pub cookie_secure: bool,
    /// `WEB_ROOT` — 웹 앱 빌드(dist) 폴더. 있으면 `/`에서 웹 앱을 함께 제공
    pub web_root: Option<PathBuf>,
    /// `LEASE_TTL_SECS` — 편집 임대 유지 시간
    pub lease_ttl_secs: i64,
    /// `DOWNLOADS_DIR` — 설치 파일 폴더(manifest.json 포함). 없으면 다운로드 기능 끔
    pub downloads_dir: Option<PathBuf>,
    /// `ADMIN_EMAILS` — 관리자 이메일(쉼표 구분, 소문자 정규화). Kubernetes에서는 사용자가 만든 Secret에서 주입.
    /// 비어 있으면 관리자 화면은 아무에게도 열리지 않음
    pub admin_emails: Vec<String>,
    /// `TRASH_DAYS` — 지운 메모·프로젝트 보관 기간(일)
    pub trash_days: i64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            database_url: None,
            max_accounts: DEFAULT_MAX_ACCOUNTS,
            free_storage_mb: DEFAULT_FREE_STORAGE_MB,
            registration: Registration::Open,
            session_days: DEFAULT_SESSION_DAYS,
            cookie_secure: true,
            web_root: None,
            lease_ttl_secs: DEFAULT_LEASE_TTL_SECS,
            downloads_dir: None,
            admin_emails: Vec::new(),
            trash_days: DEFAULT_TRASH_DAYS,
        }
    }
}

impl Config {
    pub fn from_env() -> Self {
        let d = Self::default();
        let num = |key: &str, default: i64| {
            env::var(key)
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(default)
        };
        Self {
            database_url: env::var("DATABASE_URL").ok().filter(|v| !v.is_empty()),
            max_accounts: num("MAX_ACCOUNTS", d.max_accounts),
            free_storage_mb: num("FREE_STORAGE_MB", d.free_storage_mb),
            registration: match env::var("REGISTRATION").as_deref() {
                Ok("closed") => Registration::Closed,
                _ => Registration::Open,
            },
            session_days: num("SESSION_DAYS", d.session_days),
            cookie_secure: env::var("COOKIE_SECURE").map_or(true, |v| v != "false"),
            web_root: env::var_os("WEB_ROOT").map(PathBuf::from),
            lease_ttl_secs: num("LEASE_TTL_SECS", d.lease_ttl_secs).max(1),
            downloads_dir: env::var_os("DOWNLOADS_DIR").map(PathBuf::from),
            admin_emails: parse_admin_emails(&env::var("ADMIN_EMAILS").unwrap_or_default()),
            trash_days: num("TRASH_DAYS", d.trash_days).clamp(1, 365),
        }
    }

    pub fn is_admin_email(&self, email: Option<&str>) -> bool {
        email.is_some_and(|e| {
            let e = e.trim().to_lowercase();
            self.admin_emails.contains(&e)
        })
    }

    pub fn free_quota_bytes(&self) -> i64 {
        self.free_storage_mb.saturating_mul(1024 * 1024)
    }
}

/// 쉼표·공백으로 구분된 관리자 이메일 목록 (빈 항목 제외, 소문자)
pub fn parse_admin_emails(raw: &str) -> Vec<String> {
    raw.split([',', ' ', '\n', ';'])
        .map(|e| e.trim().to_lowercase())
        .filter(|e| e.contains('@'))
        .collect()
}

#[cfg(test)]
mod admin_tests {
    #[test]
    fn admin_emails_are_normalized() {
        let c = super::Config {
            admin_emails: super::parse_admin_emails(" Admin@Example.com, ,ops@example.com\n"),
            ..Default::default()
        };
        assert_eq!(c.admin_emails, ["admin@example.com", "ops@example.com"]);
        assert!(c.is_admin_email(Some("ADMIN@example.com")));
        assert!(!c.is_admin_email(Some("user@example.com")));
        assert!(!c.is_admin_email(None));
        assert!(!super::Config::default().is_admin_email(Some("admin@example.com")));
    }
}

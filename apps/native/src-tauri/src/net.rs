//! 서버 연결용 HTTP 클라이언트 설정 (프록시·인증서·연결 실패 원인)
//!
//! 2026-11-16 사용 의견: 회사 Windows 서버에서 브라우저 로그인은 되는데 앱은 "서버에 연결할 수 없습니다".
//! 앱은 브라우저와 달리 OS 프록시 설정과 OS 인증서 저장소를 쓰지 않았기 때문입니다. 그래서
//! 1. 연결 실패 원인을 분류해 화면에 전달하고 (`offline: <원인> | <상세>`, 화면 account/offline-reason.ts)
//! 2. Windows 프록시 설정(인터넷 옵션 → LAN 설정의 프록시 서버)을 따르고 (자동 구성 스크립트 PAC는 미지원)
//! 3. 데스크톱은 내장 루트 인증서 목록에 OS 인증서 저장소의 루트를 더해 신뢰합니다
//!    (보안 장비의 HTTPS 검사용 회사 인증서). 내장 목록은 그대로라 기존에 되던 연결은 그대로 됩니다.
//!
//! 프록시 주소는 환경 변수(`HTTPS_PROXY` 등)를 먼저, 없으면 Windows 설정을 씁니다.
//! 오류 문구에는 프록시의 호스트·포트만 넣고 사용자 이름·비밀번호는 넣지 않습니다.

use ureq::config::ConfigBuilder;
use ureq::typestate::AgentScope;
use ureq::{Proxy, ProxyProtocol};

/// 연결 실패 문자열의 앞부분 (remote.rs OFFLINE_PREFIX와 같음 → 화면이 '서버 연결 안 됨'으로 처리)
const OFFLINE_PREFIX: &str = crate::remote::OFFLINE_PREFIX;
/// 프록시를 거치지 않을 주소 (개발용 로컬 서버)
const ALWAYS_DIRECT: [&str; 2] = ["localhost", "127.0.0.1"];
/// 화면에 보낼 상세 문구 최대 길이
const MAX_DETAIL: usize = 300;

/// 공통 설정 (프록시 + 인증서). 호출하는 쪽에서 시간 제한 등을 더 붙입니다.
pub fn config_builder() -> ConfigBuilder<AgentScope> {
    ureq::Agent::config_builder()
        .user_agent(concat!("MemoNest/", env!("CARGO_PKG_VERSION")))
        .proxy(proxy())
        .tls_config(tls::config())
}

/// 연결 실패 → `offline: <원인> | <상세>` (원인: dns, timeout, tls, proxy, refused, network)
pub fn offline_error(e: &ureq::Error) -> String {
    let mut detail = e.to_string();
    if let Some(p) = proxy() {
        detail.push_str(&format!(" (proxy {}:{})", p.host(), p.port()));
    }
    let detail: String = detail.chars().take(MAX_DETAIL).collect();
    format!("{OFFLINE_PREFIX} {} | {detail}", offline_reason(e))
}

/// ureq 오류 → 화면이 안내 문구를 고를 원인 이름
fn offline_reason(e: &ureq::Error) -> &'static str {
    use std::io::ErrorKind;
    use ureq::Error;
    match e {
        Error::HostNotFound => "dns",
        Error::Timeout(_) => "timeout",
        Error::Tls(_) | Error::Rustls(_) | Error::TlsRequired => "tls",
        Error::ConnectProxyFailed(_) | Error::InvalidProxyUrl => "proxy",
        Error::Io(io) => match io.kind() {
            ErrorKind::ConnectionRefused => "refused",
            ErrorKind::TimedOut | ErrorKind::WouldBlock => "timeout",
            // 주소 찾기 실패는 OS 오류 문구로 옴 (Linux/macOS/Windows)
            _ if is_dns_message(&io.to_string()) => "dns",
            // 인증서 확인 실패는 TLS 연결 중 입출력 오류로 감싸져 옴
            _ if is_tls_message(&io.to_string()) => "tls",
            _ => "network",
        },
        _ => "network",
    }
}

fn is_dns_message(text: &str) -> bool {
    let text = text.to_ascii_lowercase();
    [
        "lookup address",            // Rust 표준 라이브러리
        "name or service not known", // Linux
        "temporary failure in name resolution",
        "nodename nor servname", // macOS
        "no such host",          // Windows (os error 11001)
    ]
    .iter()
    .any(|w| text.contains(w))
}

fn is_tls_message(text: &str) -> bool {
    let text = text.to_ascii_lowercase();
    ["certificate", "handshake", "tls", "alert"]
        .iter()
        .any(|w| text.contains(w))
}

// === 프록시 ===

fn proxy() -> Option<Proxy> {
    Proxy::try_from_env().or_else(system_proxy)
}

/// Windows 인터넷 옵션의 프록시 (현재 사용자). 사용 안 함이거나 읽을 수 없으면 None
#[cfg(windows)]
fn system_proxy() -> Option<Proxy> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};
    use winreg::RegKey;
    const KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";
    let settings = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(KEY, KEY_READ)
        .ok()?;
    let enabled: u32 = settings.get_value("ProxyEnable").ok()?;
    if enabled != 1 {
        return None;
    }
    let server: String = settings.get_value("ProxyServer").ok()?;
    let bypass: String = settings.get_value("ProxyOverride").unwrap_or_default();
    build_proxy(&server, &bypass)
}

#[cfg(not(windows))]
fn system_proxy() -> Option<Proxy> {
    None
}

/// Windows `ProxyServer` 값 + `ProxyOverride`(예외 목록) → ureq 프록시
#[cfg_attr(not(windows), allow(dead_code))]
fn build_proxy(server: &str, bypass: &str) -> Option<Proxy> {
    let (host, port) = proxy_address(server)?;
    let mut builder = Proxy::builder(ProxyProtocol::Http).host(&host).port(port);
    for entry in ALWAYS_DIRECT.into_iter().chain(bypass_entries(bypass)) {
        builder = builder.no_proxy(entry);
    }
    builder.build().ok()
}

/// `ProxyServer` 값에서 HTTPS 요청에 쓸 프록시 (호스트, 포트).
/// 형식: `host:port` (모든 프로토콜 같은 프록시) 또는 `http=host:port;https=host:port;...`
fn proxy_address(server: &str) -> Option<(String, u16)> {
    let entries: Vec<&str> = server
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    let per_scheme = |scheme: &str| {
        entries.iter().find_map(|e| {
            e.split_once('=')
                .filter(|(k, _)| k.trim().eq_ignore_ascii_case(scheme))
                .map(|(_, v)| v.trim())
        })
    };
    let raw = if entries.iter().any(|e| e.contains('=')) {
        per_scheme("https").or_else(|| per_scheme("http"))?
    } else {
        *entries.first()?
    };
    let raw = raw
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_end_matches('/');
    let (host, port) = match raw.rsplit_once(':') {
        Some((host, port)) => (host, port.parse().ok()?),
        None => (raw, 80),
    };
    (!host.is_empty()).then(|| (host.to_owned(), port))
}

/// `ProxyOverride`(세미콜론 구분) → 예외 주소. `<local>`(점 없는 이름)은 표현할 수 없어 뺍니다.
fn bypass_entries(bypass: &str) -> impl Iterator<Item = &str> {
    bypass
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != "<local>")
}

// === 인증서 ===

#[cfg(desktop)]
mod tls {
    use ureq::tls::{Certificate, RootCerts, TlsConfig};

    pub fn config() -> TlsConfig {
        TlsConfig::builder().root_certs(root_certs()).build()
    }

    /// 내장 목록(Mozilla, 지금까지 쓰던 것과 같음) + OS 인증서 저장소의 루트
    pub fn root_certs() -> RootCerts {
        let mut certs: Vec<Certificate<'static>> = webpki_root_certs::TLS_SERVER_ROOT_CERTS
            .iter()
            .map(|c| Certificate::from_der(c.as_ref()))
            .collect();
        // 일부를 읽지 못해도 읽은 것은 씀 (내장 목록은 항상 있음)
        let native = rustls_native_certs::load_native_certs();
        certs.extend(
            native
                .certs
                .iter()
                .map(|c| Certificate::from_der(c.as_ref()).to_owned()),
        );
        RootCerts::from(certs)
    }
}

/// 모바일: 내장 목록만 (Android는 OS 저장소를 읽으려면 별도 JNI 연동이 필요)
#[cfg(mobile)]
mod tls {
    pub fn config() -> ureq::tls::TlsConfig {
        ureq::tls::TlsConfig::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_address_formats() {
        assert_eq!(
            proxy_address("proxy.example:8080"),
            Some(("proxy.example".into(), 8080))
        );
        assert_eq!(
            proxy_address("http=p1.example:3128;https=p2.example:8443;ftp=p3:21"),
            Some(("p2.example".into(), 8443))
        );
        assert_eq!(
            proxy_address("http=p1.example:3128"),
            Some(("p1.example".into(), 3128))
        );
        assert_eq!(
            proxy_address("http://proxy.example:8080/"),
            Some(("proxy.example".into(), 8080))
        );
        assert_eq!(
            proxy_address("proxy.example"),
            Some(("proxy.example".into(), 80))
        );
        assert_eq!(proxy_address("ftp=p3:21"), None);
        assert_eq!(proxy_address(""), None);
        assert_eq!(proxy_address("proxy.example:abc"), None);
    }

    #[test]
    fn bypass_list_skips_local_marker() {
        let entries: Vec<&str> = bypass_entries("*.corp.example; 10.0.0.1;<local>;").collect();
        assert_eq!(entries, ["*.corp.example", "10.0.0.1"]);
    }

    #[test]
    fn windows_proxy_setting_builds_proxy_with_bypass() {
        let proxy = build_proxy("proxy.example:8080", "*.corp.example;<local>").unwrap();
        assert_eq!((proxy.host(), proxy.port()), ("proxy.example", 8080));
        let direct = |u: &str| proxy.is_no_proxy(&u.parse().unwrap());
        assert!(direct("http://localhost:8080/api"));
        assert!(direct("https://notes.corp.example/"));
        assert!(!direct("https://note.example.com/"));
    }

    #[test]
    fn offline_reasons() {
        use std::io::{Error as IoError, ErrorKind};
        use ureq::Error;
        assert_eq!(offline_reason(&Error::HostNotFound), "dns");
        assert_eq!(
            offline_reason(&Error::Timeout(ureq::Timeout::Connect)),
            "timeout"
        );
        assert_eq!(
            offline_reason(&Error::ConnectProxyFailed("407".into())),
            "proxy"
        );
        assert_eq!(
            offline_reason(&Error::Io(IoError::from(ErrorKind::ConnectionRefused))),
            "refused"
        );
        assert_eq!(
            offline_reason(&Error::Io(IoError::other(
                "invalid peer certificate: UnknownIssuer"
            ))),
            "tls"
        );
        assert_eq!(
            offline_reason(&Error::Io(IoError::from(ErrorKind::ConnectionReset))),
            "network"
        );
        for dns in [
            "failed to lookup address information: Name or service not known",
            "No such host is known. (os error 11001)",
        ] {
            assert_eq!(
                offline_reason(&Error::Io(IoError::other(dns))),
                "dns",
                "{dns}"
            );
        }
        let text = offline_error(&Error::HostNotFound);
        assert!(text.starts_with("offline: dns | "), "{text}");
    }

    #[cfg(desktop)]
    #[test]
    fn desktop_trusts_bundled_roots_and_more() {
        let ureq::tls::RootCerts::Specific(certs) = tls::root_certs() else {
            panic!("specific roots expected");
        };
        assert!(certs.len() >= webpki_root_certs::TLS_SERVER_ROOT_CERTS.len());
    }
}

//! 서버 연결 (로드맵 06 데스크톱·모바일, ADR-0004)
//!
//! - 장비 등록: 웹에서 받은 일회용 코드를 `POST /api/v1/pairing/finish`로 장비 토큰과 바꿉니다.
//! - 장비 토큰은 **화면(JS)에 넘기지 않습니다.** 데스크톱은 OS 보안 저장소(keyring),
//!   모바일은 앱 전용 폴더(다른 앱 접근 불가)에 둡니다.
//! - 화면의 API 요청은 [`server_request`]가 대신 보내고 `Authorization: Bearer`를 붙입니다.
//!   그래서 서버 CORS나 웹뷰 CSP를 넓히지 않습니다.
//! - 서버 주소는 등록할 때 사용자가 입력합니다 (저장소·설치 파일에 도메인을 넣지 않음).

use std::{path::PathBuf, time::Duration};

use serde::{Deserialize, Serialize};
use serde_json::Value;

// === 0. 서버 연결 설정 변수 (자격 증명 없음) ===
/// API 경로 접두사 (서버 crates/server/src/lib.rs와 같음)
const API_PREFIX: &str = "/api/v1";
/// 화면이 이 명령으로 부를 수 있는 API 경로 (장비 토큰으로 쓸 일만. 계정·장비 관리는 웹에서)
const ALLOWED_PATHS: [&str; 5] = ["/notes", "/projects", "/search", "/account", "/info"]; // /projects: ADR-0005
/// 상태를 바꾸는 요청에 붙이는 CSRF 방지 헤더 (서버 security.rs CSRF_HEADER)
const CSRF_HEADER: (&str, &str) = ("X-MemoNest-Request", "1");
/// 서버 응답 대기 한도 (웹 account/api.ts REQUEST_TIMEOUT_MS와 같음).
/// 국내→Cloudflare 미국 엣지 경유로 응답이 최대 9초까지 걸려(2026-11-12 측정) 30초. 편집 임대(90초)보다 짧게
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// 응답 본문 최대 크기 (서버 본문 제한 1MB + 여유)
const MAX_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;
/// OS 보안 저장소 항목 이름 (데스크톱)
#[cfg(desktop)]
const KEYRING_SERVICE: &str = "dev.memonest.notes";
#[cfg(desktop)]
const KEYRING_USER: &str = "device-token";
/// 모바일: 앱 전용 데이터 폴더 안의 토큰 파일
#[cfg(mobile)]
const TOKEN_FILE: &str = "device-token";
/// 등록 정보 파일 (서버 주소, 장비 ID/이름 — 비밀 아님). 앱 데이터 폴더 안
const DEVICE_FILE: &str = "device.json";

/// 화면에 보여줄 등록 상태 (토큰은 포함하지 않음)
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub server_url: String,
    pub device_id: String,
    pub device_name: String,
    pub paired_at: String,
}

/// 화면으로 돌려주는 서버 응답 (account/api.ts가 fetch 응답처럼 해석)
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerResponse {
    pub status: u16,
    /// JSON이 아니면 None (정적 서버 등 → 화면은 '서버 없음'으로 처리)
    pub body: Option<Value>,
}

/// 명령 오류: `offline:`으로 시작하면 화면이 '서버 연결 안 됨'으로 처리
pub type RemoteResult<T> = Result<T, String>;
pub const OFFLINE_PREFIX: &str = "offline:";
pub const NOT_PAIRED: &str = "not_paired";

pub struct Remote {
    data_dir: PathBuf,
    agent: ureq::Agent,
}

impl Remote {
    pub fn new(data_dir: PathBuf) -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(REQUEST_TIMEOUT))
            .http_status_as_error(false) // 4xx/5xx도 본문(error 코드)을 화면에 전달
            .user_agent(concat!("MemoNest/", env!("CARGO_PKG_VERSION")))
            .build();
        Self {
            data_dir,
            agent: config.into(),
        }
    }

    pub fn status(&self) -> Option<DeviceInfo> {
        let raw = std::fs::read_to_string(self.data_dir.join(DEVICE_FILE)).ok()?;
        let info: DeviceInfo = serde_json::from_str(&raw).ok()?;
        token::load(&self.data_dir).ok().flatten().map(|_| info)
    }

    /// 등록 코드 → 장비 토큰. 성공하면 토큰은 보안 저장소에, 주소·ID는 device.json에.
    pub fn pair(
        &self,
        server_url: &str,
        code: &str,
        device_name: &str,
        platform: &str,
        now: String,
    ) -> RemoteResult<DeviceInfo> {
        let base = normalize_server_url(server_url)?;
        // 등록 코드는 한 번만 쓸 수 있으므로, 서버에 보내기 전에 토큰을 저장할 수 있는지 먼저 확인
        token::check(&self.data_dir)?;
        let body = serde_json::json!({ "code": code.trim(), "deviceName": device_name.trim(), "platform": platform });
        let response = self.send(
            "POST",
            &format!("{base}{API_PREFIX}/pairing/finish"),
            Some(&body),
            None,
        )?;
        let data = response.body.unwrap_or(Value::Null);
        if response.status != 201 {
            let message = data["message"]
                .as_str()
                .unwrap_or("장비 등록에 실패했습니다");
            return Err(format!(
                "{}: {message}",
                data["error"].as_str().unwrap_or("pairing_failed")
            ));
        }
        let (Some(device_id), Some(token_value)) =
            (data["deviceId"].as_str(), data["token"].as_str())
        else {
            return Err("pairing_failed: 서버 응답 형식이 올바르지 않습니다".into());
        };
        token::store(&self.data_dir, token_value)?;
        let info = DeviceInfo {
            server_url: base,
            device_id: device_id.to_owned(),
            device_name: device_name.trim().to_owned(),
            paired_at: now,
        };
        std::fs::create_dir_all(&self.data_dir).map_err(|e| e.to_string())?;
        let json = serde_json::to_string_pretty(&info).map_err(|e| e.to_string())?;
        std::fs::write(self.data_dir.join(DEVICE_FILE), json).map_err(|e| e.to_string())?;
        Ok(info)
    }

    /// 이 기기의 등록 정보를 지웁니다 (서버의 장비 폐기는 웹 계정 화면에서).
    pub fn unpair(&self) -> RemoteResult<()> {
        token::clear(&self.data_dir)?;
        match std::fs::remove_file(self.data_dir.join(DEVICE_FILE)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }

    /// 화면의 API 요청을 장비 토큰으로 보냅니다. `path`는 `/api/v1` 뒤 부분 (예: `/notes?since=0`).
    pub fn request(
        &self,
        method: &str,
        path: &str,
        body: Option<&Value>,
    ) -> RemoteResult<ServerResponse> {
        check_path(path)?;
        let info = self.status().ok_or(NOT_PAIRED)?;
        let token = token::load(&self.data_dir)?.ok_or(NOT_PAIRED)?;
        self.send(
            method,
            &format!("{}{API_PREFIX}{path}", info.server_url),
            body,
            Some(&token),
        )
    }

    fn send(
        &self,
        method: &str,
        url: &str,
        body: Option<&Value>,
        token: Option<&str>,
    ) -> RemoteResult<ServerResponse> {
        let method = method.to_ascii_uppercase();
        let auth = token.map(|t| format!("Bearer {t}"));
        let json = body
            .map(serde_json::to_vec)
            .transpose()
            .map_err(|e| e.to_string())?;
        let send_json = |req: ureq::RequestBuilder<ureq::typestate::WithBody>, data: &[u8]| {
            req.header("Content-Type", "application/json").send(data)
        };
        let result = match (method.as_str(), json.as_deref()) {
            ("GET", _) => with_headers(self.agent.get(url), auth.as_deref(), false).call(),
            ("DELETE", _) => with_headers(self.agent.delete(url), auth.as_deref(), true).call(),
            ("POST", Some(b)) => {
                send_json(with_headers(self.agent.post(url), auth.as_deref(), true), b)
            }
            ("POST", None) => {
                with_headers(self.agent.post(url), auth.as_deref(), true).send_empty()
            }
            ("PUT", Some(b)) => {
                send_json(with_headers(self.agent.put(url), auth.as_deref(), true), b)
            }
            ("PUT", None) => with_headers(self.agent.put(url), auth.as_deref(), true).send_empty(),
            _ => return Err(format!("지원하지 않는 요청: {method}")),
        };
        let mut response = result.map_err(|e| format!("{OFFLINE_PREFIX} {e}"))?;
        let status = response.status().as_u16();
        let is_json = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.contains("application/json"));
        let body = if status == 204 || !is_json {
            None
        } else {
            let text = response
                .body_mut()
                .with_config()
                .limit(MAX_RESPONSE_BYTES)
                .read_to_string()
                .map_err(|e| format!("{OFFLINE_PREFIX} {e}"))?;
            serde_json::from_str(&text).ok()
        };
        Ok(ServerResponse { status, body })
    }
}

fn with_headers<B>(
    req: ureq::RequestBuilder<B>,
    auth: Option<&str>,
    csrf: bool,
) -> ureq::RequestBuilder<B> {
    let req = req.header("Accept", "application/json");
    let req = match auth {
        Some(a) => req.header("Authorization", a),
        None => req,
    };
    if csrf {
        req.header(CSRF_HEADER.0, CSRF_HEADER.1)
    } else {
        req
    }
}

/// 서버 주소 정리: https만 (개발용 localhost/127.0.0.1은 http 허용), 경로·쿼리 없이 끝의 `/` 제거
pub fn normalize_server_url(raw: &str) -> RemoteResult<String> {
    let url = raw.trim().trim_end_matches('/');
    let lower = url.to_ascii_lowercase();
    let rest = if let Some(r) = lower.strip_prefix("https://") {
        r
    } else if let Some(r) = lower.strip_prefix("http://") {
        let host = r.split([':', '/']).next().unwrap_or("");
        if !matches!(host, "localhost" | "127.0.0.1" | "[::1]") {
            return Err("invalid_server: 서버 주소는 https:// 로 시작해야 합니다".into());
        }
        r
    } else {
        return Err("invalid_server: 서버 주소는 https:// 로 시작해야 합니다".into());
    };
    if rest.is_empty() || rest.contains(['/', '?', '#', '@', ' ']) {
        return Err(
            "invalid_server: 서버 주소에는 도메인만 입력하세요 (예: https://notes.example.com)"
                .into(),
        );
    }
    Ok(url.to_owned())
}

/// 화면이 보낼 수 있는 경로만 통과 (토큰을 다른 API에 쓰지 못하게)
pub fn check_path(path: &str) -> RemoteResult<()> {
    let route = path.split('?').next().unwrap_or("");
    let ok = path.starts_with('/')
        && !route.contains("..")
        && !path.contains('#')
        && ALLOWED_PATHS
            .iter()
            .any(|p| route == *p || route.starts_with(&format!("{p}/")));
    if ok {
        Ok(())
    } else {
        Err(format!("허용되지 않은 API 경로: {route}"))
    }
}

/// 장비 토큰 보관
mod token {
    use std::path::Path;

    use super::RemoteResult;

    #[cfg(desktop)]
    fn entry() -> RemoteResult<keyring::Entry> {
        keyring::Entry::new(super::KEYRING_SERVICE, super::KEYRING_USER)
            .map_err(|e| format!("secure_storage: {e}"))
    }

    #[cfg(desktop)]
    pub fn store(_dir: &Path, token: &str) -> RemoteResult<()> {
        entry()?.set_password(token).map_err(|e| {
            format!("secure_storage: OS 보안 저장소에 토큰을 저장하지 못했습니다 ({e})")
        })
    }

    /// 보안 저장소에 쓰고 지울 수 있는지 시험 (등록 코드를 쓰기 전에)
    #[cfg(desktop)]
    pub fn check(_dir: &Path) -> RemoteResult<()> {
        let probe = keyring::Entry::new(super::KEYRING_SERVICE, "probe")
            .map_err(|e| format!("secure_storage: {e}"))?;
        probe
            .set_password("probe")
            .and_then(|()| probe.delete_credential())
            .map_err(|e| format!("secure_storage: OS 보안 저장소를 쓸 수 없습니다 ({e})"))
    }

    #[cfg(desktop)]
    pub fn load(_dir: &Path) -> RemoteResult<Option<String>> {
        match entry()?.get_password() {
            Ok(t) => Ok(Some(t)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(format!("secure_storage: {e}")),
        }
    }

    #[cfg(desktop)]
    pub fn clear(_dir: &Path) -> RemoteResult<()> {
        match entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("secure_storage: {e}")),
        }
    }

    #[cfg(mobile)]
    pub fn store(dir: &Path, token: &str) -> RemoteResult<()> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        std::fs::write(dir.join(super::TOKEN_FILE), token).map_err(|e| e.to_string())
    }

    #[cfg(mobile)]
    pub fn check(dir: &Path) -> RemoteResult<()> {
        std::fs::create_dir_all(dir).map_err(|e| format!("secure_storage: {e}"))
    }

    #[cfg(mobile)]
    pub fn load(dir: &Path) -> RemoteResult<Option<String>> {
        match std::fs::read_to_string(dir.join(super::TOKEN_FILE)) {
            Ok(t) if !t.trim().is_empty() => Ok(Some(t.trim().to_owned())),
            Ok(_) => Ok(None),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    #[cfg(mobile)]
    pub fn clear(dir: &Path) -> RemoteResult<()> {
        match std::fs::remove_file(dir.join(super::TOKEN_FILE)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_url_rules() {
        assert_eq!(
            normalize_server_url(" https://notes.example.com/ ").unwrap(),
            "https://notes.example.com"
        );
        assert_eq!(
            normalize_server_url("https://notes.example.com:8443").unwrap(),
            "https://notes.example.com:8443"
        );
        assert_eq!(
            normalize_server_url("http://127.0.0.1:8080").unwrap(),
            "http://127.0.0.1:8080"
        );
        assert_eq!(
            normalize_server_url("http://localhost:5173").unwrap(),
            "http://localhost:5173"
        );
        for bad in [
            "http://notes.example.com",
            "notes.example.com",
            "https://",
            "https://a.com/path",
            "https://user@a.com",
            "ftp://a.com",
            "http://127.0.0.1.evil.com",
        ] {
            assert!(normalize_server_url(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn only_note_paths_are_allowed() {
        for ok in [
            "/notes?since=0",
            "/notes/abc",
            "/notes/abc/lease?clientId=x",
            "/search?q=a",
            "/account",
            "/info",
        ] {
            assert!(check_path(ok).is_ok(), "{ok}");
        }
        for bad in [
            "/devices",
            "/pairing/start",
            "/integrations/tokens",
            "/downloads",
            "notes",
            "/notes/../devices",
            "/notesx",
            "/auth/logout",
        ] {
            assert!(check_path(bad).is_err(), "{bad}");
        }
    }
}

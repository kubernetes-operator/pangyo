//! 계정·로그인·장비 등록·용량 API (로드맵 05). 로그인 방식: DB(이메일+비밀번호). Google은 후속.
//!
//! 인증 수단
//! - 웹 브라우저: HttpOnly 세션 쿠키 + 상태 변경 요청에 `X-MemoNest-Request: 1` 헤더 (CSRF 방지)
//! - 데스크톱/모바일/n8n: 장비 등록으로 받은 `Authorization: Bearer <토큰>`
use axum::{
    extract::{FromRequestParts, Path, Query, State},
    http::{request::Parts, HeaderMap, Method, StatusCode},
    Json,
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    config::{Registration, PAIRING_CODE_TTL},
    error::{ApiError, ApiJson, ApiResult},
    security::{self, CSRF_HEADER},
    AppState,
};

// === 0. 변수 ===
/// 세션 쿠키 이름
pub const SESSION_COOKIE: &str = "memonest_session";
/// 장비 이름/플랫폼 최대 길이
const DEVICE_NAME_MAX: usize = 80;
const DISPLAY_NAME_MAX: usize = 80;

// ---------- 요청/응답 모양 (packages/protocol/openapi.json 과 일치) ----------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterRequest {
    email: String,
    password: String,
    #[serde(default)]
    display_name: String,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    email: String,
    password: String,
}

#[derive(Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AccountView {
    id: Uuid,
    email: Option<String>,
    display_name: String,
    auth_provider: String,
    tier: String,
    quota_bytes: i64,
    used_bytes: i64,
    created_at: DateTime<Utc>,
    /// 기본 프로젝트 표시 이름 ('' = 화면 기본값, ADR-0005)
    default_project_name: String,
    /// 관리자 화면(저장공간 설정)을 보여 줄지. ADMIN_EMAILS 기준
    #[sqlx(skip)]
    is_admin: bool,
}

impl AccountView {
    fn with_admin(mut self, config: &crate::config::Config) -> Self {
        self.is_admin = config.is_admin_email(self.email.as_deref());
        self
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaView {
    tier: String,
    quota_bytes: i64,
    used_bytes: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingStartResponse {
    code: String,
    expires_at: DateTime<Utc>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingFinishRequest {
    code: String,
    device_name: String,
    platform: String,
    /// 기기 고유값(앱에서 한 번 해시한 값). 같은 계정·같은 값이면 기존 장비 기록을 이어 씀. 없으면 새 장비
    #[serde(default)]
    device_key: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingStartQuery {
    /// 이 장비로 다시 연결하는 코드 (웹 장비 목록에서 고름)
    #[serde(default)]
    device_id: Option<Uuid>,
}

/// 계정과 묶은 기기 고유값 해시 (원래 값은 저장하지 않음)
fn device_key_hash(account: Uuid, key: &str) -> Option<String> {
    let key = key.trim();
    (8..=128)
        .contains(&key.len())
        .then(|| security::token_hash(&format!("{account}:{key}")))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingFinishResponse {
    device_id: Uuid,
    /// 이 응답에서 한 번만 보여 줍니다. 장비는 OS 보안 저장소에 보관해야 합니다.
    token: String,
}

#[derive(Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct DeviceView {
    id: Uuid,
    name: String,
    platform: String,
    created_at: DateTime<Utc>,
    last_seen_at: Option<DateTime<Utc>>,
    revoked_at: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvidersView {
    local: bool,
    google: bool,
    registration: &'static str,
}

// ---------- 인증된 사용자 추출 ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthVia {
    Session(Uuid),
    Device(Uuid),
    /// 자동화 토큰(n8n 등): 범위가 제한됨
    Token {
        id: Uuid,
        read: bool,
        write: bool,
    },
}

/// 자동화 토큰이 접근할 수 있는 범위 (세션·장비는 모두 허용)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    NotesRead,
    NotesWrite,
    /// 계정·장비·토큰 관리 — 자동화 토큰으로는 불가
    Account,
}

/// 자동화 토큰 접두사 (장비 토큰과 구별, 유출 탐지 도구가 찾기 쉽게)
pub const INTEGRATION_TOKEN_PREFIX: &str = "mn_pat_";

/// 핸들러 인자로 쓰면 로그인한 사용자만 통과합니다.
pub struct CurrentAccount {
    pub id: Uuid,
    pub via: AuthVia,
}

impl CurrentAccount {
    /// 범위 검사: 자동화 토큰은 허용된 범위만, 세션·장비는 모두 허용
    pub fn require(&self, scope: Scope) -> ApiResult<()> {
        let allowed = match (self.via, scope) {
            (AuthVia::Token { read, .. }, Scope::NotesRead) => read,
            (AuthVia::Token { write, .. }, Scope::NotesWrite) => write,
            (AuthVia::Token { .. }, Scope::Account) => false,
            _ => true,
        };
        if allowed {
            Ok(())
        } else {
            Err(ApiError::forbidden(
                "insufficient_scope",
                "이 토큰으로는 할 수 없는 작업입니다",
            ))
        }
    }
}

impl FromRequestParts<AppState> for CurrentAccount {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> ApiResult<Self> {
        let db = state.db()?;
        if let Some(token) =
            bearer_token(&parts.headers).filter(|t| t.starts_with(INTEGRATION_TOKEN_PREFIX))
        {
            let row: Option<(Uuid, Uuid, Vec<String>)> = sqlx::query_as(
                "UPDATE integration_tokens SET last_used_at = NOW()
                 WHERE token_hash = $1 AND revoked_at IS NULL AND expires_at > NOW()
                 RETURNING id, account_id, scopes",
            )
            .bind(security::token_hash(token))
            .fetch_optional(db)
            .await?;
            let (id, account, scopes) = row.ok_or_else(ApiError::unauthorized)?;
            return Ok(Self {
                id: account,
                via: AuthVia::Token {
                    id,
                    read: scopes.iter().any(|s| s == "notes:read"),
                    write: scopes.iter().any(|s| s == "notes:write"),
                },
            });
        }
        if let Some(token) = bearer_token(&parts.headers) {
            let row: Option<(Uuid, Uuid)> = sqlx::query_as(
                "UPDATE devices SET last_seen_at = NOW()
                 WHERE token_hash = $1 AND revoked_at IS NULL RETURNING id, account_id",
            )
            .bind(security::token_hash(token))
            .fetch_optional(db)
            .await?;
            let (device, account) = row.ok_or_else(ApiError::unauthorized)?;
            return Ok(Self {
                id: account,
                via: AuthVia::Device(device),
            });
        }
        let jar = CookieJar::from_headers(&parts.headers);
        let token = jar
            .get(SESSION_COOKIE)
            .map(|c| c.value().to_owned())
            .ok_or_else(ApiError::unauthorized)?;
        // 쿠키 인증은 브라우저가 자동으로 붙이므로, 상태 변경 요청에는 CSRF 헤더 확인
        if !matches!(parts.method, Method::GET | Method::HEAD) {
            require_csrf_header(&parts.headers)?;
        }
        let row: Option<(Uuid, Uuid)> = sqlx::query_as(
            "SELECT id, account_id FROM sessions WHERE token_hash = $1 AND expires_at > NOW()",
        )
        .bind(security::token_hash(&token))
        .fetch_optional(db)
        .await?;
        let (session, account) = row.ok_or_else(ApiError::unauthorized)?;
        Ok(Self {
            id: account,
            via: AuthVia::Session(session),
        })
    }
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|t| !t.is_empty())
}

fn require_csrf_header(headers: &HeaderMap) -> ApiResult<()> {
    match headers.get(CSRF_HEADER).and_then(|v| v.to_str().ok()) {
        Some("1") => Ok(()),
        _ => Err(ApiError::forbidden(
            "csrf",
            format!("{CSRF_HEADER}: 1 헤더가 필요합니다"),
        )),
    }
}

fn rate_limit(state: &AppState, headers: &HeaderMap, action: &str) -> ApiResult<()> {
    let key = format!("{action}:{}", security::client_key(headers));
    if state.limiter.check(&key) {
        Ok(())
    } else {
        Err(ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "요청이 너무 많습니다. 잠시 후 다시 시도하세요",
        ))
    }
}

// ---------- 핸들러 ----------

pub async fn providers(State(state): State<AppState>) -> Json<ProvidersView> {
    Json(ProvidersView {
        local: state.database_configured(),
        google: false, // 후속: Google OIDC (OAuth 클라이언트는 사용자가 Google Cloud에서 발급)
        registration: match state.config.registration {
            Registration::Open => "open",
            Registration::Closed => "closed",
        },
    })
}

pub async fn register(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    ApiJson(req): ApiJson<RegisterRequest>,
) -> ApiResult<(StatusCode, CookieJar, Json<AccountView>)> {
    require_csrf_header(&headers)?;
    let db = state.db()?;
    rate_limit(&state, &headers, "register")?;
    if state.config.registration == Registration::Closed {
        return Err(ApiError::forbidden(
            "registration_closed",
            "현재 가입을 받지 않습니다",
        ));
    }
    let email = security::normalize_email(&req.email)
        .ok_or_else(|| ApiError::bad_request("invalid_email", "이메일 형식이 올바르지 않습니다"))?;
    if let Some(msg) = security::password_policy_error(&req.password) {
        return Err(ApiError::bad_request("weak_password", msg));
    }
    let display_name: String = req
        .display_name
        .trim()
        .chars()
        .take(DISPLAY_NAME_MAX)
        .collect();
    let password = req.password.clone();
    let hash = tokio::task::spawn_blocking(move || security::hash_password(&password))
        .await
        .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", "서버 오류"))?
        .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", "서버 오류"))?;

    let mut tx = db.begin().await?;
    // 동시 가입에서도 계정 수 상한을 지키도록 트랜잭션 잠금 (요구 17)
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('memonest_accounts'))")
        .execute(&mut *tx)
        .await?;
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM accounts")
        .fetch_one(&mut *tx)
        .await?;
    if count >= state.config.max_accounts {
        return Err(ApiError::forbidden(
            "account_limit_reached",
            "가입 가능한 계정 수를 모두 채웠습니다",
        ));
    }
    let inserted: Result<AccountView, sqlx::Error> = sqlx::query_as(
        "INSERT INTO accounts (id, tier, quota_bytes, email, display_name, password_hash, auth_provider)
         VALUES ($1, 'free', $2, $3, $4, $5, 'local')
         RETURNING id, email, display_name, auth_provider, tier, quota_bytes, used_bytes, created_at, default_project_name",
    )
    .bind(Uuid::new_v4())
    .bind(crate::admin::default_quota_bytes(&mut *tx, &state.config).await?)
    .bind(&email)
    .bind(&display_name)
    .bind(&hash)
    .fetch_one(&mut *tx)
    .await;
    let account = match inserted {
        Ok(a) => a.with_admin(&state.config),
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "email_taken",
                "이미 가입된 이메일입니다",
            ))
        }
        Err(e) => return Err(e.into()),
    };
    let jar = start_session(&state, &mut tx, jar, account.id, &headers).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, jar, Json(account)))
}

pub async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    ApiJson(req): ApiJson<LoginRequest>,
) -> ApiResult<(CookieJar, Json<AccountView>)> {
    require_csrf_header(&headers)?;
    let db = state.db()?;
    rate_limit(&state, &headers, "login")?;
    let invalid = || {
        ApiError::new(
            StatusCode::UNAUTHORIZED,
            "invalid_credentials",
            "이메일 또는 비밀번호가 올바르지 않습니다",
        )
    };
    let email = security::normalize_email(&req.email).ok_or_else(invalid)?;
    let row: Option<(Uuid, Option<String>)> = sqlx::query_as(
        "SELECT id, password_hash FROM accounts WHERE lower(email) = $1 AND auth_provider = 'local'",
    )
    .bind(&email)
    .fetch_optional(db)
    .await?;
    let password = req.password.clone();
    let verified = tokio::task::spawn_blocking(move || match row {
        Some((id, Some(hash))) => security::verify_password(&password, &hash).then_some(id),
        _ => {
            security::burn_password_check(&password); // 없는 계정도 비슷한 응답 시간
            None
        }
    })
    .await
    .ok()
    .flatten();
    let id = verified.ok_or_else(invalid)?;
    let mut tx = db.begin().await?;
    let jar = start_session(&state, &mut tx, jar, id, &headers).await?;
    tx.commit().await?;
    Ok((
        jar,
        Json(load_account(db, id).await?.with_admin(&state.config)),
    ))
}

pub async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
    current: CurrentAccount,
) -> ApiResult<(StatusCode, CookieJar)> {
    if let AuthVia::Session(session) = current.via {
        sqlx::query("DELETE FROM sessions WHERE id = $1")
            .bind(session)
            .execute(state.db()?)
            .await?;
    }
    Ok((
        StatusCode::NO_CONTENT,
        jar.remove(session_cookie(&state, String::new())),
    ))
}

pub async fn me(
    State(state): State<AppState>,
    current: CurrentAccount,
) -> ApiResult<Json<AccountView>> {
    current.require(Scope::Account)?;
    Ok(Json(
        load_account(state.db()?, current.id)
            .await?
            .with_admin(&state.config),
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountUpdate {
    /// 기본 프로젝트 표시 이름 ('' = 화면 기본값)
    default_project_name: Option<String>,
}

/// 계정 설정 바꾸기 (지금은 기본 프로젝트 표시 이름만). 장비(앱)에서도 가능
pub async fn update_me(
    State(state): State<AppState>,
    current: CurrentAccount,
    ApiJson(req): ApiJson<AccountUpdate>,
) -> ApiResult<Json<AccountView>> {
    current.require(Scope::Account)?;
    let db = state.db()?;
    if let Some(raw) = req.default_project_name {
        let name = if raw.trim().is_empty() {
            String::new()
        } else {
            crate::projects::project_name(&raw)?
        };
        // 다른 기기는 변경 목록(`projects=1`) 응답의 defaultProjectName으로 받음
        sqlx::query("UPDATE accounts SET default_project_name = $2 WHERE id = $1")
            .bind(current.id)
            .bind(&name)
            .execute(db)
            .await?;
    }
    Ok(Json(
        load_account(db, current.id)
            .await?
            .with_admin(&state.config),
    ))
}

pub async fn quota(
    State(state): State<AppState>,
    current: CurrentAccount,
) -> ApiResult<Json<QuotaView>> {
    current.require(Scope::Account)?;
    let a = load_account(state.db()?, current.id).await?;
    Ok(Json(QuotaView {
        tier: a.tier,
        quota_bytes: a.quota_bytes,
        used_bytes: a.used_bytes,
    }))
}

/// 로그인한 웹 브라우저에서 장비 등록 코드를 만듭니다 (장비 토큰으로는 다른 장비를 등록할 수 없음).
pub async fn pairing_start(
    State(state): State<AppState>,
    current: CurrentAccount,
    Query(q): Query<PairingStartQuery>,
) -> ApiResult<(StatusCode, Json<PairingStartResponse>)> {
    if !matches!(current.via, AuthVia::Session(_)) {
        return Err(ApiError::forbidden(
            "session_required",
            "웹에서 로그인한 상태에서만 장비를 등록할 수 있습니다",
        ));
    }
    if let Some(device) = q.device_id {
        let mine: Option<(i32,)> =
            sqlx::query_as("SELECT 1 FROM devices WHERE id = $1 AND account_id = $2")
                .bind(device)
                .bind(current.id)
                .fetch_optional(state.db()?)
                .await?;
        if mine.is_none() {
            return Err(ApiError::not_found());
        }
    }
    let code = security::new_pairing_code();
    let expires_at = Utc::now() + chrono::Duration::from_std(PAIRING_CODE_TTL).expect("짧은 기간");
    sqlx::query(
        "INSERT INTO pairing_codes (code_hash, account_id, expires_at, device_id) VALUES ($1, $2, $3, $4)",
    )
    .bind(security::token_hash(&code))
    .bind(current.id)
    .bind(expires_at)
    .bind(q.device_id)
    .execute(state.db()?)
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(PairingStartResponse { code, expires_at }),
    ))
}

/// 장비 앱이 등록 코드를 장비 토큰으로 교환합니다 (코드는 한 번만, 10분 안에).
pub async fn pairing_finish(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiJson(req): ApiJson<PairingFinishRequest>,
) -> ApiResult<(StatusCode, Json<PairingFinishResponse>)> {
    require_csrf_header(&headers)?;
    let db = state.db()?;
    rate_limit(&state, &headers, "pairing")?;
    let name: String = req
        .device_name
        .trim()
        .chars()
        .take(DEVICE_NAME_MAX)
        .collect();
    let platform: String = req
        .platform
        .trim()
        .to_lowercase()
        .chars()
        .take(32)
        .collect();
    if name.is_empty() || platform.is_empty() {
        return Err(ApiError::bad_request(
            "invalid_device",
            "장비 이름과 플랫폼이 필요합니다",
        ));
    }
    let code = req.code.trim().to_uppercase();
    let mut tx = db.begin().await?;
    let account: Option<(Uuid, Option<Uuid>)> = sqlx::query_as(
        "UPDATE pairing_codes SET used_at = NOW()
         WHERE code_hash = $1 AND used_at IS NULL AND expires_at > NOW() RETURNING account_id, device_id",
    )
    .bind(security::token_hash(&code))
    .fetch_optional(&mut *tx)
    .await?;
    let (account, chosen) = account.ok_or_else(|| {
        ApiError::bad_request("invalid_code", "등록 코드가 올바르지 않거나 만료되었습니다")
    })?;
    let token = security::new_token();
    let key_hash = req
        .device_key
        .as_deref()
        .and_then(|k| device_key_hash(account, k));
    // 다시 등록: 웹에서 고른 장비, 아니면 같은 기기 고유값의 장비 → 그 기록을 이어 씀 (새 토큰, 폐기 해제)
    let existing: Option<(Uuid,)> = match (chosen, &key_hash) {
        (Some(id), _) => Some((id,)),
        (None, Some(hash)) => {
            sqlx::query_as("SELECT id FROM devices WHERE account_id = $1 AND device_key_hash = $2")
                .bind(account)
                .bind(hash)
                .fetch_optional(&mut *tx)
                .await?
        }
        (None, None) => None,
    };
    let device_id = match existing {
        Some((id,)) => {
            // 같은 기기 값이 다른 장비 기록에 있으면 비움 (고른 장비로 옮김)
            if let Some(hash) = &key_hash {
                sqlx::query(
                    "UPDATE devices SET device_key_hash = NULL
                     WHERE account_id = $1 AND device_key_hash = $2 AND id <> $3",
                )
                .bind(account)
                .bind(hash)
                .bind(id)
                .execute(&mut *tx)
                .await?;
            }
            sqlx::query(
                "UPDATE devices SET name = $3, platform = $4, token_hash = $5, revoked_at = NULL,
                   device_key_hash = COALESCE($6, device_key_hash), last_seen_at = NOW()
                 WHERE id = $1 AND account_id = $2",
            )
            .bind(id)
            .bind(account)
            .bind(&name)
            .bind(&platform)
            .bind(security::token_hash(&token))
            .bind(&key_hash)
            .execute(&mut *tx)
            .await?;
            id
        }
        None => {
            let id = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO devices (id, account_id, name, platform, token_hash, device_key_hash)
                 VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(id)
            .bind(account)
            .bind(&name)
            .bind(&platform)
            .bind(security::token_hash(&token))
            .bind(&key_hash)
            .execute(&mut *tx)
            .await?;
            id
        }
    };
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(PairingFinishResponse { device_id, token }),
    ))
}

pub async fn list_devices(
    State(state): State<AppState>,
    current: CurrentAccount,
) -> ApiResult<Json<Vec<DeviceView>>> {
    current.require(Scope::Account)?;
    let devices = sqlx::query_as(
        "SELECT id, name, platform, created_at, last_seen_at, revoked_at FROM devices
         WHERE account_id = $1 ORDER BY created_at",
    )
    .bind(current.id)
    .fetch_all(state.db()?)
    .await?;
    Ok(Json(devices))
}

pub async fn revoke_device(
    State(state): State<AppState>,
    current: CurrentAccount,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    current.require(Scope::Account)?;
    let done = sqlx::query(
        "UPDATE devices SET revoked_at = COALESCE(revoked_at, NOW()) WHERE id = $1 AND account_id = $2",
    )
    .bind(id)
    .bind(current.id)
    .execute(state.db()?)
    .await?;
    if done.rows_affected() == 0 {
        return Err(ApiError::not_found());
    }
    Ok(StatusCode::NO_CONTENT)
}

/// 폐기한 장비의 기록 지우기 (2026-11-13 사용 의견: 해제한 장비 정보를 목록에서 없애기). 폐기 전 장비는 409
pub async fn forget_device(
    State(state): State<AppState>,
    current: CurrentAccount,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    current.require(Scope::Account)?;
    let row: Option<(bool,)> = sqlx::query_as(
        "SELECT revoked_at IS NOT NULL FROM devices WHERE id = $1 AND account_id = $2",
    )
    .bind(id)
    .bind(current.id)
    .fetch_optional(state.db()?)
    .await?;
    match row {
        None => Err(ApiError::not_found()),
        Some((false,)) => Err(ApiError::new(
            StatusCode::CONFLICT,
            "device_active",
            "연결된 장비는 먼저 폐기한 뒤 지울 수 있습니다",
        )),
        Some((true,)) => {
            sqlx::query("DELETE FROM devices WHERE id = $1 AND account_id = $2")
                .bind(id)
                .bind(current.id)
                .execute(state.db()?)
                .await?;
            Ok(StatusCode::NO_CONTENT)
        }
    }
}

// ---------- 내부 도우미 ----------

async fn load_account(db: &PgPool, id: Uuid) -> ApiResult<AccountView> {
    sqlx::query_as(
        "SELECT id, email, display_name, auth_provider, tier, quota_bytes, used_bytes, created_at,
           default_project_name
         FROM accounts WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or_else(ApiError::unauthorized)
}

async fn start_session(
    state: &AppState,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    jar: CookieJar,
    account: Uuid,
    headers: &HeaderMap,
) -> ApiResult<CookieJar> {
    let token = security::new_token();
    let user_agent: String = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .chars()
        .take(200)
        .collect();
    sqlx::query(
        "INSERT INTO sessions (id, account_id, token_hash, user_agent, expires_at)
         VALUES ($1, $2, $3, $4, NOW() + make_interval(days => $5))",
    )
    .bind(Uuid::new_v4())
    .bind(account)
    .bind(security::token_hash(&token))
    .bind(user_agent)
    .bind(state.config.session_days as i32)
    .execute(&mut **tx)
    .await?;
    Ok(jar.add(session_cookie(state, token)))
}

fn session_cookie(state: &AppState, value: String) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, value))
        .path("/")
        .http_only(true)
        .secure(state.config.cookie_secure)
        .same_site(SameSite::Strict)
        .max_age(time_days(state.config.session_days))
        .build()
}

fn time_days(days: i64) -> time::Duration {
    time::Duration::days(days)
}

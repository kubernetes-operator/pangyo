//! 자동화 토큰 (n8n 등, 로드맵 07 / 요구 19)
//!
//! - 웹에서 로그인한 사용자만 발급·목록·해제 (자동화 토큰·장비 토큰으로는 불가)
//! - 범위: `notes:read`, `notes:write`. 만료 필수(최대 365일). 원문은 발급 응답에서 한 번만.
//! - 사용: `Authorization: Bearer mn_pat_...` → 변경 목록·읽기·검색(읽기), 임대·저장·삭제(쓰기)
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    accounts::{AuthVia, CurrentAccount, INTEGRATION_TOKEN_PREFIX},
    error::{ApiError, ApiJson, ApiResult},
    security, AppState,
};

// === 0. 정책 변수 ===
/// 허용 범위
pub const ALLOWED_SCOPES: [&str; 2] = ["notes:read", "notes:write"];
/// 계정당 유효한 토큰 최대 개수
pub const MAX_ACTIVE_TOKENS: i64 = 20;
/// 만료 기본값/최대값(일)
pub const DEFAULT_TTL_DAYS: i64 = 90;
pub const MAX_TTL_DAYS: i64 = 365;
const NAME_MAX: usize = 60;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTokenRequest {
    name: String,
    scopes: Vec<String>,
    #[serde(default)]
    ttl_days: Option<i64>,
}

#[derive(Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct TokenView {
    id: Uuid,
    name: String,
    scopes: Vec<String>,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    last_used_at: Option<DateTime<Utc>>,
    revoked_at: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedToken {
    #[serde(flatten)]
    info: TokenView,
    /// 이 응답에서만 보입니다. n8n 자격 증명에 바로 저장하세요.
    token: String,
}

fn require_web_session(current: &CurrentAccount) -> ApiResult<()> {
    match current.via {
        AuthVia::Session(_) => Ok(()),
        _ => Err(ApiError::forbidden(
            "session_required",
            "자동화 토큰은 웹에서 로그인한 상태에서만 관리할 수 있습니다",
        )),
    }
}

pub async fn create_token(
    State(state): State<AppState>,
    current: CurrentAccount,
    ApiJson(req): ApiJson<CreateTokenRequest>,
) -> ApiResult<(StatusCode, Json<CreatedToken>)> {
    require_web_session(&current)?;
    let name: String = req.name.trim().chars().take(NAME_MAX).collect();
    if name.is_empty() {
        return Err(ApiError::bad_request(
            "invalid_name",
            "토큰 이름이 필요합니다",
        ));
    }
    let mut scopes: Vec<String> = req.scopes.iter().map(|s| s.trim().to_owned()).collect();
    scopes.sort();
    scopes.dedup();
    if scopes.is_empty() || scopes.iter().any(|s| !ALLOWED_SCOPES.contains(&s.as_str())) {
        return Err(ApiError::bad_request(
            "invalid_scopes",
            format!("범위는 {} 중에서 하나 이상", ALLOWED_SCOPES.join(", ")),
        ));
    }
    let ttl = req.ttl_days.unwrap_or(DEFAULT_TTL_DAYS);
    if !(1..=MAX_TTL_DAYS).contains(&ttl) {
        return Err(ApiError::bad_request(
            "invalid_ttl",
            format!("만료는 1~{MAX_TTL_DAYS}일"),
        ));
    }
    let mut tx = state.db()?.begin().await?;
    let (active,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM integration_tokens
         WHERE account_id = $1 AND revoked_at IS NULL AND expires_at > NOW()",
    )
    .bind(current.id)
    .fetch_one(&mut *tx)
    .await?;
    if active >= MAX_ACTIVE_TOKENS {
        return Err(ApiError::forbidden(
            "too_many_tokens",
            format!("유효한 토큰은 {MAX_ACTIVE_TOKENS}개까지입니다. 쓰지 않는 토큰을 해제하세요"),
        ));
    }
    let token = format!("{INTEGRATION_TOKEN_PREFIX}{}", security::new_token());
    let info: TokenView = sqlx::query_as(
        "INSERT INTO integration_tokens (id, account_id, name, token_hash, scopes, expires_at)
         VALUES ($1, $2, $3, $4, $5, NOW() + make_interval(days => $6))
         RETURNING id, name, scopes, created_at, expires_at, last_used_at, revoked_at",
    )
    .bind(Uuid::new_v4())
    .bind(current.id)
    .bind(&name)
    .bind(security::token_hash(&token))
    .bind(&scopes)
    .bind(ttl as i32)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(CreatedToken { info, token })))
}

pub async fn list_tokens(
    State(state): State<AppState>,
    current: CurrentAccount,
) -> ApiResult<Json<Vec<TokenView>>> {
    require_web_session(&current)?;
    let tokens = sqlx::query_as(
        "SELECT id, name, scopes, created_at, expires_at, last_used_at, revoked_at
         FROM integration_tokens WHERE account_id = $1 ORDER BY created_at DESC",
    )
    .bind(current.id)
    .fetch_all(state.db()?)
    .await?;
    Ok(Json(tokens))
}

pub async fn revoke_token(
    State(state): State<AppState>,
    current: CurrentAccount,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    require_web_session(&current)?;
    let done = sqlx::query(
        "UPDATE integration_tokens SET revoked_at = COALESCE(revoked_at, NOW())
         WHERE id = $1 AND account_id = $2",
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

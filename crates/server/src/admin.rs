//! 관리자 화면 API (2026-11-13 사용 의견 6번: 계정당 저장공간을 시스템에서 변경)
//!
//! - 관리자: `ADMIN_EMAILS`(Kubernetes에서는 사용자가 만든 Secret)에 있는 이메일로 로그인한 사용자. 자동화 토큰은 불가
//! - 기본 용량: `system_settings.default_quota_mb`가 있으면 그 값, 없으면 `FREE_STORAGE_MB`.
//!   바꾸면 계정별로 따로 정하지 않은 모든 계정에 바로 적용
//! - 계정별 용량: 따로 정하면 `quota_custom = true` (기본값 변경의 영향 없음), `null`로 보내면 기본값으로 되돌림
use axum::{
    extract::{Path, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    accounts::{CurrentAccount, Scope},
    config::Config,
    error::{ApiError, ApiJson, ApiResult},
    AppState,
};

// === 0. 변수 ===
const DEFAULT_QUOTA_KEY: &str = "default_quota_mb";
/// 관리자가 정할 수 있는 용량 범위 (MB)
pub const QUOTA_MB_MIN: i64 = 1;
pub const QUOTA_MB_MAX: i64 = 1024 * 1024; // 1 TB
const MIB: i64 = 1024 * 1024;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminSettings {
    /// 지금 적용 중인 기본 용량 (MB)
    default_quota_mb: i64,
    /// 화면에서 정하지 않았을 때 쓰는 서버 설정값(FREE_STORAGE_MB)
    env_default_quota_mb: i64,
    /// 이번 변경으로 용량이 바뀐 계정 수 (PUT 응답에만 의미)
    updated_accounts: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminSettingsUpdate {
    default_quota_mb: i64,
}

#[derive(Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AdminAccount {
    id: Uuid,
    email: Option<String>,
    display_name: String,
    used_bytes: i64,
    quota_bytes: i64,
    quota_custom: bool,
    created_at: DateTime<Utc>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminAccountList {
    accounts: Vec<AdminAccount>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaUpdate {
    /// MB. `null`이면 기본 용량으로 되돌림
    quota_mb: Option<i64>,
}

/// 지금 적용 중인 기본 용량 (바이트). 가입할 때와 관리자 화면에서 사용
pub async fn default_quota_bytes<'e>(
    db: impl sqlx::PgExecutor<'e>,
    config: &Config,
) -> ApiResult<i64> {
    Ok(default_quota_mb(db, config).await?.saturating_mul(MIB))
}

async fn default_quota_mb<'e>(db: impl sqlx::PgExecutor<'e>, config: &Config) -> ApiResult<i64> {
    let row: Option<(i64,)> =
        sqlx::query_as("SELECT (value #>> '{}')::bigint FROM system_settings WHERE key = $1")
            .bind(DEFAULT_QUOTA_KEY)
            .fetch_optional(db)
            .await?;
    Ok(row.map_or(config.free_storage_mb, |(mb,)| mb))
}

/// 이 요청을 보낸 사람이 관리자인지
pub async fn is_admin(db: &PgPool, config: &Config, account: Uuid) -> ApiResult<bool> {
    if config.admin_emails.is_empty() {
        return Ok(false);
    }
    let (email,): (Option<String>,) = sqlx::query_as("SELECT email FROM accounts WHERE id = $1")
        .bind(account)
        .fetch_one(db)
        .await?;
    Ok(config.is_admin_email(email.as_deref()))
}

async fn require_admin(state: &AppState, current: &CurrentAccount) -> ApiResult<()> {
    current.require(Scope::Account)?;
    if is_admin(state.db()?, &state.config, current.id).await? {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "admin_only",
            "관리자만 할 수 있는 작업입니다",
        ))
    }
}

fn check_quota_mb(mb: i64) -> ApiResult<()> {
    if (QUOTA_MB_MIN..=QUOTA_MB_MAX).contains(&mb) {
        Ok(())
    } else {
        Err(ApiError::bad_request(
            "invalid_quota",
            format!("용량은 {QUOTA_MB_MIN}~{QUOTA_MB_MAX} MB 사이로 정하세요"),
        ))
    }
}

pub async fn get_settings(
    State(state): State<AppState>,
    current: CurrentAccount,
) -> ApiResult<Json<AdminSettings>> {
    require_admin(&state, &current).await?;
    Ok(Json(AdminSettings {
        default_quota_mb: default_quota_mb(state.db()?, &state.config).await?,
        env_default_quota_mb: state.config.free_storage_mb,
        updated_accounts: 0,
    }))
}

pub async fn put_settings(
    State(state): State<AppState>,
    current: CurrentAccount,
    ApiJson(req): ApiJson<AdminSettingsUpdate>,
) -> ApiResult<Json<AdminSettings>> {
    require_admin(&state, &current).await?;
    check_quota_mb(req.default_quota_mb)?;
    let mut tx = state.db()?.begin().await?;
    sqlx::query(
        "INSERT INTO system_settings (key, value, updated_at) VALUES ($1, to_jsonb($2::bigint), NOW())
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()",
    )
    .bind(DEFAULT_QUOTA_KEY)
    .bind(req.default_quota_mb)
    .execute(&mut *tx)
    .await?;
    let updated = sqlx::query(
        "UPDATE accounts SET quota_bytes = $1 WHERE NOT quota_custom AND quota_bytes <> $1",
    )
    .bind(req.default_quota_mb.saturating_mul(MIB))
    .execute(&mut *tx)
    .await?
    .rows_affected();
    tx.commit().await?;
    tracing::info!(admin = %current.id, default_quota_mb = req.default_quota_mb, updated, "admin changed default quota");
    Ok(Json(AdminSettings {
        default_quota_mb: req.default_quota_mb,
        env_default_quota_mb: state.config.free_storage_mb,
        updated_accounts: updated,
    }))
}

pub async fn list_accounts(
    State(state): State<AppState>,
    current: CurrentAccount,
) -> ApiResult<Json<AdminAccountList>> {
    require_admin(&state, &current).await?;
    let accounts = sqlx::query_as(
        "SELECT id, email, display_name, used_bytes, quota_bytes, quota_custom, created_at
         FROM accounts ORDER BY created_at LIMIT 2000",
    )
    .fetch_all(state.db()?)
    .await?;
    Ok(Json(AdminAccountList { accounts }))
}

pub async fn put_account_quota(
    State(state): State<AppState>,
    current: CurrentAccount,
    Path(id): Path<Uuid>,
    ApiJson(req): ApiJson<QuotaUpdate>,
) -> ApiResult<Json<AdminAccount>> {
    require_admin(&state, &current).await?;
    let db = state.db()?;
    let (bytes, custom) = match req.quota_mb {
        Some(mb) => {
            check_quota_mb(mb)?;
            (mb.saturating_mul(MIB), true)
        }
        None => (default_quota_bytes(db, &state.config).await?, false),
    };
    let account: Option<AdminAccount> = sqlx::query_as(
        "UPDATE accounts SET quota_bytes = $2, quota_custom = $3 WHERE id = $1
         RETURNING id, email, display_name, used_bytes, quota_bytes, quota_custom, created_at",
    )
    .bind(id)
    .bind(bytes)
    .bind(custom)
    .fetch_optional(db)
    .await?;
    let account = account.ok_or_else(|| {
        ApiError::new(
            axum::http::StatusCode::NOT_FOUND,
            "account_not_found",
            "없는 계정입니다",
        )
    })?;
    tracing::info!(admin = %current.id, account = %id, quota_bytes = bytes, custom, "admin changed account quota");
    Ok(Json(account))
}

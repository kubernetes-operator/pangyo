//! 메모 동기화 API (로드맵 06, ADR-0003 A안)
//!
//! - 변경 목록: `GET /api/v1/notes?since=<cursor>` — 저장·삭제마다 증가하는 change_seq 커서
//! - 저장: `PUT /api/v1/notes/{id}` — `baseVersion` 검사(다르면 409, 덮어쓰지 않음),
//!   기존 메모는 **유효한 편집 임대 + fencing token** 필요(없으면 423)
//! - 편집 임대: `POST/DELETE /api/v1/notes/{id}/lease` — 편집 시작 시 획득, 주기적 갱신, TTL 후 자동 해제
//! - 삭제: 묘비(deleted_at)로 남겨 다른 기기에 전달
//!
//! 경로/파일명 규칙은 memonest-core(웹·데스크톱과 같은 규칙)를 그대로 사용합니다.
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use memonest_core::{parse_folder, safe_filename, NoteFormat, MAX_FOLDER_DEPTH};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::{
    accounts::{AuthVia, CurrentAccount},
    config::{CHANGES_PAGE_MAX, NOTE_CONTENT_MAX_BYTES},
    error::{ApiError, ApiJson, ApiResult},
    AppState,
};

// === 0. 변수 ===
/// 클라이언트(브라우저 탭/앱 인스턴스) 식별자 최대 길이
const CLIENT_ID_MAX: usize = 64;
/// 임대 보유자 표시 이름 최대 길이
const HOLDER_LABEL_MAX: usize = 60;

const NOTE_COLUMNS: &str =
    "id, title, folder, format, content, version, content_sha256, size_bytes,
    created_at, modified_at, (deleted_at IS NOT NULL) AS deleted, change_seq";

#[derive(Debug, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct NoteView {
    id: Uuid,
    title: String,
    folder: String,
    format: String,
    content: String,
    version: i64,
    content_sha256: String,
    size_bytes: i64,
    created_at: DateTime<Utc>,
    modified_at: DateTime<Utc>,
    deleted: bool,
    change_seq: i64,
}

#[derive(Deserialize)]
pub struct ChangesQuery {
    #[serde(default)]
    since: i64,
    limit: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangesResponse {
    notes: Vec<NoteView>,
    /// 다음 요청의 `since`
    cursor: i64,
    has_more: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PutNoteRequest {
    /// 클라이언트가 마지막으로 받은 서버 버전 (새 메모는 0)
    base_version: i64,
    client_id: String,
    #[serde(default)]
    fencing_token: Option<i64>,
    title: String,
    #[serde(default)]
    folder: String,
    format: String,
    content: String,
    #[serde(default)]
    created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    modified_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteNoteQuery {
    base_version: i64,
    client_id: String,
    fencing_token: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LeaseRequest {
    client_id: String,
    #[serde(default)]
    label: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LeaseReleaseQuery {
    client_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LeaseView {
    fencing_token: i64,
    expires_at: DateTime<Utc>,
    ttl_seconds: i64,
}

// ---------- 도우미 ----------

/// 임대 보유자 = 인증 수단(세션/장비) + 클라이언트 ID. 같은 사람의 다른 탭/기기도 서로 다른 보유자입니다.
fn holder(current: &CurrentAccount, client_id: &str) -> ApiResult<String> {
    let valid = !client_id.is_empty()
        && client_id.len() <= CLIENT_ID_MAX
        && client_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if !valid {
        return Err(ApiError::bad_request(
            "invalid_client_id",
            "clientId 형식이 올바르지 않습니다",
        ));
    }
    Ok(match current.via {
        AuthVia::Session(id) => format!("session:{id}:{client_id}"),
        AuthVia::Device(id) => format!("device:{id}:{client_id}"),
    })
}

fn sha256_hex(content: &str) -> String {
    Sha256::digest(content.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn not_found() -> ApiError {
    ApiError::new(
        StatusCode::NOT_FOUND,
        "note_not_found",
        "메모를 찾을 수 없습니다",
    )
}

/// 저장/삭제 전에 호출: 이 보유자가 지금 유효한 임대와 같은 fencing token을 갖고 있는지
async fn require_lease(
    tx: &mut Transaction<'_, Postgres>,
    note: Uuid,
    me: &str,
    token: Option<i64>,
) -> ApiResult<()> {
    let lease: Option<(String, String, i64, DateTime<Utc>, bool)> = sqlx::query_as(
        "SELECT holder, holder_label, fencing_token, expires_at, expires_at > NOW()
         FROM note_leases WHERE note_id = $1 FOR UPDATE",
    )
    .bind(note)
    .fetch_optional(&mut **tx)
    .await?;
    match lease {
        Some((holder, _, lease_token, _, true)) if holder == me && Some(lease_token) == token => {
            Ok(())
        }
        Some((holder, label, _, expires_at, true)) if holder != me => {
            Err(lease_held(&label, expires_at))
        }
        _ => Err(ApiError::new(
            StatusCode::LOCKED,
            "lease_required",
            "편집 임대가 없거나 만료되었습니다. 임대를 다시 받은 뒤 저장하세요",
        )),
    }
}

fn lease_held(label: &str, expires_at: DateTime<Utc>) -> ApiError {
    let who = if label.is_empty() {
        "다른 기기"
    } else {
        label
    };
    ApiError::new(
        StatusCode::LOCKED,
        "lease_held",
        format!("{who}에서 편집 중입니다"),
    )
    .with_details(json!({ "holderLabel": label, "expiresAt": expires_at }))
}

fn version_conflict(current: i64, deleted: bool) -> ApiError {
    ApiError::new(
        StatusCode::CONFLICT,
        "version_conflict",
        "다른 기기에서 먼저 바뀌었습니다. 충돌을 검토하세요",
    )
    .with_details(json!({ "currentVersion": current, "deleted": deleted }))
}

/// 계정 사용량 갱신. 늘어나는 경우에만 할당량 검사.
async fn charge_quota(
    tx: &mut Transaction<'_, Postgres>,
    account: Uuid,
    delta: i64,
) -> ApiResult<()> {
    let updated = sqlx::query(
        "UPDATE accounts SET used_bytes = GREATEST(used_bytes + $2, 0)
         WHERE id = $1 AND ($2 <= 0 OR used_bytes + $2 <= quota_bytes)",
    )
    .bind(account)
    .bind(delta)
    .execute(&mut **tx)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(ApiError::forbidden(
            "quota_exceeded",
            "저장 공간이 부족합니다",
        ));
    }
    Ok(())
}

async fn load_note(tx: &mut Transaction<'_, Postgres>, id: Uuid) -> ApiResult<NoteView> {
    Ok(
        sqlx::query_as(&format!("SELECT {NOTE_COLUMNS} FROM notes WHERE id = $1"))
            .bind(id)
            .fetch_one(&mut **tx)
            .await?,
    )
}

// ---------- 핸들러 ----------

pub async fn changes(
    State(state): State<AppState>,
    current: CurrentAccount,
    Query(q): Query<ChangesQuery>,
) -> ApiResult<Json<ChangesResponse>> {
    let limit = q
        .limit
        .unwrap_or(CHANGES_PAGE_MAX)
        .clamp(1, CHANGES_PAGE_MAX);
    let mut notes: Vec<NoteView> = sqlx::query_as(&format!(
        "SELECT {NOTE_COLUMNS} FROM notes WHERE owner_id = $1 AND change_seq > $2
         ORDER BY change_seq LIMIT $3"
    ))
    .bind(current.id)
    .bind(q.since)
    .bind(limit + 1)
    .fetch_all(state.db()?)
    .await?;
    let has_more = notes.len() as i64 > limit;
    notes.truncate(limit as usize);
    let cursor = notes.last().map_or(q.since, |n| n.change_seq);
    Ok(Json(ChangesResponse {
        notes,
        cursor,
        has_more,
    }))
}

pub async fn get_note(
    State(state): State<AppState>,
    current: CurrentAccount,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<NoteView>> {
    let note: Option<NoteView> = sqlx::query_as(&format!(
        "SELECT {NOTE_COLUMNS} FROM notes WHERE id = $1 AND owner_id = $2"
    ))
    .bind(id)
    .bind(current.id)
    .fetch_optional(state.db()?)
    .await?;
    note.map(Json).ok_or_else(not_found)
}

pub async fn put_note(
    State(state): State<AppState>,
    current: CurrentAccount,
    Path(id): Path<Uuid>,
    ApiJson(req): ApiJson<PutNoteRequest>,
) -> ApiResult<Json<NoteView>> {
    let me = holder(&current, &req.client_id)?;
    let format = NoteFormat::from_extension(&req.format)
        .ok_or_else(|| ApiError::bad_request("invalid_format", "format은 md 또는 txt"))?;
    let folder = parse_folder(&req.folder, MAX_FOLDER_DEPTH)
        .map_err(|e| ApiError::bad_request("invalid_folder", e.to_string()))?;
    let title = safe_filename(&req.title);
    if req.content.len() > NOTE_CONTENT_MAX_BYTES {
        return Err(ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "note_too_large",
            format!(
                "메모 한 개는 {}KB까지 저장할 수 있습니다",
                NOTE_CONTENT_MAX_BYTES / 1024
            ),
        ));
    }
    let path = if folder.is_empty() {
        format!("{title}.{format}")
    } else {
        format!("{folder}/{title}.{format}")
    };
    let size = req.content.len() as i64;
    let hash = sha256_hex(&req.content);
    let modified_at = req.modified_at.unwrap_or_else(Utc::now);

    let db = state.db()?;
    let mut tx = db.begin().await?;
    let existing: Option<(Uuid, i64, i64, bool)> = sqlx::query_as(
        "SELECT owner_id, version, size_bytes, deleted_at IS NOT NULL FROM notes WHERE id = $1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let old_size = match existing {
        // 다른 계정의 메모 ID는 존재 자체를 알려주지 않음
        Some((owner, ..)) if owner != current.id => return Err(not_found()),
        Some((_, version, old_size, deleted)) => {
            require_lease(&mut tx, id, &me, req.fencing_token).await?;
            if req.base_version != version || deleted {
                return Err(version_conflict(version, deleted));
            }
            old_size
        }
        None if req.base_version != 0 => return Err(version_conflict(0, true)),
        None => 0,
    };
    charge_quota(&mut tx, current.id, size - old_size).await?;
    let saved = if existing.is_some() {
        sqlx::query(
            "UPDATE notes SET title = $2, folder = $3, format = $4, path = $5, content = $6,
               content_sha256 = $7, size_bytes = $8, version = version + 1, modified_at = $9,
               change_seq = nextval('note_change_seq')
             WHERE id = $1",
        )
        .bind(id)
        .bind(&title)
        .bind(&folder)
        .bind(format.extension())
        .bind(&path)
        .bind(&req.content)
        .bind(&hash)
        .bind(size)
        .bind(modified_at)
        .execute(&mut *tx)
        .await
    } else {
        sqlx::query(
            "INSERT INTO notes (id, owner_id, title, folder, format, path, content, content_sha256,
               size_bytes, version, created_at, modified_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 1, $10, $11)",
        )
        .bind(id)
        .bind(current.id)
        .bind(&title)
        .bind(&folder)
        .bind(format.extension())
        .bind(&path)
        .bind(&req.content)
        .bind(&hash)
        .bind(size)
        .bind(req.created_at.unwrap_or(modified_at))
        .bind(modified_at)
        .execute(&mut *tx)
        .await
    };
    if let Err(sqlx::Error::Database(e)) = &saved {
        if e.is_unique_violation() {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "path_conflict",
                format!("같은 위치에 같은 이름의 메모가 있습니다: {path}"),
            ));
        }
    }
    saved?;
    let note = load_note(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(note))
}

pub async fn delete_note(
    State(state): State<AppState>,
    current: CurrentAccount,
    Path(id): Path<Uuid>,
    Query(q): Query<DeleteNoteQuery>,
) -> ApiResult<Json<NoteView>> {
    let me = holder(&current, &q.client_id)?;
    let mut tx = state.db()?.begin().await?;
    let existing: Option<(Uuid, i64, i64, bool)> = sqlx::query_as(
        "SELECT owner_id, version, size_bytes, deleted_at IS NOT NULL FROM notes WHERE id = $1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let (version, size) = match existing {
        Some((owner, version, size, false)) if owner == current.id => (version, size),
        Some((owner, version, _, true)) if owner == current.id => {
            return Err(version_conflict(version, true))
        }
        _ => return Err(not_found()),
    };
    require_lease(&mut tx, id, &me, q.fencing_token).await?;
    if q.base_version != version {
        return Err(version_conflict(version, false));
    }
    charge_quota(&mut tx, current.id, -size).await?;
    sqlx::query(
        "UPDATE notes SET deleted_at = NOW(), content = '', size_bytes = 0, content_sha256 = $2,
           version = version + 1, modified_at = NOW(), change_seq = nextval('note_change_seq')
         WHERE id = $1",
    )
    .bind(id)
    .bind(sha256_hex(""))
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM note_leases WHERE note_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let note = load_note(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(note))
}

/// 편집 임대 획득 또는 갱신. 다른 보유자가 유효한 임대를 갖고 있으면 423.
pub async fn acquire_lease(
    State(state): State<AppState>,
    current: CurrentAccount,
    Path(id): Path<Uuid>,
    ApiJson(req): ApiJson<LeaseRequest>,
) -> ApiResult<Json<LeaseView>> {
    let me = holder(&current, &req.client_id)?;
    let label: String = req.label.trim().chars().take(HOLDER_LABEL_MAX).collect();
    let ttl = state.config.lease_ttl_secs;
    let mut tx = state.db()?.begin().await?;
    let note: Option<(Uuid, bool)> = sqlx::query_as(
        "SELECT owner_id, deleted_at IS NOT NULL FROM notes WHERE id = $1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    match note {
        Some((owner, false)) if owner == current.id => {}
        _ => return Err(not_found()),
    }
    let lease: Option<(String, String, i64, DateTime<Utc>, bool)> = sqlx::query_as(
        "SELECT holder, holder_label, fencing_token, expires_at, expires_at > NOW()
         FROM note_leases WHERE note_id = $1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let token = match lease {
        Some((holder, label, _, expires_at, true)) if holder != me => {
            return Err(lease_held(&label, expires_at))
        }
        // 같은 보유자의 유효한 임대 → 갱신 (token 유지)
        Some((_, _, token, _, true)) => token,
        // 없거나 만료 → 새 fencing token
        _ => {
            let (epoch,): (i64,) = sqlx::query_as(
                "UPDATE notes SET lease_epoch = lease_epoch + 1 WHERE id = $1 RETURNING lease_epoch",
            )
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
            epoch
        }
    };
    let (expires_at,): (DateTime<Utc>,) = sqlx::query_as(
        "INSERT INTO note_leases (note_id, account_id, holder, holder_label, fencing_token, expires_at)
         VALUES ($1, $2, $3, $4, $5, NOW() + make_interval(secs => $6))
         ON CONFLICT (note_id) DO UPDATE SET holder = excluded.holder, holder_label = excluded.holder_label,
           fencing_token = excluded.fencing_token, expires_at = excluded.expires_at,
           acquired_at = CASE WHEN note_leases.fencing_token = excluded.fencing_token
                              THEN note_leases.acquired_at ELSE NOW() END
         RETURNING expires_at",
    )
    .bind(id)
    .bind(current.id)
    .bind(&me)
    .bind(&label)
    .bind(token)
    .bind(ttl as f64)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(LeaseView {
        fencing_token: token,
        expires_at,
        ttl_seconds: ttl,
    }))
}

/// 편집 종료 시 임대 반납 (내 임대만, 없으면 그대로 성공)
pub async fn release_lease(
    State(state): State<AppState>,
    current: CurrentAccount,
    Path(id): Path<Uuid>,
    Query(q): Query<LeaseReleaseQuery>,
) -> ApiResult<StatusCode> {
    let me = holder(&current, &q.client_id)?;
    sqlx::query("DELETE FROM note_leases WHERE note_id = $1 AND holder = $2 AND account_id = $3")
        .bind(id)
        .bind(&me)
        .bind(current.id)
        .execute(state.db()?)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

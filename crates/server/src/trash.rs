//! 휴지통 (2026-11-13 사용 의견: 프로젝트·폴더·메모를 지우면 1주일 보관)
//!
//! - 지운 메모: 묘비(deleted_at, 본문 '')로 다른 기기에 전달되고, 본문은 `trashed_content`에 보관 (마이그레이션 011)
//! - `GET /api/v1/trash`: 보관 기간(TRASH_DAYS, 기본 7일) 안의 지운 메모·프로젝트
//! - `POST /api/v1/notes/{id}/restore`: 메모 되살리기 (프로젝트가 지워졌으면 프로젝트도). 같은 위치에 같은 이름이 있으면 409
//! - `POST /api/v1/projects/{id}/restore`: 프로젝트와, 그 프로젝트에서 보관 중인 메모를 함께 되살리기
//! - 보관 기간이 지나면 [`purge`]가 본문을 비움 (묘비는 동기화용으로 남김). 폴더는 메모 경로라 메모와 함께 돌아옴
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
    accounts::{CurrentAccount, Scope},
    error::{ApiError, ApiResult},
    notes::{charge_quota, sha256_hex},
    AppState,
};

#[derive(Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct TrashNote {
    id: Uuid,
    title: String,
    folder: String,
    format: String,
    project_id: Option<Uuid>,
    size_bytes: i64,
    deleted_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

#[derive(Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct TrashProject {
    id: Uuid,
    name: String,
    deleted_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashList {
    notes: Vec<TrashNote>,
    projects: Vec<TrashProject>,
    retention_days: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreResult {
    restored_notes: u64,
    restored_projects: u64,
}

fn gone() -> ApiError {
    ApiError::new(
        StatusCode::NOT_FOUND,
        "not_in_trash",
        "휴지통에 없습니다 (보관 기간이 지났거나 이미 되살림)",
    )
}

pub async fn list(
    State(state): State<AppState>,
    current: CurrentAccount,
) -> ApiResult<Json<TrashList>> {
    current.require(Scope::NotesRead)?;
    let db = state.db()?;
    let days = state.config.trash_days as i32;
    let notes = sqlx::query_as(
        "SELECT id, title, folder, format, project_id, trashed_size AS size_bytes, deleted_at,
           deleted_at + make_interval(days => $2) AS expires_at
         FROM notes WHERE owner_id = $1 AND deleted_at IS NOT NULL AND trashed_content IS NOT NULL
           AND deleted_at > NOW() - make_interval(days => $2)
         ORDER BY deleted_at DESC LIMIT 500",
    )
    .bind(current.id)
    .bind(days)
    .fetch_all(db)
    .await?;
    let projects = sqlx::query_as(
        "SELECT id, name, deleted_at, deleted_at + make_interval(days => $2) AS expires_at
         FROM projects WHERE owner_id = $1 AND deleted_at IS NOT NULL
           AND deleted_at > NOW() - make_interval(days => $2)
         ORDER BY deleted_at DESC",
    )
    .bind(current.id)
    .bind(days)
    .fetch_all(db)
    .await?;
    Ok(Json(TrashList {
        notes,
        projects,
        retention_days: state.config.trash_days,
    }))
}

/// 보관 중인 프로젝트를 되살림 (이름이 겹치면 409)
async fn restore_project_row(
    tx: &mut Transaction<'_, Postgres>,
    owner: Uuid,
    project: Uuid,
    days: i32,
) -> ApiResult<bool> {
    let restored = sqlx::query(
        "UPDATE projects SET deleted_at = NULL, modified_at = NOW(), change_seq = nextval('note_change_seq')
         WHERE id = $1 AND owner_id = $2 AND deleted_at IS NOT NULL
           AND deleted_at > NOW() - make_interval(days => $3)",
    )
    .bind(project)
    .bind(owner)
    .bind(days)
    .execute(&mut **tx)
    .await;
    match restored {
        Ok(r) => Ok(r.rows_affected() > 0),
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Err(ApiError::new(
            StatusCode::CONFLICT,
            "project_name_taken",
            "같은 이름의 프로젝트가 있어 되살릴 수 없습니다. 그 프로젝트 이름을 바꾼 뒤 다시 시도하세요",
        )),
        Err(e) => Err(e.into()),
    }
}

/// 보관 중인 메모 하나를 되살림 (본문·용량 복구, 다른 기기에는 새 버전으로 전달)
async fn restore_note_row(
    tx: &mut Transaction<'_, Postgres>,
    owner: Uuid,
    note: Uuid,
    days: i32,
) -> ApiResult<bool> {
    let row: Option<(i64, String)> = sqlx::query_as(
        "SELECT trashed_size, trashed_content FROM notes
         WHERE id = $1 AND owner_id = $2 AND deleted_at IS NOT NULL AND trashed_content IS NOT NULL
           AND deleted_at > NOW() - make_interval(days => $3) FOR UPDATE",
    )
    .bind(note)
    .bind(owner)
    .bind(days)
    .fetch_optional(&mut **tx)
    .await?;
    let Some((size, content)) = row else {
        return Ok(false);
    };
    charge_quota(tx, owner, size).await?;
    let restored = sqlx::query(
        "UPDATE notes SET deleted_at = NULL, content = trashed_content, size_bytes = trashed_size,
           content_sha256 = $2, trashed_content = NULL, trashed_size = 0, version = version + 1,
           modified_at = NOW(), change_seq = nextval('note_change_seq')
         WHERE id = $1",
    )
    .bind(note)
    .bind(sha256_hex(&content))
    .execute(&mut **tx)
    .await;
    match restored {
        Ok(_) => Ok(true),
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Err(ApiError::new(
            StatusCode::CONFLICT,
            "path_conflict",
            "같은 위치에 같은 이름의 메모가 있어 되살릴 수 없습니다. 그 메모 이름을 바꾼 뒤 다시 시도하세요",
        )),
        Err(e) => Err(e.into()),
    }
}

pub async fn restore_note(
    State(state): State<AppState>,
    current: CurrentAccount,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<RestoreResult>> {
    current.require(Scope::NotesWrite)?;
    let days = state.config.trash_days as i32;
    let mut tx = state.db()?.begin().await?;
    // 지워진 프로젝트에 있던 메모면 프로젝트도 함께 (메모만 기본으로 옮기지 않음)
    let project: Option<(Option<Uuid>,)> =
        sqlx::query_as("SELECT project_id FROM notes WHERE id = $1 AND owner_id = $2")
            .bind(id)
            .bind(current.id)
            .fetch_optional(&mut *tx)
            .await?;
    let mut restored_projects = 0;
    if let Some((Some(project),)) = project {
        if restore_project_row(&mut tx, current.id, project, days).await? {
            restored_projects = 1;
        }
    }
    if !restore_note_row(&mut tx, current.id, id, days).await? {
        return Err(gone());
    }
    tx.commit().await?;
    Ok(Json(RestoreResult {
        restored_notes: 1,
        restored_projects,
    }))
}

pub async fn restore_project(
    State(state): State<AppState>,
    current: CurrentAccount,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<RestoreResult>> {
    current.require(Scope::NotesWrite)?;
    let days = state.config.trash_days as i32;
    let mut tx = state.db()?.begin().await?;
    if !restore_project_row(&mut tx, current.id, id, days).await? {
        return Err(gone());
    }
    let notes: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM notes WHERE project_id = $1 AND owner_id = $2 AND deleted_at IS NOT NULL
           AND trashed_content IS NOT NULL AND deleted_at > NOW() - make_interval(days => $3)",
    )
    .bind(id)
    .bind(current.id)
    .bind(days)
    .fetch_all(&mut *tx)
    .await?;
    let mut restored_notes = 0;
    for (note,) in notes {
        if restore_note_row(&mut tx, current.id, note, days).await? {
            restored_notes += 1;
        }
    }
    tx.commit().await?;
    Ok(Json(RestoreResult {
        restored_notes,
        restored_projects: 1,
    }))
}

/// 보관 기간이 지난 메모의 본문을 비움 (묘비는 남김). 비운 개수
pub async fn purge(db: &PgPool, days: i64) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query(
        "UPDATE notes SET trashed_content = NULL, trashed_size = 0
         WHERE trashed_content IS NOT NULL AND deleted_at < NOW() - make_interval(days => $1)",
    )
    .bind(days as i32)
    .execute(db)
    .await?
    .rows_affected())
}

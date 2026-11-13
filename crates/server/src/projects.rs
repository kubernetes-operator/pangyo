//! 프로젝트 API (ADR-0005: 폴더 위 단계)
//!
//! - `GET /api/v1/projects`: 살아 있는 프로젝트 목록
//! - `PUT /api/v1/projects/{id}`: 만들기·이름 바꾸기. 메모처럼 ID를 클라이언트가 정함(오프라인에서 만들 수 있음)
//! - `DELETE /api/v1/projects/{id}`: 빈 프로젝트만. 묘비(deleted_at)로 남겨 다른 기기에 전달
//!
//! 메모의 `project_id`가 NULL이면 "기본" 프로젝트입니다. 이름 규칙은 폴더 한 칸과 같습니다(`paths.json`).
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use memonest_core::parse_folder;
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::{
    accounts::{CurrentAccount, Scope},
    error::{ApiError, ApiJson, ApiResult},
    AppState,
};

/// 데스크톱에서 프로젝트 폴더를 `@이름`으로 표시하므로 이름 자체는 `@`로 시작할 수 없음
const RESERVED_PREFIX: char = '@';

pub const PROJECT_COLUMNS: &str =
    "id, name, created_at, modified_at, (deleted_at IS NOT NULL) AS deleted, change_seq";

#[derive(Debug, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ProjectView {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
    pub deleted: bool,
    pub change_seq: i64,
}

#[derive(Deserialize)]
pub struct PutProjectRequest {
    name: String,
}

fn not_found() -> ApiError {
    ApiError::new(
        StatusCode::NOT_FOUND,
        "project_not_found",
        "프로젝트를 찾을 수 없습니다",
    )
}

/// 이름 검사·정규화 (폴더 한 칸 규칙 + `/`·`@` 시작 금지)
pub fn project_name(raw: &str) -> ApiResult<String> {
    let invalid = |m: String| ApiError::bad_request("invalid_project_name", m);
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(invalid("프로젝트 이름을 입력하세요".into()));
    }
    if trimmed.contains('/') || trimmed.contains('\\') {
        return Err(invalid("프로젝트 이름에는 / 를 쓸 수 없습니다".into()));
    }
    if trimmed.starts_with(RESERVED_PREFIX) {
        return Err(invalid("프로젝트 이름은 @로 시작할 수 없습니다".into()));
    }
    parse_folder(trimmed, 1).map_err(|e| invalid(e.to_string()))
}

/// 메모 저장 전에: 이 계정의 살아 있는 프로젝트인지 (삭제와 동시에 일어나지 않게 FOR SHARE 잠금)
pub async fn require_live_project(
    tx: &mut Transaction<'_, Postgres>,
    owner: Uuid,
    project: Uuid,
) -> ApiResult<()> {
    let row: Option<(i32,)> = sqlx::query_as(
        "SELECT 1 FROM projects WHERE id = $1 AND owner_id = $2 AND deleted_at IS NULL FOR SHARE",
    )
    .bind(project)
    .bind(owner)
    .fetch_optional(&mut **tx)
    .await?;
    row.map(|_| ()).ok_or_else(|| {
        ApiError::bad_request(
            "invalid_project",
            "없거나 삭제된 프로젝트입니다. 목록을 새로 받으세요",
        )
    })
}

pub async fn list(
    State(state): State<AppState>,
    current: CurrentAccount,
) -> ApiResult<Json<Vec<ProjectView>>> {
    current.require(Scope::NotesRead)?;
    let projects = sqlx::query_as(&format!(
        "SELECT {PROJECT_COLUMNS} FROM projects WHERE owner_id = $1 AND deleted_at IS NULL ORDER BY lower(name)"
    ))
    .bind(current.id)
    .fetch_all(state.db()?)
    .await?;
    Ok(Json(projects))
}

pub async fn put(
    State(state): State<AppState>,
    current: CurrentAccount,
    Path(id): Path<Uuid>,
    ApiJson(req): ApiJson<PutProjectRequest>,
) -> ApiResult<Json<ProjectView>> {
    current.require(Scope::NotesWrite)?;
    let name = project_name(&req.name)?;
    let mut tx = state.db()?.begin().await?;
    let existing: Option<(Uuid, bool)> = sqlx::query_as(
        "SELECT owner_id, deleted_at IS NOT NULL FROM projects WHERE id = $1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let saved = match existing {
        Some((owner, _)) if owner != current.id => return Err(not_found()),
        Some((_, true)) => return Err(not_found()),
        Some(_) => {
            sqlx::query(
                "UPDATE projects SET name = $2, modified_at = NOW(), change_seq = nextval('note_change_seq')
                 WHERE id = $1 AND name <> $2",
            )
            .bind(id)
            .bind(&name)
            .execute(&mut *tx)
            .await
        }
        None => {
            sqlx::query("INSERT INTO projects (id, owner_id, name) VALUES ($1, $2, $3)")
                .bind(id)
                .bind(current.id)
                .bind(&name)
                .execute(&mut *tx)
                .await
        }
    };
    if let Err(sqlx::Error::Database(e)) = &saved {
        if e.is_unique_violation() {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "project_name_taken",
                format!("같은 이름의 프로젝트가 있습니다: {name}"),
            ));
        }
    }
    saved?;
    let project = sqlx::query_as(&format!(
        "SELECT {PROJECT_COLUMNS} FROM projects WHERE id = $1"
    ))
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(project))
}

pub async fn delete(
    State(state): State<AppState>,
    current: CurrentAccount,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ProjectView>> {
    current.require(Scope::NotesWrite)?;
    let mut tx = state.db()?.begin().await?;
    let existing: Option<(bool,)> = sqlx::query_as(
        "SELECT deleted_at IS NOT NULL FROM projects WHERE id = $1 AND owner_id = $2 FOR UPDATE",
    )
    .bind(id)
    .bind(current.id)
    .fetch_optional(&mut *tx)
    .await?;
    match existing {
        None | Some((true,)) => return Err(not_found()),
        Some((false,)) => {}
    }
    let (notes,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM notes WHERE project_id = $1 AND deleted_at IS NULL")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    if notes > 0 {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "project_not_empty",
            format!("메모가 {notes}개 있는 프로젝트는 지울 수 없습니다. 메모를 옮기거나 지운 뒤 다시 시도하세요"),
        ));
    }
    let project = sqlx::query_as(&format!(
        "UPDATE projects SET deleted_at = NOW(), modified_at = NOW(), change_seq = nextval('note_change_seq')
         WHERE id = $1 RETURNING {PROJECT_COLUMNS}"
    ))
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(project))
}

#[cfg(test)]
mod tests {
    #[test]
    fn project_names() {
        assert_eq!(super::project_name("  업무 ").unwrap(), "업무");
        for bad in ["", "  ", "a/b", "@개인", ".hidden", "CON", "a:b"] {
            assert!(super::project_name(bad).is_err(), "{bad}");
        }
    }
}

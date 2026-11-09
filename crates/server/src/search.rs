//! 서버 검색·연관 메모 (로드맵 07, 요구 20). PostgreSQL pg_trgm (db/postgres/004_search.sql)
//!
//! - 검색: 제목·본문 부분 일치(대소문자 무시) + 3글자 조각 유사도로 순위. 한글도 글자 단위로 동작
//! - 연관 메모: 기준 메모의 제목+본문 앞부분과 다른 메모의 유사도 (임계값 이상만)
//!   본인 계정 메모만, 삭제된 메모 제외.
use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    accounts::{CurrentAccount, Scope},
    error::{ApiError, ApiResult},
    AppState,
};

// === 0. 검색 정책 변수 ===
/// 검색어 길이 (문자)
const QUERY_MIN: usize = 1;
const QUERY_MAX: usize = 200;
/// 결과 수 기본/최대
const LIMIT_DEFAULT: i64 = 20;
const LIMIT_MAX: i64 = 50;
/// 미리보기 길이(문자)와 일치 위치 앞쪽 여유
const SNIPPET_LEN: i32 = 160;
const SNIPPET_BEFORE: i32 = 40;
/// 연관 메모 비교에 쓰는 본문 앞부분 길이와 최소 유사도
const SIMILAR_PREFIX: i32 = 2000;
const SIMILAR_MIN_SCORE: f32 = 0.08;

#[derive(Deserialize)]
pub struct SearchQuery {
    q: String,
    limit: Option<i64>,
}

#[derive(Deserialize)]
pub struct SimilarQuery {
    limit: Option<i64>,
}

#[derive(Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    id: Uuid,
    title: String,
    folder: String,
    format: String,
    snippet: String,
    score: f32,
}

/// LIKE 패턴에서 특수문자(%, _, \)를 글자 그대로 찾도록 이스케이프
fn like_escape(q: &str) -> String {
    q.chars()
        .flat_map(|c| match c {
            '%' | '_' | '\\' => vec!['\\', c],
            c => vec![c],
        })
        .collect()
}

pub async fn search(
    State(state): State<AppState>,
    current: CurrentAccount,
    Query(params): Query<SearchQuery>,
) -> ApiResult<Json<Vec<SearchHit>>> {
    current.require(Scope::NotesRead)?;
    let q = params.q.trim();
    let n = q.chars().count();
    if !(QUERY_MIN..=QUERY_MAX).contains(&n) {
        return Err(ApiError::bad_request(
            "invalid_query",
            format!("검색어는 {QUERY_MIN}~{QUERY_MAX}자"),
        ));
    }
    let limit = params.limit.unwrap_or(LIMIT_DEFAULT).clamp(1, LIMIT_MAX);
    let hits = sqlx::query_as(
        "SELECT id, title, folder, format,
           CASE WHEN strpos(lower(content), lower($2)) > 0
                THEN substr(content, greatest(strpos(lower(content), lower($2)) - $5, 1), $4)
                ELSE left(content, $4) END AS snippet,
           -- 제목 일치가 본문 일치보다 항상 앞서도록 가산점 (제목 2, 본문 0.5)
           (greatest(word_similarity($2, title) * 2, word_similarity($2, content))
             + CASE WHEN title ILIKE $3 THEN 2 ELSE 0 END
             + CASE WHEN content ILIKE $3 THEN 0.5 ELSE 0 END)::real AS score
         FROM notes
         WHERE owner_id = $1 AND deleted_at IS NULL
           AND (title ILIKE $3 OR content ILIKE $3 OR title % $2 OR $2 <% content)
         ORDER BY score DESC, modified_at DESC
         LIMIT $6",
    )
    .bind(current.id)
    .bind(q)
    .bind(format!("%{}%", like_escape(q)))
    .bind(SNIPPET_LEN)
    .bind(SNIPPET_BEFORE)
    .bind(limit)
    .fetch_all(state.db()?)
    .await?;
    Ok(Json(hits))
}

pub async fn similar(
    State(state): State<AppState>,
    current: CurrentAccount,
    Path(id): Path<Uuid>,
    Query(params): Query<SimilarQuery>,
) -> ApiResult<Json<Vec<SearchHit>>> {
    current.require(Scope::NotesRead)?;
    let limit = params.limit.unwrap_or(5).clamp(1, LIMIT_MAX);
    let db = state.db()?;
    let base: Option<(String,)> = sqlx::query_as(
        "SELECT title || ' ' || left(content, $3) FROM notes
         WHERE id = $1 AND owner_id = $2 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(current.id)
    .bind(SIMILAR_PREFIX)
    .fetch_optional(db)
    .await?;
    let (base,) = base.ok_or_else(|| {
        ApiError::new(
            axum::http::StatusCode::NOT_FOUND,
            "note_not_found",
            "메모를 찾을 수 없습니다",
        )
    })?;
    let hits = sqlx::query_as(
        "SELECT id, title, folder, format, left(content, $5) AS snippet, score FROM (
           SELECT *, similarity(title || ' ' || left(content, $4), $3)::real AS score
           FROM notes WHERE owner_id = $1 AND id <> $2 AND deleted_at IS NULL
         ) t
         WHERE score >= $6
         ORDER BY score DESC, modified_at DESC
         LIMIT $7",
    )
    .bind(current.id)
    .bind(id)
    .bind(&base)
    .bind(SIMILAR_PREFIX)
    .bind(SNIPPET_LEN)
    .bind(SIMILAR_MIN_SCORE)
    .bind(limit)
    .fetch_all(db)
    .await?;
    Ok(Json(hits))
}

#[cfg(test)]
mod tests {
    #[test]
    fn like_escape_keeps_wildcards_literal() {
        assert_eq!(super::like_escape("100%_a\\b"), "100\\%\\_a\\\\b");
        assert_eq!(super::like_escape("회의록"), "회의록");
    }
}

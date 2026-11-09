//! 설치 파일 다운로드 (2026-11-09 사용자 요청: Kubernetes로 배포, 실제 주소 비노출)
//!
//! - 목록: `GET /api/v1/downloads` — 로그인 사용자(세션·장비)만. 저장 경로는 응답에 넣지 않음
//! - 링크: `POST /api/v1/downloads/{id}/link` → `/dl/<임의 토큰>` (기본 5분 유효, 토큰은 해시만 저장)
//! - 받기: `GET /dl/{token}` — 토큰이 곧 권한. 큰 파일은 나눠서 전송(스트리밍), 응답 시간 제한 없음
//!
//! 파일 목록은 `DOWNLOADS_DIR/manifest.json` (scripts/publish-downloads.sh 가 만듦):
//! `{"version":"0.1.0","files":[{"id":"windows-msi","platform":"Windows","label":"설치 프로그램 (.msi)",
//!   "file":"0.1.0/MemoNest_0.1.0_x64_en-US.msi","size":123,"sha256":"..."}]}`
use std::path::{Component, Path as FsPath, PathBuf};

use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    accounts::{CurrentAccount, Scope},
    config::DOWNLOAD_LINK_TTL_SECS,
    error::{ApiError, ApiResult},
    security, AppState,
};

// === 0. 변수 ===
/// 목록 파일 이름 (DOWNLOADS_DIR 안)
pub const MANIFEST: &str = "manifest.json";
/// 다운로드 링크 경로 접두사
pub const LINK_PREFIX: &str = "/dl/";

#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub version: String,
    pub files: Vec<ManifestFile>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ManifestFile {
    pub id: String,
    pub platform: String,
    pub label: String,
    /// DOWNLOADS_DIR 기준 상대 경로 (응답에 포함하지 않음)
    pub file: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadItem {
    id: String,
    platform: String,
    label: String,
    file_name: String,
    size: u64,
    sha256: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadList {
    version: String,
    files: Vec<DownloadItem>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadLink {
    url: String,
    expires_at: DateTime<Utc>,
}

fn unavailable() -> ApiError {
    ApiError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "downloads_unavailable",
        "설치 파일이 아직 준비되지 않았습니다",
    )
}

fn not_found() -> ApiError {
    ApiError::new(
        StatusCode::NOT_FOUND,
        "download_not_found",
        "찾을 수 없는 설치 파일입니다",
    )
}

/// 목록 읽기 + 경로 검증 (DOWNLOADS_DIR 밖을 가리키는 항목은 무시)
async fn load_manifest(state: &AppState) -> ApiResult<(PathBuf, Manifest)> {
    let dir = state.config.downloads_dir.clone().ok_or_else(unavailable)?;
    let raw = tokio::fs::read(dir.join(MANIFEST))
        .await
        .map_err(|_| unavailable())?;
    let mut manifest: Manifest = serde_json::from_slice(&raw).map_err(|e| {
        tracing::error!(error = %e, "downloads manifest invalid");
        unavailable()
    })?;
    manifest.files.retain(|f| safe_relative(&f.file));
    Ok((dir, manifest))
}

/// 상대 경로만 허용 (`..`, 절대 경로, 루트 금지)
fn safe_relative(path: &str) -> bool {
    let p = FsPath::new(path);
    !path.is_empty() && p.components().all(|c| matches!(c, Component::Normal(_)))
}

pub async fn list(
    State(state): State<AppState>,
    current: CurrentAccount,
) -> ApiResult<Json<DownloadList>> {
    current.require(Scope::Account)?; // 자동화 토큰으로는 불가
    let (_, manifest) = load_manifest(&state).await?;
    Ok(Json(DownloadList {
        version: manifest.version,
        files: manifest
            .files
            .into_iter()
            .map(|f| DownloadItem {
                file_name: FsPath::new(&f.file)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                id: f.id,
                platform: f.platform,
                label: f.label,
                size: f.size,
                sha256: f.sha256,
            })
            .collect(),
    }))
}

pub async fn create_link(
    State(state): State<AppState>,
    current: CurrentAccount,
    Path(id): Path<String>,
) -> ApiResult<(StatusCode, Json<DownloadLink>)> {
    current.require(Scope::Account)?;
    let (_, manifest) = load_manifest(&state).await?;
    if !manifest.files.iter().any(|f| f.id == id) {
        return Err(not_found());
    }
    let token = security::new_token();
    let (expires_at,): (DateTime<Utc>,) = sqlx::query_as(
        "INSERT INTO download_links (token_hash, account_id, file_id, expires_at)
         VALUES ($1, $2, $3, NOW() + make_interval(secs => $4)) RETURNING expires_at",
    )
    .bind(security::token_hash(&token))
    .bind(current.id)
    .bind(&id)
    .bind(DOWNLOAD_LINK_TTL_SECS as f64)
    .fetch_one(state.db()?)
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(DownloadLink {
            url: format!("{LINK_PREFIX}{token}"),
            expires_at,
        }),
    ))
}

/// 토큰 주소로 파일 받기. 유효 시간 안에서는 다시 받을 수 있음(이어받기 대비)
pub async fn download(State(state): State<AppState>, Path(token): Path<String>) -> Response {
    match serve(&state, &token).await {
        Ok(r) => r,
        Err(e) => e.into_response(),
    }
}

async fn serve(state: &AppState, token: &str) -> ApiResult<Response> {
    let gone = || {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "link_invalid",
            "링크가 없거나 만료되었습니다. 앱에서 다시 받으세요",
        )
    };
    if token.len() > 64 {
        return Err(gone());
    }
    let row: Option<(String,)> = sqlx::query_as(
        "UPDATE download_links SET first_used_at = COALESCE(first_used_at, NOW())
         WHERE token_hash = $1 AND expires_at > NOW() RETURNING file_id",
    )
    .bind(security::token_hash(token))
    .fetch_optional(state.db()?)
    .await?;
    let (file_id,) = row.ok_or_else(gone)?;
    let (dir, manifest) = load_manifest(state).await?;
    let entry = manifest
        .files
        .into_iter()
        .find(|f| f.id == file_id)
        .ok_or_else(gone)?;
    let path = dir.join(&entry.file);
    let file = tokio::fs::File::open(&path).await.map_err(|e| {
        tracing::error!(error = %e, id = %entry.id, "download file missing");
        gone()
    })?;
    let len = file.metadata().await.map(|m| m.len()).unwrap_or(entry.size);
    let name = FsPath::new(&entry.file)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "download".into());
    let ascii: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || ".-_".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    let disposition = format!(
        "attachment; filename=\"{ascii}\"; filename*=UTF-8''{}",
        percent_encode(&name)
    );
    let body = Body::from_stream(tokio_util::io::ReaderStream::new(file));
    let mut response = (StatusCode::OK, body).into_response();
    let h = response.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    h.insert(header::CONTENT_LENGTH, HeaderValue::from(len));
    if let Ok(v) = HeaderValue::from_str(&disposition) {
        h.insert(header::CONTENT_DISPOSITION, v);
    }
    if let Ok(v) = HeaderValue::from_str(&entry.sha256) {
        h.insert("x-content-sha256", v);
    }
    Ok(response)
}

fn percent_encode(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn relative_paths_only() {
        assert!(super::safe_relative("0.1.0/MemoNest.msi"));
        for bad in ["", "../etc/passwd", "/etc/passwd", "a/../../b", "./x"] {
            assert!(!super::safe_relative(bad), "{bad}");
        }
    }

    #[test]
    fn percent_encoding_for_filename_star() {
        assert_eq!(
            super::percent_encode("메모 v1.zip"),
            "%EB%A9%94%EB%AA%A8%20v1.zip"
        );
    }
}

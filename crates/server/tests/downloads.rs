//! 설치 파일 다운로드: 로그인 사용자만 목록·링크, 링크는 임의 토큰(실제 경로 비노출). 실제 Postgres 필요.
mod common;

use std::time::Duration;

use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
};
use common::{app, app_with, Auth};
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

const BODY: &[u8] = b"fake installer bytes \x00\x01\x02";

fn downloads_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("0.1.0")).unwrap();
    std::fs::write(dir.path().join("0.1.0/MemoNest_0.1.0_x64.msi"), BODY).unwrap();
    std::fs::write(
        dir.path().join("manifest.json"),
        json!({ "version": "0.1.0", "files": [
            { "id": "windows-msi", "platform": "Windows", "label": "설치 프로그램 (.msi)",
              "file": "0.1.0/MemoNest_0.1.0_x64.msi", "size": BODY.len(), "sha256": "abc" },
            { "id": "evil", "platform": "x", "label": "x", "file": "../../etc/passwd", "size": 1, "sha256": "x" }
        ]})
        .to_string(),
    )
    .unwrap();
    dir
}

#[tokio::test]
async fn list_link_and_download_without_exposing_path() {
    let dir = downloads_dir();
    let path = dir.path().to_owned();
    let Some(app) = app_with(|c| c.downloads_dir = Some(path)).await else {
        return;
    };
    let c = app.register("dl@example.com").await.cookie.unwrap();

    let list = app
        .call(Method::GET, "/api/v1/downloads", None, Auth::Cookie(&c))
        .await;
    assert_eq!(list.status, StatusCode::OK, "{}", list.json);
    assert_eq!(list.json["version"], "0.1.0");
    assert_eq!(
        list.json["files"].as_array().unwrap().len(),
        1,
        "저장소 밖 경로 항목은 제외"
    );
    assert_eq!(list.json["files"][0]["fileName"], "MemoNest_0.1.0_x64.msi");
    assert!(
        !list.json.to_string().contains("0.1.0/"),
        "저장 경로 비노출"
    );

    let link = app
        .call(
            Method::POST,
            "/api/v1/downloads/windows-msi/link",
            None,
            Auth::Cookie(&c),
        )
        .await;
    assert_eq!(link.status, StatusCode::CREATED, "{}", link.json);
    let url = link.json["url"].as_str().unwrap().to_owned();
    assert!(
        url.starts_with("/dl/") && !url.contains("msi") && !url.contains("0.1.0"),
        "{url}"
    );

    // 쿠키 없이도 링크로 받기 가능(토큰이 권한)
    let (status, headers, bytes) = app.raw(&url).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bytes, BODY);
    assert_eq!(
        headers[header::CONTENT_LENGTH],
        BODY.len().to_string().as_str()
    );
    assert!(headers[header::CONTENT_DISPOSITION]
        .to_str()
        .unwrap()
        .contains("MemoNest_0.1.0_x64.msi"));
    assert_eq!(headers["x-content-sha256"], "abc");
    // 일회용: 끝까지 받은 링크는 닫히고(완료 기록) 다시 쓸 수 없음
    assert!(wait_completed(&app, &url).await, "완료 기록");
    let (status, _, _) = app.raw(&url).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "다 받은 링크 재사용 거절");
    let (status, _, _) = app.raw("/dl/not-a-real-token").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let missing = app
        .call(
            Method::POST,
            "/api/v1/downloads/evil/link",
            None,
            Auth::Cookie(&c),
        )
        .await;
    assert_eq!(missing.json["error"], "download_not_found");
}

#[tokio::test]
async fn requires_login_and_not_automation_tokens() {
    let dir = downloads_dir();
    let path = dir.path().to_owned();
    let Some(app) = app_with(|c| c.downloads_dir = Some(path)).await else {
        return;
    };
    let anon = app
        .call(Method::GET, "/api/v1/downloads", None, Auth::None)
        .await;
    assert_eq!(anon.status, StatusCode::UNAUTHORIZED);
    let c = app.register("dl2@example.com").await.cookie.unwrap();
    let token = app
        .call(
            Method::POST,
            "/api/v1/integrations/tokens",
            Some(json!({ "name": "n8n", "scopes": ["notes:read"] })),
            Auth::Cookie(&c),
        )
        .await
        .json["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let via_token = app
        .call(Method::GET, "/api/v1/downloads", None, Auth::Bearer(&token))
        .await;
    assert_eq!(via_token.json["error"], "insufficient_scope");
}

#[tokio::test]
async fn unavailable_when_not_configured() {
    let Some(app) = app().await else { return };
    let c = app.register("dl3@example.com").await.cookie.unwrap();
    let r = app
        .call(Method::GET, "/api/v1/downloads", None, Auth::Cookie(&c))
        .await;
    assert_eq!(
        (r.status, r.json["error"].as_str()),
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Some("downloads_unavailable")
        )
    );
}

/// 완료 기록(비동기)이 남을 때까지 기다림
async fn wait_completed(app: &common::TestApp, url: &str) -> bool {
    let token = url.trim_start_matches("/dl/");
    let hash = memonest_server::security::token_hash(token);
    let Ok(db) = app.state.db() else {
        panic!("db");
    };
    for _ in 0..50 {
        let (done,): (bool,) = sqlx::query_as(
            "SELECT completed_at IS NOT NULL AND claimed_at IS NOT NULL FROM download_links WHERE token_hash = $1",
        )
        .bind(&hash)
        .fetch_one(db)
        .await
        .unwrap();
        if done {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

#[tokio::test]
async fn link_is_locked_while_downloading_and_reopens_if_interrupted() {
    let dir = downloads_dir();
    let path = dir.path().to_owned();
    let Some(app) = app_with(|c| c.downloads_dir = Some(path)).await else {
        return;
    };
    let c = app.register("dl-once@example.com").await.cookie.unwrap();
    let link = app
        .call(
            Method::POST,
            "/api/v1/downloads/windows-msi/link",
            None,
            Auth::Cookie(&c),
        )
        .await;
    let url = link.json["url"].as_str().unwrap().to_owned();
    let get = || {
        app.router
            .clone()
            .oneshot(Request::builder().uri(&url).body(Body::empty()).unwrap())
    };

    // 1) 받는 중(본문을 아직 읽지 않음)에는 같은 링크의 다른 요청 거절
    let first = get().await.unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    let second = get().await.unwrap();
    assert_eq!(
        second.status(),
        StatusCode::NOT_FOUND,
        "받는 중 동시 사용 거절"
    );
    drop(second);

    // 2) 중간에 끊기면(본문을 끝까지 읽지 않고 버림) 다시 받을 수 있음
    drop(first);
    let mut retry = StatusCode::NOT_FOUND;
    for _ in 0..50 {
        let res = get().await.unwrap();
        retry = res.status();
        if retry == StatusCode::OK {
            let bytes = res.into_body().collect().await.unwrap().to_bytes();
            assert_eq!(&bytes[..], BODY, "재시도는 전체 파일");
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(retry, StatusCode::OK, "끊긴 링크는 다시 열림");

    // 3) 끝까지 받은 뒤에는 닫힘 (HEAD·끊김으로도 다시 열리지 않음)
    assert!(wait_completed(&app, &url).await, "완료 기록");
    let head = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::HEAD)
                .uri(&url)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(head.status(), StatusCode::NOT_FOUND);
    assert_eq!(get().await.unwrap().status(), StatusCode::NOT_FOUND);
}

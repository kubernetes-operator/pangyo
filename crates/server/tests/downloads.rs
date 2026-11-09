//! 설치 파일 다운로드: 로그인 사용자만 목록·링크, 링크는 임의 토큰(실제 경로 비노출). 실제 Postgres 필요.
mod common;

use axum::http::{header, Method, StatusCode};
use common::{app, app_with, Auth};
use serde_json::json;

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

    // 쿠키 없이도 링크로 받기 가능(토큰이 권한), 유효 시간 안에서는 다시 받기 가능
    for _ in 0..2 {
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
    }
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

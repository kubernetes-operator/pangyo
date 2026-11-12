//! 운영 지표 (로드맵 10): 경로 템플릿만 라벨에, API 포트에는 /metrics 없음, DB 지표
mod common;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
};
use http_body_util::BodyExt;
use memonest_server::{metrics, router, AppState, Config};
use tower::ServiceExt;

const NOTE_ID: &str = "7d3c1b2a-0000-4000-8000-00000000abcd";

async fn get(app: &axum::Router, uri: &str) -> (StatusCode, String) {
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::test]
async fn metrics_use_route_templates_and_are_not_on_api_port() {
    let state = AppState::without_database(Config::default());
    let api = router(state.clone());
    get(&api, "/healthz").await;
    get(&api, &format!("/api/v1/notes/{NOTE_ID}")).await;
    get(&api, "/dl/secret-download-token").await;
    let (status, body) = get(&api, "/metrics").await;
    assert_ne!(
        (status, body.contains("memonest_http_requests_total")),
        (StatusCode::OK, true),
        "API(외부 공개) 포트에서는 지표를 내보내지 않음"
    );

    let (status, text) = get(&metrics::router(state), "/metrics").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        text.contains(
            r#"memonest_http_requests_total{method="GET",route="/healthz",status="200"}"#
        ),
        "{text}"
    );
    assert!(
        text.contains(r#"route="/api/v1/notes/{id}""#),
        "메모 경로는 템플릿으로: {text}"
    );
    assert!(text.contains(r#"route="/dl/{token}""#));
    assert!(
        !text.contains(NOTE_ID) && !text.contains("secret-download-token"),
        "ID·토큰이 라벨에 없어야 함"
    );
    assert!(
        text.contains(r#"route="other""#),
        "경로 템플릿이 없는 요청(/metrics 404)은 other"
    );
    assert!(text.contains("memonest_http_request_duration_seconds_bucket{method=\"GET\",route=\"/healthz\",le=\"+Inf\"}"));
    assert!(text.contains("memonest_db_configured 0"));
    assert!(text.contains("memonest_db_up 0"));
    assert!(text.contains("memonest_build_info{version=\""));
}

#[tokio::test]
async fn database_gauges() {
    let Some(app) = common::app().await else {
        return;
    };
    let reply = app.register("metrics@example.test").await;
    assert_eq!(reply.status, StatusCode::CREATED);
    let (_, text) = get(&metrics::router(app.state.clone()), "/metrics").await;
    for line in [
        "memonest_db_configured 1",
        "memonest_db_up 1",
        "memonest_accounts 1",
        "memonest_accounts_max 999",
        "memonest_notes_live 0",
        "memonest_note_leases_active 0",
    ] {
        assert!(text.lines().any(|l| l == line), "{line} 없음:\n{text}");
    }
}

//! MemoNest API 서버 라우터.
//!
//! v0.1 Go 서버(`services/api`)와 같은 계약을 유지합니다 (`packages/protocol/openapi.json`):
//! - `GET /healthz`, `GET /readyz`, `GET /api/v1/info` 만 동작
//! - 그 밖의 `/api/v1/*` 는 **501** — 인증·동기화가 구현된 것처럼 보이는 가짜 응답을 내지 않습니다.
//!
//! 인증, 동기화, 저장소, 동시 편집 잠금(요구 #28, 승인 전 금지)은 아직 없습니다.

use std::time::Duration;

use axum::{
    body::Body,
    http::{header, HeaderValue, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{any, get},
    Json, Router,
};
use serde_json::json;
use tower::ServiceBuilder;
use tower_http::{
    limit::RequestBodyLimitLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    set_header::SetResponseHeaderLayer,
    timeout::TimeoutLayer,
    trace::{DefaultOnFailure, DefaultOnResponse, TraceLayer},
};
use tracing::Level;

// === 0. 서버 설정 변수 (실제 주소는 환경변수로 주입, 비밀값 없음) ===
/// 바인드 주소 환경변수. Kubernetes에서는 `0.0.0.0:8080` (Helm deployment.yaml)
pub const BIND_ENV: &str = "API_BIND_ADDRESS";
/// 로컬 개발 기본 주소: 외부 공개 안 함
pub const DEFAULT_BIND_ADDRESS: &str = "127.0.0.1:8080";
/// 클라이언트 협상에 쓰는 서버 버전 (Cargo 워크스페이스 버전)
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// 요청 처리 제한 시간 (Go 서버의 Read/WriteTimeout 15초와 같음)
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
/// 요청 본문 최대 크기. 동기화 구현 시 할당량 정책과 함께 다시 정합니다.
pub const BODY_LIMIT_BYTES: usize = 1024 * 1024;
/// 로그 레벨 환경변수 (예: `RUST_LOG=memonest_server=debug,tower_http=debug`)
pub const LOG_ENV: &str = "RUST_LOG";

/// 전체 라우터 (미들웨어 포함)
pub fn router() -> Router {
    let middleware = ServiceBuilder::new()
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(
            TraceLayer::new_for_http()
                // 모든 로그 줄에 요청 ID를 남깁니다 (docs/09_OPERATIONS.md: request_id, latency, 4xx/5xx)
                .make_span_with(|req: &Request<Body>| {
                    let id = req
                        .headers()
                        .get("x-request-id")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("-");
                    tracing::info_span!("http", method = %req.method(), uri = %req.uri(), request_id = %id)
                })
                .on_response(DefaultOnResponse::new().level(Level::INFO))
                // 미구현 501은 의도된 응답이므로 ERROR가 아닌 WARN (5xx 경보 오탐 방지)
                .on_failure(DefaultOnFailure::new().level(Level::WARN)),
        )
        .layer(PropagateRequestIdLayer::x_request_id())
        // 본문 제한은 타임아웃 바깥에 둡니다 (응답 본문 타입이 바뀌어 순서가 중요)
        .layer(RequestBodyLimitLayer::new(BODY_LIMIT_BYTES))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            REQUEST_TIMEOUT,
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ));

    Router::new()
        // 오케스트레이터 liveness/readiness: 외부 저장소 사용 전이므로 생존 여부만 검사
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        // GET 외 메서드는 Go 서버처럼 501 (구현되지 않은 쓰기 경로)
        .route("/api/v1/info", get(info).fallback(not_implemented))
        .route("/api/v1", any(not_implemented))
        .route("/api/v1/", any(not_implemented))
        .route("/api/v1/{*rest}", any(not_implemented))
        .fallback(not_found)
        .layer(middleware)
}

async fn healthz() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

async fn readyz() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok", "note": "sync_not_ready" }))
}

async fn info() -> Json<serde_json::Value> {
    Json(json!({
        "version": VERSION,
        "authEnabled": false,
        "syncEnabled": false,
        "warning": "prototype only",
    }))
}

/// 구현되지 않은 API는 데이터 손실/인증 우회를 막기 위해 거부합니다.
async fn not_implemented() -> Response {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(json!({
            "error": "not_implemented",
            "message": "Authentication and sync are not available in v0.1",
        })),
    )
        .into_response()
}

async fn not_found() -> Response {
    (StatusCode::NOT_FOUND, Json(json!({ "error": "not_found" }))).into_response()
}

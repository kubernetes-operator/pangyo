//! MemoNest API 서버 라우터. 계약 원본: `packages/protocol/openapi.json`
//!
//! - `GET /healthz`, `GET /readyz`, `GET /api/v1/info`
//! - 계정(로드맵 05): 가입/로그인/로그아웃, 내 계정·용량, 장비 등록·목록·해제 — `DATABASE_URL` 필요
//! - 그 밖의 `/api/v1/*` 는 **501** — 동기화가 구현된 것처럼 보이는 가짜 응답을 내지 않습니다.
//! - `WEB_ROOT`가 있으면 `/`에서 웹 앱(apps/web 빌드)도 제공합니다.
//!
//! 동기화, 저장소, 동시 편집 잠금(요구 #28, 승인 전 금지)은 아직 없습니다.

pub mod accounts;
pub mod config;
pub mod error;
pub mod security;

use std::{sync::Arc, time::Duration};

use axum::routing::{delete, post};
use sqlx::{postgres::PgPoolOptions, PgPool};
use tower_http::services::{ServeDir, ServeFile};

pub use config::Config;
use error::{ApiError, ApiResult};
use security::RateLimiter;

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
/// DB 연결 수 상한 (서버 1대 기준)
pub const DB_MAX_CONNECTIONS: u32 = 10;
/// 웹 앱 보안 정책: 같은 출처의 스크립트/연결만 허용, 다른 사이트에 프레임으로 넣기 금지
const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; script-src 'self'; style-src 'self'; \
img-src 'self' data:; connect-src 'self'; manifest-src 'self'; worker-src 'self'; \
frame-ancestors 'none'; base-uri 'self'; form-action 'self'";

/// 핸들러가 공유하는 서버 상태
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: Option<PgPool>,
    pub limiter: Arc<RateLimiter>,
}

impl AppState {
    /// DB 없이 (health/info만) — 로컬 개발과 계약 테스트용
    pub fn without_database(config: Config) -> Self {
        Self {
            config: Arc::new(config),
            db: None,
            limiter: Arc::new(RateLimiter::new(
                config::AUTH_RATE_WINDOW,
                config::AUTH_RATE_LIMIT,
            )),
        }
    }

    /// 설정의 `DATABASE_URL`로 연결하고 마이그레이션(db/postgres/*.sql)을 적용합니다.
    pub async fn connect(config: Config) -> Result<Self, sqlx::Error> {
        let mut state = Self::without_database(config);
        if let Some(url) = state.config.database_url.clone() {
            let pool = PgPoolOptions::new()
                .max_connections(DB_MAX_CONNECTIONS)
                .acquire_timeout(Duration::from_secs(5))
                .connect(&url)
                .await?;
            MIGRATOR.run(&pool).await?;
            state.db = Some(pool);
        }
        Ok(state)
    }

    pub fn db(&self) -> ApiResult<&PgPool> {
        self.db.as_ref().ok_or_else(ApiError::database_unavailable)
    }
}

/// 서버 DB 마이그레이션 (빌드 시 포함). 이미 적용된 파일은 수정 금지(체크섬 검사).
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../db/postgres");

/// 전체 라우터 (미들웨어 포함)
pub fn router(state: AppState) -> Router {
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
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(CONTENT_SECURITY_POLICY),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ));

    let web_root = state.config.web_root.clone();
    let router = Router::new()
        // 오케스트레이터 liveness/readiness
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        // GET 외 메서드는 501 (구현되지 않은 쓰기 경로)
        .route("/api/v1/info", get(info).fallback(not_implemented))
        // 계정 (로드맵 05)
        .route("/api/v1/auth/providers", get(accounts::providers))
        .route("/api/v1/auth/register", post(accounts::register))
        .route("/api/v1/auth/login", post(accounts::login))
        .route("/api/v1/auth/logout", post(accounts::logout))
        .route("/api/v1/account", get(accounts::me))
        .route("/api/v1/account/quota", get(accounts::quota))
        .route("/api/v1/pairing/start", post(accounts::pairing_start))
        .route("/api/v1/pairing/finish", post(accounts::pairing_finish))
        .route("/api/v1/devices", get(accounts::list_devices))
        .route("/api/v1/devices/{id}", delete(accounts::revoke_device))
        .route("/api/v1", any(not_implemented))
        .route("/api/v1/", any(not_implemented))
        .route("/api/v1/{*rest}", any(not_implemented))
        .with_state(state);
    let router = match web_root {
        // 웹 앱 제공: 없는 경로는 index.html (단일 페이지 앱)
        Some(root) => router.fallback_service(
            ServeDir::new(&root).fallback(ServeFile::new(root.join("index.html"))),
        ),
        None => router.fallback(not_found),
    };
    router.layer(middleware)
}

async fn healthz() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

/// DB를 쓰는 경우 연결까지 확인합니다 (끊기면 503 → Kubernetes가 트래픽을 보내지 않음).
async fn readyz(axum::extract::State(state): axum::extract::State<AppState>) -> Response {
    match &state.db {
        None => {
            Json(json!({ "status": "ok", "note": "sync_not_ready", "database": "not_configured" }))
                .into_response()
        }
        Some(db) => match sqlx::query("SELECT 1").execute(db).await {
            Ok(_) => Json(json!({ "status": "ok", "note": "sync_not_ready", "database": "ok" }))
                .into_response(),
            Err(e) => {
                tracing::warn!(error = %e, "readiness: database unreachable");
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(json!({ "status": "unavailable", "note": "sync_not_ready", "database": "error" })),
                )
                    .into_response()
            }
        },
    }
}

async fn info(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Json<serde_json::Value> {
    Json(json!({
        "version": VERSION,
        "authEnabled": state.db.is_some(),
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
            "message": "Sync is not available yet",
        })),
    )
        .into_response()
}

async fn not_found() -> Response {
    (StatusCode::NOT_FOUND, Json(json!({ "error": "not_found" }))).into_response()
}

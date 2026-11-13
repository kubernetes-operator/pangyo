//! MemoNest API 서버 라우터. 계약 원본: `packages/protocol/openapi.json`
//!
//! - `GET /healthz`, `GET /readyz`, `GET /api/v1/info`
//! - 계정(로드맵 05): 가입/로그인/로그아웃, 내 계정·용량, 장비 등록·목록·해제 — `DATABASE_URL` 필요
//! - 그 밖의 `/api/v1/*` 는 **501** — 동기화가 구현된 것처럼 보이는 가짜 응답을 내지 않습니다.
//! - `WEB_ROOT`가 있으면 `/`에서 웹 앱(apps/web 빌드)도 제공합니다.
//!
//! 동기화, 저장소, 동시 편집 잠금(요구 #28, 승인 전 금지)은 아직 없습니다.

pub mod accounts;
pub mod admin;
pub mod config;
pub mod downloads;
pub mod error;
pub mod integrations;
pub mod metrics;
pub mod notes;
pub mod projects;
pub mod search;
pub mod security;
pub mod trash;

use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};

use axum::routing::{delete, post, put};
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

/// DB 연결 재시도 간격 (처음 → 최대)
const DB_RETRY_FIRST: Duration = Duration::from_secs(2);
const DB_RETRY_MAX: Duration = Duration::from_secs(30);

/// 핸들러가 공유하는 서버 상태
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    /// 연결되면 채워짐. DB가 늦게 준비돼도 서버는 먼저 떠서 /healthz에 응답합니다.
    db: Arc<OnceLock<PgPool>>,
    pub limiter: Arc<RateLimiter>,
}

impl AppState {
    /// DB 없이 (health/info만) — 로컬 개발과 계약 테스트용
    pub fn without_database(config: Config) -> Self {
        Self {
            config: Arc::new(config),
            db: Arc::new(OnceLock::new()),
            limiter: Arc::new(RateLimiter::new(
                config::AUTH_RATE_WINDOW,
                config::AUTH_RATE_LIMIT,
            )),
        }
    }

    /// 설정의 `DATABASE_URL`로 지금 연결하고 마이그레이션(db/postgres/*.sql)을 적용합니다 (테스트용).
    pub async fn connect(config: Config) -> Result<Self, sqlx::Error> {
        let state = Self::without_database(config);
        if let Some(url) = state.config.database_url.clone() {
            let _ = state.db.set(open_database(&url).await?);
        }
        Ok(state)
    }

    /// 서버용: DB가 준비될 때까지 백그라운드에서 재시도합니다 (그동안 /readyz = 503 connecting).
    pub fn connect_in_background(config: Config) -> Self {
        let state = Self::without_database(config);
        if let Some(url) = state.config.database_url.clone() {
            let cell = state.db.clone();
            tokio::spawn(async move {
                let mut wait = DB_RETRY_FIRST;
                loop {
                    match open_database(&url).await {
                        Ok(pool) => {
                            let _ = cell.set(pool);
                            tracing::info!("database connected and migrated");
                            return;
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, retry_in = ?wait, "database not ready");
                            tokio::time::sleep(wait).await;
                            wait = (wait * 2).min(DB_RETRY_MAX);
                        }
                    }
                }
            });
        }
        state
    }

    /// `DATABASE_URL`이 설정되어 있는지 (연결 여부와 무관)
    pub fn database_configured(&self) -> bool {
        self.config.database_url.is_some()
    }

    pub fn db(&self) -> ApiResult<&PgPool> {
        match self.db.get() {
            Some(pool) => Ok(pool),
            None if self.database_configured() => Err(ApiError::database_connecting()),
            None => Err(ApiError::database_unavailable()),
        }
    }
}

async fn open_database(url: &str) -> Result<PgPool, sqlx::Error> {
    let pool = PgPoolOptions::new()
        .max_connections(DB_MAX_CONNECTIONS)
        .acquire_timeout(Duration::from_secs(5))
        .connect(url)
        .await?;
    MIGRATOR.run(&pool).await?;
    Ok(pool)
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
        .route(
            "/api/v1/account",
            get(accounts::me).patch(accounts::update_me),
        )
        .route("/api/v1/account/quota", get(accounts::quota))
        .route("/api/v1/pairing/start", post(accounts::pairing_start))
        .route("/api/v1/pairing/finish", post(accounts::pairing_finish))
        .route("/api/v1/devices", get(accounts::list_devices))
        .route("/api/v1/devices/{id}", delete(accounts::revoke_device))
        // 메모 동기화 + 편집 임대 (로드맵 06, ADR-0003)
        .route("/api/v1/notes", get(notes::changes))
        .route(
            "/api/v1/notes/{id}",
            get(notes::get_note)
                .put(notes::put_note)
                .delete(notes::delete_note),
        )
        .route(
            "/api/v1/notes/{id}/lease",
            post(notes::acquire_lease).delete(notes::release_lease),
        )
        // 휴지통 (1주일 보관)
        .route("/api/v1/trash", get(trash::list))
        .route("/api/v1/notes/{id}/restore", post(trash::restore_note))
        .route(
            "/api/v1/projects/{id}/restore",
            post(trash::restore_project),
        )
        // 프로젝트 (ADR-0005)
        .route("/api/v1/projects", get(projects::list))
        .route(
            "/api/v1/projects/{id}",
            put(projects::put).delete(projects::delete),
        )
        // 서버 검색·연관 메모, 자동화 토큰 (로드맵 07)
        .route("/api/v1/search", get(search::search))
        .route("/api/v1/notes/{id}/similar", get(search::similar))
        .route(
            "/api/v1/integrations/tokens",
            get(integrations::list_tokens).post(integrations::create_token),
        )
        .route(
            "/api/v1/integrations/tokens/{id}",
            delete(integrations::revoke_token),
        )
        // 관리자: 기본 저장공간·계정별 저장공간 (ADMIN_EMAILS)
        .route(
            "/api/v1/admin/settings",
            get(admin::get_settings).put(admin::put_settings),
        )
        .route("/api/v1/admin/accounts", get(admin::list_accounts))
        .route(
            "/api/v1/admin/accounts/{id}/quota",
            put(admin::put_account_quota),
        )
        // 설치 파일 목록·링크 발급
        .route("/api/v1/downloads", get(downloads::list))
        .route("/api/v1/downloads/{id}/link", post(downloads::create_link))
        .route("/api/v1", any(not_implemented))
        .route("/api/v1/", any(not_implemented))
        .route("/api/v1/{*rest}", any(not_implemented))
        // 여기까지(API)만 응답 시간 15초 제한. 아래 큰 파일 다운로드는 제한 없음
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            REQUEST_TIMEOUT,
        ))
        .route("/dl/{token}", get(downloads::download))
        // 지표: 일치한 경로 템플릿을 응답에 기록 (메모 ID·토큰 대신 `{id}`·`{token}`)
        .route_layer(axum::middleware::from_fn(metrics::label_route))
        .with_state(state);
    let router = match web_root {
        // 웹 앱 제공: 없는 경로는 index.html (단일 페이지 앱)
        Some(root) => router.fallback_service(
            ServeDir::new(&root).fallback(ServeFile::new(root.join("index.html"))),
        ),
        None => router.fallback(not_found),
    };
    // 가장 바깥: 모든 요청의 횟수·응답 시간 (지표 서버는 별도 포트, metrics::router)
    router
        .layer(middleware)
        .layer(axum::middleware::from_fn(metrics::record))
}

async fn healthz() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

/// DB를 쓰는 경우 연결까지 확인합니다 (끊기면 503 → Kubernetes가 트래픽을 보내지 않음).
async fn readyz(axum::extract::State(state): axum::extract::State<AppState>) -> Response {
    match state.db.get() {
        None if !state.database_configured() => {
            Json(json!({ "status": "ok", "note": "local_only", "database": "not_configured" }))
                .into_response()
        }
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "status": "unavailable", "note": "sync_unavailable", "database": "connecting" })),
        )
            .into_response(),
        Some(db) => match sqlx::query("SELECT 1").execute(db).await {
            Ok(_) => Json(json!({ "status": "ok", "note": "sync_ready", "database": "ok" }))
                .into_response(),
            Err(e) => {
                tracing::warn!(error = %e, "readiness: database unreachable");
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(json!({ "status": "unavailable", "note": "sync_unavailable", "database": "error" })),
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
        "authEnabled": state.database_configured(),
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

//! 운영 지표 (로드맵 10, Prometheus 텍스트 형식). 외부 공개 포트가 아닌 **별도 내부 포트**의 `/metrics`로만 제공합니다.
//!
//! - HTTP: 요청 수(경로 템플릿·메서드·상태), 응답 시간 히스토그램 — 경로는 `/api/v1/notes/{id}` 같은 템플릿만 써서
//!   메모 ID·토큰이 라벨에 들어가지 않게 합니다 (카디널리티·개인정보).
//! - DB: 연결 여부, 계정 수/상한, 활성 장비, 메모 수, 사용 용량, 편집 임대 수 — 수집할 때마다 짧게 조회.
//! - 개인 정보(이메일, 제목, 본문)는 지표에 넣지 않습니다.

use std::{
    collections::BTreeMap,
    fmt::Write,
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};

use axum::{
    body::Body,
    extract::{MatchedPath, State},
    http::{header, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};

use crate::{AppState, VERSION};

// === 0. 지표 설정 변수 ===
/// 지표 포트 환경변수. 없으면 지표 서버를 열지 않음 (로컬 개발). Kubernetes: `0.0.0.0:9464`
pub const METRICS_BIND_ENV: &str = "METRICS_BIND_ADDRESS";
/// 응답 시간 구간(초)
const BUCKETS: [f64; 11] = [
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];
/// 지표 수집 중 DB 조회 제한 (Prometheus 수집 제한 시간보다 짧게)
const DB_QUERY_TIMEOUT: Duration = Duration::from_secs(3);
/// 경로 템플릿이 없는 요청(웹 앱 파일, 없는 경로)의 라벨
const UNMATCHED_ROUTE: &str = "other";

#[derive(Default)]
struct Histogram {
    buckets: [u64; BUCKETS.len()],
    sum: f64,
    count: u64,
}

#[derive(Default)]
struct HttpMetrics {
    /// (method, route, status) → 횟수
    requests: BTreeMap<(String, String, u16), u64>,
    /// (method, route) → 응답 시간
    durations: BTreeMap<(String, String), Histogram>,
}

static HTTP: LazyLock<Mutex<HttpMetrics>> = LazyLock::new(Default::default);

/// 라우팅 안쪽에서 경로 템플릿을 응답에 실어 바깥 측정 계층에 전달
#[derive(Clone)]
struct RouteLabel(String);

/// `Router::route_layer`용: 일치한 경로 템플릿 기록
pub async fn label_route(req: Request<Body>, next: Next) -> Response {
    let route = req
        .extensions()
        .get::<MatchedPath>()
        .map(|p| p.as_str().to_owned());
    let mut response = next.run(req).await;
    if let Some(route) = route {
        response.extensions_mut().insert(RouteLabel(route));
    }
    response
}

/// 가장 바깥 계층: 모든 요청의 횟수·응답 시간 기록
pub async fn record(req: Request<Body>, next: Next) -> Response {
    let method = req.method().as_str().to_owned();
    let started = Instant::now();
    let response = next.run(req).await;
    let elapsed = started.elapsed().as_secs_f64();
    let route = response
        .extensions()
        .get::<RouteLabel>()
        .map_or_else(|| UNMATCHED_ROUTE.to_owned(), |r| r.0.clone());
    let status = response.status().as_u16();
    if let Ok(mut m) = HTTP.lock() {
        *m.requests
            .entry((method.clone(), route.clone(), status))
            .or_default() += 1;
        let h = m.durations.entry((method, route)).or_default();
        for (i, le) in BUCKETS.iter().enumerate() {
            if elapsed <= *le {
                h.buckets[i] += 1;
            }
        }
        h.sum += elapsed;
        h.count += 1;
    }
    response
}

/// 내부 지표 서버 라우터 (`GET /metrics`)
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/metrics", get(metrics))
        .with_state(state)
}

async fn metrics(State(state): State<AppState>) -> Response {
    let mut out = String::with_capacity(8 * 1024);
    let _ = render(&state, &mut out).await;
    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        out,
    )
        .into_response()
}

fn escape(v: &str) -> String {
    v.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

async fn render(state: &AppState, out: &mut String) -> std::fmt::Result {
    writeln!(out, "# HELP memonest_build_info 서버 버전")?;
    writeln!(out, "# TYPE memonest_build_info gauge")?;
    writeln!(
        out,
        "memonest_build_info{{version=\"{}\"}} 1",
        escape(VERSION)
    )?;

    {
        let m = HTTP.lock().map_err(|_| std::fmt::Error)?;
        writeln!(
            out,
            "# HELP memonest_http_requests_total HTTP 요청 수 (경로 템플릿 기준)"
        )?;
        writeln!(out, "# TYPE memonest_http_requests_total counter")?;
        for ((method, route, status), n) in &m.requests {
            writeln!(
                out,
                "memonest_http_requests_total{{method=\"{}\",route=\"{}\",status=\"{status}\"}} {n}",
                escape(method),
                escape(route)
            )?;
        }
        writeln!(
            out,
            "# HELP memonest_http_request_duration_seconds HTTP 응답 시간"
        )?;
        writeln!(
            out,
            "# TYPE memonest_http_request_duration_seconds histogram"
        )?;
        for ((method, route), h) in &m.durations {
            let labels = format!("method=\"{}\",route=\"{}\"", escape(method), escape(route));
            for (i, le) in BUCKETS.iter().enumerate() {
                writeln!(
                    out,
                    "memonest_http_request_duration_seconds_bucket{{{labels},le=\"{le}\"}} {}",
                    h.buckets[i]
                )?;
            }
            writeln!(
                out,
                "memonest_http_request_duration_seconds_bucket{{{labels},le=\"+Inf\"}} {}",
                h.count
            )?;
            writeln!(
                out,
                "memonest_http_request_duration_seconds_sum{{{labels}}} {}",
                h.sum
            )?;
            writeln!(
                out,
                "memonest_http_request_duration_seconds_count{{{labels}}} {}",
                h.count
            )?;
        }
    }

    let configured = state.database_configured();
    writeln!(out, "# HELP memonest_db_configured DATABASE_URL 설정 여부")?;
    writeln!(out, "# TYPE memonest_db_configured gauge")?;
    writeln!(out, "memonest_db_configured {}", u8::from(configured))?;
    let stats = match state.db.get() {
        Some(db) => tokio::time::timeout(DB_QUERY_TIMEOUT, db_stats(db))
            .await
            .ok()
            .and_then(Result::ok),
        None => None,
    };
    writeln!(
        out,
        "# HELP memonest_db_up DB 조회 성공 여부 (설정됐는데 0이면 장애)"
    )?;
    writeln!(out, "# TYPE memonest_db_up gauge")?;
    writeln!(out, "memonest_db_up {}", u8::from(stats.is_some()))?;
    if let Some(s) = stats {
        let gauges: [(&str, &str, i64); 8] = [
            ("memonest_accounts", "가입 계정 수", s.accounts),
            (
                "memonest_accounts_max",
                "가입 상한 (MAX_ACCOUNTS)",
                state.config.max_accounts,
            ),
            (
                "memonest_devices_active",
                "폐기되지 않은 등록 장비 수",
                s.devices,
            ),
            ("memonest_notes_live", "삭제되지 않은 메모 수", s.notes_live),
            (
                "memonest_notes_deleted",
                "묘비(삭제) 메모 수",
                s.notes_deleted,
            ),
            (
                "memonest_storage_used_bytes",
                "전체 계정 사용 용량",
                s.used_bytes,
            ),
            (
                "memonest_storage_quota_bytes",
                "전체 계정 용량 한도 합",
                s.quota_bytes,
            ),
            (
                "memonest_note_leases_active",
                "유효한 편집 임대 수",
                s.leases,
            ),
        ];
        for (name, help, value) in gauges {
            writeln!(out, "# HELP {name} {help}")?;
            writeln!(out, "# TYPE {name} gauge")?;
            writeln!(out, "{name} {value}")?;
        }
    }
    Ok(())
}

struct DbStats {
    accounts: i64,
    devices: i64,
    notes_live: i64,
    notes_deleted: i64,
    used_bytes: i64,
    quota_bytes: i64,
    leases: i64,
}

async fn db_stats(db: &sqlx::PgPool) -> Result<DbStats, sqlx::Error> {
    let row: (i64, i64, i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
           (SELECT count(*) FROM accounts),
           (SELECT count(*) FROM devices WHERE revoked_at IS NULL),
           (SELECT count(*) FROM notes WHERE deleted_at IS NULL),
           (SELECT count(*) FROM notes WHERE deleted_at IS NOT NULL),
           (SELECT coalesce(sum(used_bytes), 0)::bigint FROM accounts),
           (SELECT coalesce(sum(quota_bytes), 0)::bigint FROM accounts),
           (SELECT count(*) FROM note_leases WHERE expires_at > NOW())",
    )
    .fetch_one(db)
    .await?;
    Ok(DbStats {
        accounts: row.0,
        devices: row.1,
        notes_live: row.2,
        notes_deleted: row.3,
        used_bytes: row.4,
        quota_bytes: row.5,
        leases: row.6,
    })
}

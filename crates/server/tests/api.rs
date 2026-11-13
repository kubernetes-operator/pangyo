//! API 회귀 + 계약 테스트 (T-07, T-08).
//! 계약 원본: packages/protocol/openapi.json — 문서에 적힌 모든 경로/메서드를 실제로 호출해 봅니다.
use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

const OPENAPI: &str = include_str!("../../../packages/protocol/openapi.json");
const SAMPLE_ID: &str = "00000000-0000-4000-8000-000000000000";

/// DB 없는 서버 (health/info만 동작, 계정 API는 503)
fn app() -> axum::Router {
    memonest_server::router(memonest_server::AppState::without_database(
        memonest_server::Config::default(),
    ))
}

struct Reply {
    status: StatusCode,
    headers: axum::http::HeaderMap,
    json: Value,
}

async fn call(method: Method, uri: &str, body: &str) -> Reply {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-memonest-request", "1")
        .body(Body::from(body.to_owned()))
        .unwrap();
    let response = app().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    Reply {
        status,
        headers,
        json,
    }
}

#[tokio::test]
async fn t07_healthz_ok_with_security_headers() {
    let r = call(Method::GET, "/healthz", "").await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.json["status"], "ok");
    assert_eq!(r.headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    assert_eq!(r.headers[header::CACHE_CONTROL], "no-store");
    assert!(r.headers[header::CONTENT_TYPE]
        .to_str()
        .unwrap()
        .starts_with("application/json"));
    assert!(
        r.headers.contains_key("x-request-id"),
        "요청 ID가 응답에 있어야 함"
    );
}

#[tokio::test]
async fn t08_sync_is_not_available() {
    let r = call(Method::POST, "/api/v1/sync/commit", "{}").await;
    assert_eq!(r.status, StatusCode::NOT_IMPLEMENTED);
    assert_eq!(r.json["error"], "not_implemented");
}

#[tokio::test]
async fn info_reports_disabled_features_and_rejects_writes() {
    let r = call(Method::GET, "/api/v1/info", "").await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.json["version"], memonest_server::VERSION);
    assert_eq!(r.json["authEnabled"], false);
    assert_eq!(r.json["syncEnabled"], false);
    // Go 서버와 같이: info에 GET 외 메서드는 501
    let r = call(Method::POST, "/api/v1/info", "{}").await;
    assert_eq!(r.status, StatusCode::NOT_IMPLEMENTED);
}

#[tokio::test]
async fn unknown_api_paths_are_501_and_others_404() {
    for uri in ["/api/v1", "/api/v1/", "/api/v1/anything/deep"] {
        assert_eq!(
            call(Method::DELETE, uri, "").await.status,
            StatusCode::NOT_IMPLEMENTED,
            "{uri}"
        );
    }
    let r = call(Method::GET, "/nope", "").await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    assert_eq!(r.headers[header::CACHE_CONTROL], "no-store");
    assert_eq!(
        call(Method::POST, "/healthz", "").await.status,
        StatusCode::METHOD_NOT_ALLOWED
    );
}

#[tokio::test]
async fn oversized_body_is_rejected() {
    let big = "x".repeat(memonest_server::BODY_LIMIT_BYTES + 1);
    let request = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/sync/commit")
        .header(header::CONTENT_LENGTH, big.len())
        .body(Body::from(big))
        .unwrap();
    let response = app().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

/// openapi.json의 모든 operation (DB 없는 서버 기준):
/// implemented → 200 + required 필드 / implemented + x-requires-database → 503 / planned → 501
/// DB가 있을 때의 동작은 tests/accounts.rs 가 검사합니다.
#[tokio::test]
async fn every_openapi_operation_matches_server() {
    let spec: Value = serde_json::from_str(OPENAPI).expect("openapi.json 형식 오류");
    let schemas = &spec["components"]["schemas"];
    let mut checked = 0;
    let mut failures = Vec::new();
    for (path, item) in spec["paths"].as_object().unwrap() {
        for (method, op) in item.as_object().unwrap() {
            if method == "parameters" {
                continue;
            }
            let http_method: Method = method.to_uppercase().parse().unwrap();
            let uri = path.replace("{id}", SAMPLE_ID);
            let body = sample_body(&spec, op);
            let r = call(http_method.clone(), &uri, &body).await;
            let status = op["x-memonest-status"].as_str().unwrap_or("?");
            let needs_db = op["x-requires-database"].as_bool().unwrap_or(false);
            let (expected_code, expected_error, schema_ref) = match (status, needs_db) {
                ("implemented", false) => (
                    StatusCode::OK,
                    None,
                    op["responses"]["200"]["content"]["application/json"]["schema"]["$ref"]
                        .as_str(),
                ),
                ("implemented", true) => (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Some("database_not_configured"),
                    None,
                ),
                ("planned", _) => (StatusCode::NOT_IMPLEMENTED, Some("not_implemented"), None),
                (other, _) => {
                    failures.push(format!(
                        "{method} {path}: 알 수 없는 x-memonest-status {other}"
                    ));
                    continue;
                }
            };
            if r.status != expected_code {
                failures.push(format!(
                    "{method} {path}: {} (기대 {expected_code})",
                    r.status
                ));
                continue;
            }
            if let Some(code) = expected_error {
                if r.json["error"] != code {
                    failures.push(format!(
                        "{method} {path}: error={} (기대 {code})",
                        r.json["error"]
                    ));
                }
            }
            if let Some(schema_ref) = schema_ref {
                let name = schema_ref.rsplit('/').next().unwrap();
                for field in schemas[name]["required"].as_array().unwrap() {
                    if r.json.get(field.as_str().unwrap()).is_none() {
                        failures.push(format!("{method} {path}: 응답에 {field} 없음"));
                    }
                }
            }
            checked += 1;
        }
    }
    assert!(failures.is_empty(), "계약 불일치:\n{}", failures.join("\n"));
    assert_eq!(
        checked, 36,
        "openapi.json operation 수가 바뀌면 이 값과 docs/05_SYNC_API.md를 함께 갱신"
    );
}

/// requestBody 스키마의 필수 필드로 형식이 맞는 예시 본문을 만듭니다.
fn sample_body(spec: &Value, op: &Value) -> String {
    let Some(schema_ref) =
        op["requestBody"]["content"]["application/json"]["schema"]["$ref"].as_str()
    else {
        return "{}".to_owned();
    };
    let name = schema_ref.rsplit('/').next().unwrap();
    let schema = &spec["components"]["schemas"][name];
    let mut body = serde_json::Map::new();
    for field in schema["required"].as_array().unwrap() {
        let field = field.as_str().unwrap();
        let value = if field == "email" {
            "contract@example.com"
        } else {
            "contract-sample-value"
        };
        body.insert(field.to_owned(), Value::from(value));
    }
    Value::Object(body).to_string()
}

#[tokio::test]
async fn malformed_body_is_json_400() {
    let r = call(Method::POST, "/api/v1/auth/login", "{not json").await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert_eq!(r.json["error"], "invalid_body");
}

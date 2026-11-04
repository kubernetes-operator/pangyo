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
        .body(Body::from(body.to_owned()))
        .unwrap();
    let response = memonest_server::router().oneshot(request).await.unwrap();
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
    let response = memonest_server::router().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

/// openapi.json의 모든 operation: implemented → 200 + required 필드, planned → 501
#[tokio::test]
async fn every_openapi_operation_matches_server() {
    let spec: Value = serde_json::from_str(OPENAPI).expect("openapi.json 형식 오류");
    let schemas = &spec["components"]["schemas"];
    let mut checked = 0;
    let mut failures = Vec::new();
    for (path, item) in spec["paths"].as_object().unwrap() {
        for (method, op) in item.as_object().unwrap() {
            let Ok(http_method) = method.to_uppercase().parse::<Method>() else {
                continue; // "parameters" 등
            };
            if http_method == Method::from_bytes(b"PARAMETERS").unwrap() {
                continue;
            }
            let uri = path.replace("{id}", SAMPLE_ID);
            let r = call(http_method.clone(), &uri, "{}").await;
            let status = op["x-memonest-status"].as_str().unwrap_or("?");
            let (expected_code, schema_ref) = match status {
                "implemented" => (
                    StatusCode::OK,
                    &op["responses"]["200"]["content"]["application/json"]["schema"]["$ref"],
                ),
                "planned" => (
                    StatusCode::NOT_IMPLEMENTED,
                    &op["responses"]["501"]["content"]["application/json"]["schema"]["$ref"],
                ),
                other => {
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
            let name = schema_ref.as_str().unwrap().rsplit('/').next().unwrap();
            for field in schemas[name]["required"].as_array().unwrap() {
                if r.json.get(field.as_str().unwrap()).is_none() {
                    failures.push(format!("{method} {path}: 응답에 {field} 없음"));
                }
            }
            checked += 1;
        }
    }
    assert!(failures.is_empty(), "계약 불일치:\n{}", failures.join("\n"));
    assert_eq!(checked, 12, "openapi.json operation 수(구현 3 + 계획 9)가 바뀌면 이 값과 docs/05_SYNC_API.md를 함께 갱신");
}

//! 통합 테스트 공통: 테스트마다 새 Postgres DB를 만들고 서버 라우터로 요청합니다.
#![allow(dead_code)]
use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
    Router,
};
use http_body_util::BodyExt;
use memonest_server::{AppState, Config};
use serde_json::{json, Value};
use sqlx::{Connection, PgConnection};
use tower::ServiceExt;

pub struct TestApp {
    pub router: Router,
}

pub struct Reply {
    pub status: StatusCode,
    pub json: Value,
    pub cookie: Option<String>,
    pub set_cookie: Option<String>,
}

/// 새 테스트 DB를 만들고 마이그레이션을 적용한 서버. DB 설정이 없으면 None.
pub async fn app_with(adjust: impl FnOnce(&mut Config)) -> Option<TestApp> {
    let Ok(admin_url) = std::env::var("TEST_DATABASE_URL") else {
        eprintln!("SKIP: TEST_DATABASE_URL 없음 (make db-test-up)");
        return None;
    };
    let name = format!("memonest_test_{}", uuid::Uuid::new_v4().simple());
    let mut admin = PgConnection::connect(&admin_url)
        .await
        .expect("테스트 DB 접속");
    sqlx::query(&format!("CREATE DATABASE {name}"))
        .execute(&mut admin)
        .await
        .unwrap();
    let base = admin_url
        .rsplit_once('/')
        .map_or(admin_url.as_str(), |(b, _)| b);
    let mut config = Config {
        database_url: Some(format!("{base}/{name}")),
        cookie_secure: true,
        ..Config::default()
    };
    adjust(&mut config);
    let state = AppState::connect(config).await.expect("마이그레이션");
    Some(TestApp {
        router: memonest_server::router(state),
    })
}

pub async fn app() -> Option<TestApp> {
    app_with(|_| {}).await
}

impl TestApp {
    pub async fn call(
        &self,
        method: Method,
        uri: &str,
        body: Option<Value>,
        auth: Auth<'_>,
    ) -> Reply {
        let mut req = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json");
        match auth {
            Auth::None => {}
            Auth::Cookie(c) | Auth::NoCsrfCookie(c) => {
                req = req.header(header::COOKIE, format!("memonest_session={c}"))
            }
            Auth::Bearer(t) => req = req.header(header::AUTHORIZATION, format!("Bearer {t}")),
        }
        if !matches!(auth, Auth::NoCsrfCookie(_)) {
            req = req.header("x-memonest-request", "1");
        }
        let body = body.map_or_else(Body::empty, |b| Body::from(b.to_string()));
        let res = self
            .router
            .clone()
            .oneshot(req.body(body).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let set_cookie = res
            .headers()
            .get(header::SET_COOKIE)
            .map(|v| v.to_str().unwrap().to_owned());
        let cookie = set_cookie.as_deref().and_then(|v| {
            v.strip_prefix("memonest_session=")
                .and_then(|rest| rest.split(';').next())
                .filter(|t| !t.is_empty())
                .map(str::to_owned)
        });
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        Reply {
            status,
            json: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
            cookie,
            set_cookie,
        }
    }

    pub async fn register(&self, email: &str) -> Reply {
        self.call(
            Method::POST,
            "/api/v1/auth/register",
            Some(json!({ "email": email, "password": "correct horse battery", "displayName": "테스터" })),
            Auth::None,
        )
        .await
    }
}

#[derive(Clone, Copy)]
pub enum Auth<'a> {
    None,
    Cookie(&'a str),
    NoCsrfCookie(&'a str),
    Bearer(&'a str),
}

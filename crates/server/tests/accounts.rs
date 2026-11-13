//! 계정 API 통합 테스트 (실제 Postgres). `TEST_DATABASE_URL`이 없으면 건너뜁니다.
//! 로컬: `make db-test-up` (Docker, 테스트 전용 trust 인증) / CI: postgres 서비스 컨테이너.
//! 테스트마다 새 데이터베이스를 만들어 서로 간섭하지 않습니다.
mod common;

use axum::http::{Method, StatusCode};
use common::{app, app_with, Auth, TestApp};
use memonest_server::{config::Registration, AppState, Config};
use serde_json::json;

#[tokio::test]
async fn register_sets_secure_session_and_reads_account_and_quota() {
    let Some(app) = app().await else { return };
    let r = app.register("New@Example.com").await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.json);
    assert_eq!(r.json["email"], "new@example.com");
    assert_eq!(r.json["displayName"], "테스터");
    assert_eq!(r.json["tier"], "free");
    assert_eq!(r.json["quotaBytes"], 100 * 1024 * 1024);
    let set_cookie = r.set_cookie.unwrap();
    for attr in ["HttpOnly", "Secure", "SameSite=Strict", "Path=/"] {
        assert!(set_cookie.contains(attr), "{attr} 없음: {set_cookie}");
    }
    let cookie = r.cookie.unwrap();
    let me = app
        .call(Method::GET, "/api/v1/account", None, Auth::Cookie(&cookie))
        .await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.json["email"], "new@example.com");
    let quota = app
        .call(
            Method::GET,
            "/api/v1/account/quota",
            None,
            Auth::Cookie(&cookie),
        )
        .await;
    assert_eq!(
        quota.json,
        json!({ "tier": "free", "quotaBytes": 104857600, "usedBytes": 0 })
    );
}

#[tokio::test]
async fn duplicate_email_and_input_validation() {
    let Some(app) = app().await else { return };
    assert_eq!(
        app.register("dup@example.com").await.status,
        StatusCode::CREATED
    );
    let r = app.register("DUP@example.com").await;
    assert_eq!(
        (r.status, r.json["error"].as_str()),
        (StatusCode::CONFLICT, Some("email_taken"))
    );
    let weak = app
        .call(
            Method::POST,
            "/api/v1/auth/register",
            Some(json!({ "email": "w@example.com", "password": "short" })),
            Auth::None,
        )
        .await;
    assert_eq!(weak.json["error"], "weak_password");
    let bad = app
        .call(
            Method::POST,
            "/api/v1/auth/register",
            Some(json!({ "email": "not-an-email", "password": "long enough password" })),
            Auth::None,
        )
        .await;
    assert_eq!(bad.json["error"], "invalid_email");
}

#[tokio::test]
async fn login_logout_and_wrong_credentials() {
    let Some(app) = app().await else { return };
    app.register("login@example.com").await;
    let login = |password: &'static str, email: &'static str| {
        app.call(
            Method::POST,
            "/api/v1/auth/login",
            Some(json!({ "email": email, "password": password })),
            Auth::None,
        )
    };
    let wrong = login("wrong password!!", "login@example.com").await;
    let unknown = login("correct horse battery", "nobody@example.com").await;
    for r in [&wrong, &unknown] {
        // 없는 계정과 틀린 비밀번호를 구별할 수 없게 같은 응답
        assert_eq!(r.status, StatusCode::UNAUTHORIZED);
        assert_eq!(r.json["error"], "invalid_credentials");
    }
    let ok = login("correct horse battery", "LOGIN@example.com").await;
    assert_eq!(ok.status, StatusCode::OK);
    let cookie = ok.cookie.unwrap();

    // 쿠키 인증의 상태 변경 요청은 CSRF 헤더 필수
    let no_csrf = app
        .call(
            Method::POST,
            "/api/v1/auth/logout",
            None,
            Auth::NoCsrfCookie(&cookie),
        )
        .await;
    assert_eq!(
        (no_csrf.status, no_csrf.json["error"].as_str()),
        (StatusCode::FORBIDDEN, Some("csrf"))
    );

    let out = app
        .call(
            Method::POST,
            "/api/v1/auth/logout",
            None,
            Auth::Cookie(&cookie),
        )
        .await;
    assert_eq!(out.status, StatusCode::NO_CONTENT);
    let after = app
        .call(Method::GET, "/api/v1/account", None, Auth::Cookie(&cookie))
        .await;
    assert_eq!(after.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn account_limit_holds_under_concurrent_registration() {
    let Some(app) = app_with(|c| c.max_accounts = 3).await else {
        return;
    };
    let router = std::sync::Arc::new(app);
    let mut tasks = Vec::new();
    for i in 0..8 {
        let app = router.clone();
        tasks.push(tokio::spawn(async move {
            app.register(&format!("u{i}@example.com")).await.status
        }));
    }
    let mut created = 0;
    let mut limited = 0;
    for t in tasks {
        match t.await.unwrap() {
            StatusCode::CREATED => created += 1,
            StatusCode::FORBIDDEN => limited += 1,
            other => panic!("예상 밖 응답 {other}"),
        }
    }
    assert_eq!(
        (created, limited),
        (3, 5),
        "동시 가입에서도 상한(3) 정확히 유지"
    );
}

#[tokio::test]
async fn closed_registration() {
    let Some(app) = app_with(|c| c.registration = Registration::Closed).await else {
        return;
    };
    let r = app.register("closed@example.com").await;
    assert_eq!(
        (r.status, r.json["error"].as_str()),
        (StatusCode::FORBIDDEN, Some("registration_closed"))
    );
    let p = app
        .call(Method::GET, "/api/v1/auth/providers", None, Auth::None)
        .await;
    assert_eq!(
        p.json,
        json!({ "local": true, "google": false, "registration": "closed" })
    );
}

#[tokio::test]
async fn device_pairing_bearer_token_and_revocation() {
    let Some(app) = app().await else { return };
    let cookie = app.register("pair@example.com").await.cookie.unwrap();
    let start = app
        .call(
            Method::POST,
            "/api/v1/pairing/start",
            None,
            Auth::Cookie(&cookie),
        )
        .await;
    assert_eq!(start.status, StatusCode::CREATED);
    let code = start.json["code"].as_str().unwrap().to_owned();
    assert_eq!(code.len(), 8);

    let finish = |code: String| {
        app.call(
            Method::POST,
            "/api/v1/pairing/finish",
            Some(json!({ "code": code.to_lowercase(), "deviceName": "내 노트북", "platform": "Linux" })),
            Auth::None,
        )
    };
    let paired = finish(code.clone()).await;
    assert_eq!(paired.status, StatusCode::CREATED, "{}", paired.json);
    let token = paired.json["token"].as_str().unwrap().to_owned();
    let device_id = paired.json["deviceId"].as_str().unwrap().to_owned();
    // 코드는 한 번만
    assert_eq!(finish(code).await.json["error"], "invalid_code");

    let me = app
        .call(Method::GET, "/api/v1/account", None, Auth::Bearer(&token))
        .await;
    assert_eq!(me.json["email"], "pair@example.com");
    // 장비 토큰으로 다른 장비를 등록할 수 없음
    let nested = app
        .call(
            Method::POST,
            "/api/v1/pairing/start",
            None,
            Auth::Bearer(&token),
        )
        .await;
    assert_eq!(nested.json["error"], "session_required");

    let list = app
        .call(Method::GET, "/api/v1/devices", None, Auth::Cookie(&cookie))
        .await;
    assert_eq!(list.json[0]["name"], "내 노트북");
    assert_eq!(list.json[0]["platform"], "linux");
    assert!(list.json[0]["lastSeenAt"].is_string());

    let uri = format!("/api/v1/devices/{device_id}");
    // 연결된 장비는 기록을 지울 수 없음 (먼저 폐기)
    let early = app
        .call(
            Method::DELETE,
            &format!("{uri}/record"),
            None,
            Auth::Cookie(&cookie),
        )
        .await;
    assert_eq!(early.json["error"], "device_active");
    assert_eq!(
        app.call(Method::DELETE, &uri, None, Auth::Cookie(&cookie))
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    let revoked = app
        .call(Method::GET, "/api/v1/account", None, Auth::Bearer(&token))
        .await;
    assert_eq!(revoked.status, StatusCode::UNAUTHORIZED);
    // 폐기한 장비의 기록 지우기 → 목록에서 사라짐
    let forget = app
        .call(
            Method::DELETE,
            &format!("{uri}/record"),
            None,
            Auth::Cookie(&cookie),
        )
        .await;
    assert_eq!(forget.status, StatusCode::NO_CONTENT);
    let list = app
        .call(Method::GET, "/api/v1/devices", None, Auth::Cookie(&cookie))
        .await;
    assert!(list
        .json
        .as_array()
        .unwrap()
        .iter()
        .all(|d| d["id"] != device_id.as_str()));
    // 다른 사람 장비는 해제 불가
    let other = app.register("other@example.com").await.cookie.unwrap();
    assert_eq!(
        app.call(Method::DELETE, &uri, None, Auth::Cookie(&other))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn login_rate_limit() {
    let Some(app) = app().await else { return };
    let mut last = StatusCode::OK;
    for _ in 0..=memonest_server::config::AUTH_RATE_LIMIT {
        last = app
            .call(
                Method::POST,
                "/api/v1/auth/login",
                Some(json!({ "email": "x@example.com", "password": "whatever-password" })),
                Auth::None,
            )
            .await
            .status;
    }
    assert_eq!(last, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn readiness_and_info_report_database() {
    let Some(app) = app().await else { return };
    let ready = app.call(Method::GET, "/readyz", None, Auth::None).await;
    assert_eq!(
        (ready.status, ready.json["database"].as_str()),
        (StatusCode::OK, Some("ok"))
    );
    assert_eq!(ready.json["note"], "sync_ready");
    let info = app
        .call(Method::GET, "/api/v1/info", None, Auth::None)
        .await;
    assert_eq!(info.json["authEnabled"], true);
}

#[tokio::test]
async fn server_starts_before_database_and_connects_later() {
    // 아무도 듣지 않는 포트 → 연결 재시도 중에도 health는 정상, readyz/계정 API는 503
    let config = Config {
        database_url: Some("postgres://postgres@127.0.0.1:9/none".into()),
        ..Config::default()
    };
    let state = AppState::connect_in_background(config);
    let app = TestApp {
        router: memonest_server::router(state.clone()),
        state,
    };
    assert_eq!(
        app.call(Method::GET, "/healthz", None, Auth::None)
            .await
            .status,
        StatusCode::OK
    );
    let ready = app.call(Method::GET, "/readyz", None, Auth::None).await;
    assert_eq!(
        (ready.status, ready.json["database"].as_str()),
        (StatusCode::SERVICE_UNAVAILABLE, Some("connecting"))
    );
    assert_eq!(ready.json["note"], "sync_unavailable");
    let r = app.register("early@example.com").await;
    assert_eq!(
        (r.status, r.json["error"].as_str()),
        (StatusCode::SERVICE_UNAVAILABLE, Some("database_connecting"))
    );
}

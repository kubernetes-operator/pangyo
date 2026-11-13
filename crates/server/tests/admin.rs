//! 관리자 저장공간 설정 (2026-11-13 사용 의견 6번). 실제 Postgres 필요.
mod common;

use axum::http::{Method, StatusCode};
use common::{app_with, Auth};
use serde_json::json;

const MB: i64 = 1024 * 1024;

#[tokio::test]
async fn admin_changes_default_and_per_account_quota() {
    let Some(app) = app_with(|c| {
        c.admin_emails = memonest_server::config::parse_admin_emails("Boss@Example.com")
    })
    .await
    else {
        return;
    };
    let admin = app.register("boss@example.com").await;
    assert_eq!(admin.json["isAdmin"], true, "{}", admin.json);
    let admin = admin.cookie.unwrap();
    let user = app.register("user@example.com").await;
    assert_eq!(user.json["isAdmin"], false);
    assert_eq!(user.json["quotaBytes"], 100 * MB);
    let user_id = user.json["id"].as_str().unwrap().to_owned();
    let user = user.cookie.unwrap();

    // 관리자가 아니면 모두 403
    for (method, uri, body) in [
        (Method::GET, "/api/v1/admin/settings".to_owned(), None),
        (
            Method::PUT,
            "/api/v1/admin/settings".to_owned(),
            Some(json!({ "defaultQuotaMb": 999 })),
        ),
        (Method::GET, "/api/v1/admin/accounts".to_owned(), None),
        (
            Method::PUT,
            format!("/api/v1/admin/accounts/{user_id}/quota"),
            Some(json!({ "quotaMb": 999 })),
        ),
    ] {
        let r = app
            .call(method.clone(), &uri, body, Auth::Cookie(&user))
            .await;
        assert_eq!(r.status, StatusCode::FORBIDDEN, "{method} {uri}");
        assert_eq!(r.json["error"], "admin_only");
    }

    let s = app
        .call(
            Method::GET,
            "/api/v1/admin/settings",
            None,
            Auth::Cookie(&admin),
        )
        .await;
    assert_eq!(s.status, StatusCode::OK, "{}", s.json);
    assert_eq!(s.json["defaultQuotaMb"], 100);
    assert_eq!(s.json["envDefaultQuotaMb"], 100);

    // 기본값 변경 → 따로 정하지 않은 모든 계정에 바로 적용
    let s = app
        .call(
            Method::PUT,
            "/api/v1/admin/settings",
            Some(json!({ "defaultQuotaMb": 250 })),
            Auth::Cookie(&admin),
        )
        .await;
    assert_eq!(s.json["defaultQuotaMb"], 250);
    assert_eq!(s.json["updatedAccounts"], 2);
    let me = app
        .call(Method::GET, "/api/v1/account", None, Auth::Cookie(&user))
        .await;
    assert_eq!(me.json["quotaBytes"], 250 * MB);

    // 계정별로 따로 정하면 기본값을 바꿔도 유지
    let q = app
        .call(
            Method::PUT,
            &format!("/api/v1/admin/accounts/{user_id}/quota"),
            Some(json!({ "quotaMb": 50 })),
            Auth::Cookie(&admin),
        )
        .await;
    assert_eq!(q.status, StatusCode::OK, "{}", q.json);
    assert_eq!(q.json["quotaBytes"], 50 * MB);
    assert_eq!(q.json["quotaCustom"], true);
    app.call(
        Method::PUT,
        "/api/v1/admin/settings",
        Some(json!({ "defaultQuotaMb": 300 })),
        Auth::Cookie(&admin),
    )
    .await;
    let list = app
        .call(
            Method::GET,
            "/api/v1/admin/accounts",
            None,
            Auth::Cookie(&admin),
        )
        .await;
    let accounts = list.json["accounts"].as_array().unwrap();
    let find = |email: &str| {
        accounts
            .iter()
            .find(|a| a["email"] == email)
            .unwrap()
            .clone()
    };
    assert_eq!(find("user@example.com")["quotaBytes"], 50 * MB);
    assert_eq!(find("boss@example.com")["quotaBytes"], 300 * MB);

    // null → 기본값으로 되돌림
    let q = app
        .call(
            Method::PUT,
            &format!("/api/v1/admin/accounts/{user_id}/quota"),
            Some(json!({ "quotaMb": null })),
            Auth::Cookie(&admin),
        )
        .await;
    assert_eq!(q.json["quotaBytes"], 300 * MB);
    assert_eq!(q.json["quotaCustom"], false);

    // 새 가입자는 화면에서 정한 기본값
    let late = app.register("late@example.com").await;
    assert_eq!(late.json["quotaBytes"], 300 * MB);

    // 잘못된 값·없는 계정
    let bad = app
        .call(
            Method::PUT,
            "/api/v1/admin/settings",
            Some(json!({ "defaultQuotaMb": 0 })),
            Auth::Cookie(&admin),
        )
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    assert_eq!(bad.json["error"], "invalid_quota");
    let missing = app
        .call(
            Method::PUT,
            "/api/v1/admin/accounts/00000000-0000-0000-0000-000000000000/quota",
            Some(json!({ "quotaMb": 10 })),
            Auth::Cookie(&admin),
        )
        .await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn without_admin_emails_nobody_is_admin() {
    let Some(app) = app_with(|_| {}).await else {
        return;
    };
    let r = app.register("first@example.com").await;
    assert_eq!(r.json["isAdmin"], false);
    let c = r.cookie.unwrap();
    let s = app
        .call(
            Method::GET,
            "/api/v1/admin/settings",
            None,
            Auth::Cookie(&c),
        )
        .await;
    assert_eq!(s.status, StatusCode::FORBIDDEN);
}

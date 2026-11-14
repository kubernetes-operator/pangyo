//! 같은 기기 다시 등록 = 기존 장비 기록 이어 쓰기 (2026-11-13 사용 의견). 실제 Postgres 필요.
mod common;

use axum::http::{Method, StatusCode};
use common::{app, Auth, TestApp};
use serde_json::{json, Value};

async fn code(app: &TestApp, c: &str, device: Option<&str>) -> String {
    let uri = match device {
        Some(id) => format!("/api/v1/pairing/start?deviceId={id}"),
        None => "/api/v1/pairing/start".to_owned(),
    };
    let r = app.call(Method::POST, &uri, None, Auth::Cookie(c)).await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.json);
    r.json["code"].as_str().unwrap().to_owned()
}

async fn pair(app: &TestApp, code: &str, name: &str, key: Option<&str>) -> Value {
    let mut body = json!({ "code": code, "deviceName": name, "platform": "android" });
    if let Some(k) = key {
        body["deviceKey"] = json!(k);
    }
    let r = app
        .call(
            Method::POST,
            "/api/v1/pairing/finish",
            Some(body),
            Auth::None,
        )
        .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.json);
    r.json
}

async fn devices(app: &TestApp, c: &str) -> Vec<Value> {
    app.call(Method::GET, "/api/v1/devices", None, Auth::Cookie(c))
        .await
        .json
        .as_array()
        .unwrap()
        .clone()
}

#[tokio::test]
async fn same_device_key_reuses_the_device_record() {
    let Some(app) = app().await else { return };
    let c = app.register("reuse@example.com").await.cookie.unwrap();
    let key = "a1b2c3d4e5f6a7b8c9d0";
    let first = pair(&app, &code(&app, &c, None).await, "Galaxy", Some(key)).await;
    let id = first["deviceId"].as_str().unwrap().to_owned();
    // 웹에서 폐기 → 재설치 후 다시 등록(같은 기기 값) → 같은 장비 기록, 폐기 해제, 예전 토큰은 무효
    app.call(
        Method::DELETE,
        &format!("/api/v1/devices/{id}"),
        None,
        Auth::Cookie(&c),
    )
    .await;
    let again = pair(&app, &code(&app, &c, None).await, "Galaxy S23", Some(key)).await;
    assert_eq!(again["deviceId"], id.as_str());
    let list = devices(&app, &c).await;
    assert_eq!(list.len(), 1);
    assert_eq!(
        (list[0]["name"].as_str(), list[0]["revokedAt"].is_null()),
        (Some("Galaxy S23"), true)
    );
    let old = app
        .call(
            Method::GET,
            "/api/v1/account",
            None,
            Auth::Bearer(first["token"].as_str().unwrap()),
        )
        .await;
    assert_eq!(old.status, StatusCode::UNAUTHORIZED);
    let new = app
        .call(
            Method::GET,
            "/api/v1/account",
            None,
            Auth::Bearer(again["token"].as_str().unwrap()),
        )
        .await;
    assert_eq!(new.status, StatusCode::OK);

    // 기기 값이 없거나 다르면 새 장비
    pair(&app, &code(&app, &c, None).await, "PC", None).await;
    pair(
        &app,
        &code(&app, &c, None).await,
        "Tablet",
        Some("zzzzzzzzzzzzzzzz"),
    )
    .await;
    assert_eq!(devices(&app, &c).await.len(), 3);

    // 다른 계정에서 같은 기기 값 → 그 계정의 새 장비 (계정마다 따로)
    let other = app
        .register("reuse-other@example.com")
        .await
        .cookie
        .unwrap();
    let theirs = pair(&app, &code(&app, &other, None).await, "Galaxy", Some(key)).await;
    assert_ne!(theirs["deviceId"], id.as_str());
}

#[tokio::test]
async fn code_for_a_chosen_device_reconnects_that_record() {
    let Some(app) = app().await else { return };
    let c = app.register("rechoose@example.com").await.cookie.unwrap();
    let first = pair(&app, &code(&app, &c, None).await, "노트북", None).await;
    let id = first["deviceId"].as_str().unwrap().to_owned();
    let again = pair(
        &app,
        &code(&app, &c, Some(&id)).await,
        "노트북(다시)",
        Some("bbbbbbbbbbbbbbbb"),
    )
    .await;
    assert_eq!(again["deviceId"], id.as_str());
    assert_eq!(devices(&app, &c).await.len(), 1);
    // 남의 장비로는 코드를 만들 수 없음
    let other = app
        .register("rechoose-other@example.com")
        .await
        .cookie
        .unwrap();
    let r = app
        .call(
            Method::POST,
            &format!("/api/v1/pairing/start?deviceId={id}"),
            None,
            Auth::Cookie(&other),
        )
        .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
}

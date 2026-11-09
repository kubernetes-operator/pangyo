//! 메모 동기화 + 편집 임대 통합 테스트 (로드맵 06, ADR-0003 A안). 실제 Postgres 필요.
mod common;

use axum::http::{Method, StatusCode};
use common::{app, app_with, Auth, Reply, TestApp};
use serde_json::{json, Value};

const NOTE: &str = "11111111-1111-4111-8111-111111111111";

/// 가입 + 로그인 쿠키 (같은 계정의 "다른 기기"는 같은 쿠키에 다른 clientId로 흉내)
async fn user(app: &TestApp, email: &str) -> String {
    app.register(email).await.cookie.expect("가입 쿠키")
}

async fn put(
    app: &TestApp,
    c: &str,
    client: &str,
    base: i64,
    token: Option<i64>,
    title: &str,
    content: &str,
) -> Reply {
    app.call(
        Method::PUT,
        &format!("/api/v1/notes/{NOTE}"),
        Some(
            json!({ "baseVersion": base, "clientId": client, "fencingToken": token,
                     "title": title, "folder": "Work", "format": "md", "content": content }),
        ),
        Auth::Cookie(c),
    )
    .await
}

async fn lease(app: &TestApp, c: &str, client: &str, label: &str) -> Reply {
    app.call(
        Method::POST,
        &format!("/api/v1/notes/{NOTE}/lease"),
        Some(json!({ "clientId": client, "label": label })),
        Auth::Cookie(c),
    )
    .await
}

fn token(r: &Reply) -> Option<i64> {
    r.json["fencingToken"].as_i64()
}

#[tokio::test]
async fn create_then_update_with_lease_and_versions() {
    let Some(app) = app().await else { return };
    let c = user(&app, "sync@example.com").await;
    // 새 메모: baseVersion 0, 임대 불필요
    let created = put(&app, &c, "laptop", 0, None, "회의", "v1").await;
    assert_eq!(created.status, StatusCode::OK, "{}", created.json);
    assert_eq!(
        (
            created.json["version"].as_i64(),
            created.json["folder"].as_str()
        ),
        (Some(1), Some("Work"))
    );

    // 기존 메모는 임대 없이 저장 불가
    let no_lease = put(&app, &c, "laptop", 1, None, "회의", "v2").await;
    assert_eq!(
        (no_lease.status, no_lease.json["error"].as_str()),
        (StatusCode::LOCKED, Some("lease_required"))
    );

    let l = lease(&app, &c, "laptop", "노트북").await;
    assert_eq!(l.status, StatusCode::OK, "{}", l.json);
    let t = token(&l);
    let saved = put(&app, &c, "laptop", 1, t, "회의", "v2").await;
    assert_eq!(saved.json["version"], 2);
    assert_eq!(saved.json["content"], "v2");

    // 오래된 baseVersion → 409, 덮어쓰지 않음
    let stale = put(&app, &c, "laptop", 1, t, "회의", "stale").await;
    assert_eq!(
        (stale.status, stale.json["error"].as_str()),
        (StatusCode::CONFLICT, Some("version_conflict"))
    );
    assert_eq!(stale.json["details"]["currentVersion"], 2);
    let got = app
        .call(
            Method::GET,
            &format!("/api/v1/notes/{NOTE}"),
            None,
            Auth::Cookie(&c),
        )
        .await;
    assert_eq!(got.json["content"], "v2");

    // 용량 반영
    let quota = app
        .call(Method::GET, "/api/v1/account/quota", None, Auth::Cookie(&c))
        .await;
    assert_eq!(quota.json["usedBytes"], 2);
}

#[tokio::test]
async fn second_device_is_blocked_while_lease_is_held() {
    let Some(app) = app().await else { return };
    let c = user(&app, "two@example.com").await;
    put(&app, &c, "laptop", 0, None, "공유", "v1").await;
    let first = lease(&app, &c, "laptop", "노트북").await;
    assert_eq!(first.status, StatusCode::OK);

    // 같은 계정의 다른 기기(클라이언트)는 임대를 받을 수 없음 → 읽기 전용
    let second = lease(&app, &c, "phone", "휴대폰").await;
    assert_eq!(
        (second.status, second.json["error"].as_str()),
        (StatusCode::LOCKED, Some("lease_held"))
    );
    assert_eq!(second.json["details"]["holderLabel"], "노트북");
    assert!(second.json["message"].as_str().unwrap().contains("노트북"));
    // 임대 없이 저장 시도도 거부
    let write = put(&app, &c, "phone", 1, Some(999), "공유", "phone edit").await;
    assert_eq!(write.json["error"], "lease_held");

    // 갱신은 같은 token 유지
    let renewed = lease(&app, &c, "laptop", "노트북").await;
    assert_eq!(token(&renewed), token(&first));

    // 반납 후에는 다른 기기가 새 token으로 획득
    let released = app
        .call(
            Method::DELETE,
            &format!("/api/v1/notes/{NOTE}/lease?clientId=laptop"),
            None,
            Auth::Cookie(&c),
        )
        .await;
    assert_eq!(released.status, StatusCode::NO_CONTENT);
    let taken = lease(&app, &c, "phone", "휴대폰").await;
    assert_eq!(taken.status, StatusCode::OK);
    assert!(
        token(&taken).unwrap() > token(&first).unwrap(),
        "fencing token 증가"
    );
}

#[tokio::test]
async fn expired_lease_fences_out_late_writer() {
    let Some(app) = app_with(|c| c.lease_ttl_secs = 1).await else {
        return;
    };
    let c = user(&app, "fence@example.com").await;
    put(&app, &c, "laptop", 0, None, "펜싱", "v1").await;
    let old = token(&lease(&app, &c, "laptop", "노트북").await);
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await; // 노트북 네트워크 끊김 → 임대 만료
    let phone = lease(&app, &c, "phone", "휴대폰").await;
    assert_eq!(
        phone.status,
        StatusCode::OK,
        "만료된 임대는 다른 기기가 가져갈 수 있음"
    );
    let phone_saved = put(&app, &c, "phone", 1, token(&phone), "펜싱", "phone v2").await;
    assert_eq!(phone_saved.json["version"], 2);
    // 늦게 돌아온 노트북의 저장(옛 token)은 거부 — 덮어쓰기 없음
    let late = put(&app, &c, "laptop", 1, old, "펜싱", "laptop late").await;
    assert_eq!(late.status, StatusCode::LOCKED);
    let got = app
        .call(
            Method::GET,
            &format!("/api/v1/notes/{NOTE}"),
            None,
            Auth::Cookie(&c),
        )
        .await;
    assert_eq!(got.json["content"], "phone v2");
}

#[tokio::test]
async fn change_feed_cursor_tombstone_and_tenant_isolation() {
    let Some(app) = app().await else { return };
    let c = user(&app, "feed@example.com").await;
    put(&app, &c, "laptop", 0, None, "피드", "v1").await;
    let feed = app
        .call(Method::GET, "/api/v1/notes?since=0", None, Auth::Cookie(&c))
        .await;
    assert_eq!(feed.json["notes"].as_array().unwrap().len(), 1);
    let cursor = feed.json["cursor"].as_i64().unwrap();
    let empty = app
        .call(
            Method::GET,
            &format!("/api/v1/notes?since={cursor}"),
            None,
            Auth::Cookie(&c),
        )
        .await;
    assert!(empty.json["notes"].as_array().unwrap().is_empty());
    assert_eq!(empty.json["hasMore"], false);

    // 삭제 → 묘비가 변경 목록에 나타남, 용량 반환
    let t = token(&lease(&app, &c, "laptop", "노트북").await).unwrap();
    let deleted = app
        .call(
            Method::DELETE,
            &format!("/api/v1/notes/{NOTE}?baseVersion=1&clientId=laptop&fencingToken={t}"),
            None,
            Auth::Cookie(&c),
        )
        .await;
    assert_eq!(deleted.json["deleted"], true, "{}", deleted.json);
    let after = app
        .call(
            Method::GET,
            &format!("/api/v1/notes?since={cursor}"),
            None,
            Auth::Cookie(&c),
        )
        .await;
    let changes: &Vec<Value> = after.json["notes"].as_array().unwrap();
    assert_eq!(
        (changes.len(), changes[0]["deleted"].as_bool()),
        (1, Some(true))
    );
    let quota = app
        .call(Method::GET, "/api/v1/account/quota", None, Auth::Cookie(&c))
        .await;
    assert_eq!(quota.json["usedBytes"], 0);

    // 다른 계정은 읽기/임대/덮어쓰기 모두 불가 (존재도 알 수 없음)
    let other = user(&app, "other@example.com").await;
    let peek = app
        .call(
            Method::GET,
            &format!("/api/v1/notes/{NOTE}"),
            None,
            Auth::Cookie(&other),
        )
        .await;
    assert_eq!(peek.status, StatusCode::NOT_FOUND);
    let hijack = put(&app, &other, "x", 0, None, "탈취", "x").await;
    assert_eq!(hijack.status, StatusCode::NOT_FOUND);
    let list = app
        .call(
            Method::GET,
            "/api/v1/notes?since=0",
            None,
            Auth::Cookie(&other),
        )
        .await;
    assert!(list.json["notes"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn path_rules_quota_and_validation() {
    let Some(app) = app_with(|c| c.free_storage_mb = 1).await else {
        return;
    };
    let c = user(&app, "rules@example.com").await;
    // 같은 폴더·같은 이름(대소문자 무시) 메모는 하나만
    let first = app
        .call(Method::PUT, "/api/v1/notes/22222222-2222-4222-8222-222222222222",
            Some(json!({ "baseVersion": 0, "clientId": "a", "title": "Plan", "folder": "Work", "format": "md", "content": "" })),
            Auth::Cookie(&c))
        .await;
    assert_eq!(first.status, StatusCode::OK);
    let dup = app
        .call(Method::PUT, "/api/v1/notes/33333333-3333-4333-8333-333333333333",
            Some(json!({ "baseVersion": 0, "clientId": "a", "title": "PLAN", "folder": "work", "format": "md", "content": "" })),
            Auth::Cookie(&c))
        .await;
    assert_eq!(dup.json["error"], "path_conflict");
    // 3단계 폴더 거부, 제목은 공통 규칙으로 정리
    let deep = put(&app, &c, "a", 0, None, "x", "").await; // folder Work → OK
    assert_eq!(deep.status, StatusCode::OK);
    let bad = app
        .call(Method::PUT, "/api/v1/notes/44444444-4444-4444-8444-444444444444",
            Some(json!({ "baseVersion": 0, "clientId": "a", "title": "CON", "folder": "a/b/c", "format": "md", "content": "" })),
            Auth::Cookie(&c))
        .await;
    assert_eq!(bad.json["error"], "invalid_folder");
    // 용량 1MB: 900KB 두 개째는 거부
    let big = "가".repeat(300 * 1024); // 900KB (UTF-8 3바이트)
    let ok = app
        .call(Method::PUT, "/api/v1/notes/55555555-5555-4555-8555-555555555555",
            Some(json!({ "baseVersion": 0, "clientId": "a", "title": "big1", "format": "txt", "content": big })),
            Auth::Cookie(&c))
        .await;
    assert_eq!(ok.status, StatusCode::OK, "{}", ok.json["error"]);
    let over = app
        .call(Method::PUT, "/api/v1/notes/66666666-6666-4666-8666-666666666666",
            Some(json!({ "baseVersion": 0, "clientId": "a", "title": "big2", "format": "txt", "content": big })),
            Auth::Cookie(&c))
        .await;
    assert_eq!(over.json["error"], "quota_exceeded");
    // 잘못된 clientId
    let weird = put(&app, &c, "../x", 0, None, "y", "").await;
    assert_eq!(weird.json["error"], "invalid_client_id");
}

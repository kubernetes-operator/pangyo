//! 서버 검색·연관 메모·자동화 토큰 통합 테스트 (로드맵 07). 실제 Postgres 필요.
mod common;

use axum::http::{Method, StatusCode};
use common::{app, Auth, Reply, TestApp};
use serde_json::{json, Value};

async fn user(app: &TestApp, email: &str) -> String {
    app.register(email).await.cookie.expect("가입 쿠키")
}

async fn note(app: &TestApp, auth: Auth<'_>, title: &str, content: &str) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    let r = app
        .call(
            Method::PUT,
            &format!("/api/v1/notes/{id}"),
            Some(json!({ "baseVersion": 0, "clientId": "t", "title": title, "format": "md", "content": content })),
            auth,
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.json);
    id
}

fn titles(r: &Reply) -> Vec<String> {
    r.json
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["title"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn search_finds_korean_title_and_body_with_snippet_and_isolation() {
    let Some(app) = app().await else { return };
    let c = user(&app, "search@example.com").await;
    note(&app, Auth::Cookie(&c), "주간 회의록", "프로젝트 일정 점검").await;
    note(
        &app,
        Auth::Cookie(&c),
        "장보기",
        "우유, 계란, 그리고 회의 끝나고 살 것",
    )
    .await;
    note(&app, Auth::Cookie(&c), "할인율", "100% 확정 아님").await;
    let other = user(&app, "other-search@example.com").await;
    note(&app, Auth::Cookie(&other), "남의 회의록", "보이면 안 됨").await;

    let r = app
        .call(
            Method::GET,
            "/api/v1/search?q=%ED%9A%8C%EC%9D%98",
            None,
            Auth::Cookie(&c),
        )
        .await; // q=회의
    assert_eq!(r.status, StatusCode::OK, "{}", r.json);
    assert_eq!(
        titles(&r),
        ["주간 회의록", "장보기"],
        "제목 일치가 먼저, 다른 계정 제외"
    );
    assert!(r.json[1]["snippet"]
        .as_str()
        .unwrap()
        .contains("회의 끝나고"));

    // '%'는 와일드카드가 아니라 글자 그대로
    let pct = app
        .call(
            Method::GET,
            "/api/v1/search?q=100%25",
            None,
            Auth::Cookie(&c),
        )
        .await;
    assert_eq!(titles(&pct), ["할인율"]);
    let empty = app
        .call(Method::GET, "/api/v1/search?q=%20", None, Auth::Cookie(&c))
        .await;
    assert_eq!(empty.json["error"], "invalid_query");
}

#[tokio::test]
async fn similar_notes_rank_related_content() {
    let Some(app) = app().await else { return };
    let c = user(&app, "similar@example.com").await;
    let base = note(
        &app,
        Auth::Cookie(&c),
        "프로젝트 계획",
        "프로젝트 계획 회의 일정과 담당자 정리",
    )
    .await;
    note(
        &app,
        Auth::Cookie(&c),
        "프로젝트 계획 2차",
        "프로젝트 계획 회의 일정 변경과 담당자",
    )
    .await;
    note(&app, Auth::Cookie(&c), "장보기", "우유 계란 빵").await;
    let r = app
        .call(
            Method::GET,
            &format!("/api/v1/notes/{base}/similar"),
            None,
            Auth::Cookie(&c),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.json);
    assert_eq!(titles(&r), ["프로젝트 계획 2차"], "관련 없는 메모는 제외");
}

#[tokio::test]
async fn integration_tokens_scopes_and_revocation() {
    let Some(app) = app().await else { return };
    let c = user(&app, "n8n@example.com").await;
    let existing = note(&app, Auth::Cookie(&c), "n8n 대상", "v1").await;

    let create = |scopes: Value| {
        app.call(
            Method::POST,
            "/api/v1/integrations/tokens",
            Some(json!({ "name": "n8n", "scopes": scopes, "ttlDays": 30 })),
            Auth::Cookie(&c),
        )
    };
    let ro = create(json!(["notes:read"])).await;
    assert_eq!(ro.status, StatusCode::CREATED, "{}", ro.json);
    let ro_token = ro.json["token"].as_str().unwrap().to_owned();
    assert!(ro_token.starts_with("mn_pat_"));

    // 읽기 토큰: 변경 목록·검색 가능, 쓰기·계정 정보 불가
    let feed = app
        .call(
            Method::GET,
            "/api/v1/notes?since=0",
            None,
            Auth::Bearer(&ro_token),
        )
        .await;
    assert_eq!(feed.json["notes"].as_array().unwrap().len(), 1);
    let search = app
        .call(
            Method::GET,
            "/api/v1/search?q=n8n",
            None,
            Auth::Bearer(&ro_token),
        )
        .await;
    assert_eq!(search.status, StatusCode::OK);
    let write = app
        .call(
            Method::POST,
            &format!("/api/v1/notes/{existing}/lease"),
            Some(json!({ "clientId": "n8n" })),
            Auth::Bearer(&ro_token),
        )
        .await;
    assert_eq!(write.json["error"], "insufficient_scope");
    let me = app
        .call(
            Method::GET,
            "/api/v1/account",
            None,
            Auth::Bearer(&ro_token),
        )
        .await;
    assert_eq!(me.json["error"], "insufficient_scope");
    // 토큰으로 토큰을 만들 수 없음
    let nested = app
        .call(
            Method::POST,
            "/api/v1/integrations/tokens",
            Some(json!({ "name": "x", "scopes": ["notes:read"] })),
            Auth::Bearer(&ro_token),
        )
        .await;
    assert_eq!(nested.json["error"], "session_required");

    // 쓰기 토큰: 임대를 받아 저장 (웹과 같은 규칙)
    let rw = create(json!(["notes:read", "notes:write"])).await.json["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let lease = app
        .call(
            Method::POST,
            &format!("/api/v1/notes/{existing}/lease"),
            Some(json!({ "clientId": "n8n", "label": "n8n" })),
            Auth::Bearer(&rw),
        )
        .await;
    assert_eq!(lease.status, StatusCode::OK, "{}", lease.json);
    let saved = app
        .call(Method::PUT, &format!("/api/v1/notes/{existing}"),
            Some(json!({ "baseVersion": 1, "clientId": "n8n", "fencingToken": lease.json["fencingToken"],
                         "title": "n8n 대상", "format": "md", "content": "n8n이 추가" })),
            Auth::Bearer(&rw))
        .await;
    assert_eq!(saved.json["content"], "n8n이 추가");
    // 웹 사용자는 n8n이 편집 중이라는 표시를 봄
    let web = app
        .call(
            Method::POST,
            &format!("/api/v1/notes/{existing}/lease"),
            Some(json!({ "clientId": "web" })),
            Auth::Cookie(&c),
        )
        .await;
    assert_eq!(web.json["details"]["holderLabel"], "n8n");

    // 목록에는 원문이 없고, 해제하면 즉시 거부
    let list = app
        .call(
            Method::GET,
            "/api/v1/integrations/tokens",
            None,
            Auth::Cookie(&c),
        )
        .await;
    assert_eq!(list.json.as_array().unwrap().len(), 2);
    assert!(
        list.json.to_string().find("mn_pat_").is_none(),
        "토큰 원문 노출 금지"
    );
    let id = ro.json["id"].as_str().unwrap();
    let revoked = app
        .call(
            Method::DELETE,
            &format!("/api/v1/integrations/tokens/{id}"),
            None,
            Auth::Cookie(&c),
        )
        .await;
    assert_eq!(revoked.status, StatusCode::NO_CONTENT);
    let after = app
        .call(
            Method::GET,
            "/api/v1/notes?since=0",
            None,
            Auth::Bearer(&ro_token),
        )
        .await;
    assert_eq!(after.status, StatusCode::UNAUTHORIZED);

    // 입력 검증
    assert_eq!(
        create(json!(["admin"])).await.json["error"],
        "invalid_scopes"
    );
    let long = app
        .call(
            Method::POST,
            "/api/v1/integrations/tokens",
            Some(json!({ "name": "x", "scopes": ["notes:read"], "ttlDays": 1000 })),
            Auth::Cookie(&c),
        )
        .await;
    assert_eq!(long.json["error"], "invalid_ttl");
}

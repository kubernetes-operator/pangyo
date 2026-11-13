//! 프로젝트 (ADR-0005) 통합 테스트. 실제 Postgres 필요.
mod common;

use axum::http::{Method, StatusCode};
use common::{app, Auth, Reply, TestApp};
use serde_json::{json, Value};

const WORK: &str = "aaaaaaaa-0000-4000-8000-000000000001";
const HOME: &str = "aaaaaaaa-0000-4000-8000-000000000002";
const NOTE_A: &str = "bbbbbbbb-0000-4000-8000-000000000001";
const NOTE_B: &str = "bbbbbbbb-0000-4000-8000-000000000002";

async fn put_project(app: &TestApp, c: &str, id: &str, name: &str) -> Reply {
    app.call(
        Method::PUT,
        &format!("/api/v1/projects/{id}"),
        Some(json!({ "name": name })),
        Auth::Cookie(c),
    )
    .await
}

/// 새 메모 저장 (project: 필드 없음 = None, null = Some(Value::Null))
async fn put_note(
    app: &TestApp,
    c: &str,
    id: &str,
    base: i64,
    token: Option<i64>,
    project: Option<Value>,
) -> Reply {
    let mut body = json!({ "baseVersion": base, "clientId": "laptop", "fencingToken": token,
        "title": "회의", "folder": "업무", "format": "md", "content": "본문" });
    if let Some(p) = project {
        body["projectId"] = p;
    }
    app.call(
        Method::PUT,
        &format!("/api/v1/notes/{id}"),
        Some(body),
        Auth::Cookie(c),
    )
    .await
}

async fn lease(app: &TestApp, c: &str, id: &str) -> Option<i64> {
    app.call(
        Method::POST,
        &format!("/api/v1/notes/{id}/lease"),
        Some(json!({ "clientId": "laptop" })),
        Auth::Cookie(c),
    )
    .await
    .json["fencingToken"]
        .as_i64()
}

async fn changes(app: &TestApp, c: &str, query: &str) -> Value {
    app.call(
        Method::GET,
        &format!("/api/v1/notes?{query}"),
        None,
        Auth::Cookie(c),
    )
    .await
    .json
}

#[tokio::test]
async fn projects_create_rename_list_delete_and_note_paths() {
    let Some(app) = app().await else { return };
    let c = app.register("proj@example.com").await.cookie.unwrap();

    let p = put_project(&app, &c, WORK, " 업무 ").await;
    assert_eq!(p.status, StatusCode::OK, "{}", p.json);
    assert_eq!(p.json["name"], "업무");
    put_project(&app, &c, HOME, "개인").await;
    // 이름 중복(대소문자·공백 무시), 잘못된 이름
    assert_eq!(
        put_project(&app, &c, HOME, "업무").await.json["error"],
        "project_name_taken"
    );
    for bad in ["a/b", "@개인", ""] {
        let r = put_project(&app, &c, "aaaaaaaa-0000-4000-8000-0000000000ff", bad).await;
        assert_eq!(r.json["error"], "invalid_project_name", "{bad}");
    }
    // 이름 바꾸기 = 같은 ID로 PUT
    assert_eq!(put_project(&app, &c, HOME, "집").await.json["name"], "집");
    let list = app
        .call(Method::GET, "/api/v1/projects", None, Auth::Cookie(&c))
        .await;
    let names: Vec<_> = list
        .json
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(names, ["업무", "집"]);

    // 같은 경로(업무/회의.md)라도 프로젝트가 다르면 허용
    assert_eq!(
        put_note(&app, &c, NOTE_A, 0, None, None).await.status,
        StatusCode::OK
    );
    let b = put_note(&app, &c, NOTE_B, 0, None, Some(json!(WORK))).await;
    assert_eq!(b.status, StatusCode::OK, "{}", b.json);
    assert_eq!(b.json["projectId"], WORK);
    // 없는 프로젝트
    let bad = put_note(
        &app,
        &c,
        "bbbbbbbb-0000-4000-8000-0000000000ff",
        0,
        None,
        Some(json!("aaaaaaaa-0000-4000-8000-000000000009")),
    )
    .await;
    assert_eq!(bad.json["error"], "invalid_project");

    // 메모가 있는 프로젝트는 지울 수 없음 → 메모를 기본으로 옮긴 뒤(null) 지울 수 있음… 단, 기본에 같은 경로가 있으면 409
    let del = app
        .call(
            Method::DELETE,
            &format!("/api/v1/projects/{WORK}"),
            None,
            Auth::Cookie(&c),
        )
        .await;
    assert_eq!(del.json["error"], "project_not_empty");
    let t = lease(&app, &c, NOTE_B).await;
    let moved = put_note(&app, &c, NOTE_B, 1, t, Some(Value::Null)).await;
    assert_eq!(
        moved.json["error"], "path_conflict",
        "기본 프로젝트에 같은 경로 메모가 있음"
    );
    let moved = put_note(&app, &c, NOTE_B, 1, t, Some(json!(HOME))).await;
    assert_eq!(moved.json["projectId"], HOME);
    let del = app
        .call(
            Method::DELETE,
            &format!("/api/v1/projects/{WORK}"),
            None,
            Auth::Cookie(&c),
        )
        .await;
    assert_eq!(del.status, StatusCode::OK, "{}", del.json);
    assert_eq!(del.json["deleted"], true);
    // 삭제된 프로젝트에는 저장·이름 변경 불가
    assert_eq!(
        put_project(&app, &c, WORK, "다시").await.status,
        StatusCode::NOT_FOUND
    );
    let gone = put_note(
        &app,
        &c,
        "bbbbbbbb-0000-4000-8000-0000000000fe",
        0,
        None,
        Some(json!(WORK)),
    )
    .await;
    assert_eq!(gone.json["error"], "invalid_project");

    // 다른 계정은 내 프로젝트를 볼 수도, 쓸 수도 없음
    let other = app.register("other-proj@example.com").await.cookie.unwrap();
    assert_eq!(
        put_project(&app, &other, HOME, "가로채기").await.status,
        StatusCode::NOT_FOUND
    );
    let list = app
        .call(Method::GET, "/api/v1/projects", None, Auth::Cookie(&other))
        .await;
    assert_eq!(list.json.as_array().unwrap().len(), 0);
    let r = put_note(
        &app,
        &other,
        "bbbbbbbb-0000-4000-8000-0000000000fd",
        0,
        None,
        Some(json!(HOME)),
    )
    .await;
    assert_eq!(r.json["error"], "invalid_project");
}

#[tokio::test]
async fn change_feed_with_projects_and_legacy_clients() {
    let Some(app) = app().await else { return };
    let c = app.register("feed-proj@example.com").await.cookie.unwrap();
    put_project(&app, &c, WORK, "업무").await;
    put_note(&app, &c, NOTE_A, 0, None, None).await; // 기본
    put_note(&app, &c, NOTE_B, 0, None, Some(json!(WORK))).await;

    // 프로젝트를 아는 클라이언트: 프로젝트 + 두 메모(프로젝트 표시)
    let feed = changes(&app, &c, "since=0&projects=1").await;
    assert_eq!(feed["projects"].as_array().unwrap().len(), 1);
    let notes = feed["notes"].as_array().unwrap();
    let b = notes.iter().find(|n| n["id"] == NOTE_B).unwrap();
    assert_eq!(
        (b["projectId"].as_str(), b["deleted"].as_bool()),
        (Some(WORK), Some(false))
    );

    // 예전 클라이언트: 프로젝트 없음, 다른 프로젝트 메모는 묘비(본문 없음)
    let legacy = changes(&app, &c, "since=0").await;
    assert_eq!(legacy["projects"].as_array().unwrap().len(), 0);
    let lb = legacy["notes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == NOTE_B)
        .unwrap()
        .clone();
    assert_eq!(
        (lb["deleted"].as_bool(), lb["content"].as_str()),
        (Some(true), Some(""))
    );
    let la = legacy["notes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == NOTE_A)
        .unwrap()
        .clone();
    assert_eq!(la["deleted"], false);

    // 예전 클라이언트가 projectId 없이 저장해도 프로젝트는 그대로
    let t = lease(&app, &c, NOTE_B).await;
    let saved = put_note(&app, &c, NOTE_B, 1, t, None).await;
    assert_eq!(saved.json["projectId"], WORK, "{}", saved.json);

    // 페이지 나누기: limit=1씩 받아도 메모·프로젝트를 빠짐없이, 커서는 계속 증가
    let (mut since, mut seen_projects, mut seen_notes) = (0, 0, 0);
    loop {
        let page = changes(&app, &c, &format!("since={since}&limit=1&projects=1")).await;
        let n = page["notes"].as_array().unwrap().len();
        let p = page["projects"].as_array().unwrap().len();
        assert_eq!(n + p, 1, "{page}");
        seen_notes += n;
        seen_projects += p;
        let next = page["cursor"].as_i64().unwrap();
        assert!(next > since);
        since = next;
        if !page["hasMore"].as_bool().unwrap() {
            break;
        }
    }
    assert_eq!((seen_projects, seen_notes), (1, 2));
}

#[tokio::test]
async fn default_project_display_name() {
    let Some(app) = app().await else { return };
    let c = app
        .register("default-name@example.com")
        .await
        .cookie
        .unwrap();
    assert_eq!(
        changes(&app, &c, "since=0&projects=1").await["defaultProjectName"],
        ""
    );
    let r = app
        .call(
            Method::PATCH,
            "/api/v1/account",
            Some(json!({ "defaultProjectName": " 일상 " })),
            Auth::Cookie(&c),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["defaultProjectName"], "일상");
    // 다른 기기는 변경 목록에서 받음 (바뀐 메모가 없어도 매번)
    let feed = changes(&app, &c, "since=999999&projects=1").await;
    assert_eq!(feed["defaultProjectName"], "일상");
    assert!(
        changes(&app, &c, "since=0")
            .await
            .get("defaultProjectName")
            .is_none(),
        "예전 클라이언트에는 없음"
    );
    let bad = app
        .call(
            Method::PATCH,
            "/api/v1/account",
            Some(json!({ "defaultProjectName": "@x" })),
            Auth::Cookie(&c),
        )
        .await;
    assert_eq!(bad.json["error"], "invalid_project_name");
    let reset = app
        .call(
            Method::PATCH,
            "/api/v1/account",
            Some(json!({ "defaultProjectName": "" })),
            Auth::Cookie(&c),
        )
        .await;
    assert_eq!(reset.json["defaultProjectName"], "");
}

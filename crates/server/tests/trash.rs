//! 휴지통 1주일 보관 (2026-11-13 사용 의견). 실제 Postgres 필요.
mod common;

use axum::http::{Method, StatusCode};
use common::{app, Auth, Reply, TestApp};
use serde_json::{json, Value};

const NOTE: &str = "cccccccc-0000-4000-8000-000000000001";
const NOTE2: &str = "cccccccc-0000-4000-8000-000000000002";
const PROJECT: &str = "dddddddd-0000-4000-8000-000000000001";

async fn put(app: &TestApp, c: &str, id: &str, title: &str, project: Option<&str>) -> Reply {
    app.call(
        Method::PUT,
        &format!("/api/v1/notes/{id}"),
        Some(json!({ "baseVersion": 0, "clientId": "t", "title": title, "folder": "f", "format": "md",
                     "content": "보관할 본문", "projectId": project })),
        Auth::Cookie(c),
    )
    .await
}

async fn delete(app: &TestApp, c: &str, id: &str) -> Reply {
    let lease = app
        .call(
            Method::POST,
            &format!("/api/v1/notes/{id}/lease"),
            Some(json!({ "clientId": "t" })),
            Auth::Cookie(c),
        )
        .await;
    let token = lease.json["fencingToken"].as_i64().unwrap();
    let version = app
        .call(
            Method::GET,
            &format!("/api/v1/notes/{id}"),
            None,
            Auth::Cookie(c),
        )
        .await
        .json["version"]
        .as_i64()
        .unwrap();
    app.call(
        Method::DELETE,
        &format!("/api/v1/notes/{id}?baseVersion={version}&clientId=t&fencingToken={token}"),
        None,
        Auth::Cookie(c),
    )
    .await
}

async fn trash(app: &TestApp, c: &str) -> Value {
    app.call(Method::GET, "/api/v1/trash", None, Auth::Cookie(c))
        .await
        .json
}

#[tokio::test]
async fn deleted_note_is_kept_and_restored_with_content_and_quota() {
    let Some(app) = app().await else { return };
    let c = app.register("trash@example.com").await.cookie.unwrap();
    put(&app, &c, NOTE, "지울 메모", None).await;
    let used = || async {
        app.call(Method::GET, "/api/v1/account", None, Auth::Cookie(&c))
            .await
            .json["usedBytes"]
            .clone()
    };
    let before = used().await;
    let d = delete(&app, &c, NOTE).await;
    assert_eq!(
        (d.json["deleted"].as_bool(), d.json["content"].as_str()),
        (Some(true), Some("")),
        "다른 기기에는 묘비"
    );
    assert_eq!(used().await, 0);
    let t = trash(&app, &c).await;
    assert_eq!(t["retentionDays"], 7);
    assert_eq!(t["notes"][0]["title"], "지울 메모");
    assert!(t["notes"][0]["expiresAt"].is_string());

    let r = app
        .call(
            Method::POST,
            &format!("/api/v1/notes/{NOTE}/restore"),
            None,
            Auth::Cookie(&c),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.json);
    let note = app
        .call(
            Method::GET,
            &format!("/api/v1/notes/{NOTE}"),
            None,
            Auth::Cookie(&c),
        )
        .await
        .json;
    assert_eq!(
        (note["deleted"].as_bool(), note["content"].as_str()),
        (Some(false), Some("보관할 본문"))
    );
    assert_eq!(used().await, before);
    assert_eq!(trash(&app, &c).await["notes"].as_array().unwrap().len(), 0);
    // 두 번 되살리기는 404
    let again = app
        .call(
            Method::POST,
            &format!("/api/v1/notes/{NOTE}/restore"),
            None,
            Auth::Cookie(&c),
        )
        .await;
    assert_eq!(again.json["error"], "not_in_trash");

    // 같은 자리에 새 메모가 생기면 되살리기는 409
    delete(&app, &c, NOTE).await;
    put(&app, &c, NOTE2, "지울 메모", None).await;
    let clash = app
        .call(
            Method::POST,
            &format!("/api/v1/notes/{NOTE}/restore"),
            None,
            Auth::Cookie(&c),
        )
        .await;
    assert_eq!(clash.json["error"], "path_conflict");

    // 보관 기간이 지나면 본문을 비우고 휴지통에서 빠짐
    let db = app.state.db().ok().unwrap();
    assert!(memonest_server::trash::purge(db, 7).await.unwrap() == 0);
    sqlx::query("UPDATE notes SET deleted_at = NOW() - interval '8 days' WHERE id = $1::uuid")
        .bind(NOTE)
        .execute(db)
        .await
        .unwrap();
    assert_eq!(memonest_server::trash::purge(db, 7).await.unwrap(), 1);
    assert_eq!(trash(&app, &c).await["notes"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn project_restore_brings_back_its_notes() {
    let Some(app) = app().await else { return };
    let c = app
        .register("trash-project@example.com")
        .await
        .cookie
        .unwrap();
    app.call(
        Method::PUT,
        &format!("/api/v1/projects/{PROJECT}"),
        Some(json!({ "name": "임시" })),
        Auth::Cookie(&c),
    )
    .await;
    put(&app, &c, NOTE, "안의 메모", Some(PROJECT)).await;
    delete(&app, &c, NOTE).await;
    let d = app
        .call(
            Method::DELETE,
            &format!("/api/v1/projects/{PROJECT}"),
            None,
            Auth::Cookie(&c),
        )
        .await;
    assert_eq!(d.status, StatusCode::OK, "{}", d.json);
    let t = trash(&app, &c).await;
    assert_eq!(
        (
            t["projects"][0]["name"].as_str(),
            t["notes"][0]["projectId"].as_str()
        ),
        (Some("임시"), Some(PROJECT))
    );

    let r = app
        .call(
            Method::POST,
            &format!("/api/v1/projects/{PROJECT}/restore"),
            None,
            Auth::Cookie(&c),
        )
        .await;
    assert_eq!(
        (
            r.json["restoredProjects"].as_i64(),
            r.json["restoredNotes"].as_i64()
        ),
        (Some(1), Some(1)),
        "{}",
        r.json
    );
    let projects = app
        .call(Method::GET, "/api/v1/projects", None, Auth::Cookie(&c))
        .await
        .json;
    assert_eq!(projects[0]["name"], "임시");

    // 메모만 되살려도 지워진 프로젝트가 함께 돌아옴
    delete(&app, &c, NOTE).await;
    app.call(
        Method::DELETE,
        &format!("/api/v1/projects/{PROJECT}"),
        None,
        Auth::Cookie(&c),
    )
    .await;
    let r = app
        .call(
            Method::POST,
            &format!("/api/v1/notes/{NOTE}/restore"),
            None,
            Auth::Cookie(&c),
        )
        .await;
    assert_eq!(r.json["restoredProjects"], 1);
    // 다른 계정은 남의 휴지통을 되살릴 수 없음
    let other = app
        .register("trash-other@example.com")
        .await
        .cookie
        .unwrap();
    assert_eq!(
        trash(&app, &other).await["notes"].as_array().unwrap().len(),
        0
    );
    let steal = app
        .call(
            Method::POST,
            &format!("/api/v1/notes/{NOTE}/restore"),
            None,
            Auth::Cookie(&other),
        )
        .await;
    assert_eq!(steal.status, StatusCode::NOT_FOUND);
}

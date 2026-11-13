//! 데스크톱 작업공간 통합 테스트 (`cargo test -p memonest-core --features workspace`)
#![cfg(feature = "workspace")]

use std::fs;

use memonest_core::{
    workspace::{EditSession, NoteRecord, Snapshot, Workspace},
    CoreError, NoteFormat,
};

struct Fixture {
    _dir: tempfile::TempDir,
    notes: std::path::PathBuf,
    index: std::path::PathBuf,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    Fixture {
        notes: dir.path().join("Notes"),
        index: dir.path().join("app-data/index.sqlite"),
        _dir: dir,
    }
}

impl Fixture {
    fn open(&self) -> Workspace {
        Workspace::open(&self.notes, &self.index).unwrap()
    }
}

fn note(id: &str, folder: &str, title: &str, content: &str) -> NoteRecord {
    NoteRecord {
        id: id.to_owned(),
        title: title.to_owned(),
        folder: folder.to_owned(),
        format: NoteFormat::Md,
        content: content.to_owned(),
        created_at: "2026-11-04T00:00:00.000Z".to_owned(),
        modified_at: "2026-11-04T00:00:01.000Z".to_owned(),
        sessions: vec![EditSession {
            id: "s-1".to_owned(),
            started_at: "2026-11-04T00:00:00.000Z".to_owned(),
            ended_at: None,
            active_ms: 1_500,
            open_ms: 9_000,
        }],
        undo: vec![Snapshot {
            title: title.to_owned(),
            content: String::new(),
            format: NoteFormat::Md,
        }],
        redo: vec![],
        sync: None,
        project_id: None,
    }
}

const ID_A: &str = "11111111-1111-4111-8111-111111111111";
const ID_B: &str = "22222222-2222-4222-8222-222222222222";

#[test]
fn saved_note_is_plain_file_and_round_trips_with_metadata_after_reopen() {
    let f = fixture();
    let original = note(ID_A, "Work", "회의", "# 안건\n");
    f.open().save_note(&original).unwrap();

    // 정본은 사람이 읽는 파일, 메타데이터는 파일에 들어가지 않음
    assert_eq!(
        fs::read_to_string(f.notes.join("Work/회의.md")).unwrap(),
        "# 안건\n"
    );

    let listed = f.open().list_notes().unwrap(); // 앱 재시작
    assert_eq!(listed, vec![original]);
}

#[test]
fn rename_moves_file_without_leaving_old_one() {
    let f = fixture();
    let mut ws = f.open();
    let mut n = note(ID_A, "", "초안", "본문");
    ws.save_note(&n).unwrap();
    n.title = "최종".into();
    n.folder = "Work/Project".into();
    n.format = NoteFormat::Txt;
    ws.save_note(&n).unwrap();
    assert!(!f.notes.join("초안.md").exists());
    assert_eq!(
        fs::read_to_string(f.notes.join("Work/Project/최종.txt")).unwrap(),
        "본문"
    );
    let listed = ws.list_notes().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, ID_A, "이름이 바뀌어도 ID 유지");
}

#[test]
fn never_overwrites_another_note() {
    let f = fixture();
    let mut ws = f.open();
    ws.save_note(&note(ID_A, "", "A", "first")).unwrap();
    ws.save_note(&note(ID_B, "", "B", "second")).unwrap();
    // B를 A의 이름으로 바꾸려 하면 충돌
    let err = ws.save_note(&note(ID_B, "", "A", "second")).unwrap_err();
    assert!(
        matches!(err, CoreError::Conflict(ref p) if p == "A.md"),
        "{err}"
    );
    // 인덱스에 없는 바깥 파일도 덮어쓰지 않음
    fs::write(f.notes.join("Outside.md"), "keep").unwrap();
    let err = ws.save_note(&note(ID_B, "", "Outside", "x")).unwrap_err();
    assert!(matches!(err, CoreError::Conflict(_)));
    assert_eq!(fs::read_to_string(f.notes.join("A.md")).unwrap(), "first");
    assert_eq!(
        fs::read_to_string(f.notes.join("Outside.md")).unwrap(),
        "keep"
    );
}

#[test]
fn external_files_are_adopted_and_external_edits_detected() {
    let f = fixture();
    let mut ws = f.open();
    fs::create_dir_all(f.notes.join("BSD")).unwrap();
    fs::write(f.notes.join("BSD/from-device.md"), "v1").unwrap();
    let first = ws.list_notes().unwrap();
    assert_eq!(first.len(), 1);
    let adopted = &first[0];
    assert!(uuid_like(&adopted.id));
    assert_eq!(
        (adopted.folder.as_str(), adopted.title.as_str()),
        ("BSD", "from-device")
    );

    // 같은 파일은 다시 조회해도 같은 ID
    assert_eq!(ws.list_notes().unwrap()[0].id, adopted.id);

    // 바깥 편집 → 내용 반영
    fs::write(f.notes.join("BSD/from-device.md"), "v2 edited elsewhere").unwrap();
    let again = ws.list_notes().unwrap();
    assert_eq!(again[0].id, adopted.id);
    assert_eq!(again[0].content, "v2 edited elsewhere");

    // 바깥에서 삭제 → 목록과 인덱스에서 제거
    fs::remove_file(f.notes.join("BSD/from-device.md")).unwrap();
    assert!(ws.list_notes().unwrap().is_empty());
}

// ---------- 동기화 중인 메모의 바깥 변경 (ADR-0004) ----------

fn synced(id: &str, title: &str, content: &str, version: i64) -> NoteRecord {
    let mut n = note(id, "", title, content);
    n.sync = Some(serde_json::json!({ "version": version, "dirty": false, "error": "old" }));
    n
}

#[test]
fn external_edit_of_synced_note_is_marked_for_upload() {
    let f = fixture();
    let mut ws = f.open();
    ws.save_note(&synced(ID_A, "동기화", "v1", 3)).unwrap();
    assert_eq!(
        ws.list_notes().unwrap()[0].sync.as_ref().unwrap()["dirty"],
        false
    );
    fs::write(f.notes.join("동기화.md"), "바깥에서 고침").unwrap();
    let listed = ws.list_notes().unwrap();
    let sync = listed[0].sync.as_ref().unwrap();
    assert_eq!(listed[0].content, "바깥에서 고침");
    assert_eq!(sync["dirty"], true, "올릴 변경으로 표시");
    assert_eq!(sync["version"], 3, "서버 기준 버전은 그대로");
    assert!(sync.get("error").is_none(), "이전 거부 사유는 지움");
    // 한 번 표시한 뒤 다시 조회해도 그대로 (중복 처리 없음)
    assert_eq!(
        ws.list_notes().unwrap()[0].sync.as_ref().unwrap()["dirty"],
        true
    );
}

#[test]
fn external_rename_keeps_id_sessions_and_sync_state() {
    let f = fixture();
    let mut ws = f.open();
    ws.save_note(&synced(ID_A, "원래", "같은 내용", 2)).unwrap();
    let mut other = note(ID_B, "", "다른것", "다른 내용");
    other.sessions.clear(); // 세션 ID가 겹치지 않게
    ws.save_note(&other).unwrap();
    fs::create_dir_all(f.notes.join("옮김")).unwrap();
    fs::rename(f.notes.join("원래.md"), f.notes.join("옮김/새이름.txt")).unwrap();
    let listed = ws.list_notes().unwrap();
    assert_eq!(listed.len(), 2);
    let moved = listed.iter().find(|n| n.id == ID_A).expect("같은 ID 유지");
    assert_eq!(
        (moved.folder.as_str(), moved.title.as_str(), moved.format),
        ("옮김", "새이름", NoteFormat::Txt)
    );
    assert_eq!(moved.content, "같은 내용");
    assert_eq!(moved.sessions.len(), 1, "세션 기록 유지");
    assert_eq!(
        moved.sync.as_ref().unwrap()["dirty"],
        true,
        "이름 변경도 올릴 변경"
    );
    assert!(
        ws.list_external_deletions().unwrap().is_empty(),
        "삭제로 보지 않음"
    );
}

#[test]
fn ambiguous_external_rename_is_not_guessed() {
    let f = fixture();
    let mut ws = f.open();
    ws.save_note(&synced(ID_A, "가", "똑같음", 1)).unwrap();
    ws.save_note(&synced(ID_B, "나", "똑같음", 1)).unwrap();
    fs::remove_file(f.notes.join("가.md")).unwrap();
    fs::remove_file(f.notes.join("나.md")).unwrap();
    fs::write(f.notes.join("다.md"), "똑같음").unwrap();
    let listed = ws.list_notes().unwrap();
    assert_eq!(listed.len(), 1);
    assert!(
        listed[0].id != ID_A && listed[0].id != ID_B,
        "어느 쪽인지 모르면 새 메모"
    );
    assert_eq!(ws.list_external_deletions().unwrap().len(), 2);
}

#[test]
fn external_delete_of_synced_note_waits_for_user_choice() {
    let f = fixture();
    let mut ws = f.open();
    ws.save_note(&synced(ID_A, "서버에있음", "x", 5)).unwrap();
    let mut local_only = note(ID_B, "", "로컬전용", "y");
    local_only.sync = Some(serde_json::json!({ "version": 0, "dirty": true }));
    ws.save_note(&local_only).unwrap();
    fs::remove_file(f.notes.join("서버에있음.md")).unwrap();
    fs::remove_file(f.notes.join("로컬전용.md")).unwrap();
    assert!(ws.list_notes().unwrap().is_empty());
    let pending = ws.list_external_deletions().unwrap();
    assert_eq!(
        pending.len(),
        1,
        "서버에 올라간 적 없는 메모는 보관하지 않음"
    );
    assert_eq!(pending[0].id, ID_A);
    assert_eq!(pending[0].title, "서버에있음");
    assert_eq!(pending[0].sync["version"], 5);
    // 다시 조회해도 한 번만
    ws.list_notes().unwrap();
    assert_eq!(ws.list_external_deletions().unwrap().len(), 1);
    // 다시 받기 = 같은 ID로 저장 → 기록이 사라짐
    ws.save_note(&synced(ID_A, "서버에있음", "x", 5)).unwrap();
    assert!(ws.list_external_deletions().unwrap().is_empty());
    // 서버에서도 삭제한 경우 → 기록만 지움
    fs::remove_file(f.notes.join("서버에있음.md")).unwrap();
    ws.list_notes().unwrap();
    ws.forget_external_deletion(ID_A).unwrap();
    assert!(ws.list_external_deletions().unwrap().is_empty());
}

#[test]
fn saving_sessions_only_never_overwrites_external_edits() {
    let f = fixture();
    let mut ws = f.open();
    let mut n = synced(ID_A, "열어둠", "앱이 아는 내용", 1);
    ws.save_note(&n).unwrap();
    fs::write(f.notes.join("열어둠.md"), "바깥에서 고친 내용").unwrap();
    n.sessions[0].open_ms = 60_000; // 열어 두기만 한 메모의 주기 저장
    ws.save_sessions(ID_A, &n.sessions).unwrap();
    let listed = ws.list_notes().unwrap();
    assert_eq!(
        listed[0].content, "바깥에서 고친 내용",
        "본문을 덮어쓰지 않음"
    );
    assert_eq!(listed[0].sessions[0].open_ms, 60_000, "세션은 저장");
    assert_eq!(
        listed[0].sync.as_ref().unwrap()["dirty"],
        true,
        "바깥 편집은 올릴 변경"
    );
    ws.save_sessions("33333333-3333-4333-8333-333333333333", &n.sessions)
        .unwrap(); // 없는 메모는 무시
}

#[test]
fn sessions_are_replaced_not_duplicated() {
    let f = fixture();
    let mut ws = f.open();
    let mut n = note(ID_A, "", "S", "x");
    ws.save_note(&n).unwrap();
    n.sessions[0].open_ms = 20_000;
    n.sessions.push(EditSession {
        id: "s-2".into(),
        started_at: "2026-11-04T01:00:00.000Z".into(),
        ended_at: Some("2026-11-04T01:05:00.000Z".into()),
        active_ms: 0,
        open_ms: 300_000,
    });
    ws.save_note(&n).unwrap();
    let listed = ws.list_notes().unwrap();
    assert_eq!(listed[0].sessions, n.sessions);
}

#[test]
fn delete_note_removes_file_and_index_and_keeps_sync_state() {
    let f = fixture();
    let mut ws = f.open();
    let mut n = note(ID_A, "", "지울것", "x");
    n.sync = Some(serde_json::json!({ "version": 2, "dirty": true }));
    ws.save_note(&n).unwrap();
    assert_eq!(ws.list_notes().unwrap()[0].sync, n.sync, "동기화 상태 보존");
    ws.delete_note(ID_A).unwrap();
    assert!(!f.notes.join("지울것.md").exists());
    assert!(ws.list_notes().unwrap().is_empty());
    ws.delete_note(ID_A).unwrap(); // 두 번째 삭제도 성공
}

#[test]
fn meta_values_and_folders() {
    let f = fixture();
    let mut ws = f.open();
    assert_eq!(ws.get_meta("locale").unwrap(), None);
    ws.set_meta("locale", &serde_json::json!("en")).unwrap();
    ws.set_meta("locale", &serde_json::json!("ko")).unwrap();
    assert_eq!(
        ws.get_meta("locale").unwrap(),
        Some(serde_json::json!("ko"))
    );
    assert_eq!(ws.create_folder("Work/Project").unwrap(), "Work/Project");
    assert!(ws.create_folder("a/b/c").is_err());
    assert_eq!(ws.list_folders().unwrap(), ["Work", "Work/Project"]);
}

#[test]
fn rejects_invalid_ids_and_paths() {
    let f = fixture();
    let mut ws = f.open();
    assert!(matches!(
        ws.save_note(&note("../../etc", "", "x", "")),
        Err(CoreError::InvalidLocation(_))
    ));
    assert!(matches!(
        ws.save_note(&note(ID_A, "a/b/c", "x", "")),
        Err(CoreError::Folder(_))
    ));
}

#[test]
fn migrations_are_idempotent() {
    let f = fixture();
    drop(f.open());
    drop(f.open());
    let db = rusqlite::Connection::open(&f.index).unwrap();
    let version: i64 = db
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(version, 6);
}

fn uuid_like(s: &str) -> bool {
    s.len() == 36 && s.chars().filter(|c| *c == '-').count() == 4
}

// === 프로젝트 (ADR-0005) ===
const P_WORK: &str = "aaaaaaaa-0000-4000-8000-000000000001";

fn projects(list: &[(&str, &str)]) -> serde_json::Value {
    serde_json::Value::Array(
        list.iter()
            .map(|(id, name)| serde_json::json!({ "id": id, "name": name, "sync": { "synced": false, "dirty": true } }))
            .collect(),
    )
}

#[test]
fn project_notes_live_under_at_folder_and_follow_renames() {
    let f = fixture();
    let mut ws = f.open();
    ws.set_meta("projects", &projects(&[(P_WORK, "업무")]))
        .unwrap();
    // 기본 프로젝트와 같은 경로라도 프로젝트가 다르면 따로
    ws.save_note(&note(ID_A, "회의", "주간", "기본")).unwrap();
    let mut b = note(ID_B, "회의", "주간", "업무");
    b.project_id = Some(P_WORK.to_owned());
    ws.save_note(&b).unwrap();
    assert_eq!(
        fs::read_to_string(f.notes.join("회의/주간.md")).unwrap(),
        "기본"
    );
    assert_eq!(
        fs::read_to_string(f.notes.join("@업무/회의/주간.md")).unwrap(),
        "업무"
    );

    let listed = ws.list_notes().unwrap();
    let by_id = |id: &str| listed.iter().find(|n| n.id == id).unwrap().clone();
    assert_eq!(by_id(ID_A).project_id, None);
    assert_eq!(by_id(ID_B).project_id.as_deref(), Some(P_WORK));
    assert_eq!(by_id(ID_B).folder, "회의");

    // 폴더 목록·만들기: 화면과 같은 '<ID>::<경로>' 키
    assert_eq!(
        ws.create_folder(&format!("{P_WORK}::보관/2024")).unwrap(),
        format!("{P_WORK}::보관/2024")
    );
    let folders = ws.list_folders().unwrap();
    assert!(
        folders.contains(&format!("{P_WORK}::보관/2024")),
        "{folders:?}"
    );
    assert!(folders.contains(&"회의".to_owned()));
    assert!(!folders.iter().any(|f| f.starts_with('@')));

    // 프로젝트 이름 바꾸기 → 폴더 이름도, 메모 ID 유지
    ws.set_meta("projects", &projects(&[(P_WORK, "회사")]))
        .unwrap();
    assert!(f.notes.join("@회사/회의/주간.md").is_file());
    assert!(!f.notes.join("@업무").exists());
    let listed = ws.list_notes().unwrap();
    assert_eq!(
        listed
            .iter()
            .find(|n| n.id == ID_B)
            .unwrap()
            .project_id
            .as_deref(),
        Some(P_WORK)
    );

    // 메모를 기본 프로젝트로 옮기기 (같은 경로 메모가 있으면 충돌)
    b.project_id = None;
    assert!(matches!(ws.save_note(&b), Err(CoreError::Conflict(_))));
    b.folder = "옮김".to_owned();
    ws.save_note(&b).unwrap();
    assert!(f.notes.join("옮김/주간.md").is_file());

    // 목록에서 빠진(지워진) 빈 프로젝트 → 빈 폴더째 정리
    ws.set_meta("projects", &projects(&[])).unwrap();
    assert!(!f.notes.join("@회사").exists());
    // 모르는 프로젝트로 저장하면 거절
    b.project_id = Some(P_WORK.to_owned());
    assert!(ws.save_note(&b).is_err());
}

#[test]
fn outside_at_folder_becomes_a_project_and_keeps_note_ids_on_rename() {
    let f = fixture();
    let mut ws = f.open();
    ws.list_notes().unwrap();
    fs::create_dir_all(f.notes.join("@개인/일기")).unwrap();
    fs::write(f.notes.join("@개인/일기/오늘.md"), "맑음").unwrap();
    fs::create_dir_all(f.notes.join("@잘못:이름")).unwrap_or(()); // Windows에서는 만들 수 없음 — 있으면 무시돼야 함
    let listed = ws.list_notes().unwrap();
    let today = listed.iter().find(|n| n.title == "오늘").unwrap().clone();
    let project = today.project_id.clone().expect("새 프로젝트로 등록");
    // 화면 목록(meta)에도 올릴 변경으로 추가됨
    let meta = ws.get_meta("projects").unwrap().unwrap();
    let entry = meta
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == project.as_str())
        .unwrap()
        .clone();
    assert_eq!(
        (entry["name"].as_str(), entry["sync"]["dirty"].as_bool()),
        (Some("개인"), Some(true))
    );

    // 앱 밖에서 프로젝트 폴더 이름을 바꾸면: 새 프로젝트 + 같은 메모 ID(내용으로 대응)
    fs::rename(f.notes.join("@개인"), f.notes.join("@사적")).unwrap();
    let listed = ws.list_notes().unwrap();
    let moved = listed.iter().find(|n| n.title == "오늘").unwrap();
    assert_eq!(moved.id, today.id);
    assert_ne!(moved.project_id, Some(project));
}

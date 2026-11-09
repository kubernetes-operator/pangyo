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
    let ws = f.open();
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
    assert_eq!(version, 4);
}

fn uuid_like(s: &str) -> bool {
    s.len() == 36 && s.chars().filter(|c| *c == '-').count() == 4
}

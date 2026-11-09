//! 파일 저장소 통합 테스트 (임시 폴더 사용)
use std::fs;

use memonest_core::{CoreError, FileStore, FolderError, NoteFormat, NoteLocation};

fn store() -> (tempfile::TempDir, FileStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = FileStore::open(dir.path().join("Notes")).unwrap();
    (dir, store)
}

#[test]
fn write_read_list_roundtrip_in_human_readable_layout() {
    let (_dir, store) = store();
    let plan = NoteLocation::new("Work", "Plan", NoteFormat::Md).unwrap();
    let meeting = NoteLocation::new("Work/Project", "회의록", NoteFormat::Txt).unwrap();
    let inbox = NoteLocation::new("", "Inbox", NoteFormat::Md).unwrap();
    store.write(&plan, "# 계획\n").unwrap();
    store.write(&meeting, "한글 본문").unwrap();
    store.write(&inbox, "").unwrap();

    // 정본은 사람이 읽을 수 있는 일반 파일 (요구 02, docs/04_DATA.md)
    let raw = fs::read_to_string(store.root().join("Work/Project/회의록.txt")).unwrap();
    assert_eq!(raw, "한글 본문");
    assert_eq!(store.read(&plan).unwrap(), "# 계획\n");

    let listed: Vec<String> = store
        .list()
        .unwrap()
        .iter()
        .map(|n| n.display_path())
        .collect();
    assert_eq!(
        listed,
        ["Inbox.md", "Work/Plan.md", "Work/Project/회의록.txt"]
    );
    assert_eq!(store.folders().unwrap(), ["Work", "Work/Project"]);
}

#[test]
fn overwrite_is_atomic_and_leaves_no_temp_files() {
    let (_dir, store) = store();
    let note = NoteLocation::parse("Draft").unwrap();
    store.write(&note, "v1").unwrap();
    store.write(&note, "v2").unwrap();
    assert_eq!(store.read(&note).unwrap(), "v2");
    let names: Vec<_> = fs::read_dir(store.root())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(names, ["Draft.md"]);
}

#[test]
fn folder_depth_limit_is_enforced() {
    let (_dir, store) = store();
    assert_eq!(
        store.create_folder(" Work / Project ").unwrap(),
        "Work/Project"
    );
    let err = store.create_folder("Work/Project/Deep").unwrap_err();
    assert!(matches!(
        err,
        CoreError::Folder(FolderError::TooDeep { max: 2 })
    ));
    assert!(NoteLocation::new("a/b/c", "x", NoteFormat::Md).is_err());
}

#[test]
fn deep_folders_and_foreign_files_are_ignored() {
    let (_dir, store) = store();
    let root = store.root();
    fs::create_dir_all(root.join("A/B/C")).unwrap();
    fs::write(root.join("A/B/C/too-deep.md"), "x").unwrap();
    fs::write(root.join("A/photo.png"), "x").unwrap();
    fs::write(root.join(".hidden.md"), "x").unwrap();
    fs::write(root.join("A/ok.md"), "x").unwrap();
    let listed: Vec<String> = store
        .list()
        .unwrap()
        .iter()
        .map(|n| n.display_path())
        .collect();
    assert_eq!(listed, ["A/ok.md"]);
}

#[test]
fn parse_spec_handles_folder_and_extension() {
    let n = NoteLocation::parse("Work/Project/Meeting.txt").unwrap();
    assert_eq!(
        (n.folder.as_str(), n.title.as_str(), n.format),
        ("Work/Project", "Meeting", NoteFormat::Txt)
    );
    let n = NoteLocation::parse("v1.2 notes").unwrap(); // 지원하지 않는 확장자는 제목의 일부
    assert_eq!((n.title.as_str(), n.format), ("v1.2 notes", NoteFormat::Md));
    let n = NoteLocation::parse("../../etc/passwd").unwrap_err();
    assert!(matches!(n, CoreError::Folder(_)));
}

#[test]
fn missing_and_non_utf8_files_report_clear_errors() {
    let (_dir, store) = store();
    let note = NoteLocation::parse("missing").unwrap();
    assert!(matches!(store.read(&note), Err(CoreError::NotFound(_))));
    fs::write(store.root().join("bin.md"), [0xff, 0xfe, 0x00]).unwrap();
    let bin = NoteLocation::parse("bin").unwrap();
    assert!(matches!(store.read(&bin), Err(CoreError::NotUtf8(_))));
}

#[cfg(unix)]
#[test]
fn symlinks_are_never_followed() {
    let (dir, store) = store();
    let outside = dir.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("secret.md"), "secret").unwrap();
    std::os::unix::fs::symlink(&outside, store.root().join("Link")).unwrap();
    std::os::unix::fs::symlink(outside.join("secret.md"), store.root().join("alias.md")).unwrap();

    assert!(store.list().unwrap().is_empty());
    let via_dir = NoteLocation::parse("Link/secret").unwrap();
    assert!(matches!(store.read(&via_dir), Err(CoreError::Symlink(_))));
    assert!(matches!(
        store.write(&via_dir, "x"),
        Err(CoreError::Symlink(_))
    ));
    let via_file = NoteLocation::parse("alias").unwrap();
    assert!(matches!(
        store.write(&via_file, "x"),
        Err(CoreError::Symlink(_))
    ));
    assert_eq!(
        fs::read_to_string(outside.join("secret.md")).unwrap(),
        "secret"
    );
}

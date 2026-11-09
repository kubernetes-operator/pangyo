//! 데스크톱 작업공간 (로드맵 03단계): **`.md`/`.txt` 파일이 정본**, SQLite는 보조 인덱스.
//!
//! - 파일: [`FileStore`] (`<notes_root>/<폴더>/<제목>.<md|txt>`) — 앱 없이도 읽을 수 있음
//! - 인덱스(SQLite, 앱 데이터 폴더): 메모 UUID ↔ 경로, 내용 해시/버전, 편집 세션, Undo/Redo, 화면 설정
//!   사용자 텍스트 파일에는 아무것도 덧붙이지 않습니다 (docs/02_ARCHITECTURE.md).
//! - 바깥(BSD 앱, 다른 편집기)에서 만든 파일은 목록 조회 때 새 UUID로 등록하고,
//!   바깥에서 내용이 바뀌면 해시로 감지해 버전과 수정 시각을 올립니다. 사라진 파일은 인덱스에서 뺍니다.
//! - 제목/폴더/형식이 바뀌면 파일을 옮기며, 다른 메모 파일을 **덮어쓰지 않습니다** ([`CoreError::Conflict`]).
//!
//! 웹 화면의 `Note` 타입(apps/web/src/domain/types.ts)과 같은 JSON 모양(camelCase)을 씁니다.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    error::{CoreError, Result},
    format::NoteFormat,
    store::{FileStore, NoteLocation},
};

// === 0. 인덱스 마이그레이션: db/sqlite/*.sql 을 순서대로 적용 (PRAGMA user_version = 적용 개수) ===
const MIGRATIONS: [&str; 4] = [
    include_str!("../../../db/sqlite/001_initial.sql"),
    include_str!("../../../db/sqlite/002_session_open_time.sql"),
    include_str!("../../../db/sqlite/003_workspace.sql"),
    include_str!("../../../db/sqlite/004_sync_state.sql"),
];

/// Undo/Redo 한 칸 (웹과 같은 전체 스냅샷)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub title: String,
    pub content: String,
    pub format: NoteFormat,
}

/// 편집 세션: 작성 시간(activeMs) + 열어둔 시간(openMs) (요구 23·24)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditSession {
    pub id: String,
    pub started_at: String,
    #[serde(default)]
    pub ended_at: Option<String>,
    #[serde(default)]
    pub active_ms: u64,
    #[serde(default)]
    pub open_ms: u64,
}

/// 화면과 주고받는 메모 한 건
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteRecord {
    pub id: String,
    pub title: String,
    pub folder: String,
    pub format: NoteFormat,
    pub content: String,
    pub created_at: String,
    pub modified_at: String,
    #[serde(default)]
    pub sessions: Vec<EditSession>,
    #[serde(default)]
    pub undo: Vec<Snapshot>,
    #[serde(default)]
    pub redo: Vec<Snapshot>,
    /// 동기화 상태 (웹 `Note.sync`와 같은 JSON). 데스크톱 동기화 전까지는 보관만 합니다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync: Option<serde_json::Value>,
}

/// 인덱스의 notes 행
struct IndexRow {
    id: String,
    content_hash: String,
    version: i64,
    created_at: String,
    modified_at: String,
    sync_json: Option<String>,
}

pub struct Workspace {
    store: FileStore,
    db: Connection,
}

impl Workspace {
    /// 메모 폴더와 인덱스 파일을 엽니다 (없으면 만들고 마이그레이션 적용).
    pub fn open(notes_root: impl Into<PathBuf>, index_path: &Path) -> Result<Self> {
        let store = FileStore::open(notes_root)?;
        if let Some(parent) = index_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let db = Connection::open(index_path)?;
        db.pragma_update(None, "foreign_keys", true)?;
        migrate(&db)?;
        Ok(Self { store, db })
    }

    pub fn root(&self) -> &Path {
        self.store.root()
    }

    /// 디스크의 메모 파일을 기준으로 인덱스를 맞추고 전체 목록을 돌려줍니다.
    pub fn list_notes(&mut self) -> Result<Vec<NoteRecord>> {
        let files = self.store.list()?;
        let tx = self.db.transaction()?;
        let mut indexed = load_index(&tx)?;
        let mut out = Vec::with_capacity(files.len());
        for location in files {
            let content = match self.store.read(&location) {
                Ok(c) => c,
                Err(CoreError::NotUtf8(_)) => continue, // 텍스트가 아닌 파일은 메모로 취급하지 않음
                Err(e) => return Err(e),
            };
            let path = location.display_path();
            let hash = sha256_hex(&content);
            let row = match indexed.remove(&path) {
                Some(mut row) => {
                    if row.content_hash != hash {
                        // 바깥 편집기에서 바뀐 내용
                        row.version += 1;
                        row.content_hash = hash;
                        row.modified_at = iso(self.store.modified(&location)?);
                        tx.execute(
                            "UPDATE notes SET content_hash = ?2, version = ?3, modified_at = ?4 WHERE id = ?1",
                            params![row.id, row.content_hash, row.version, row.modified_at],
                        )?;
                    }
                    row
                }
                None => {
                    // 바깥에서 만든 파일 → 새 UUID로 등록
                    let time = iso(self.store.modified(&location)?);
                    let row = IndexRow {
                        id: uuid::Uuid::new_v4().to_string(),
                        content_hash: hash,
                        version: 1,
                        created_at: time.clone(),
                        modified_at: time,
                        sync_json: None,
                    };
                    tx.execute(
                        "INSERT INTO notes (id, path, format, content_hash, version, created_at, modified_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                        params![row.id, path, location.format.extension(), row.content_hash, row.version, row.created_at, row.modified_at],
                    )?;
                    row
                }
            };
            let (undo, redo) = load_history(&tx, &row.id)?;
            out.push(NoteRecord {
                sessions: load_sessions(&tx, &row.id)?,
                id: row.id,
                title: location.title,
                folder: location.folder,
                format: location.format,
                content,
                created_at: row.created_at,
                modified_at: row.modified_at,
                undo,
                redo,
                sync: row
                    .sync_json
                    .as_deref()
                    .map(serde_json::from_str)
                    .transpose()?,
            });
        }
        // 디스크에서 사라진 파일 → 인덱스에서 제거 (세션/Undo는 CASCADE)
        for row in indexed.values() {
            tx.execute("DELETE FROM notes WHERE id = ?1", params![row.id])?;
        }
        tx.commit()?;
        Ok(out)
    }

    /// 메모를 저장합니다. 파일을 먼저 (원자적으로) 쓰고 인덱스를 갱신합니다.
    pub fn save_note(&mut self, note: &NoteRecord) -> Result<()> {
        uuid::Uuid::parse_str(&note.id)
            .map_err(|_| CoreError::InvalidLocation(format!("잘못된 메모 ID: {}", note.id)))?;
        let target = NoteLocation::new(&note.folder, &note.title, note.format)?;
        let target_path = target.display_path();
        let tx = self.db.transaction()?;

        let owner: Option<String> = tx
            .query_row(
                "SELECT id FROM notes WHERE path = ?1",
                params![target_path],
                |r| r.get(0),
            )
            .optional()?;
        if owner.as_deref().is_some_and(|id| id != note.id) {
            return Err(CoreError::Conflict(target_path));
        }
        let existing: Option<(String, String, i64, String)> = tx
            .query_row(
                "SELECT path, content_hash, version, created_at FROM notes WHERE id = ?1",
                params![note.id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;

        match &existing {
            Some((old_path, ..)) if *old_path != target_path => {
                let old = NoteLocation::from_relative(old_path)?;
                if self.store.exists(&target) && !self.store.same_file(&old, &target) {
                    return Err(CoreError::Conflict(target_path));
                }
                // 옮긴 뒤 덮어쓰기: 중간에 실패해도 내용이 사라지지 않음 (대소문자만 바뀐 경우도 안전)
                if self.store.exists(&old) {
                    self.store.rename(&old, &target)?;
                }
            }
            Some(_) => {}
            None if self.store.exists(&target) => return Err(CoreError::Conflict(target_path)),
            None => {}
        }
        self.store.write(&target, &note.content)?;

        let hash = sha256_hex(&note.content);
        let (version, created_at) = match existing {
            Some((_, old_hash, version, created)) => (
                if old_hash == hash {
                    version
                } else {
                    version + 1
                },
                created,
            ),
            None => (1, note.created_at.clone()),
        };
        tx.execute(
            "INSERT INTO notes (id, path, format, content_hash, version, created_at, modified_at, sync_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET path = excluded.path, format = excluded.format,
               content_hash = excluded.content_hash, version = excluded.version, modified_at = excluded.modified_at,
               sync_json = excluded.sync_json",
            params![note.id, target_path, note.format.extension(), hash, version, created_at, note.modified_at,
                    note.sync.as_ref().map(serde_json::to_string).transpose()?],
        )?;
        tx.execute(
            "DELETE FROM edit_sessions WHERE note_id = ?1",
            params![note.id],
        )?;
        for s in &note.sessions {
            tx.execute(
                "INSERT INTO edit_sessions (id, note_id, started_at, ended_at, active_ms, open_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(id) DO UPDATE SET note_id = excluded.note_id, started_at = excluded.started_at,
                   ended_at = excluded.ended_at, active_ms = excluded.active_ms, open_ms = excluded.open_ms",
                params![s.id, note.id, s.started_at, s.ended_at, s.active_ms as i64, s.open_ms as i64],
            )?;
        }
        tx.execute(
            "INSERT INTO history_stacks (note_id, undo_json, redo_json) VALUES (?1, ?2, ?3)
             ON CONFLICT(note_id) DO UPDATE SET undo_json = excluded.undo_json, redo_json = excluded.redo_json",
            params![note.id, serde_json::to_string(&note.undo)?, serde_json::to_string(&note.redo)?],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// 메모 파일과 인덱스 기록을 지웁니다 (없으면 그대로 성공).
    pub fn delete_note(&mut self, id: &str) -> Result<()> {
        let path: Option<String> = self
            .db
            .query_row("SELECT path FROM notes WHERE id = ?1", params![id], |r| {
                r.get(0)
            })
            .optional()?;
        if let Some(path) = path {
            self.store.remove(&NoteLocation::from_relative(&path)?)?;
            self.db
                .execute("DELETE FROM notes WHERE id = ?1", params![id])?;
        }
        Ok(())
    }

    pub fn list_folders(&self) -> Result<Vec<String>> {
        self.store.folders()
    }

    pub fn create_folder(&self, raw: &str) -> Result<String> {
        self.store.create_folder(raw)
    }

    pub fn get_meta(&self, key: &str) -> Result<Option<serde_json::Value>> {
        let raw: Option<String> = self
            .db
            .query_row(
                "SELECT value_json FROM meta WHERE key = ?1",
                params![key],
                |r| r.get(0),
            )
            .optional()?;
        Ok(raw.map(|s| serde_json::from_str(&s)).transpose()?)
    }

    pub fn set_meta(&self, key: &str, value: &serde_json::Value) -> Result<()> {
        self.db.execute(
            "INSERT INTO meta (key, value_json) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json",
            params![key, serde_json::to_string(value)?],
        )?;
        Ok(())
    }
}

fn migrate(db: &Connection) -> Result<()> {
    let applied: i64 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
    for (index, sql) in MIGRATIONS.iter().enumerate().skip(applied.max(0) as usize) {
        db.execute_batch(sql)?;
        db.pragma_update(None, "user_version", (index + 1) as i64)?;
    }
    Ok(())
}

fn load_index(tx: &Transaction<'_>) -> Result<HashMap<String, IndexRow>> {
    let mut stmt = tx.prepare(
        "SELECT path, id, content_hash, version, created_at, modified_at, sync_json FROM notes",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            IndexRow {
                id: r.get(1)?,
                content_hash: r.get(2)?,
                version: r.get(3)?,
                created_at: r.get(4)?,
                modified_at: r.get(5)?,
                sync_json: r.get(6)?,
            },
        ))
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn load_sessions(tx: &Transaction<'_>, note_id: &str) -> Result<Vec<EditSession>> {
    let mut stmt = tx.prepare(
        "SELECT id, started_at, ended_at, active_ms, open_ms FROM edit_sessions
         WHERE note_id = ?1 ORDER BY started_at, rowid",
    )?;
    let rows = stmt.query_map(params![note_id], |r| {
        Ok(EditSession {
            id: r.get(0)?,
            started_at: r.get(1)?,
            ended_at: r.get(2)?,
            active_ms: r.get::<_, i64>(3)?.max(0) as u64,
            open_ms: r.get::<_, i64>(4)?.max(0) as u64,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn load_history(tx: &Transaction<'_>, note_id: &str) -> Result<(Vec<Snapshot>, Vec<Snapshot>)> {
    let raw: Option<(String, String)> = tx
        .query_row(
            "SELECT undo_json, redo_json FROM history_stacks WHERE note_id = ?1",
            params![note_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    match raw {
        Some((undo, redo)) => Ok((serde_json::from_str(&undo)?, serde_json::from_str(&redo)?)),
        None => Ok((Vec::new(), Vec::new())),
    }
}

fn sha256_hex(content: &str) -> String {
    Sha256::digest(content.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// JavaScript `toISOString()`과 같은 형식 (`2026-11-04T05:00:00.123Z`)
fn iso(time: SystemTime) -> String {
    humantime::format_rfc3339_millis(time).to_string()
}

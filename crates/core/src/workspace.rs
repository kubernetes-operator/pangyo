//! 데스크톱 작업공간 (로드맵 03단계): **`.md`/`.txt` 파일이 정본**, SQLite는 보조 인덱스.
//!
//! - 파일: [`FileStore`] (`<notes_root>/<폴더>/<제목>.<md|txt>`) — 앱 없이도 읽을 수 있음
//! - 인덱스(SQLite, 앱 데이터 폴더): 메모 UUID ↔ 경로, 내용 해시/버전, 편집 세션, Undo/Redo, 화면 설정
//!   사용자 텍스트 파일에는 아무것도 덧붙이지 않습니다 (docs/02_ARCHITECTURE.md).
//! - 바깥(BSD 앱, 다른 편집기)에서 만든 파일은 목록 조회 때 새 UUID로 등록하고,
//!   바깥에서 내용이 바뀌면 해시로 감지해 버전과 수정 시각을 올립니다. 사라진 파일은 인덱스에서 뺍니다.
//! - 동기화 중인 메모(ADR-0004): 바깥 편집은 '올릴 변경'(sync.dirty)으로 표시하고,
//!   같은 내용의 파일이 사라지면서 새로 생기면 바깥 이름 변경으로 보고 ID를 유지합니다.
//!   서버에 올라간 메모 파일이 사라지면 [`ExternalDeletion`]으로 보관해 사용자가 고르게 합니다.
//! - 제목/폴더/형식이 바뀌면 파일을 옮기며, 다른 메모 파일을 **덮어쓰지 않습니다** ([`CoreError::Conflict`]).
//!
//! - 프로젝트(ADR-0005): 기본 프로젝트는 루트, 다른 프로젝트는 `@<이름>/` 폴더. 화면이 meta `projects`를 저장할 때마다
//!   ID ↔ 이름 표와 폴더 이름(이름 바꾸기)을 맞춥니다. 앱 밖에서 만든 `@이름` 폴더는 새 프로젝트로 등록합니다.
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
const MIGRATIONS: [&str; 6] = [
    include_str!("../../../db/sqlite/001_initial.sql"),
    include_str!("../../../db/sqlite/002_session_open_time.sql"),
    include_str!("../../../db/sqlite/003_workspace.sql"),
    include_str!("../../../db/sqlite/004_sync_state.sql"),
    include_str!("../../../db/sqlite/005_external_deletions.sql"),
    include_str!("../../../db/sqlite/006_projects.sql"),
];
/// 화면이 프로젝트 목록을 저장하는 meta 키 (apps/web/src/domain/projects.ts PROJECTS_META_KEY)
pub const PROJECTS_META_KEY: &str = "projects";
/// 화면의 프로젝트별 폴더 키 구분자 (`<프로젝트ID>::<경로>`, projects.ts folderKey)
const FOLDER_KEY_SEPARATOR: &str = "::";

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
    /// 프로젝트 ID (없음 = 기본, ADR-0005)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
}

/// 앱 밖에서 지워진, 서버에 올라간 적 있는 메모 (사용자가 서버 삭제/다시 받기를 고를 때까지 보관)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalDeletion {
    pub id: String,
    pub title: String,
    pub folder: String,
    pub format: NoteFormat,
    /// 지워지기 직전의 동기화 상태 (웹 `Note.sync`와 같은 JSON, `version` = 서버 삭제 시 baseVersion)
    pub sync: serde_json::Value,
    pub detected_at: String,
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
        self.adopt_project_dirs()?;
        let ids_by_name: HashMap<String, String> = self
            .project_names()?
            .into_iter()
            .map(|(id, name)| (name, id))
            .collect();
        // 이름표에 없는 프로젝트 폴더(규칙에 맞지 않는 등)의 파일은 건드리지 않음
        let files: Vec<NoteLocation> = self
            .store
            .list()?
            .into_iter()
            .filter(|l| l.project.is_empty() || ids_by_name.contains_key(&l.project))
            .collect();
        let tx = self.db.transaction()?;
        let mut indexed = load_index(&tx)?;
        // 1) 경로가 그대로인 파일: 바깥 편집 감지
        let mut found: Vec<(NoteLocation, String, IndexRow)> = Vec::with_capacity(files.len());
        let mut unknown: Vec<(NoteLocation, String, String)> = Vec::new(); // (위치, 내용, 해시)
        for location in files {
            let content = match self.store.read(&location) {
                Ok(c) => c,
                Err(CoreError::NotUtf8(_)) => continue, // 텍스트가 아닌 파일은 메모로 취급하지 않음
                Err(e) => return Err(e),
            };
            let hash = sha256_hex(&content);
            match indexed.remove(&location.display_path()) {
                Some(mut row) => {
                    if row.content_hash != hash {
                        // 바깥 편집기에서 바뀐 내용 → 동기화 중이면 올릴 변경으로 표시
                        row.version += 1;
                        row.content_hash = hash;
                        row.modified_at = iso(self.store.modified(&location)?);
                        row.sync_json = mark_sync_dirty(row.sync_json.as_deref())?;
                        tx.execute(
                            "UPDATE notes SET content_hash = ?2, version = ?3, modified_at = ?4, sync_json = ?5
                             WHERE id = ?1",
                            params![row.id, row.content_hash, row.version, row.modified_at, row.sync_json],
                        )?;
                    }
                    found.push((location, content, row));
                }
                None => unknown.push((location, content, hash)),
            }
        }
        // 2) 사라진 행과 새 파일의 내용이 1:1로 같으면 바깥 이름 변경 → ID·세션·동기화 상태 유지
        let mut missing: Vec<(String, IndexRow)> = indexed.into_iter().collect();
        missing.sort_by(|a, b| a.0.cmp(&b.0));
        let count = |hash: &str, list: &[(NoteLocation, String, String)]| {
            list.iter().filter(|(_, _, h)| h == hash).count()
        };
        let mut renamed = vec![false; unknown.len()];
        for (index, (location, content, hash)) in unknown.iter().enumerate() {
            let candidates: Vec<usize> = missing
                .iter()
                .enumerate()
                .filter(|(_, (_, row))| row.content_hash == *hash)
                .map(|(i, _)| i)
                .collect();
            if candidates.len() != 1 || count(hash, &unknown) != 1 {
                continue; // 애매하면 추측하지 않음 (새 메모 + 사라진 메모로 처리)
            }
            let (_, mut row) = missing.remove(candidates[0]);
            row.modified_at = iso(self.store.modified(location)?);
            row.sync_json = mark_sync_dirty(row.sync_json.as_deref())?;
            tx.execute(
                "UPDATE notes SET path = ?2, format = ?3, modified_at = ?4, sync_json = ?5 WHERE id = ?1",
                params![row.id, location.display_path(), location.format.extension(), row.modified_at, row.sync_json],
            )?;
            renamed[index] = true;
            found.push((location.clone(), content.clone(), row));
        }
        // 3) 처음 보는 파일 → 새 UUID로 등록
        for ((location, content, hash), was_renamed) in unknown.into_iter().zip(renamed) {
            if was_renamed {
                continue;
            }
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
                params![row.id, location.display_path(), location.format.extension(), row.content_hash, row.version, row.created_at, row.modified_at],
            )?;
            found.push((location, content, row));
        }
        // 4) 디스크에서 사라진 파일 → 인덱스에서 제거 (세션/Undo는 CASCADE).
        //    서버에 올라간 적 있는 메모는 사용자가 고를 때까지 external_deletions에 보관
        let now = iso(SystemTime::now());
        for (path, row) in &missing {
            if let Some(sync) = row.sync_json.as_deref().filter(|s| synced_version(s) > 0) {
                let format = NoteLocation::from_relative(path)
                    .map(|l| l.format.extension())
                    .unwrap_or("md");
                tx.execute(
                    "INSERT INTO external_deletions (id, path, format, sync_json, detected_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(id) DO UPDATE SET path = excluded.path, format = excluded.format,
                       sync_json = excluded.sync_json",
                    params![row.id, path, format, sync, now],
                )?;
            }
            tx.execute("DELETE FROM notes WHERE id = ?1", params![row.id])?;
        }
        let mut out = Vec::with_capacity(found.len());
        for (location, content, row) in found {
            let (undo, redo) = load_history(&tx, &row.id)?;
            let project_id = ids_by_name.get(&location.project).cloned();
            out.push(NoteRecord {
                project_id,
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
        tx.commit()?;
        Ok(out)
    }

    /// 앱 밖에서 지워진 동기화 메모 목록 (오래된 것부터)
    pub fn list_external_deletions(&self) -> Result<Vec<ExternalDeletion>> {
        let mut stmt = self.db.prepare(
            "SELECT id, path, sync_json, detected_at FROM external_deletions ORDER BY detected_at, id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, path, sync, detected_at) = row?;
            let location = NoteLocation::from_relative(&path)?;
            out.push(ExternalDeletion {
                id,
                title: location.title,
                folder: location.folder,
                format: location.format,
                sync: serde_json::from_str(&sync)?,
                detected_at,
            });
        }
        Ok(out)
    }

    /// 사용자가 처리한(서버에서 삭제 / 다시 받기) 항목을 목록에서 뺍니다.
    pub fn forget_external_deletion(&self, id: &str) -> Result<()> {
        self.db
            .execute("DELETE FROM external_deletions WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// 메모를 저장합니다. 파일을 먼저 (원자적으로) 쓰고 인덱스를 갱신합니다.
    pub fn save_note(&mut self, note: &NoteRecord) -> Result<()> {
        uuid::Uuid::parse_str(&note.id)
            .map_err(|_| CoreError::InvalidLocation(format!("잘못된 메모 ID: {}", note.id)))?;
        let project = self.project_name_of(note.project_id.as_deref())?;
        let target =
            NoteLocation::new(&note.folder, &note.title, note.format)?.in_project(&project)?;
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
        write_sessions(&tx, &note.id, &note.sessions)?;
        tx.execute(
            "INSERT INTO history_stacks (note_id, undo_json, redo_json) VALUES (?1, ?2, ?3)
             ON CONFLICT(note_id) DO UPDATE SET undo_json = excluded.undo_json, redo_json = excluded.redo_json",
            params![note.id, serde_json::to_string(&note.undo)?, serde_json::to_string(&note.redo)?],
        )?;
        // 같은 메모가 다시 저장되면(다시 받기, 서버에서 내려옴) 바깥 삭제 기록은 의미가 없음
        tx.execute(
            "DELETE FROM external_deletions WHERE id = ?1",
            params![note.id],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// 편집 세션 기록만 저장합니다 (본문은 건드리지 않음). 열어 두기만 한 메모의 주기 저장에 씁니다.
    /// 본문까지 다시 쓰면 그 사이 바깥 편집기에서 고친 내용을 덮어쓰기 때문입니다 (ADR-0004).
    /// 인덱스에 없는 메모(바깥에서 지워짐 등)는 아무것도 하지 않습니다.
    pub fn save_sessions(&mut self, id: &str, sessions: &[EditSession]) -> Result<()> {
        let tx = self.db.transaction()?;
        let known: Option<i64> = tx
            .query_row("SELECT 1 FROM notes WHERE id = ?1", params![id], |r| {
                r.get(0)
            })
            .optional()?;
        if known.is_some() {
            write_sessions(&tx, id, sessions)?;
        }
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

    /// 폴더 목록. 프로젝트 폴더 안은 화면과 같은 `<프로젝트ID>::<경로>` 키
    pub fn list_folders(&self) -> Result<Vec<String>> {
        let names = self.project_names()?;
        let ids: HashMap<&str, &str> = names
            .iter()
            .map(|(id, n)| (n.as_str(), id.as_str()))
            .collect();
        let mut out = Vec::new();
        for folder in self.store.folders()? {
            match folder.strip_prefix(crate::store::PROJECT_DIR_PREFIX) {
                None => out.push(folder),
                Some(rest) => {
                    if let Some((project, path)) = rest.split_once('/') {
                        if let Some(id) = ids.get(project) {
                            out.push(format!("{id}{FOLDER_KEY_SEPARATOR}{path}"));
                        }
                    }
                }
            }
        }
        Ok(out)
    }

    /// 폴더 만들기. `<프로젝트ID>::<경로>`면 그 프로젝트 안에 (화면과 같은 키를 돌려줌)
    pub fn create_folder(&self, raw: &str) -> Result<String> {
        match raw.split_once(FOLDER_KEY_SEPARATOR) {
            None => self.store.create_folder(raw),
            Some((id, path)) => {
                let project = self.project_name_of(Some(id))?;
                let folder = self.store.create_project_folder(&project, path)?;
                Ok(format!("{id}{FOLDER_KEY_SEPARATOR}{folder}"))
            }
        }
    }

    /// 프로젝트 ID → 이름 (없음 = 기본 `""`). 모르는 ID면 오류 (프로젝트 목록을 먼저 저장해야 함)
    fn project_name_of(&self, id: Option<&str>) -> Result<String> {
        let Some(id) = id.filter(|i| !i.is_empty()) else {
            return Ok(String::new());
        };
        self.db
            .query_row(
                "SELECT name FROM projects WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| CoreError::InvalidLocation(format!("알 수 없는 프로젝트: {id}")))
    }

    /// (ID, 이름) 목록
    pub fn project_names(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self
            .db
            .prepare("SELECT id, name FROM projects ORDER BY name")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// 화면이 저장한 프로젝트 목록에 맞춤: 새 프로젝트 등록, 이름 바꾸기(폴더 이름도), 빠진 프로젝트 정리(빈 폴더만 지움)
    fn apply_projects(&mut self, value: &serde_json::Value) -> Result<()> {
        let wanted: Vec<(String, String)> = value
            .as_array()
            .map(|list| {
                list.iter()
                    .filter_map(|p| {
                        Some((
                            p.get("id")?.as_str()?.to_owned(),
                            p.get("name")?.as_str()?.to_owned(),
                        ))
                    })
                    .filter(|(id, name)| !id.is_empty() && crate::store::valid_project_name(name))
                    .collect()
            })
            .unwrap_or_default();
        let current: HashMap<String, String> = self.project_names()?.into_iter().collect();
        let tx = self.db.transaction()?;
        for (id, name) in &wanted {
            match current.get(id) {
                Some(old) if old == name => {}
                Some(old) => {
                    self.store.rename_project(old, name)?;
                    let (from, to) = (
                        format!("{}{old}/", crate::store::PROJECT_DIR_PREFIX),
                        format!("{}{name}/", crate::store::PROJECT_DIR_PREFIX),
                    );
                    for table in ["notes", "external_deletions"] {
                        tx.execute(
                            &format!(
                                "UPDATE {table} SET path = ?2 || substr(path, length(?1) + 1)
                                 WHERE substr(path, 1, length(?1)) = ?1"
                            ),
                            params![from, to],
                        )?;
                    }
                    tx.execute(
                        "UPDATE projects SET name = ?2 WHERE id = ?1",
                        params![id, name],
                    )?;
                }
                None => {
                    // 같은 이름이 다른 ID로 남아 있으면(지운 뒤 다시 만듦 등) 그 행을 바꿈
                    tx.execute(
                        "DELETE FROM projects WHERE lower(name) = lower(?1)",
                        params![name],
                    )?;
                    tx.execute(
                        "INSERT INTO projects (id, name) VALUES (?1, ?2)",
                        params![id, name],
                    )?;
                }
            }
        }
        for (id, name) in &current {
            if !wanted.iter().any(|(w, _)| w == id) {
                tx.execute("DELETE FROM projects WHERE id = ?1", params![id])?;
                self.store.remove_project_dir_if_empty(name)?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// 앱 밖에서 만든 `@이름` 폴더 → 새 프로젝트로 등록하고 화면 목록(meta)에도 추가 (올릴 변경으로 표시)
    fn adopt_project_dirs(&mut self) -> Result<()> {
        let known: Vec<String> = self
            .project_names()?
            .into_iter()
            .map(|(_, n)| n.to_lowercase())
            .collect();
        let new: Vec<String> = self
            .store
            .project_dirs()?
            .into_iter()
            .filter(|n| !known.contains(&n.to_lowercase()))
            .collect();
        if new.is_empty() {
            return Ok(());
        }
        let mut list = match self.get_meta(PROJECTS_META_KEY)? {
            Some(serde_json::Value::Array(list)) => list,
            _ => Vec::new(),
        };
        let now = iso(SystemTime::now());
        for name in new {
            let id = uuid::Uuid::new_v4().to_string();
            self.db.execute(
                "INSERT INTO projects (id, name) VALUES (?1, ?2)",
                params![id, name],
            )?;
            list.push(serde_json::json!({
                "id": id, "name": name, "createdAt": now, "modifiedAt": now,
                "sync": { "synced": false, "dirty": true },
            }));
        }
        self.put_meta(PROJECTS_META_KEY, &serde_json::Value::Array(list))
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

    /// 화면 설정 저장. `projects`(프로젝트 목록)면 ID ↔ 이름 표와 폴더 이름도 맞춤 (ADR-0005)
    pub fn set_meta(&mut self, key: &str, value: &serde_json::Value) -> Result<()> {
        if key == PROJECTS_META_KEY {
            self.apply_projects(value)?;
        }
        self.put_meta(key, value)
    }

    fn put_meta(&self, key: &str, value: &serde_json::Value) -> Result<()> {
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

fn write_sessions(tx: &Transaction<'_>, note_id: &str, sessions: &[EditSession]) -> Result<()> {
    tx.execute(
        "DELETE FROM edit_sessions WHERE note_id = ?1",
        params![note_id],
    )?;
    for s in sessions {
        tx.execute(
            "INSERT INTO edit_sessions (id, note_id, started_at, ended_at, active_ms, open_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET note_id = excluded.note_id, started_at = excluded.started_at,
               ended_at = excluded.ended_at, active_ms = excluded.active_ms, open_ms = excluded.open_ms",
            params![s.id, note_id, s.started_at, s.ended_at, s.active_ms as i64, s.open_ms as i64],
        )?;
    }
    Ok(())
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

/// 동기화 상태 JSON에 '올릴 변경' 표시 (웹 SyncEngine.markDirty와 같음: 서버 거부 사유는 지움)
fn mark_sync_dirty(sync: Option<&str>) -> Result<Option<String>> {
    let Some(raw) = sync else { return Ok(None) };
    let mut value: serde_json::Value = serde_json::from_str(raw)?;
    if let Some(map) = value.as_object_mut() {
        map.insert("dirty".into(), serde_json::Value::Bool(true));
        map.remove("error");
    }
    Ok(Some(serde_json::to_string(&value)?))
}

/// 동기화 상태의 서버 버전 (0 = 아직 서버에 없음)
fn synced_version(sync: &str) -> i64 {
    serde_json::from_str::<serde_json::Value>(sync)
        .ok()
        .and_then(|v| v.get("version")?.as_i64())
        .unwrap_or(0)
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

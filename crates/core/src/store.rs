//! 로컬 파일 저장소: `<root>/<폴더 0~2단계>/<제목>.<md|txt>` 파일이 정본입니다 (docs/02_ARCHITECTURE.md).
//! 프로젝트(ADR-0005): 기본 프로젝트는 루트 바로 아래, 다른 프로젝트는 `<root>/@<프로젝트 이름>/<폴더 0~2단계>/…`.
//! 맨 위의 `@`로 시작하는 폴더만 프로젝트로 읽습니다 (일반 폴더 이름은 `@`로 시작할 수 없음, paths.json).
//!
//! 안전 규칙
//! - 모든 경로는 [`parse_folder`]/[`safe_filename`] 규칙을 통과한 이름으로만 만듭니다 (`..` 탈출 불가).
//! - 루트 안의 심볼릭 링크는 따라가지 않습니다 (목록에서 제외, 읽기/쓰기 거부).
//! - 쓰기는 같은 폴더의 임시 파일에 쓴 뒤 fsync + rename 하여 전원 차단 시 반쯤 쓰인 파일을 남기지 않습니다.
//! - 이름이 `.`으로 시작하는 파일/폴더(임시 파일 포함)는 목록에서 숨깁니다.
//!
//! UUID ↔ 경로 인덱스, 편집 세션, Undo 기록(SQLite)은 후속 단계에서 추가합니다.

use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{
    error::{CoreError, Result},
    format::NoteFormat,
    paths::{folder_ancestors, parse_folder, safe_filename},
    MAX_FOLDER_DEPTH,
};

/// 임시 파일 이름 충돌 방지용 카운터 (같은 프로세스 안)
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// 프로젝트 폴더 표시 (`@업무`)
pub const PROJECT_DIR_PREFIX: char = '@';

/// 저장소 안에서 메모 하나의 위치. 폴더는 `"A/B"`, 루트는 `""`. 프로젝트는 이름(`""` = 기본).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NoteLocation {
    pub project: String,
    pub folder: String,
    pub title: String,
    pub format: NoteFormat,
}

impl NoteLocation {
    /// 사용자 입력으로 위치를 만듭니다. 폴더는 검증하고(실패 시 오류), 제목은 안전한 이름으로 바꿉니다.
    pub fn new(folder: &str, title: &str, format: NoteFormat) -> Result<Self> {
        Ok(Self {
            project: String::new(),
            folder: parse_folder(folder, MAX_FOLDER_DEPTH)?,
            title: safe_filename(title),
            format,
        })
    }

    /// 프로젝트를 정합니다 (`""` = 기본). 이름은 폴더 한 칸 규칙을 따라야 합니다.
    pub fn in_project(mut self, project: &str) -> Result<Self> {
        if !project.is_empty() && !valid_project_name(project) {
            return Err(CoreError::InvalidLocation(format!(
                "{PROJECT_DIR_PREFIX}{project}"
            )));
        }
        self.project = project.to_owned();
        Ok(self)
    }

    /// `"Work/Plan.md"` 형태 문자열을 위치로 바꿉니다. 확장자가 없으면 `.md`.
    pub fn parse(spec: &str) -> Result<Self> {
        let spec = spec.trim().replace('\\', "/");
        let (folder, file) = spec.rsplit_once('/').unwrap_or(("", spec.as_str()));
        let (title, format) = match file.rsplit_once('.') {
            Some((stem, ext)) => match NoteFormat::from_extension(ext) {
                Some(format) => (stem, format),
                None => (file, NoteFormat::Md),
            },
            None => (file, NoteFormat::Md),
        };
        Self::new(folder, title, format)
    }

    /// 인덱스에 저장된 상대 경로(`"Work/Plan.md"`)를 그대로 위치로 바꿉니다 (이름 정규화 없음, 안전성만 검사).
    pub fn from_relative(path: &str) -> Result<Self> {
        let invalid = || CoreError::InvalidLocation(path.to_owned());
        let (project, rest) = match path.strip_prefix(PROJECT_DIR_PREFIX) {
            Some(tail) => tail.split_once('/').ok_or_else(invalid)?,
            None => ("", path),
        };
        let (folder, file) = rest.rsplit_once('/').unwrap_or(("", rest));
        let (title, ext) = file.rsplit_once('.').ok_or_else(invalid)?;
        let format = NoteFormat::from_extension(ext).ok_or_else(invalid)?;
        let location = Self {
            project: project.to_owned(),
            folder: folder.to_owned(),
            title: title.to_owned(),
            format,
        };
        location.validate()?;
        Ok(location)
    }

    pub fn file_name(&self) -> String {
        format!("{}.{}", self.title, self.format)
    }

    /// 사람이 읽는 상대 경로 `"Work/Plan.md"`, 프로젝트면 `"@업무/Work/Plan.md"`
    pub fn display_path(&self) -> String {
        let inner = if self.folder.is_empty() {
            self.file_name()
        } else {
            format!("{}/{}", self.folder, self.file_name())
        };
        if self.project.is_empty() {
            inner
        } else {
            format!("{PROJECT_DIR_PREFIX}{}/{inner}", self.project)
        }
    }

    /// 디스크에서 읽은 이름 등 외부 값으로 만든 위치가 저장소 밖을 가리키지 않는지 확인합니다.
    fn validate(&self) -> Result<()> {
        let bad_component =
            |s: &str| s.is_empty() || s == "." || s == ".." || s.contains(['/', '\\', '\0']);
        let folder_ok = self.folder.is_empty()
            || (self.folder.split('/').count() <= MAX_FOLDER_DEPTH
                && !self.folder.split('/').any(bad_component));
        let project_ok = self.project.is_empty() || valid_project_name(&self.project);
        if folder_ok && project_ok && !bad_component(&self.title) {
            Ok(())
        } else {
            Err(CoreError::InvalidLocation(self.display_path()))
        }
    }
}

/// `.md`/`.txt` 파일 저장소
#[derive(Debug, Clone)]
pub struct FileStore {
    root: PathBuf,
}

impl FileStore {
    /// 루트 폴더를 (없으면 만들어) 엽니다.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        Ok(Self {
            root: root.canonicalize()?,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 폴더를 만듭니다 (상위 폴더 포함). 정규화된 폴더 경로를 돌려줍니다.
    pub fn create_folder(&self, raw: &str) -> Result<String> {
        let folder = parse_folder(raw, MAX_FOLDER_DEPTH)?;
        self.ensure_folder(&folder)?;
        Ok(folder)
    }

    /// 프로젝트 안에 폴더를 만듭니다 (`project` = 이름, `""` = 기본).
    pub fn create_project_folder(&self, project: &str, raw: &str) -> Result<String> {
        if !project.is_empty() && !valid_project_name(project) {
            return Err(CoreError::InvalidLocation(format!(
                "{PROJECT_DIR_PREFIX}{project}"
            )));
        }
        let folder = parse_folder(raw, MAX_FOLDER_DEPTH)?;
        self.ensure_folder_in(project, &folder)?;
        Ok(folder)
    }

    /// 디스크의 프로젝트 폴더 이름 목록 (`@` 제외, 정렬됨)
    pub fn project_dirs(&self) -> Result<Vec<String>> {
        let mut out = Vec::new();
        self.walk(&mut |kind, _| {
            if let Entry::Project(name) = kind {
                out.push(name);
            }
        })?;
        out.sort();
        Ok(out)
    }

    /// 프로젝트 폴더 이름 바꾸기 (`@from` → `@to`). 없으면 아무것도 하지 않음, 대상이 있으면 충돌.
    pub fn rename_project(&self, from: &str, to: &str) -> Result<()> {
        if !valid_project_name(from) || !valid_project_name(to) {
            return Err(CoreError::InvalidLocation(format!(
                "{PROJECT_DIR_PREFIX}{to}"
            )));
        }
        let source = self.project_dir(from);
        let target = self.project_dir(to);
        reject_symlink(&source)?;
        reject_symlink(&target)?;
        if !source.is_dir() {
            return Ok(());
        }
        let case_only = from.to_lowercase() == to.to_lowercase();
        if target.exists() && !case_only {
            return Err(CoreError::Conflict(format!("{PROJECT_DIR_PREFIX}{to}")));
        }
        fs::rename(&source, &target)?;
        sync_dir(&self.root)?;
        Ok(())
    }

    /// 프로젝트 폴더에 메모 파일이 없으면 (빈 하위 폴더째) 지웁니다. 파일이 남아 있으면 그대로 둡니다.
    pub fn remove_project_dir_if_empty(&self, name: &str) -> Result<()> {
        if !valid_project_name(name) {
            return Ok(());
        }
        let dir = self.project_dir(name);
        reject_symlink(&dir)?;
        if dir.is_dir() {
            remove_empty_dirs(&dir)?;
        }
        Ok(())
    }

    /// 폴더가 비어 있으면(빈 하위 폴더째) 지웁니다. 메모 파일이 있으면 그대로 둡니다.
    pub fn remove_folder_if_empty(&self, project: &str, raw: &str) -> Result<()> {
        let folder = parse_folder(raw, MAX_FOLDER_DEPTH)?;
        if folder.is_empty() {
            return Ok(()); // 루트는 지우지 않음
        }
        let base = if project.is_empty() {
            self.root.clone()
        } else {
            if !valid_project_name(project) {
                return Err(CoreError::InvalidLocation(format!(
                    "{PROJECT_DIR_PREFIX}{project}"
                )));
            }
            self.project_dir(project)
        };
        let mut dir = base.clone();
        for part in folder_ancestors(&folder) {
            dir = base.join(part);
            reject_symlink(&dir)?;
        }
        if dir.is_dir() {
            remove_empty_dirs(&dir)?;
        }
        Ok(())
    }

    fn project_dir(&self, name: &str) -> PathBuf {
        self.root.join(format!("{PROJECT_DIR_PREFIX}{name}"))
    }

    /// 폴더 목록 (깊이 1~2, 정렬됨). 프로젝트 폴더 안은 `"@업무/Work"` 형태
    pub fn folders(&self) -> Result<Vec<String>> {
        let mut out = Vec::new();
        self.walk(&mut |kind, rel| {
            if let Entry::Folder = kind {
                out.push(rel.to_owned());
            }
        })?;
        out.sort();
        Ok(out)
    }

    /// 메모 목록 (정렬됨). 3단계 이상 깊은 폴더와 다른 확장자 파일은 무시합니다.
    pub fn list(&self) -> Result<Vec<NoteLocation>> {
        let mut out = Vec::new();
        self.walk(&mut |kind, rel| {
            if let Entry::Note(location) = kind {
                debug_assert_eq!(location.display_path(), rel);
                out.push(location);
            }
        })?;
        out.sort();
        Ok(out)
    }

    pub fn exists(&self, location: &NoteLocation) -> bool {
        self.checked_path(location)
            .map(|p| p.is_file())
            .unwrap_or(false)
    }

    /// 메모 본문을 읽습니다 (UTF-8 필수).
    pub fn read(&self, location: &NoteLocation) -> Result<String> {
        let path = self.checked_path(location)?;
        let bytes = match fs::read(&path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Err(CoreError::NotFound(location.display_path()))
            }
            other => other?,
        };
        String::from_utf8(bytes).map_err(|_| CoreError::NotUtf8(path))
    }

    /// 메모를 원자적으로 저장합니다 (폴더가 없으면 만듦).
    pub fn write(&self, location: &NoteLocation, content: &str) -> Result<()> {
        location.validate()?;
        let dir = self.ensure_folder_in(&location.project, &location.folder)?;
        let target = dir.join(location.file_name());
        reject_symlink(&target)?;
        let temp = dir.join(format!(
            ".{}.tmp-{}-{}",
            location.file_name(),
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let result = (|| {
            let mut file = File::create(&temp)?;
            file.write_all(content.as_bytes())?;
            file.sync_all()?;
            fs::rename(&temp, &target)?;
            sync_dir(&dir)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        Ok(result?)
    }

    /// 메모 파일을 지웁니다 (없으면 무시).
    pub fn remove(&self, location: &NoteLocation) -> Result<()> {
        let path = self.checked_path(location)?;
        match fs::remove_file(&path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            other => Ok(other?),
        }
    }

    /// 메모 파일 이름을 바꿉니다. 대상 폴더가 없으면 만듭니다. 대상이 있으면 덮어쓰므로 호출 쪽에서 충돌을 확인하세요.
    pub fn rename(&self, from: &NoteLocation, to: &NoteLocation) -> Result<()> {
        let source = self.checked_path(from)?;
        to.validate()?;
        let dir = self.ensure_folder_in(&to.project, &to.folder)?;
        let target = dir.join(to.file_name());
        reject_symlink(&target)?;
        fs::rename(&source, &target)?;
        sync_dir(&dir)?;
        Ok(())
    }

    /// 파일 수정 시각
    pub fn modified(&self, location: &NoteLocation) -> Result<std::time::SystemTime> {
        Ok(fs::metadata(self.checked_path(location)?)?.modified()?)
    }

    /// 두 위치가 디스크에서 같은 파일인지 (대소문자를 구분하지 않는 파일시스템에서 이름 대소문자만 다른 경우 등)
    #[cfg(feature = "workspace")]
    pub fn same_file(&self, a: &NoteLocation, b: &NoteLocation) -> bool {
        match (self.checked_path(a), self.checked_path(b)) {
            (Ok(a), Ok(b)) => same_file::is_same_file(a, b).unwrap_or(false),
            _ => false,
        }
    }

    /// 위치 검증 + 경로 각 단계가 심볼릭 링크가 아닌지 확인한 실제 경로
    fn checked_path(&self, location: &NoteLocation) -> Result<PathBuf> {
        location.validate()?;
        let base = if location.project.is_empty() {
            self.root.clone()
        } else {
            let dir = self.project_dir(&location.project);
            reject_symlink(&dir)?;
            dir
        };
        let mut path = base.clone();
        for part in folder_ancestors(&location.folder) {
            path = base.join(part);
            reject_symlink(&path)?;
        }
        let file = path.join(location.file_name());
        reject_symlink(&file)?;
        Ok(file)
    }

    /// 폴더를 한 단계씩 만들며, 이미 있는 단계가 심볼릭 링크면 거부합니다.
    fn ensure_folder(&self, folder: &str) -> Result<PathBuf> {
        self.ensure_folder_in("", folder)
    }

    /// 프로젝트 폴더(있으면)부터 한 단계씩 만듭니다.
    fn ensure_folder_in(&self, project: &str, folder: &str) -> Result<PathBuf> {
        let base = if project.is_empty() {
            self.root.clone()
        } else {
            let dir = self.project_dir(project);
            reject_symlink(&dir)?;
            match fs::create_dir(&dir) {
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists && dir.is_dir() => {}
                other => other?,
            }
            dir
        };
        let mut path = base.clone();
        for part in folder_ancestors(folder) {
            path = base.join(&part);
            reject_symlink(&path)?;
            match fs::create_dir(&path) {
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists && path.is_dir() => {}
                other => other?,
            }
        }
        Ok(path)
    }

    /// 루트부터 깊이 2까지 폴더와 메모를 방문합니다. 숨김 항목과 심볼릭 링크는 건너뜁니다.
    /// 루트의 `@이름` 폴더는 프로젝트로 보고 그 안을 같은 규칙(깊이 2)으로 방문합니다.
    fn walk(&self, visit: &mut dyn FnMut(Entry, &str)) -> Result<()> {
        fn inner(
            dir: &Path,
            project: &str,
            rel: &str,
            depth: usize,
            visit: &mut dyn FnMut(Entry, &str),
        ) -> Result<()> {
            for item in fs::read_dir(dir)? {
                let item = item?;
                let Ok(name) = item.file_name().into_string() else {
                    continue; // UTF-8이 아닌 이름은 다루지 않음
                };
                if name.starts_with('.') {
                    continue;
                }
                let kind = item.file_type()?;
                if kind.is_dir() && project.is_empty() && depth == 0 {
                    if let Some(project_name) = name.strip_prefix(PROJECT_DIR_PREFIX) {
                        if valid_project_name(project_name) {
                            visit(Entry::Project(project_name.to_owned()), &name);
                            inner(&item.path(), project_name, "", 0, visit)?;
                        }
                        continue; // 규칙에 맞지 않는 `@` 폴더는 무시
                    }
                }
                let child_rel = if rel.is_empty() {
                    name.clone()
                } else {
                    format!("{rel}/{name}")
                };
                let shown = |r: &str| {
                    if project.is_empty() {
                        r.to_owned()
                    } else {
                        format!("{PROJECT_DIR_PREFIX}{project}/{r}")
                    }
                };
                if kind.is_dir() && depth < MAX_FOLDER_DEPTH {
                    visit(Entry::Folder, &shown(&child_rel));
                    inner(&item.path(), project, &child_rel, depth + 1, visit)?;
                } else if kind.is_file() {
                    let Some((stem, ext)) = name.rsplit_once('.') else {
                        continue;
                    };
                    let Some(format) = NoteFormat::from_extension(ext) else {
                        continue;
                    };
                    if ext != format.extension() {
                        continue; // 대문자 확장자(.MD)는 같은 이름 규칙으로 다시 열 수 없으므로 제외
                    }
                    let location = NoteLocation {
                        project: project.to_owned(),
                        folder: rel.to_owned(),
                        title: stem.to_owned(),
                        format,
                    };
                    let display = location.display_path();
                    visit(Entry::Note(location), &display);
                }
            }
            Ok(())
        }
        inner(&self.root, "", "", 0, visit)
    }
}

enum Entry {
    Folder,
    Project(String),
    Note(NoteLocation),
}

/// 프로젝트 이름(폴더 표시 `@` 제외)이 폴더 한 칸 규칙에 맞는지 (정규화해도 그대로여야 함)
pub fn valid_project_name(name: &str) -> bool {
    !name.is_empty() && parse_folder(name, 1).is_ok_and(|n| n == name)
}

/// 아래쪽부터 빈 폴더를 지움. 파일(숨김 포함)이 있으면 그 폴더와 상위는 남김
fn remove_empty_dirs(dir: &Path) -> Result<bool> {
    let mut empty = true;
    for item in fs::read_dir(dir)? {
        let item = item?;
        let kind = item.file_type()?;
        if kind.is_dir() && !kind.is_symlink() {
            if !remove_empty_dirs(&item.path())? {
                empty = false;
            }
        } else {
            empty = false;
        }
    }
    if empty {
        fs::remove_dir(dir)?;
    }
    Ok(empty)
}

fn reject_symlink(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => Err(CoreError::Symlink(path.to_owned())),
        _ => Ok(()),
    }
}

/// rename 결과가 디스크에 반영되도록 폴더도 fsync (Unix). Windows는 지원하지 않아 생략.
#[cfg(unix)]
fn sync_dir(dir: &Path) -> io::Result<()> {
    File::open(dir)?.sync_all()
}
#[cfg(not(unix))]
fn sync_dir(_dir: &Path) -> io::Result<()> {
    Ok(())
}

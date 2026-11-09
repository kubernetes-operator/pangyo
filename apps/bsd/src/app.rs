//! 앱 상태와 키 처리 (화면·터미널과 분리 → 키 입력 시나리오를 그대로 테스트)
//!
//! 화면 구성(docs/03_UX.md): 관리(트리) ↔ 작성(편집). 넓으면 두 칸, 좁으면 한 칸 + F6 전환.
use std::time::{Duration, Instant};

use memonest_core::{FileStore, NoteLocation, MAX_FOLDER_DEPTH};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::{device::DeviceStatus, editor::TextBuffer};

// === 0. 동작 변수 ===
/// 입력이 멈춘 뒤 자동 저장까지 (전원이 갑자기 꺼지는 장비 대비)
pub const AUTOSAVE_AFTER: Duration = Duration::from_secs(5);
/// 이 폭 이상이면 트리·편집 두 칸 동시 표시
pub const SPLIT_MIN_WIDTH: u16 = 100;
/// PageUp/PageDown 이동 줄 수
pub const PAGE_LINES: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Tree,
    Editor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeItem {
    Folder(String),
    Note(NoteLocation),
}

impl TreeItem {
    pub fn depth(&self) -> usize {
        match self {
            TreeItem::Folder(f) => f.split('/').count() - 1,
            TreeItem::Note(n) if n.folder.is_empty() => 0,
            TreeItem::Note(n) => n.folder.split('/').count(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    NewNote,
    NewFolder,
}

pub struct Prompt {
    pub kind: PromptKind,
    pub input: String,
}

pub struct App {
    pub store: FileStore,
    pub items: Vec<TreeItem>,
    pub selected: usize,
    pub focus: Pane,
    pub current: Option<NoteLocation>,
    pub buffer: TextBuffer,
    pub scroll: usize,
    pub prompt: Option<Prompt>,
    pub message: Option<String>,
    pub status: DeviceStatus,
    pub should_quit: bool,
    quit_armed: bool,
    last_edit: Option<Instant>,
}

impl App {
    pub fn new(store: FileStore) -> Self {
        let mut app = Self {
            store,
            items: Vec::new(),
            selected: 0,
            focus: Pane::Tree,
            current: None,
            buffer: TextBuffer::from_text(""),
            scroll: 0,
            prompt: None,
            message: Some(
                "F2 새 메모 · F3 새 폴더 · Enter 열기 · F6 전환 · Ctrl+S 저장 · Ctrl+Q 종료".into(),
            ),
            status: DeviceStatus::default(),
            should_quit: false,
            quit_armed: false,
            last_edit: None,
        };
        app.refresh();
        app
    }

    /// 디스크에서 트리 다시 읽기 (폴더 → 그 안의 메모 순서)
    pub fn refresh(&mut self) {
        let folders = self.store.folders().unwrap_or_default();
        let notes = self.store.list().unwrap_or_default();
        let mut items: Vec<TreeItem> = notes
            .iter()
            .filter(|n| n.folder.is_empty())
            .cloned()
            .map(TreeItem::Note)
            .collect();
        for folder in folders {
            items.push(TreeItem::Folder(folder.clone()));
            items.extend(
                notes
                    .iter()
                    .filter(|n| n.folder == folder)
                    .cloned()
                    .map(TreeItem::Note),
            );
        }
        self.items = items;
        self.selected = self.selected.min(self.items.len().saturating_sub(1));
    }

    pub fn title(&self) -> String {
        self.current
            .as_ref()
            .map_or_else(|| "(메모 없음)".to_owned(), NoteLocation::display_path)
    }

    /// 1초마다: 자동 저장
    pub fn tick(&mut self, now: Instant) {
        if self.buffer.is_dirty() && self.last_edit.is_some_and(|t| now - t >= AUTOSAVE_AFTER) {
            self.save();
        }
    }

    pub fn save(&mut self) {
        let Some(loc) = self.current.clone() else {
            return;
        };
        match self.store.write(&loc, &self.buffer.text()) {
            Ok(()) => {
                self.buffer.mark_saved();
                self.last_edit = None;
                self.message = Some(format!("저장됨: {}", loc.display_path()));
            }
            Err(e) => self.message = Some(format!("저장 실패: {e}")), // 앱은 계속 실행
        }
    }

    fn open(&mut self, loc: NoteLocation) {
        if self.buffer.is_dirty() {
            self.save(); // 다른 메모로 옮기기 전에 자동 저장
        }
        match self.store.read(&loc) {
            Ok(text) => {
                self.buffer = TextBuffer::from_text(&text);
                self.scroll = 0;
                self.message = Some(format!("열림: {}", loc.display_path()));
                self.current = Some(loc);
                self.focus = Pane::Editor;
            }
            Err(e) => self.message = Some(format!("열 수 없음: {e}")),
        }
    }

    fn submit_prompt(&mut self, prompt: Prompt) {
        let input = prompt.input.trim();
        if input.is_empty() {
            return;
        }
        match prompt.kind {
            PromptKind::NewFolder => match self.store.create_folder(input) {
                Ok(f) => {
                    self.message = Some(format!("폴더: {f}/"));
                    self.refresh();
                }
                Err(e) => self.message = Some(e.to_string()),
            },
            PromptKind::NewNote => match NoteLocation::parse(input) {
                Ok(loc) if self.store.exists(&loc) => self.open(loc),
                Ok(loc) => match self.store.write(&loc, "") {
                    Ok(()) => {
                        self.refresh();
                        if let Some(i) = self
                            .items
                            .iter()
                            .position(|i| i == &TreeItem::Note(loc.clone()))
                        {
                            self.selected = i;
                        }
                        self.open(loc);
                    }
                    Err(e) => self.message = Some(e.to_string()),
                },
                Err(e) => self.message = Some(e.to_string()),
            },
        }
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if !(ctrl && key.code == KeyCode::Char('q')) {
            self.quit_armed = false;
        }
        // 입력 줄(새 메모/폴더 이름)
        if let Some(prompt) = &mut self.prompt {
            match key.code {
                KeyCode::Esc => self.prompt = None,
                KeyCode::Enter => {
                    let p = self.prompt.take().expect("prompt");
                    self.submit_prompt(p);
                }
                KeyCode::Backspace => {
                    prompt.input.pop();
                }
                KeyCode::Char(c) if !ctrl => prompt.input.push(c),
                _ => {}
            }
            return;
        }
        match (key.code, ctrl) {
            (KeyCode::F(6), _) => {
                self.focus = if self.focus == Pane::Tree {
                    Pane::Editor
                } else {
                    Pane::Tree
                };
                return;
            }
            (KeyCode::Char('s'), true) => return self.save(),
            (KeyCode::Char('q'), true) => {
                if self.buffer.is_dirty() && !self.quit_armed {
                    self.quit_armed = true;
                    self.message = Some(
                        "저장하지 않은 변경이 있습니다. Ctrl+S 저장, 다시 Ctrl+Q는 버리고 종료"
                            .into(),
                    );
                } else {
                    self.should_quit = true;
                }
                return;
            }
            (KeyCode::F(2), _) => {
                let folder = match self.items.get(self.selected) {
                    Some(TreeItem::Folder(f)) => format!("{f}/"),
                    Some(TreeItem::Note(n)) if !n.folder.is_empty() => format!("{}/", n.folder),
                    _ => String::new(),
                };
                self.prompt = Some(Prompt {
                    kind: PromptKind::NewNote,
                    input: folder,
                });
                return;
            }
            (KeyCode::F(3), _) => {
                self.prompt = Some(Prompt {
                    kind: PromptKind::NewFolder,
                    input: String::new(),
                });
                return;
            }
            _ => {}
        }
        match self.focus {
            Pane::Tree => self.on_tree_key(key),
            Pane::Editor => self.on_editor_key(key, ctrl),
        }
    }

    fn on_tree_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => {
                self.selected = (self.selected + 1).min(self.items.len().saturating_sub(1))
            }
            KeyCode::Home => self.selected = 0,
            KeyCode::End => self.selected = self.items.len().saturating_sub(1),
            KeyCode::Enter => {
                if let Some(TreeItem::Note(loc)) = self.items.get(self.selected).cloned() {
                    self.open(loc);
                }
            }
            _ => {}
        }
    }

    fn on_editor_key(&mut self, key: KeyEvent, ctrl: bool) {
        if self.current.is_none() {
            self.message = Some(format!(
                "먼저 메모를 여세요 (F2: 새 메모, 폴더는 최대 {MAX_FOLDER_DEPTH}단계)"
            ));
            return;
        }
        let b = &mut self.buffer;
        let edited = match (key.code, ctrl) {
            (KeyCode::Char('z'), true) => b.undo(),
            (KeyCode::Char('y'), true) => b.redo(),
            (KeyCode::Char(c), false) => {
                b.insert_char(c);
                true
            }
            (KeyCode::Enter, _) => {
                b.insert_newline();
                true
            }
            (KeyCode::Backspace, _) => {
                b.backspace();
                true
            }
            (KeyCode::Delete, _) => {
                b.delete();
                true
            }
            (KeyCode::Tab, _) => {
                b.indent(false);
                true
            }
            (KeyCode::BackTab, _) => {
                b.indent(true);
                true
            }
            (KeyCode::Left, _) => {
                b.left();
                false
            }
            (KeyCode::Right, _) => {
                b.right();
                false
            }
            (KeyCode::Up, _) => {
                b.up(1);
                false
            }
            (KeyCode::Down, _) => {
                b.down(1);
                false
            }
            (KeyCode::PageUp, _) => {
                b.up(PAGE_LINES);
                false
            }
            (KeyCode::PageDown, _) => {
                b.down(PAGE_LINES);
                false
            }
            (KeyCode::Home, _) => {
                b.home();
                false
            }
            (KeyCode::End, _) => {
                b.end();
                false
            }
            (KeyCode::Esc, _) => {
                self.focus = Pane::Tree;
                false
            }
            _ => false,
        };
        if edited {
            self.last_edit = Some(Instant::now());
        }
    }

    /// 화면 높이에 맞춰 커서가 보이도록 스크롤 조정
    pub fn ensure_cursor_visible(&mut self, height: usize) {
        let (row, _) = self.buffer.cursor();
        if row < self.scroll {
            self.scroll = row;
        } else if height > 0 && row >= self.scroll + height {
            self.scroll = row + 1 - height;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::KeyEventKind;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new_with_kind(code, KeyModifiers::NONE, KeyEventKind::Press)
    }
    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new_with_kind(KeyCode::Char(c), KeyModifiers::CONTROL, KeyEventKind::Press)
    }
    fn typ(app: &mut App, s: &str) {
        for c in s.chars() {
            app.on_key(if c == '\n' {
                key(KeyCode::Enter)
            } else {
                key(KeyCode::Char(c))
            });
        }
    }
    fn app() -> (tempfile::TempDir, App) {
        let dir = tempfile::tempdir().unwrap();
        let store = FileStore::open(dir.path()).unwrap();
        (dir, App::new(store))
    }

    #[test]
    fn create_folder_and_note_type_korean_and_save() {
        let (dir, mut a) = app();
        a.on_key(key(KeyCode::F(3)));
        typ(&mut a, "업무/회의\n");
        assert_eq!(
            a.items,
            [
                TreeItem::Folder("업무".into()),
                TreeItem::Folder("업무/회의".into())
            ]
        );
        a.selected = 1;
        a.on_key(key(KeyCode::F(2)));
        assert_eq!(
            a.prompt.as_ref().unwrap().input,
            "업무/회의/",
            "선택한 폴더가 미리 채워짐"
        );
        typ(&mut a, "1차\n");
        assert_eq!(a.focus, Pane::Editor);
        typ(&mut a, "# 안건\n- 예산");
        a.on_key(key(KeyCode::Tab));
        a.on_key(ctrl('s'));
        let saved = std::fs::read_to_string(dir.path().join("업무/회의/1차.md")).unwrap();
        assert_eq!(saved, "# 안건\n  - 예산");
        assert!(a.message.as_deref().unwrap().starts_with("저장됨"));
    }

    #[test]
    fn third_level_folder_is_rejected_and_app_keeps_running() {
        let (_dir, mut a) = app();
        a.on_key(key(KeyCode::F(3)));
        typ(&mut a, "a/b/c\n");
        assert!(a.message.as_deref().unwrap().contains("최대 2단계"));
        assert!(a.items.is_empty());
        assert!(!a.should_quit);
    }

    #[test]
    fn f6_flips_panes_and_quit_warns_when_unsaved() {
        let (_dir, mut a) = app();
        a.on_key(key(KeyCode::F(6)));
        assert_eq!(a.focus, Pane::Editor);
        a.on_key(key(KeyCode::F(6)));
        assert_eq!(a.focus, Pane::Tree);
        a.on_key(key(KeyCode::F(2)));
        typ(&mut a, "메모\n본문");
        a.on_key(ctrl('q'));
        assert!(!a.should_quit, "첫 Ctrl+Q는 경고");
        a.on_key(ctrl('q'));
        assert!(a.should_quit);
    }

    #[test]
    fn autosave_after_idle_and_before_switching_notes() {
        let (dir, mut a) = app();
        a.on_key(key(KeyCode::F(2)));
        typ(&mut a, "첫번째\n내용");
        a.tick(Instant::now()); // 아직 대기 시간 전
        assert!(a.buffer.is_dirty());
        a.tick(Instant::now() + AUTOSAVE_AFTER + Duration::from_millis(10));
        assert!(!a.buffer.is_dirty());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("첫번째.md")).unwrap(),
            "내용"
        );

        typ(&mut a, " 추가");
        a.on_key(key(KeyCode::F(2)));
        typ(&mut a, "두번째\n");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("첫번째.md")).unwrap(),
            "내용 추가",
            "전환 전 자동 저장"
        );
        assert_eq!(a.title(), "두번째.md");
    }

    #[test]
    fn opens_existing_files_from_other_apps_and_undo() {
        let (dir, mut a) = app();
        std::fs::write(dir.path().join("Inbox.md"), "웹에서 쓴 메모").unwrap();
        a.refresh();
        a.on_key(key(KeyCode::Enter));
        assert_eq!(a.buffer.text(), "웹에서 쓴 메모");
        a.on_key(key(KeyCode::End));
        typ(&mut a, "!");
        a.on_key(ctrl('z'));
        assert_eq!(a.buffer.text(), "웹에서 쓴 메모");
    }
}

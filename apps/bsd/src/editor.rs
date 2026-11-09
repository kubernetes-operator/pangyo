//! 여러 줄 텍스트 편집 버퍼: 커서 이동, 한글(2칸) 폭 계산, 블록 들여쓰기(요구 26), Undo/Redo 250회(요구 27)
use unicode_width::UnicodeWidthStr;

// === 0. 편집 정책 변수 (웹 apps/web/src/settings.ts 와 같은 값) ===
/// Undo/Redo 최대 기록 수
pub const HISTORY_LIMIT: usize = 250;
/// 들여쓰기 단위
pub const INDENT: &str = "  ";

#[derive(Clone, PartialEq, Eq, Debug)]
struct Snapshot {
    lines: Vec<String>,
    row: usize,
    col: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EditKind {
    Typing,
    Other,
}

/// 줄 단위 텍스트. `col`은 바이트가 아니라 **글자(char) 위치**입니다.
pub struct TextBuffer {
    lines: Vec<String>,
    row: usize,
    col: usize,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    last_edit: Option<EditKind>,
    dirty: bool,
}

impl TextBuffer {
    pub fn from_text(text: &str) -> Self {
        Self {
            lines: text.split('\n').map(str::to_owned).collect(),
            row: 0,
            col: 0,
            undo: Vec::new(),
            redo: Vec::new(),
            last_edit: None,
            dirty: false,
        }
    }

    pub fn text(&self) -> String {
        self.lines.join("\n")
    }
    pub fn lines(&self) -> &[String] {
        &self.lines
    }
    pub fn cursor(&self) -> (usize, usize) {
        (self.row, self.col)
    }
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }
    pub fn mark_saved(&mut self) {
        self.dirty = false;
    }

    /// 커서 앞 글자들의 화면 폭 (한글·전각 2칸)
    pub fn cursor_x(&self) -> usize {
        UnicodeWidthStr::width(prefix(&self.lines[self.row], self.col))
    }

    fn char_len(&self, row: usize) -> usize {
        self.lines[row].chars().count()
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            lines: self.lines.clone(),
            row: self.row,
            col: self.col,
        }
    }

    /// 편집 직전 상태 기록. 연속 입력은 공백/줄바꿈 단위로 묶어 한 번에 되돌림
    fn remember(&mut self, kind: EditKind, boundary: bool) {
        if kind == EditKind::Other || self.last_edit != Some(EditKind::Typing) || boundary {
            self.undo.push(self.snapshot());
            if self.undo.len() > HISTORY_LIMIT {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
        self.last_edit = Some(kind);
        self.dirty = true;
    }

    pub fn insert_char(&mut self, c: char) {
        self.remember(EditKind::Typing, c.is_whitespace());
        let at = byte_at(&self.lines[self.row], self.col);
        self.lines[self.row].insert(at, c);
        self.col += 1;
    }

    /// 줄바꿈 + 앞 줄의 들여쓰기 유지
    pub fn insert_newline(&mut self) {
        self.remember(EditKind::Other, true);
        let at = byte_at(&self.lines[self.row], self.col);
        let rest = self.lines[self.row].split_off(at);
        let indent: String = self.lines[self.row]
            .chars()
            .take_while(|c| *c == ' ')
            .collect();
        self.row += 1;
        self.col = indent.chars().count();
        self.lines.insert(self.row, indent + &rest);
    }

    pub fn backspace(&mut self) {
        if self.col > 0 {
            self.remember(EditKind::Other, false);
            let start = byte_at(&self.lines[self.row], self.col - 1);
            let end = byte_at(&self.lines[self.row], self.col);
            self.lines[self.row].replace_range(start..end, "");
            self.col -= 1;
        } else if self.row > 0 {
            self.remember(EditKind::Other, false);
            let line = self.lines.remove(self.row);
            self.row -= 1;
            self.col = self.char_len(self.row);
            self.lines[self.row].push_str(&line);
        }
    }

    pub fn delete(&mut self) {
        if self.col < self.char_len(self.row) {
            self.remember(EditKind::Other, false);
            let start = byte_at(&self.lines[self.row], self.col);
            let end = byte_at(&self.lines[self.row], self.col + 1);
            self.lines[self.row].replace_range(start..end, "");
        } else if self.row + 1 < self.lines.len() {
            self.remember(EditKind::Other, false);
            let next = self.lines.remove(self.row + 1);
            self.lines[self.row].push_str(&next);
        }
    }

    /// 현재 줄 들여쓰기 / 내어쓰기 (공백 2칸 또는 탭 1개 제거)
    pub fn indent(&mut self, outdent: bool) {
        let line = &self.lines[self.row];
        if outdent {
            let remove = if line.starts_with(INDENT) {
                INDENT.len()
            } else if line.starts_with(' ') || line.starts_with('\t') {
                1
            } else {
                return;
            };
            self.remember(EditKind::Other, true);
            self.lines[self.row].replace_range(..remove, "");
            self.col = self.col.saturating_sub(remove);
        } else {
            self.remember(EditKind::Other, true);
            self.lines[self.row].insert_str(0, INDENT);
            self.col += INDENT.chars().count();
        }
    }

    pub fn undo(&mut self) -> bool {
        self.swap_history(true)
    }
    pub fn redo(&mut self) -> bool {
        self.swap_history(false)
    }
    fn swap_history(&mut self, undo: bool) -> bool {
        let source = if undo {
            self.undo.pop()
        } else {
            self.redo.pop()
        };
        let Some(snap) = source else { return false };
        let current = self.snapshot();
        if undo {
            self.redo.push(current);
        } else {
            self.undo.push(current);
        }
        self.lines = snap.lines;
        self.row = snap.row;
        self.col = snap.col;
        self.last_edit = None;
        self.dirty = true;
        true
    }

    // ---------- 커서 이동 ----------
    pub fn left(&mut self) {
        self.last_edit = None;
        if self.col > 0 {
            self.col -= 1;
        } else if self.row > 0 {
            self.row -= 1;
            self.col = self.char_len(self.row);
        }
    }
    pub fn right(&mut self) {
        self.last_edit = None;
        if self.col < self.char_len(self.row) {
            self.col += 1;
        } else if self.row + 1 < self.lines.len() {
            self.row += 1;
            self.col = 0;
        }
    }
    pub fn up(&mut self, n: usize) {
        self.last_edit = None;
        self.row = self.row.saturating_sub(n);
        self.col = self.col.min(self.char_len(self.row));
    }
    pub fn down(&mut self, n: usize) {
        self.last_edit = None;
        self.row = (self.row + n).min(self.lines.len() - 1);
        self.col = self.col.min(self.char_len(self.row));
    }
    pub fn home(&mut self) {
        self.col = 0;
    }
    pub fn end(&mut self) {
        self.col = self.char_len(self.row);
    }
}

fn prefix(line: &str, chars: usize) -> &str {
    &line[..byte_at(line, chars)]
}

fn byte_at(line: &str, chars: usize) -> usize {
    line.char_indices()
        .nth(chars)
        .map_or(line.len(), |(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(s: &str) -> TextBuffer {
        let mut b = TextBuffer::from_text("");
        for c in s.chars() {
            if c == '\n' {
                b.insert_newline()
            } else {
                b.insert_char(c)
            }
        }
        b
    }

    #[test]
    fn korean_cursor_width_and_editing() {
        let mut b = typed("한글 ab");
        assert_eq!(b.text(), "한글 ab");
        assert_eq!(b.cursor(), (0, 5));
        assert_eq!(b.cursor_x(), 7, "한글 2칸×2 + 공백 + ab");
        b.left();
        b.left();
        b.left();
        b.backspace(); // '글' 삭제
        assert_eq!(b.text(), "한 ab");
        assert_eq!(b.cursor_x(), 2);
    }

    #[test]
    fn newline_keeps_indent_and_backspace_joins_lines() {
        let mut b = typed("  - 항목");
        b.insert_newline();
        assert_eq!(b.text(), "  - 항목\n  ");
        b.backspace();
        b.backspace();
        b.backspace();
        assert_eq!(b.text(), "  - 항목");
        assert_eq!(b.cursor(), (0, 6));
    }

    #[test]
    fn indent_outdent_current_line() {
        let mut b = typed("목록");
        b.indent(false);
        assert_eq!(b.text(), "  목록");
        assert_eq!(b.cursor(), (0, 4));
        b.indent(true);
        b.indent(true); // 더 내어쓸 것 없음
        assert_eq!(b.text(), "목록");
    }

    #[test]
    fn undo_groups_words_and_keeps_250_limit() {
        let mut b = typed("hello world");
        assert!(b.undo());
        assert_eq!(b.text(), "hello", "단어 단위로 되돌림");
        assert!(b.redo());
        assert_eq!(b.text(), "hello world");

        let mut many = TextBuffer::from_text("");
        for _ in 0..300 {
            many.insert_newline();
        }
        let mut steps = 0;
        while many.undo() {
            steps += 1;
        }
        assert_eq!(steps, HISTORY_LIMIT);
    }

    #[test]
    fn roundtrip_text_with_trailing_newline() {
        let b = TextBuffer::from_text("a\n\nb\n");
        assert_eq!(b.lines().len(), 4);
        assert_eq!(b.text(), "a\n\nb\n");
        assert!(!b.is_dirty());
    }
}

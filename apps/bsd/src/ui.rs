//! 화면 그리기 (ratatui). 단색 위주, 상단 상태줄 / 본문(트리·편집) / 하단 안내줄
use ratatui::{
    layout::{Constraint, Layout, Position, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::{App, Pane, TreeItem, SPLIT_MIN_WIDTH};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    // 상단: 앱 · 현재 메모 · 저장 상태 · 기기 상태 · 시계 (요구 09)
    let saved = if app.buffer.is_dirty() {
        "○ 수정됨"
    } else {
        "● 저장됨"
    };
    let clock = chrono::Local::now().format("%m-%d %H:%M").to_string();
    let head = format!(
        " MEMONEST │ {} │ {} │ {} │ {}",
        app.title(),
        saved,
        app.status.summary(),
        clock
    );
    frame.render_widget(
        Paragraph::new(head).style(Style::new().add_modifier(Modifier::REVERSED)),
        header,
    );

    if body.width >= SPLIT_MIN_WIDTH {
        let [left, right] =
            Layout::horizontal([Constraint::Percentage(32), Constraint::Percentage(68)])
                .areas(body);
        draw_tree(frame, app, left);
        draw_editor(frame, app, right);
    } else if app.focus == Pane::Tree {
        draw_tree(frame, app, body);
    } else {
        draw_editor(frame, app, body);
    }

    let foot = match &app.prompt {
        Some(p) => {
            let label = match p.kind {
                crate::app::PromptKind::NewNote => "새 메모 [폴더/]제목[.txt]: ",
                crate::app::PromptKind::NewFolder => "새 폴더 (최대 2단계): ",
            };
            let text = format!("{label}{}", p.input);
            let x = footer.x + unicode_width::UnicodeWidthStr::width(text.as_str()) as u16;
            frame.set_cursor_position(Position::new(
                x.min(footer.right().saturating_sub(1)),
                footer.y,
            ));
            text
        }
        None => app.message.clone().unwrap_or_default(),
    };
    frame.render_widget(Paragraph::new(foot), footer);
}

fn pane_block(title: &str, focused: bool) -> Block<'_> {
    let style = if focused {
        Style::new().add_modifier(Modifier::BOLD)
    } else {
        Style::new()
    };
    Block::new()
        .borders(Borders::ALL)
        .title(title)
        .border_style(style)
}

fn draw_tree(frame: &mut Frame, app: &App, area: Rect) {
    let lines: Vec<Line> = if app.items.is_empty() {
        vec![Line::from("(비어 있음 — F2 새 메모)")]
    } else {
        app.items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let indent = "  ".repeat(item.depth());
                let text = match item {
                    TreeItem::Folder(f) => {
                        format!("{indent}▸ {}/", f.rsplit('/').next().unwrap_or(f))
                    }
                    TreeItem::Note(n) => format!("{indent}· {}", n.file_name()),
                };
                let style = if i == app.selected && app.focus == Pane::Tree {
                    Style::new().add_modifier(Modifier::REVERSED)
                } else if i == app.selected {
                    Style::new().add_modifier(Modifier::UNDERLINED)
                } else {
                    Style::new()
                };
                Line::from(Span::styled(text, style))
            })
            .collect()
    };
    frame.render_widget(
        Paragraph::new(lines).block(pane_block(" 관리 ", app.focus == Pane::Tree)),
        area,
    );
}

fn draw_editor(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = pane_block(" 작성 (F6 전환) ", app.focus == Pane::Editor);
    let inner = block.inner(area);
    app.ensure_cursor_visible(inner.height as usize);
    let lines: Vec<Line> = app
        .buffer
        .lines()
        .iter()
        .skip(app.scroll)
        .take(inner.height as usize)
        .map(|l| Line::from(l.as_str()))
        .collect();
    frame.render_widget(Paragraph::new(lines).block(block), area);
    if app.focus == Pane::Editor && app.prompt.is_none() && app.current.is_some() {
        let (row, _) = app.buffer.cursor();
        let x = inner.x + (app.buffer.cursor_x() as u16).min(inner.width.saturating_sub(1));
        let y = inner.y + (row - app.scroll) as u16;
        frame.set_cursor_position(Position::new(x, y));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::{Battery, DeviceStatus, Wifi};
    use memonest_core::FileStore;
    use ratatui::{backend::TestBackend, Terminal};

    /// 그려진 화면을 문자열로. 한글(2칸)의 두 번째 칸은 빈 칸이므로 비교는 공백을 뺀 문자열로 합니다.
    fn screen(width: u16, height: u16, app: &mut App) -> String {
        let mut term = Terminal::new(TestBackend::new(width, height)).unwrap();
        term.draw(|f| draw(f, app)).unwrap();
        let buf = term.backend().buffer().clone();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buf[(x, y)].symbol().to_owned())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
            .replace(' ', "")
    }

    #[test]
    fn wide_terminal_shows_both_panes_with_korean_and_status() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("업무")).unwrap();
        std::fs::write(dir.path().join("업무/회의.md"), "안녕하세요 메모입니다").unwrap();
        let mut app = App::new(FileStore::open(dir.path()).unwrap());
        app.status = DeviceStatus {
            battery: Some(Battery {
                percent: 55,
                charging: false,
            }),
            wifi: Wifi::Connected("home".into()),
            bluetooth: Some(true),
        };
        app.selected = 1;
        app.on_key(ratatui::crossterm::event::KeyEvent::from(
            ratatui::crossterm::event::KeyCode::Enter,
        ));
        let s = screen(120, 10, &mut app);
        assert!(s.contains("관리"), "{s}");
        assert!(s.contains("작성"));
        assert!(s.contains("BAT55%·WiFihome·BTon"), "{s}");
        assert!(s.contains("업무/회의.md"));
        assert!(s.contains("안녕하세요메모입니다"), "{s}");
    }

    #[test]
    fn narrow_terminal_shows_one_pane_and_f6_flips() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = App::new(FileStore::open(dir.path()).unwrap());
        let tree = screen(60, 8, &mut app);
        assert!(tree.contains("관리") && !tree.contains("작성"));
        app.on_key(ratatui::crossterm::event::KeyEvent::from(
            ratatui::crossterm::event::KeyCode::F(6),
        ));
        let editor = screen(60, 8, &mut app);
        assert!(editor.contains("작성") && !editor.contains("관리"));
    }
}

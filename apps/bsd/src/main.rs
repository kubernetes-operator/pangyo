//! MemoNest 전용 장비 앱 (FreeBSD arm64 키오스크용 전체 화면 TUI). 로드맵 08단계.
//!
//! - 메모 위치: `$MEMONEST_NOTES_DIR` 또는 `$HOME/MemoNest/Notes` (웹·데스크톱과 같은 .md 파일)
//! - 기기 상태(배터리·Wi-Fi·BT)는 별도 스레드에서 10초마다 읽음 (화면이 멈추지 않게)
//! - 앱 안에서 셸 명령을 실행하는 기능은 없습니다 (키오스크 정책, docs/10_BSD.md)
mod app;
mod device;
mod editor;
mod ui;

// === 0. 실행 변수 ===
/// 메모 폴더 환경변수
const NOTES_DIR_ENV: &str = "MEMONEST_NOTES_DIR";
/// 기본 메모 폴더 ($HOME 기준)
const APP_FOLDER: &str = "MemoNest/Notes";
/// 화면 갱신(입력 대기) 간격
const TICK: std::time::Duration = std::time::Duration::from_millis(250);
/// 기기 상태 갱신 간격
const STATUS_EVERY: std::time::Duration = std::time::Duration::from_secs(10);

use std::{env, io, path::PathBuf, sync::mpsc, thread, time::Instant};

use memonest_core::FileStore;
use ratatui::crossterm::event::{self, Event, KeyEventKind};

fn notes_dir() -> PathBuf {
    env::var_os(NOTES_DIR_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env::var_os("HOME").unwrap_or_else(|| ".".into())).join(APP_FOLDER)
        })
}

fn main() {
    if env::args().any(|a| a == "--version") {
        println!("memonest-bsd {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let store = match FileStore::open(notes_dir()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("MemoNest: 메모 폴더를 열 수 없습니다: {e}");
            std::process::exit(1);
        }
    };
    // 상태 읽기 스레드 (명령 실행이 느려도 입력은 계속 처리)
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || loop {
        if tx.send(device::read_status()).is_err() {
            break;
        }
        thread::sleep(STATUS_EVERY);
    });

    let mut terminal = ratatui::init(); // 원시 모드 + 대체 화면, 패닉 시 터미널 복구
    let result = run(&mut terminal, app::App::new(store), &rx);
    ratatui::restore();
    if let Err(e) = result {
        eprintln!("MemoNest error: {e}");
        std::process::exit(1);
    }
}

fn run(
    terminal: &mut ratatui::DefaultTerminal,
    mut app: app::App,
    status: &mpsc::Receiver<device::DeviceStatus>,
) -> io::Result<()> {
    while !app.should_quit {
        if let Ok(s) = status.try_recv() {
            app.status = s;
        }
        terminal.draw(|f| ui::draw(f, &mut app))?;
        if event::poll(TICK)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    app.on_key(key);
                }
            }
        }
        app.tick(Instant::now());
    }
    Ok(())
}

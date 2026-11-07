// === 0. 데스크톱 앱 설정 변수 (자격 증명 없음) ===
/// 메모 폴더를 직접 지정하는 환경변수 (BSD 앱과 같은 이름)
const NOTES_DIR_ENV: &str = "MEMONEST_NOTES_DIR";
/// 데스크톱 기본 메모 폴더: $HOME 기준. BSD 앱과 같은 위치라 두 앱이 같은 .md 파일을 봅니다.
#[cfg(desktop)]
const APP_FOLDER: &str = "MemoNest/Notes";
/// 모바일(Android/iOS) 메모 폴더: 앱 전용 데이터 폴더 아래 (홈 폴더 접근 불가)
#[cfg(mobile)]
const MOBILE_NOTES_FOLDER: &str = "Notes";
/// 보조 인덱스 파일 (OS 앱 데이터 폴더 안). 사용자 메모 폴더에는 만들지 않습니다.
const INDEX_FILE: &str = "index.sqlite";
/// 인덱스 파일 위치를 직접 지정 (테스트·이동식 설치용). 없으면 OS 앱 데이터 폴더.
const INDEX_PATH_ENV: &str = "MEMONEST_INDEX_PATH";
/// 진단: 1이면 웹뷰 JS 콘솔 메시지를 표준 출력으로 보냄 (리눅스 WebKitGTK)
const WEBVIEW_CONSOLE_ENV: &str = "MEMONEST_WEBVIEW_CONSOLE";

use std::{path::PathBuf, sync::Mutex};

use memonest_core::workspace::{NoteRecord, Workspace};
use tauri::{Manager, State};

/// 명령 처리 중 동시 접근을 막기 위해 작업공간을 잠급니다 (창 하나, 요청 순차 처리).
struct AppState(Mutex<Workspace>);

type CommandResult<T> = Result<T, String>;

fn with_workspace<T>(
    state: &State<'_, AppState>,
    f: impl FnOnce(&mut Workspace) -> memonest_core::Result<T>,
) -> CommandResult<T> {
    let mut ws = state
        .0
        .lock()
        .map_err(|_| "작업공간 잠금 오류".to_owned())?;
    f(&mut ws).map_err(|e| e.to_string())
}

// 명령 이름은 apps/web/src/storage/tauri.ts 의 COMMANDS와 같아야 합니다.
#[tauri::command]
fn notes_root(state: State<'_, AppState>) -> CommandResult<String> {
    with_workspace(&state, |ws| Ok(ws.root().display().to_string()))
}

#[tauri::command]
fn list_notes(state: State<'_, AppState>) -> CommandResult<Vec<NoteRecord>> {
    with_workspace(&state, |ws| ws.list_notes())
}

#[tauri::command]
fn save_note(state: State<'_, AppState>, note: NoteRecord) -> CommandResult<()> {
    with_workspace(&state, |ws| ws.save_note(&note))
}

#[tauri::command]
fn list_folders(state: State<'_, AppState>) -> CommandResult<Vec<String>> {
    with_workspace(&state, |ws| ws.list_folders())
}

#[tauri::command]
fn create_folder(state: State<'_, AppState>, path: String) -> CommandResult<String> {
    with_workspace(&state, |ws| ws.create_folder(&path))
}

#[tauri::command]
fn get_meta(state: State<'_, AppState>, key: String) -> CommandResult<Option<serde_json::Value>> {
    with_workspace(&state, |ws| ws.get_meta(&key))
}

#[tauri::command]
fn set_meta(
    state: State<'_, AppState>,
    key: String,
    value: serde_json::Value,
) -> CommandResult<()> {
    with_workspace(&state, |ws| ws.set_meta(&key, &value))
}

fn notes_dir(app: &tauri::App) -> tauri::Result<PathBuf> {
    if let Some(dir) = std::env::var_os(NOTES_DIR_ENV) {
        return Ok(PathBuf::from(dir));
    }
    #[cfg(desktop)]
    let dir = app.path().home_dir()?.join(APP_FOLDER);
    #[cfg(mobile)]
    let dir = app.path().app_data_dir()?.join(MOBILE_NOTES_FOLDER);
    Ok(dir)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let diagnostics = std::env::var_os(WEBVIEW_CONSOLE_ENV).is_some_and(|v| v == "1");
    tauri::Builder::default()
        .on_page_load(move |webview, payload| {
            if diagnostics {
                eprintln!(
                    "[memonest] page {:?}: {} ({})",
                    payload.event(),
                    payload.url(),
                    webview.label()
                );
            }
        })
        .setup(|app| {
            let index = match std::env::var_os(INDEX_PATH_ENV) {
                Some(path) => PathBuf::from(path),
                None => app.path().app_data_dir()?.join(INDEX_FILE),
            };
            let workspace = Workspace::open(notes_dir(app)?, &index)?;
            app.manage(AppState(Mutex::new(workspace)));
            if std::env::var_os(WEBVIEW_CONSOLE_ENV).is_some_and(|v| v == "1") {
                enable_console_to_stdout(app);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            notes_root,
            list_notes,
            save_note,
            list_folders,
            create_folder,
            get_meta,
            set_meta
        ])
        .run(tauri::generate_context!())
        .expect("MemoNest native application failed to start");
}

#[cfg(target_os = "linux")]
fn enable_console_to_stdout(app: &tauri::App) {
    use webkit2gtk::{SettingsExt, WebViewExt};
    for window in app.webview_windows().values() {
        let _ = window.with_webview(|webview| {
            if let Some(settings) = webview.inner().settings() {
                settings.set_enable_write_console_messages_to_stdout(true);
            }
        });
    }
}

#[cfg(not(target_os = "linux"))]
fn enable_console_to_stdout(_app: &tauri::App) {}

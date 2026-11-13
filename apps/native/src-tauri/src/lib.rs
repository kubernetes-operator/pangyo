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
/// 시험 전용(`e2e` feature): 페이지 로드 후 실행할 JS 파일 경로
#[cfg(feature = "e2e")]
const E2E_SCRIPT_ENV: &str = "MEMONEST_E2E_SCRIPT";

mod remote;

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use memonest_core::workspace::{EditSession, ExternalDeletion, NoteRecord, Workspace};
use remote::{DeviceInfo, Remote, ServerResponse};
use tauri::{Manager, State};

/// 명령 처리 중 동시 접근을 막기 위해 작업공간을 잠급니다 (창 하나, 요청 순차 처리).
struct AppState(Mutex<Workspace>);
/// 서버 연결 (장비 토큰은 여기서만 다룸, 화면에 넘기지 않음)
struct RemoteState(Arc<Remote>);

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
fn save_sessions(
    state: State<'_, AppState>,
    id: String,
    sessions: Vec<EditSession>,
) -> CommandResult<()> {
    with_workspace(&state, |ws| ws.save_sessions(&id, &sessions))
}

#[tauri::command]
fn delete_note(state: State<'_, AppState>, id: String) -> CommandResult<()> {
    with_workspace(&state, |ws| ws.delete_note(&id))
}

#[tauri::command]
fn list_folders(state: State<'_, AppState>) -> CommandResult<Vec<String>> {
    with_workspace(&state, |ws| ws.list_folders())
}

#[tauri::command]
fn create_folder(state: State<'_, AppState>, path: String) -> CommandResult<String> {
    with_workspace(&state, |ws| ws.create_folder(&path))
}

/// 비어 있는 폴더 지우기 (메모 파일이 남아 있으면 그대로)
#[tauri::command]
fn delete_folder(state: State<'_, AppState>, path: String) -> CommandResult<()> {
    with_workspace(&state, |ws| ws.delete_folder(&path))
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

#[tauri::command]
fn list_external_deletions(state: State<'_, AppState>) -> CommandResult<Vec<ExternalDeletion>> {
    with_workspace(&state, |ws| ws.list_external_deletions())
}

#[tauri::command]
fn forget_external_deletion(state: State<'_, AppState>, id: String) -> CommandResult<()> {
    with_workspace(&state, |ws| ws.forget_external_deletion(&id))
}

/// 네트워크·보안 저장소 작업은 화면을 멈추지 않도록 별도 스레드에서
async fn blocking<T: Send + 'static>(
    remote: &State<'_, RemoteState>,
    f: impl FnOnce(&Remote) -> CommandResult<T> + Send + 'static,
) -> CommandResult<T> {
    let remote = Arc::clone(&remote.0);
    tauri::async_runtime::spawn_blocking(move || f(&remote))
        .await
        .map_err(|e| e.to_string())?
}

/// 이 앱의 버전·빌드 (앱 안 업데이트 확인용). 빌드는 CI가 `MEMONEST_BUILD`(커밋)로 넣음, 로컬 빌드는 None
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    version: &'static str,
    build: Option<&'static str>,
    platform: &'static str,
}

#[tauri::command]
fn app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        build: option_env!("MEMONEST_BUILD").filter(|b| !b.is_empty()),
        platform: std::env::consts::OS,
    }
}

/// 서버가 발급한 일회용 다운로드 주소(`<연결한 서버>/dl/<토큰>`)만 OS 브라우저(Android: 다운로드 → 설치 화면)로 엶.
/// 다른 주소는 거절 (화면이 임의 주소를 열지 못하게)
#[tauri::command]
async fn open_download(
    app: tauri::AppHandle,
    remote: State<'_, RemoteState>,
    url: String,
) -> CommandResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let info = blocking(&remote, |r| Ok(r.status())).await?;
    let base = info.map(|i| i.server_url).ok_or("not_paired")?;
    let valid = url.strip_prefix(&format!("{base}/dl/")).is_some_and(|t| {
        !t.is_empty()
            && t.len() <= 64
            && t.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    });
    if !valid {
        return Err("허용되지 않은 다운로드 주소".into());
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

/// 내보낸 파일(프로젝트 전체 다운로드 .zip)을 OS 다운로드 폴더에 저장. 같은 이름이 있으면 " (2)"를 붙임.
/// 이름은 파일 이름만(경로 없이) 받으며, 저장한 전체 경로를 돌려줌
#[tauri::command]
fn save_download(app: tauri::AppHandle, file_name: String, data: Vec<u8>) -> CommandResult<String> {
    let name = memonest_core::safe_filename(
        file_name
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or("MemoNest.zip"),
    );
    let (stem, ext) = name.rsplit_once('.').unwrap_or((name.as_str(), "zip"));
    let dir = app.path().download_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut target = dir.join(format!("{stem}.{ext}"));
    for i in 2.. {
        if !target.exists() {
            break;
        }
        target = dir.join(format!("{stem} ({i}).{ext}"));
    }
    std::fs::write(&target, data).map_err(|e| e.to_string())?;
    Ok(target.display().to_string())
}

/// 장비 등록 화면의 기본 장비 이름에 쓸 호스트 이름 (2026-11-13 사용 의견: 내 기기 이름이 먼저 보이게).
/// 알 수 없거나 의미 없는 값(localhost 등, Android에서 흔함)이면 None → 화면이 기기 모델/OS로 대신 채움
#[tauri::command]
fn device_hostname() -> Option<String> {
    let name = whoami::fallible::hostname().unwrap_or_default();
    let name = name.trim();
    let useless = name.is_empty()
        || name.eq_ignore_ascii_case("localhost")
        || name.eq_ignore_ascii_case("unknown");
    (!useless).then(|| name.chars().take(60).collect())
}

#[tauri::command]
async fn device_status(remote: State<'_, RemoteState>) -> CommandResult<Option<DeviceInfo>> {
    blocking(&remote, |r| Ok(r.status())).await
}

#[tauri::command]
async fn device_pair(
    remote: State<'_, RemoteState>,
    server_url: String,
    code: String,
    device_name: String,
    now: String,
) -> CommandResult<DeviceInfo> {
    blocking(&remote, move |r| {
        r.pair(&server_url, &code, &device_name, std::env::consts::OS, now)
    })
    .await
}

#[tauri::command]
async fn device_unpair(remote: State<'_, RemoteState>) -> CommandResult<()> {
    blocking(&remote, |r| r.unpair()).await
}

#[tauri::command]
async fn server_request(
    remote: State<'_, RemoteState>,
    method: String,
    path: String,
    body: Option<serde_json::Value>,
) -> CommandResult<ServerResponse> {
    blocking(&remote, move |r| r.request(&method, &path, body.as_ref())).await
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
        .plugin(tauri_plugin_opener::init())
        .on_page_load(move |webview, payload| {
            if diagnostics {
                eprintln!(
                    "[memonest] page {:?}: {} ({})",
                    payload.event(),
                    payload.url(),
                    webview.label()
                );
            }
            #[cfg(feature = "e2e")]
            if payload.event() == tauri::webview::PageLoadEvent::Finished {
                if let Some(script) = std::env::var_os(E2E_SCRIPT_ENV)
                    .and_then(|path| std::fs::read_to_string(path).ok())
                {
                    let _ = webview.eval(&script);
                }
            }
        })
        .setup(|app| {
            let index = match std::env::var_os(INDEX_PATH_ENV) {
                Some(path) => PathBuf::from(path),
                None => app.path().app_data_dir()?.join(INDEX_FILE),
            };
            let workspace = Workspace::open(notes_dir(app)?, &index)?;
            app.manage(AppState(Mutex::new(workspace)));
            app.manage(RemoteState(Arc::new(Remote::new(
                app.path().app_data_dir()?,
            ))));
            if std::env::var_os(WEBVIEW_CONSOLE_ENV).is_some_and(|v| v == "1") {
                enable_console_to_stdout(app);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            notes_root,
            list_notes,
            save_note,
            save_sessions,
            delete_note,
            list_folders,
            create_folder,
            delete_folder,
            get_meta,
            set_meta,
            list_external_deletions,
            forget_external_deletion,
            device_status,
            device_hostname,
            app_info,
            open_download,
            save_download,
            device_pair,
            device_unpair,
            server_request
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

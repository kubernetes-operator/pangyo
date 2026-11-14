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

mod platform;
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
    /// Windows 설치 방식: "msi"(Program Files) 또는 "nsis"(사용자 폴더 .exe). 같은 방식의 설치 파일로 업데이트해야 함
    install_kind: Option<&'static str>,
}

#[tauri::command]
fn app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        build: option_env!("MEMONEST_BUILD").filter(|b| !b.is_empty()),
        platform: std::env::consts::OS,
        install_kind: cfg!(windows)
            .then(|| {
                std::env::current_exe()
                    .ok()
                    .map(|p| install_kind(&p.display().to_string()))
            })
            .flatten(),
    }
}

/// Windows 설치 위치로 설치 방식 판단 (2026-11-14 사용자 확인: msi로 설치한 앱을 .exe로 업데이트하면
/// 다른 폴더에 따로 설치되고 예전 앱이 다시 열려 업데이트가 안 된 것처럼 보였음)
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
fn install_kind(exe: &str) -> &'static str {
    if exe
        .to_lowercase()
        .replace('/', "\\")
        .contains("\\appdata\\local\\")
    {
        "nsis"
    } else {
        "msi"
    }
}

/// 지난 Windows 업데이트 결과 (설치 프로그램 종료 코드). 한 번 읽으면 지움. 기록이 없으면 None
#[tauri::command]
fn last_update_result() -> Option<String> {
    let log = std::env::temp_dir().join("MemoNest-update.log");
    let text = std::fs::read_to_string(&log).ok()?;
    let _ = std::fs::remove_file(&log);
    Some(text.trim().to_owned())
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

/// Windows 앱 안 업데이트 (2026-11-13 사용 의견: 버튼 한 번으로): 서버의 일회용 주소에서 설치 파일을 받아
/// 게시 목록의 sha256과 맞는지 확인한 뒤, 조용히(/S) 설치하고 앱을 다시 실행. 다른 OS는 지원하지 않음(화면이 받기로 대신)
#[tauri::command]
async fn install_update(
    app: tauri::AppHandle,
    remote: State<'_, RemoteState>,
    url: String,
    sha256: String,
) -> CommandResult<()> {
    let info = blocking(&remote, |r| Ok(r.status())).await?;
    let base = info.map(|i| i.server_url).ok_or("not_paired")?;
    if !url.starts_with(&format!("{base}/dl/")) {
        return Err("허용되지 않은 다운로드 주소".into());
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, sha256);
        Err("unsupported".into())
    }
    #[cfg(target_os = "windows")]
    {
        use sha2::{Digest, Sha256};
        const MAX_BYTES: u64 = 300 * 1024 * 1024;
        let data = tauri::async_runtime::spawn_blocking(move || -> Result<Vec<u8>, String> {
            let mut response = ureq::get(&url)
                .call()
                .map_err(|e| format!("offline: {e}"))?;
            response
                .body_mut()
                .with_config()
                .limit(MAX_BYTES)
                .read_to_vec()
                .map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())??;
        let actual: String = Sha256::digest(&data)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        if !actual.eq_ignore_ascii_case(sha256.trim()) {
            return Err("받은 설치 파일이 게시된 파일과 다릅니다 (sha256 불일치)".into());
        }
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let exe = exe.display().to_string();
        let kind = install_kind(&exe);
        let temp = std::env::temp_dir();
        let installer = temp.join(if kind == "msi" {
            "MemoNest-update.msi"
        } else {
            "MemoNest-update-setup.exe"
        });
        std::fs::write(&installer, &data).map_err(|e| e.to_string())?;
        let log = temp.join("MemoNest-update.log");
        let _ = std::fs::remove_file(&log);
        // 배치 파일로 실행: 한 줄 명령의 따옴표 문제(2026-11-14 '\\를 찾을 수 없음')를 피하고, 설치가 끝날 때까지 확실히 기다림
        let script = temp.join("MemoNest-update.cmd");
        std::fs::write(
            &script,
            update_script(
                kind,
                &installer.display().to_string(),
                &exe,
                &log.display().to_string(),
            ),
        )
        .map_err(|e| e.to_string())?;
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::process::Command::new("cmd")
            .raw_arg(format!("/S /C \"\"{}\"\"", script.display()))
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| e.to_string())?;
        app.exit(0);
        Ok(())
    }
}

/// Windows 업데이트 배치 파일: 앱이 닫힐 때까지 2초 → 같은 방식으로 설치(msi: msiexec /passive — 관리자 확인 창,
/// nsis: /S 조용히) → 종료 코드를 기록 → 앱 다시 실행. 배치에서는 GUI 설치 프로그램도 끝날 때까지 기다림
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
fn update_script(kind: &str, installer: &str, exe: &str, log: &str) -> String {
    let install = if kind == "msi" {
        format!("msiexec /i \"{installer}\" /passive /norestart")
    } else {
        format!("\"{installer}\" /S")
    };
    format!(
        "@echo off\r\nchcp 65001 >nul\r\nping -n 3 127.0.0.1 >nul\r\n{install}\r\necho exit=%errorlevel% kind={kind}> \"{log}\"\r\nstart \"\" \"{exe}\"\r\n"
    )
}

#[cfg(test)]
mod update_tests {
    #[test]
    fn install_kind_from_location() {
        assert_eq!(
            super::install_kind(r"C:\Users\me\AppData\Local\MemoNest\memonest-native.exe"),
            "nsis"
        );
        assert_eq!(
            super::install_kind(r"C:\Program Files\MemoNest\memonest-native.exe"),
            "msi"
        );
    }

    #[test]
    fn update_script_uses_matching_installer_and_logs_exit_code() {
        let msi = super::update_script(
            "msi",
            r"C:\T\MemoNest-update.msi",
            r"C:\Program Files\MemoNest\memonest-native.exe",
            r"C:\T\MemoNest-update.log",
        );
        assert!(
            msi.contains(r#"msiexec /i "C:\T\MemoNest-update.msi" /passive /norestart"#),
            "{msi}"
        );
        assert!(msi.contains(r#"start "" "C:\Program Files\MemoNest\memonest-native.exe""#));
        assert!(
            msi.starts_with("@echo off\r\nchcp 65001"),
            "한글 경로(사용자 이름)를 위해 UTF-8"
        );
        assert!(msi.contains(r#"echo exit=%errorlevel% kind=msi> "C:\T\MemoNest-update.log""#));
        let nsis = super::update_script(
            "nsis",
            r"C:\T\setup.exe",
            r"C:\Users\me\AppData\Local\MemoNest\m.exe",
            r"C:\T\log",
        );
        assert!(nsis.contains("\r\n\"C:\\T\\setup.exe\" /S\r\n"), "{nsis}");
        assert!(!nsis.contains("\\\""), "cmd가 이해하지 못하는 \\\" 없음");
    }
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
    // Android: 앱 전용 폴더는 사용자가 볼 수 없음 → 공용 '다운로드/MemoNest' (2026-11-13 사용자 확인)
    #[cfg(target_os = "android")]
    {
        let _ = &app;
        return platform::android::save_to_downloads(&name, "application/zip", &data);
    }
    #[allow(unreachable_code)]
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
/// 화면 위쪽 여백(상태바 높이, CSS px). Android에서 메뉴가 상태바와 겹치지 않게 (2026-11-14 사용 의견)
#[tauri::command]
fn top_inset() -> f64 {
    platform::top_inset()
}

/// 화면 아래쪽 여백(내비게이션 막대 높이, CSS px). 키보드가 내려갔을 때 아래 메뉴가 가려지지 않게
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct BottomInset {
    /// 실제 창 여백(WindowInsets). 못 읽으면 0
    insets: f64,
    /// 시스템 기본 내비게이션 막대 높이
    nav_bar: f64,
}

#[tauri::command]
fn bottom_inset() -> BottomInset {
    let (insets, nav_bar) = platform::bottom_inset();
    BottomInset { insets, nav_bar }
}

/// Android는 설정의 '기기 이름'(없으면 제조사·모델), 데스크톱은 호스트 이름 (2026-11-13 사용자 확인: Android에서 비어 있었음)
#[tauri::command]
fn device_hostname() -> Option<String> {
    platform::device_name()
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
        r.pair(
            &server_url,
            &code,
            &device_name,
            std::env::consts::OS,
            platform::device_key(),
            now,
        )
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
            top_inset,
            bottom_inset,
            app_info,
            open_download,
            save_download,
            install_update,
            last_update_result,
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

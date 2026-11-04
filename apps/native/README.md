# 데스크톱 앱 (Tauri 2)

화면은 `apps/web`(웹과 같은 코드), 저장은 `crates/core`의 `Workspace`를 씁니다 (로드맵 03단계).

## 저장 방식
| 무엇 | 어디 | 비고 |
|---|---|---|
| 메모 원본 | `$MEMONEST_NOTES_DIR` 또는 `~/MemoNest/Notes/<폴더>/<제목>.md\|.txt` | 앱 없이도 열 수 있는 일반 파일. BSD 앱과 같은 위치 |
| 보조 인덱스 | OS 앱 데이터 폴더 `dev.memonest.notes/index.sqlite` (리눅스: `~/.local/share/...`) | 메모 ID↔경로, 편집 세션, Undo/Redo, 화면 설정. 메모 폴더에는 만들지 않음 |

- 다른 편집기/BSD 앱에서 만든 파일은 앱이 목록을 읽을 때 자동 등록되고, 바깥에서 고친 내용도 반영됩니다.
- 제목·폴더·형식을 바꾸면 파일을 옮깁니다. 같은 이름의 다른 파일은 **덮어쓰지 않고** 저장 실패로 알립니다.
- 화면 ↔ Rust 명령: `notes_root, list_notes, save_note, list_folders, create_folder, get_meta, set_meta` (`src/lib.rs` ↔ `apps/web/src/storage/tauri.ts`).

## 실행 / 검증
- 실행: `make desktop` (웹 빌드 후 앱 실행). 개발 서버로 화면을 고치며 보려면 `make dev-web` + `cargo run -p memonest-native`.
- 실행 확인: `make desktop-smoke` — Xvfb에서 임시 폴더로 15초 실행 후 인덱스에 목록·설정·열어둔 시간이 기록됐는지 검사 (CI 포함).
- 진단: `MEMONEST_WEBVIEW_CONSOLE=1` 이면 웹뷰 JS 콘솔과 페이지 로드 로그를 터미널에 출력 (리눅스).
- 리눅스 빌드 의존성: `libwebkit2gtk-4.1-dev libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev` (+ 실행 확인용 `xvfb`).

## 검증 범위 (2026-11-04)
- **Linux x86_64에서만** 빌드·실행 확인. Windows/macOS/Android/iOS 빌드와 패키징(`tauri build`, 서명)은 미검증.
- 아이콘(`icons/`)은 임시 단색 이미지. Windows `.ico`/macOS `.icns`는 패키징 단계에서 생성.
- 듀얼 디스플레이, OS 상태(배터리·Wi-Fi·BT) 어댑터는 후속.

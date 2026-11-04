# Native wrapper (Tauri 2) — 설계 골격

- 이 폴더는 `../web`의 HTML/JS UI를 Tauri로 패키징하기 위한 **시작점**입니다.
- 플랫폼: Windows, Linux, Android, iOS. 실제 각 플랫폼 SDK/권한/배포 검증은 아직 하지 않았습니다.
- `src-tauri/tauri.conf.json`의 frontendDist는 공통 `apps/web`을 사용합니다.
- 현 브라우저 UI는 IndexedDB를 사용합니다. **Native MD/TXT 실파일 및 SQLite 사용은 다음 단계의 로컬 어댑터 작업**입니다.
- Tauri CLI, Rust 및 모바일 SDK 설치 후 각 플랫폼에서 `tauri dev/build` 검증해야 합니다.
- 저장소 노출 범위는 Tauri capability로 최소화하고 OS 내 보호된 앱 데이터 경로만 사용합니다.

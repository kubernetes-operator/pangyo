# MemoNest 전용 장비 앱 (FreeBSD arm64, 로드맵 08)

전체 화면 터미널 앱(TUI). 웹·데스크톱과 같은 `.md/.txt` 파일 규칙(`crates/core`)을 씁니다.

## 화면과 키
| 키 | 동작 |
|---|---|
| F6 | 관리(트리) ↔ 작성(편집) 전환. 터미널 폭 100칸 이상이면 두 칸 동시 표시 |
| F2 / F3 | 새 메모(`[폴더/]제목[.txt]`) / 새 폴더(최대 2단계) |
| Enter (트리) | 메모 열기 (다른 메모로 옮기기 전 자동 저장) |
| Tab / Shift+Tab | 현재 줄 들여쓰기 / 내어쓰기 |
| Ctrl+Z / Ctrl+Y | 실행 취소 / 다시 실행 (최대 250, 단어 단위) |
| Ctrl+S | 저장 (입력이 5초 멈추면 자동 저장) |
| Ctrl+Q | 종료 (저장 안 된 변경이 있으면 한 번 더 눌러야 종료) |
- 상단: 현재 메모 · 저장 상태 · 배터리 · Wi-Fi · Bluetooth · 시계 (10초마다 갱신, `src/device.rs`)
- 메모 위치: `$MEMONEST_NOTES_DIR` 또는 `$HOME/MemoNest/Notes`

## 키오스크 설치 (FreeBSD, root)
```sh
cargo build --release -p memonest-bsd            # 또는 VM 검증에서 만든 바이너리
sh apps/bsd/kiosk/install.sh target/release/memonest-bsd --dry-run   # 바뀔 내용 확인
sh apps/bsd/kiosk/install.sh target/release/memonest-bsd             # 적용 (여러 번 실행해도 동일)
```
전용 계정 `memonest`(로그인 셸 = `memonest-kiosk`, 암호 로그인 불가) + ttyv0 자동 로그인. 앱이 종료돼도 셸로 빠지지 않고 다시 실행됩니다. 관리는 다른 콘솔(Alt+F2)에서 관리자 계정으로.

## 검증
- `cargo test -p memonest-bsd`: 편집(한글 폭·Undo·들여쓰기), 앱 키 시나리오(폴더/메모/저장/자동 저장/종료 경고), 화면 렌더링(두 칸/한 칸), 기기 상태 출력 해석
- `python3 scripts/bsd-vm-verify.py [--reuse]`: **FreeBSD 15.1 arm64 VM(QEMU)** 에서 실제 빌드·테스트·TUI 키 입력→저장·키오스크 설치 확인
  - 최근 결과(2026-11-09): **8/8 통과** (arch·pkg rust·소스·cargo test 16개·release 빌드·버전·TUI 한글 입력 저장·키오스크 멱등 설치)

## 아직 검증하지 못한 것 (실기기 필요)
- 보드/디스플레이(1·2화면), Wi-Fi·BT 칩셋별 출력, 배터리 센서 — `device.rs` 해석은 표준 출력 예시로만 테스트
- 콘솔(vt)에서 **한글 입력**: FreeBSD 콘솔에는 한글 입력기가 없음. 한글 표시는 vt용 한글 글꼴 필요. 대안: SSH/터미널 에뮬레이터 경유 입력, 또는 별도 입력기 개발
- 전원 강제 차단 시 파일 일관성 (원자적 저장 + 5초 자동 저장으로 완화)

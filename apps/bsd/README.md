# FreeBSD arm64 전용 콘솔 프로토타입

**주의:** 현재는 전용 장비 완성판이 아닌 단순한 명령형 터미널 편집기입니다. 2개의 물리 화면, 키보드 한글 조합기, Wi-Fi/BT 설정은 구현하지 않았습니다.

- `cargo run --manifest-path apps/bsd/Cargo.toml` (Rust가 설치된 개발 호스트)
- 실행 후 `:new 제목`, 일반 입력 줄, `:save`, `:list`, `:open 제목`, `:quit`.
- 안전을 위해 메모는 사용자 홈의 `~/MemoNest/Notes/`에 파일로 저장합니다.
- Bash는 필수가 아닙니다. Unix 기본 `/bin/sh`로 실행 가능.
- 연결 장비는 `docs/10_BSD.md` 체크리스트에 따라 먼저 선정/검증해야 합니다.

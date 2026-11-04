# CHANGELOG

## v0.1.0 — initial scaffold
- 요구사항 29개 추적표, ADR, 플랫폼·저장소·API 설계 문서 작성.
- 오프라인 브라우저 메모 프로토타입, Go 헬스체크 API, Helm·Tauri·FreeBSD 골격.
- 실행/빌드 전제조건 및 보안상 미구현 항목을 분리 기록.

### 검증 이력
- 초기 `scripts/check.sh` 실행 결과는 실제 사용 시 기록. 개별 디바이스 빌드는 아직 검증되지 않았음.

## Unreleased — 개발 도구/CI 정리 (ADR-0002 이전 1단계, `feature/01-dev-tooling`)
- 변경 파일: `.gitignore`(신규), `Cargo.toml`·`rust-toolchain.toml`(Rust 워크스페이스, 신규), `Makefile`(신규), `.github/workflows/ci.yml`(신규), `CLAUDE.md`(신규), `docs/decisions/ADR-0002.md`(신규), `scripts/check.sh`, `apps/bsd/Cargo.toml`·`apps/bsd/src/main.rs`(워크스페이스 편입, clippy 1건 수정 + rustfmt 정렬, 동작 변화 없음), `AGENTS.md`, `README.md`, `docs/11_DEV_LOOP.md`, `docs/13_CONFIG_MAP.md`, `scripts/loop.sh`(Roo Code 참조 제거).
- 테스트: `make check` 통과(JSON 3개, JS 문법, Rust fmt/clippy `-D warnings`/test). Go 테스트는 이 환경에 Go가 없어 SKIP — CI에서는 `REQUIRE_GO=1`로 실행. `make helm-lint` 통과. CI 워크플로는 아직 GitHub에서 실행해 보지 않음.
- 위험: 기능 변화 없음. `config/project.env`(실제 값 가능)는 `.gitignore`로 제외했으나 파일 자체는 남아 있음 → 사용자가 `.env`로 옮기는 것을 권장. Tauri는 WebKit/GTK 시스템 라이브러리 부재로 워크스페이스에서 제외. `FILE_INDEX.sha256`은 v0.1 시점 해시이며 갱신하지 않음.
- 후속: 2단계 웹 TypeScript+Vite 이전과 Playwright 회귀 테스트(T-01~T-06, 기존 IndexedDB 데이터 호환).

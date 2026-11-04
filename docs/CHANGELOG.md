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

## Unreleased — 웹 TypeScript + Vite 이전 (ADR-0002 이전 2/4단계, `feature/02-web-typescript`)
- 변경 파일: `apps/web/src/**`(신규: `settings.ts`, `domain/{types,paths,history,editing}.ts`, `storage/indexeddb.ts`, `ui/app.ts`, `main.ts`), `apps/web/build/offline-sw.ts`(서비스워커 생성 플러그인), `apps/web/{package.json,tsconfig.json,vite.config.ts,playwright.config.ts}`, `apps/web/e2e/app.spec.ts`, `apps/web/src/domain/domain.test.ts`, 루트 `package.json`/`package-lock.json`, `apps/web/index.html`(스크립트 경로), `manifest.webmanifest`→`public/`, `main.css`→`src/ui/`; 삭제: `apps/web/assets/{app,store}.js`, `apps/web/sw.js`. 연결: `scripts/check.sh`, `scripts/dev-web.sh`, `Makefile`, CI, `tauri.conf.json`(dist/devUrl). 문서: README, CLAUDE.md, 12_TESTPLAN, 13_CONFIG_MAP.
- 동작: v0.1과 동일하도록 이식 (UI/문구/저장 형식 변경 없음). IndexedDB `memonest_local_v1` v1 그대로 사용, 옛 레코드는 `normalizeNote`로 기본값 보정.
- 테스트: `make check` 통과 — Vitest 14개, Playwright E2E 8개(T-01~T-06 + v0.1 데이터 호환), Rust, 비밀값 가드. Go는 미설치로 SKIP. 의존성은 `npm install --before=2026-10-20`으로 고정(vite 8.3.4, typescript 7.0.2, vitest 5.0.5, @playwright/test 1.64.1).
- 이전 중 발견·수정: Vite 빌드의 `crossorigin` 스크립트 요청이 `Vary: Origin` 때문에 서비스워커 캐시와 매칭되지 않아 **오프라인 로드 실패** → `caches.match(..., { ignoreVary: true })`로 수정(E2E가 검출).
- 위험: 서비스워커 캐시 이름이 바뀌어 첫 방문 시 v0.1 캐시(`memonest-v01-static-1`)는 삭제됨(메모 데이터와 무관). Tauri 빌드는 WebKit/GTK 미설치로 미검증. E2E는 Chromium만.
- 후속: ① T-05 무입력 60초가 활성 시간에 포함되는 문제(요구 23 "키보드 활동 시간만") 수정 여부 결정 ② `prompt/alert` 대화상자를 앱 내 UI로 교체 ③ 3/4단계 `crates/core` + BSD 전환.

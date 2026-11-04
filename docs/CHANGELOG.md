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

## Unreleased — 편집 시간: 작성 시간 + 열어둔 시간 (요구 23·24, `feature/23-edit-time`)
- 요구 변경(사용자 확정): 편집 시간은 "타이핑하는 시간"과 "창을 열어놓은 시간"을 모두 포함. 두 값을 분리 기록·표시.
- 변경 파일: `apps/web/src/domain/{types,editing}.ts`(openMs, typingIncrement, totalOpenMs, isNegligibleSession, normalizeSession), `apps/web/src/ui/app.ts`(세션 = 열기~전환/숨김/종료, 숨김 복귀 시 재개 세션, 10초 주기 저장, pagehide 저장), `apps/web/src/settings.ts`, `apps/web/index.html`(작성/열림 표시), 테스트, `db/sqlite/002_session_open_time.sql`, `config/client.example.yaml`, docs 01/04/12.
- 동작 변화: ① 무입력 60초 동안의 시간이 작성 시간에 들어가던 문제 해소 ② 무입력만으로는 세션이 끊기지 않음(창 숨김/전환 시 끊김) ③ 입력 없이 2초 미만 연 세션은 기록 안 함.
- 데이터: IndexedDB 레코드에 `sessions[].openMs` 필드 추가(추가형). v0.1 레코드는 0으로 보정, DB_VERSION 변경 없음. SQLite 초안은 migration 002 추가.
- 테스트: 단위(typingIncrement 경계값, openMs 보정) + E2E T-05(작성 10초 / 열림 2분대), T-05b(숨김 10분 제외, 재개 세션), 호환 테스트 통과.
- 위험: 열어둔 시간이 10초마다 노트 전체(Undo 스냅샷 포함)를 저장 → 큰 문서에서 쓰기량 증가. 3단계 이후 세션을 별도 저장소로 분리 검토. 실제 창 최소화는 headless에서 흉내만 냄(visibilitychange).

## Unreleased — Rust 공통 코어 + BSD 전환 (ADR-0002 이전 3/4단계, `feature/03-rust-core`)
- 변경 파일: `crates/core/**`(신규: `paths.rs` 규칙, `store.rs` 파일 저장소, `format.rs`, `error.rs`, `tests/{parity,store}.rs`), `packages/protocol/fixtures/paths.json`(신규, TS·Rust 공통 기준), `apps/web/src/domain/paths.ts`(공통 규칙으로 정렬), `apps/web/src/domain/domain.test.ts`, `apps/bsd/{Cargo.toml,src/main.rs}`(core 사용, 테스트 가능 구조), `Cargo.toml`(members), docs 04/10/13, ADR-0002, README, CLAUDE.md.
- 규칙 변화(웹·BSD 공통): 길이를 코드 포인트 기준으로(이모지 등이 반으로 잘리던 문제 해소), DEL(U+007F) 금지, Windows 예약어·끝의 점/공백·`.` 시작 이름 처리, U+0085/U+FEFF 공백 처리 통일. **폴더는 금지 문자·예약어를 이제 거부**(이전 웹은 `:`만 거부). BSD는 금지 문자를 지우던 방식 → `_` 치환(웹과 동일), 제목 100자 → 200자.
- BSD 동작: 폴더(최대 2단계)·`.txt` 지원, `:mkdir`/`:show`/`:help`/`:quit!` 추가, 미저장 종료 경고, 저장 실패 시 종료하지 않음, `:new`가 기존 파일을 덮어쓰지 않음, `\:` 이스케이프.
- 테스트: `make check` 통과 — Web 단위 58개(공통 기준 42건 포함) + E2E 9개, Rust 17개(공통 기준, 저장소 7개: 원자적 덮어쓰기·깊이 제한·심볼릭 링크 차단·비 UTF-8 등, BSD 6개). BSD 바이너리 수동 실행 확인(Linux).
- 위험: 기존 웹 데이터에 금지 문자가 든 폴더(예: `a*b`)가 있으면 그대로 표시되지만 같은 이름으로 새로 만들 수는 없음. FreeBSD aarch64 빌드는 미검증(Tier 3 타깃, 실기기 필요). Tauri 파일 어댑터 연결은 로드맵 03단계.
- 후속: 4/4단계 서버 Go → Rust(axum) 이전.

## Unreleased — 서버 Go → Rust(axum) 이전 (ADR-0002 이전 4/4단계, `feature/04-rust-server`)
- 변경 파일: `crates/server/**`(신규: `lib.rs` 라우터, `main.rs`, `tests/api.rs`, `Dockerfile`), `packages/protocol/openapi.json`(신규, API 계약 원본), `.dockerignore`(신규, 허용 목록), `Cargo.toml`(members), `scripts/{check,dev-api}.sh`, `Makefile`(`image`), CI(Go 제거), Helm `Chart.yaml` 설명, `infra/k8s/README.md`, docs 01/05/09/12/13, ADR-0001(대체 표기)/0002, README, CLAUDE.md, `packages/protocol/README.md`. 삭제: `services/api`(Go).
- 동작 유지: `/healthz`, `/readyz`, `/api/v1/info` 응답 본문 동일, `/api/v1/*` 그 밖은 501(info에 GET 외 메서드도 501), `nosniff`·`no-store` 헤더, 15초 타임아웃, 기본 주소 `127.0.0.1:8080`·`API_BIND_ADDRESS`, 비-root UID 65532(Helm 값과 일치).
- 달라진 점: 모르는 비 API 경로 404가 텍스트 → JSON `{"error":"not_found"}`; `Content-Type`에서 `charset=utf-8` 생략(JSON은 UTF-8 고정). 추가: `x-request-id`, 요청 로그, SIGTERM graceful shutdown, 1MB 본문 제한(413).
- 테스트: `make check` 통과 — 서버 통합 6개(T-07, T-08, OpenAPI 12개 operation 계약 대조, 413), 수동: 로컬 실행 + curl + SIGTERM 종료 코드 0, `docker build`(41.7MB) 후 `--read-only --cap-drop ALL` 컨테이너로 `/readyz` 200 확인. 이제 Go 없이 모든 검사가 SKIP 없이 실행됨.
- 위험: 이미지가 Go 버전보다 큼(distroless/cc + glibc 동적 링크, 41.7MB). Helm 실제 배포는 하지 않음(클러스터 미지정). CI에서 이미지 빌드는 아직 안 함. 헬스 프로브 요청도 INFO 로그로 남음(`RUST_LOG`로 조정).
- 후속: 로드맵 03단계(공통 도메인/로컬 파일 어댑터 — Tauri와 `crates/core` 연결, 웹 저장소 분리).

## Unreleased — 로드맵 03단계: 저장소 어댑터 + 데스크톱 파일 저장 (`feature/roadmap03-storage-adapters`)
- 웹: `NoteRepository` 인터페이스 도입, 화면은 저장 방식과 무관. IndexedDB 어댑터(기존 동작 그대로), Tauri 파일 어댑터(`storage/tauri.ts`), 실행 환경 자동 선택(`storage/index.ts`). 서비스워커는 Tauri에서 등록하지 않음. 하단 문구에 저장 위치 표시.
- Rust: `crates/core` `workspace` feature — `Workspace`(.md 파일 정본 + SQLite 보조 인덱스). 바깥 파일 자동 등록·수정 감지·삭제 반영, 이름 변경 시 파일 이동(ID 유지), 다른 파일 덮어쓰기 금지(Conflict), 대소문자만 바뀐 이름 안전 처리. `FileStore`에 rename/remove/from_relative/modified 추가. SQLite migration `003_workspace.sql`(meta, history_stacks).
- 데스크톱: Tauri 명령 7개, 앱 데이터 폴더에 인덱스, `custom-protocol` feature, 임시 아이콘, CSP에 `ipc:` 허용, 진단 옵션 `MEMONEST_WEBVIEW_CONSOLE`. 워크스페이스 멤버로 편입(WebKitGTK 없으면 check에서 자동 제외).
- 검증: `make check` 통과 — Web 단위 64개(저장소 계약 6개 포함) + E2E 9개, Rust(작업공간 8개 포함) 전부. `make desktop-smoke`(Xvfb): 화면→Rust→파일/인덱스 동작, 콘솔 오류 없음. CI에 WebKitGTK 설치 + desktop-smoke 추가, actions v5로 갱신(Node 20 경고 해소).
- 발견·수정: 디버그 빌드가 개발 서버(devUrl)를 열어 빈 화면이 되던 문제 → `custom-protocol`; CSP가 Tauri IPC를 막아 느린 대체 경로를 쓰던 문제 → `connect-src ipc: http://ipc.localhost`.
- 위험: 저장할 때마다 노트 전체(Undo 250개 스냅샷 포함)를 IPC로 보내고 세션·Undo를 다시 씀 → 큰 문서에서 느릴 수 있음(델타 저장은 후속). 커밋 중 실패 시 파일 이동과 인덱스가 어긋날 수 있으나 다음 목록 조회에서 파일 기준으로 복구(세션 기록은 잃을 수 있음). Linux에서만 검증. 같은 제목으로 바꾸면 저장 실패 알림만 뜸(화면 내 안내는 후속).
- 환경: 개발 서버에 WebKitGTK 등 설치 중 systemd 259.9 업그레이드가 함께 진행되었고 `systemd-resolved` 설정 단계가 미완료(iU) 상태 — 사용자 확인 대기.
- 도메인: 사용자가 준비 중(현재 후보 1개). 저장소에는 기록하지 않고 `.env`의 `APP_BASE_URL`로만 받으며, 외부 공개(HTTPRoute)는 05단계(OIDC/TLS) 이후.

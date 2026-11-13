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

## Unreleased — 로드맵 04단계: 패키징 (`feature/roadmap04-packaging`)
- 패키징: `apps/native`를 npm 워크스페이스로(@tauri-apps/cli 2.12.3), `npm run -w apps/native bundle|icons|android:*|ios:*`. `tauri.conf.json` 번들 메타데이터(분류, 설명, deb section, rpm release, NSIS 현재 사용자 설치, macOS 11+, Android minSdk 24, iOS 14+). 1024px 원본(`icon-source.png`, 임시 디자인)에서 Windows/macOS/Android/iOS 아이콘 전체 생성.
- 앱: 모바일은 앱 데이터 폴더 `Notes/`에 저장(데스크톱은 기존 `~/MemoNest/Notes`). `MEMONEST_INDEX_PATH`로 인덱스 위치 지정(테스트가 실제 사용자 폴더를 건드리지 않게).
- 실행 확인: `scripts/desktop_smoke.py`(Linux/Windows/macOS 공통) — 조건 충족까지 최대 60초 폴링, 프로세스 그룹 전체 종료(이전 bash 버전은 `timeout`/`terminate`가 xvfb-run만 끝내 앱·WebKit 고아 프로세스가 남던 문제 수정), Linux 시작 멈춤 시 최대 2회 재실행(표시함).
- CI: `.github/workflows/package.yml` — Linux/Windows/macOS 설치 파일 빌드 + 실행 확인 + 산출물 14일 보관, Android 디버그 APK, iOS 시뮬레이터 빌드(실패 허용). 서명/공증 없음.
- 검증(로컬, Linux): `tauri build` → .deb 4.1MB / .rpm 4.1MB / .AppImage 85MB, AppImage 실행 확인 통과, .deb 의존성 `libwebkit2gtk-4.1-0, libgtk-3-0` 확인. `make check` 통과.
- 알려진 문제: 개발 서버 Xvfb에서 Mesa 초기화가 간헐적으로 멈춤(gdb로 확인, 앱 코드 이전 단계) — docs/12_TESTPLAN.md.
- 위험: 설치 파일 서명 없음(SmartScreen/Gatekeeper 경고). AppImage 크기 큼(WebKit 포함). 모바일 화면 레이아웃·파일 내보내기·권한은 미검증.
- CI 결과(https://github.com/kubernetes-operator/pangyo/actions/runs/42343557275): Linux·Windows·macOS 빌드+실행 확인 통과, Android 디버그 APK·iOS 시뮬레이터 빌드 통과. 첫 실행에서 Windows 실행 확인이 스크립트의 콘솔 인코딩(cp1252) 문제로 실패 → UTF-8 고정으로 수정. upload-artifact v7.

## Unreleased — 로드맵 05단계: 계정·로그인·장비 등록·용량 (`feature/roadmap05-accounts`)
- 서버: Postgres 연결(sqlx, `db/postgres/002_accounts.sql`), DB 로그인(이메일+비밀번호, Argon2id), 세션 쿠키(HttpOnly·Secure·SameSite=Strict, 토큰 SHA-256 저장), CSRF 헤더, IP별 요청 제한, 계정 수 상한(트랜잭션 잠금), 등급별 용량 조회, 장비 등록(일회용 코드→Bearer 토큰)·목록·해제, `/readyz` DB 확인, 웹 앱 제공(`WEB_ROOT`), 보안 헤더(CSP 등), 잘못된 본문도 JSON 400.
- 웹: 계정 화면(로그인/가입/로그아웃, 용량, 장비 등록 코드, 장비 목록), 서버 없을 때 안내, Tauri에서는 숨김, Vite 개발 프록시.
- 계약: OpenAPI 20개 operation(구현 13, 계획 7), `x-requires-database`. Google 로그인은 계획(501).
- 배포: Helm 0.2.0 — 내장 Postgres(StatefulSet, Debian 이미지, NetworkPolicy), 사용자 생성 Secret 참조, HTTPRoute(공용 Gateway http/https), 이미지에 웹 앱+마이그레이션 포함(47.6MB).
- 검증: 서버 계정 통합 8개(실제 Postgres, 동시 가입 상한 포함), 무DB 계약 테스트, E2E 12개(실서버+Postgres 2개 포함), 이미지 운영 조건 실행 확인.
- 의존성 조정: rusqlite 0.37 → 0.32 (sqlx와 SQLite 네이티브 라이브러리 버전 충돌 해소).
- 클러스터 조회 결과: 같은 NFS에서 Alpine Postgres가 lock 파일 권한 오류로 재시작 반복 사례 → Debian 이미지 사용.
- 위험: NFS(`nolock`) 위 Postgres — 백업 전 중요 데이터 금지. 요청 제한이 메모리 기반(1대). 비밀번호 재설정·이메일 인증 없음. 공용 Gateway 인증서에 도메인 없음(Cloudflare가 사용자 TLS 처리).
- 첫 배포(2026-11-07) 실패 대응: ① Postgres 첫 initdb가 NFS에서 30초를 넘겨 liveness가 재시작 → 데이터 폴더 손상(`global/pg_filenode.map` 없음), ② API가 시작 시 DB 연결 실패로 종료 → CrashLoop, ③ `helm --wait` 10분 초과로 릴리스 failed. 수정: Postgres `startupProbe`(최대 10분)+liveness는 시작 후, 종료 유예 120초 / API는 DB를 백그라운드 재시도로 연결(그동안 `/healthz` 200, `/readyz` 503 `connecting`, 계정 API 503 `database_connecting`). 테스트 추가(DB보다 서버가 먼저 떠도 정상).
- 두 번째 배포 시도(2026-11-09): 이 클러스터 NFS에서 initdb가 10분을 넘겨(bootstrap만 5분 20초) 시작 검사 한도에 걸림 → 공식 이미지의 초기화 후 단계(DB 생성·pg_hba)가 빠진 채 남음. 수정: `POSTGRES_INITDB_ARGS=--no-sync`(첫 초기화의 fsync만 생략), 시작 대기 30분.
- **배포 완료(2026-11-09)**: `memonest` 네임스페이스, 이미지 `sha-336230a`, Helm 릴리스 `memonest-api` rev 1 deployed. Postgres 초기화 약 10분(재시작 0회). 도메인 확인: `/readyz` database ok, 웹 앱·manifest·sw.js 200, 보안 헤더, 가입 검증/CSRF/로그인 실패/비로그인 401 정상(운영 DB에 테스트 데이터 생성 안 함). Cloudflare 봇 탐지 스크립트가 CSP에 막혀 콘솔 경고(동작 영향 없음).

## Unreleased — 로드맵 06단계: 메모 동기화 + 편집 임대 (ADR-0003 A안, `feature/roadmap06-sync`)
- 결정: 사용자 A안 승인(2026-11-09) → `docs/decisions/ADR-0003.md`, `docs/07_LOCK_APPROVAL.md` 상태 갱신.
- 서버: `db/postgres/003_notes_sync.sql`(본문·폴더·묘비·변경 커서·lease_epoch, note_leases, 대소문자 무시 경로 유일성), 변경 목록/저장/삭제/임대 API, 용량 반영, memonest-core 경로 규칙 공유, 오류 `details`.
- 웹: `src/sync/engine.ts`(가져오기·올리기·임대 획득/갱신/반납·오프라인 초안·충돌 보관/해결), `src/sync/api.ts`, `src/ui/sync-view.ts`(상단 상태, 읽기 전용/충돌/초안/오류 알림), 로그인 시 자동 동기화·첫 연결 시 기존 메모 업로드·다른 계정 메모 섞임 방지, 입력 2분 없으면 임대 반납, 탭 닫을 때 keepalive 반납.
- 데스크톱/코어: 저장소 `deleteNote`, 동기화 상태 보존(`db/sqlite/004_sync_state.sql`). 데스크톱 동기화 연결은 후속.
- 발견·수정: ① 입력 직후 상태가 '동기화됨'으로 남던 문제 → 즉시 '올릴 메모 n' ② 새 메모가 동기화 대상에서 빠지던 문제 ③ 읽기 전용 화면이 최신 내용을 늦게 보이던 문제 → 즉시 가져오기 ④ 숨긴 알림이 빈 상자로 보이던 CSS 버그(`display:grid`가 `hidden`을 덮음).
- DB 점검(2026-11-09, 운영): 재시작 0, 마이그레이션 2/2 성공, fsync/동기 커밋/전체 페이지 쓰기 on, 인덱스 184개 정상, 교착 0, 카탈로그 전수 읽기 정상. 04:02 NAS 순간 지연으로 pg_isready 1초 초과 1회(API 영향 없음) → readiness timeout 5초. **NAS 공유 90% 사용(1.3T 중 1.2T, PVC 39개 공유) — 용량 확보 필요.**
- 테스트: 서버 동기화 5개 + 계정 9개, 웹 단위 76개(엔진 8개), E2E 13개(실서버 두 브라우저 동기화 1개, 3회 반복 통과).
- 위험: 본문을 Postgres에 저장(S3 전) — NAS 성능·용량 영향. 편집 세션·Undo는 기기별(동기화 안 함). 같은 브라우저의 두 탭도 서로 다른 기기로 취급(동시 편집 불가). 웹에 메모 삭제 기능은 아직 없음(서버·동기화는 지원).
- **배포(2026-11-09)**: 릴리스 `memonest-api` rev 2, 이미지 `sha-a61938d`, 마이그레이션 003 적용(`note_leases` 생성), 도메인 `/readyz` ok·웹 200·동기화 API 비로그인 401 확인. 롤아웃 약 75초 중 Postgres 재시작(상태 검사 설정 변경)으로 **약 30~45초 503**(계정·메모 0건이라 데이터 영향 없음). 후속: Postgres 재시작이 필요한 변경은 사용자 공지 후 진행, `/readyz`의 `note: sync_not_ready` 문구 갱신.

## Unreleased — DB 백업 (`feature/roadmap06-db-backup`)
- 조사: Velero 매일 백업은 memonest 미포함, 주간 전체 백업은 실행 중 Postgres 파일을 복사(일관성 보장 없음), Velero 저장소 MinIO도 같은 NAS.
- 추가: Helm `backup.*` — CronJob(매일 03:00 Asia/Seoul) init 컨테이너 `pg_dump -Fc`(임시 이름→완료 시 이름 변경, 14일 보관) + 본 컨테이너가 노드 로컬 임시 Postgres에 복원·검사(테이블 7개, 실패 마이그레이션 0). 백업 PVC 1Gi(`resource-policy: keep`), NetworkPolicy에 백업 Pod 허용. 스크립트 `infra/k8s/helm/memonest/files/`.
- 검증(로컬, 같은 이미지·같은 실행 사용자): 정상 백업 복원 성공 / 깨진 백업·백업 없음은 이유를 남기고 실패. 처음 실패 사례: 백업 폴더 권한 700이면 검증 컨테이너(uid 999)가 못 읽음 → 백업 스크립트가 폴더 755·파일 644로 설정.
- 운영 영향: 기존 Postgres/API 매니페스트 변경 없음(재시작 없음).
- 후속: NAS 밖 사본(다른 장비/외부 S3), WAL 보관(PITR), 백업 실패 알림.
- **적용(2026-11-09)**: 릴리스 rev 3(CronJob·PVC·ConfigMap 추가, NetworkPolicy 갱신, Postgres/API 재시작 없음). 첫 수동 백업 Job 성공: 덤프 16K, 임시 Postgres 복원 검증 성공(마이그레이션 3, 계정 0, 메모 0). 이후 매일 03:00 자동 실행.

## Unreleased — 로드맵 07단계: 서버 검색·연관 메모·n8n 토큰 (`feature/roadmap07-search-n8n`)
- 서버: `db/postgres/004_search.sql`(pg_trgm GIN), `005_integration_tokens.sql`; `GET /search`, `GET /notes/{id}/similar`, `GET/POST /integrations/tokens`, `DELETE /integrations/tokens/{id}`; 모든 노트·계정 API 범위 검사(`insufficient_scope`), 임대 보유자에 토큰 구분.
- 웹: 계정 화면 자동화 토큰(만들기·원문 1회·목록·해제), 메모 화면 '연관 메모'(펼칠 때만 요청).
- 문서: `docs/14_N8N.md`(n8n 사용법), `docs/15_CLOUD_ADAPTERS.md`(07-3 설계안·결정 요청).
- 발견·수정: 본문 일치가 제목 일치보다 앞서던 검색 순위 → 제목 가산점 2.
- 테스트: 서버 3개(검색·연관·토큰), OpenAPI 26 operation 계약, E2E 2개 추가(실서버).
- 운영 정리(2026-11-09, 승인): Released PV 2개 삭제(memonest 실패 볼륨 5Gi, monitoring/timescaledb 50Gi). **NFS CSI 컨트롤러가 NAS를 마운트하지 못해(`mount.nfs: mount system call failed`) PV 삭제 시 NAS 데이터가 지워지지 않음** — NAS 공간 확보는 CSI 문제 해결 또는 NAS에서 직접 정리 필요. 워크로드 없는 PVC 16개(minio 7×500Gi 등)는 소유 팀 확인 대기.
- 위험: 자동화 토큰 요청 제한 없음(후속). pg_trgm 색인으로 저장 시 쓰기량 소폭 증가.
- **배포(2026-11-09)**: 릴리스 rev 4, 이미지 `sha-eecf9e0`, 마이그레이션 004(pg_trgm 1.6, 색인 2개)·005 적용. 배포 중 `/readyz` 2초 간격 확인 전부 200(무중단), Postgres 재시작 없음. 도메인에서 검색·연관·토큰 API 비로그인/가짜 토큰 401 확인.

## Unreleased — 로드맵 08단계: FreeBSD arm64 전용 장비 (`feature/roadmap08-bsd-tui`)
- 앱: 줄 단위 콘솔 → **전체 화면 TUI**(ratatui 0.29): 관리/작성 F6 전환(폭 100칸 이상 두 칸), F2/F3 새 메모·폴더, 한글 2칸 폭 커서, Undo/Redo 250(단어 단위), Tab/Shift+Tab, Ctrl+S, 5초 자동 저장·메모 전환 전 자동 저장, 미저장 종료 경고, 상단 기기 상태·시계.
- 기기 상태(`device.rs`): FreeBSD `sysctl hw.acpi.battery.*`, `ifconfig wlan0`(SSID·연결), `hccontrol read_scan_enable` / Linux sysfs. 셸 없이 고정 명령, 10초마다 별도 스레드.
- 키오스크(`apps/bsd/kiosk/`): 전용 계정(셸=재실행 실행기, 암호 로그인 불가), ttyv0 gettytab 자동 로그인, 멱등 설치·`--dry-run`. 옛 `launch.sh` 제거.
- 검증 도구: `scripts/bsd-vm-verify.py` — FreeBSD 15.1-RELEASE arm64 공식 이미지(SHA256 확인)를 QEMU(TCG)로 부팅, 시리얼 콘솔 자동 조작으로 pkg rust 설치·`cargo test`·release 빌드·pty에서 TUI 키 입력→저장·키오스크 설치 확인. 원본 이미지 불변(오버레이), `--reuse`.
- 개발 서버: QEMU 10.2 설치(새 패키지 19개만, 기존 패키지 업그레이드 없음, 설정 유지 옵션).
- CI: GitHub 러너 Ubuntu 26 전환으로 linuxdeploy(FUSE2 필요) 실행 실패 → `APPIMAGE_EXTRACT_AND_RUN=1`; Android 최신 NDK 30의 32비트 링커 없음 → 러너 LTS NDK, Gradle 다운로드 시간 초과 → 캐시·재시도; 번들 도구 504 → 재시도.
- 테스트: TUI 16개(Linux). **FreeBSD 15.1-RELEASE arm64 VM(QEMU, cortex-a72) 검증 통과 8/8** — arch(aarch64), pkg rust 설치, 소스 전송, `cargo test` 16개 통과, release 빌드(ELF aarch64 FreeBSD), `--version`, TUI 키 입력(F2→새 메모→"안녕 FreeBSD"→Ctrl+S→Ctrl+Q) 저장 확인, 키오스크 설치(사용자·셸 등록·gettytab·ttyv0 자동 로그인, 2회 실행 멱등). 총 3.9분(이미지 재사용).
- 검증 중 수정: `-cpu max`는 UEFI 단계가 매우 느림 → cortex-a72, FreeBSD 15.0 pkg rust가 15.1 libc 요구 → 15.1 이미지, 파이프가 실패를 가림 → `set -o pipefail`, 시리얼 콘솔 한 줄 입력 한도 초과 → 키오스크 확인을 `apps/bsd/vm/kiosk-check.sh`로 분리.
- 미검증(실기기 필요): 보드·2화면·Wi-Fi/BT 칩셋 실제 출력·배터리 센서, **콘솔 한글 입력기 없음**(vt), 한글 글꼴.

## Unreleased — 설치 파일 배포 서비스 (`feature/downloads-service`, 2026-11-09 사용자 요청)
- 요구: 설치 파일을 사용자 Kubernetes로 외부 제공, 직접 주소가 드러나지 않게.
- 서버: `GET /api/v1/downloads`(로그인 사용자, 저장 경로 비노출), `POST /api/v1/downloads/{id}/link`(5분 유효 임의 토큰, 해시만 저장 `db/postgres/006_download_links.sql`), `GET /dl/{token}`(스트리밍, 응답 시간 제한 없음 — 15초 제한을 API 경로에만 적용하도록 분리). 목록 파일 경로 검증(저장소 밖 금지). 자동화 토큰으로는 불가.
- 웹: 계정 화면 '설치 파일' (누를 때 주소 발급 → 바로 다운로드, 화면에 주소 없음).
- 배포: Helm `downloads.*` PVC(2Gi RWX, keep), API 읽기 전용 마운트 / `scripts/publish-downloads.sh`(gh 결과물 → 임시 Pod 복사 → 목록 원자 교체 → 최근 2개 버전 보관).
- 테스트: 서버 3개(경로 비노출·토큰 링크·재다운로드·잘못된 토큰·저장소 밖 경로 제외·비로그인/자동화 토큰 거부·미설정 503), 계약 28 operation, E2E 1개(브라우저 다운로드 파일 내용 확인).
- 위험: NAS(90%) 사용, 링크는 5분 안에 공유되면 다른 사람도 받을 수 있음(로그인 사용자만 발급). iOS는 시뮬레이터 빌드라 제외.
- **배포(2026-11-09)**: 릴리스 rev 5, 이미지 `sha-b130ec5`, 마이그레이션 006, 다운로드 PVC 2Gi(RWX). 배포 중 2초 간격 확인 7회 중 2회 실패(502·응답 없음) → 차트에 `maxUnavailable: 0`, `preStop sleep 8s` 추가(다음 배포부터). 게시: 실행 42572153752의 Windows .msi/.exe, macOS .dmg, Linux .deb/.rpm/.AppImage (102MB). 디버그 APK 464MB는 용량 문제로 제외(`MAX_FILE_MB` 기본 200) — 릴리스(arm64 전용) APK 빌드는 후속. 스크립트 수정: Cargo.toml 버전 줄 주석 때문에 버전을 못 읽던 문제.

## Unreleased — 정리: 준비 상태 문구, Android 설치 파일 축소 (`feature/roadmap09-cleanup`)
- 변경 파일: `crates/server/src/lib.rs`, `crates/server/tests/accounts.rs`, `packages/protocol/openapi.json`, `.github/workflows/package.yml`, `scripts/publish-downloads.sh`.
- `/readyz`의 `note`: 동기화 구현 전 문구 `sync_not_ready` → `sync_ready`(DB 정상) / `sync_unavailable`(DB 연결 중·오류, 503) / `local_only`(DB 미설정). OpenAPI에 값 목록과 503 응답 추가.
- Android CI: arm64만, Rust 디버그 정보 제외(`CARGO_PROFILE_DEV_DEBUG=0`, `STRIP=debuginfo`) → CI 결과 APK **475MB → 29MB**(arm64-v8a만 포함 확인). 게시 스크립트는 `*-debug.apk`를 찾음.
- 운영: API 롤링 설정(preStop 8초, maxUnavailable 0) rev 6 배포 — 이미지 변경 없음, `/healthz` 30/30 200. 외부 경로(Cloudflare→게이트웨이) 응답이 1~4초로 느림, API 자체는 약 20ms.
- 테스트: 서버 테스트(실 Postgres) 통과.
- **배포(2026-11-11)**: PR #3 병합(main `c6f7802`) 후 릴리스 rev 7, 이미지 `sha-b130ec5` → `sha-c6f7802`(값·차트·마이그레이션 변경 없음). `/readyz` → `{"database":"ok","note":"sync_ready","status":"ok"}`. 배포 중 외부 `/healthz` 2초 간격 60회 중 56회 200, 4회 5초 제한 초과(간헐적으로 떨어져 발생, 배포 후에도 응답 5.3초 1회 관측 → 외부 경로 지연으로 추정, 롤링 중단 여부는 미확정).
- **설치 파일 게시(2026-11-11)**: package 실행 42850476933 — Windows .msi/.exe, macOS .dmg, Linux .deb/.rpm/.AppImage, **Android arm64 디버그 APK 29MB 첫 게시**. 다운로드 PVC 사용량 131.6MB, 버전 0.1.0 교체.

## Unreleased — 로드맵 06 마무리: 데스크톱·모바일 앱 동기화 연결 (`feature/roadmap06-device-sync`, ADR-0004)
- 변경 파일: `apps/native/src-tauri/src/{remote.rs(신규),lib.rs}`·`Cargo.toml`(ureq rustls+ring, 데스크톱 keyring 3, 시험 전용 `e2e` feature)·`tauri.conf.json`(설명 문구), `crates/core/src/workspace.rs`(바깥 편집·이름 변경·삭제 처리, `save_sessions`, `list/forget_external_deletion`), `db/sqlite/005_external_deletions.sql`(신규), `apps/web/src/account/{api.ts(전송 방식 분리),device-panel.ts(신규),panel.ts(서버 주소 표시·문구)}`, `apps/web/src/storage/{tauri.ts,repository.ts}`, `apps/web/src/sync/{engine.ts,api.ts}`(서버 삭제·다시 받기), `apps/web/src/ui/{app.ts,sync-view.ts}`, `scripts/desktop_sync_check.py`(신규), `Makefile`(`desktop-sync-check`), docs 08/12, ADR-0004.
- 동작: 앱 '계정' → **장비 연결**(서버 주소 + 웹에서 만든 등록 코드 + 장비 이름). 장비 토큰은 Rust 쪽 OS 보안 저장소에만 있고 화면에는 오지 않음. 동기화 규칙은 웹과 같음(편집 임대·읽기 전용·충돌 보관·오프라인 초안). 20초마다 디스크를 다시 읽어 **앱 밖 편집은 올림**, **앱 밖 이름 변경은 같은 ID로**, **앱 밖 삭제는 확인 후**(서버에서도 삭제 / 다시 받기). 웹에서 장비를 폐기하면 앱은 동기화를 멈추고 다시 연결 안내.
- 함께 고친 기존 버그(데스크톱): 열어 두기만 한 메모를 10초마다 저장할 때 본문까지 다시 써서 **바깥 편집기에서 고친 내용을 덮어쓰던 문제** → 본문을 바꾸지 않았으면 세션 기록만 저장.
- 서버·API 변경 없음(기존 `/pairing/*`, Bearer 장비 토큰, `DELETE /notes/{id}` 사용). 서버 배포 불필요. 웹은 계정 화면 문구와 서버 주소 표시만 바뀜.
- 테스트: `make check` 통과 — Web 단위 80개(엔진 바깥 삭제 4개 추가) + E2E 16개(실서버 포함), core 작업공간 14개(신규 5), native 2개(서버 주소·허용 경로), 서버 DB 테스트. **`make desktop-sync-check` 7/7 통과**(Linux Xvfb, 실제 앱 화면 조작, 격리 키링): 장비 연결, 파일→서버, 서버→파일, 바깥 편집, 바깥 이름 변경(같은 ID), 바깥 삭제 보관→화면에서 서버 삭제, 토큰이 앱 데이터에 없음, 장비 폐기 시 중지 안내.
- 위험: Windows·macOS는 CI 빌드만, **실제 키체인/자격 증명 관리자 동작 미확인**. Android는 토큰을 앱 전용 폴더에 평문 보관(루팅 기기에서 노출 가능, Keystore 연동은 후속), 실기기 미확인. Linux에서 Secret Service(키링)가 없으면 연결할 수 없음(안내 문구 표시, 등록 코드는 쓰지 않음). 바깥 이름 변경과 동시에 내용도 바꾸면 '삭제 + 새 메모'로 처리됨. 동기화 주기마다 전체 폴더를 다시 읽음(메모 수천 개면 느릴 수 있음, 파일 감시는 후속).
- 후속: Windows/macOS/Android 실기 확인, Android Keystore, 파일 변경 감시(notify), iOS, BSD TUI 동기화(08단계).

## Unreleased — 로드맵 10: 운영 지표·알림 (`feature/roadmap10-monitoring`)
- 변경 파일: `crates/server/src/{metrics.rs(신규),lib.rs,main.rs}`, `crates/server/tests/{metrics.rs(신규),common/mod.rs}`, Helm `values.yaml`(`monitoring.*`)·`templates/{monitoring.yaml(신규),deployment.yaml,service.yaml}`·`files/dashboard.json`(신규)·`Chart.yaml` 0.3.0, docs/09.
- 서버: `METRICS_BIND_ADDRESS`가 있을 때만 별도 포트에 `/metrics`(Prometheus 텍스트). API 포트·외부 경로에는 없음. 의존성 추가 없음(직접 구현).
  - HTTP 요청 수·응답 시간 히스토그램: 라벨은 경로 **템플릿**(`/api/v1/notes/{id}`, `/dl/{token}`)만 → 메모 ID·다운로드 토큰이 지표에 남지 않음.
  - DB: `memonest_db_up`, 계정/상한, 활성 장비, 메모(삭제 제외/묘비), 사용 용량/한도 합, 활성 편집 임대. 수집마다 3초 제한 조회. 이메일·제목·본문 등 개인 정보 없음.
- Helm(`monitoring.enabled`, 기본 꺼짐): 지표 포트, Service `component: api` 라벨, ServiceMonitor, PrometheusRule 7개(API 다운·DB 장애·5xx 비율·p95·계정 상한·백업 지연·Pod 재시작, 기준값은 values), Grafana 대시보드 12패널.
- 테스트: 서버 지표 2개(템플릿 라벨·ID 비노출·API 포트 미노출 / DB 지표, 실 Postgres), `make helm-lint`, 렌더링한 PromQL 23개를 클러스터 Prometheus 쿼리 API로 문법 확인(조회만).
- 발견: **공용 Alertmanager가 199일째 생성 실패**(`volumeClaimTemplates` storage 값 누락) → 지금은 알림이 어디로도 전달되지 않음. 공용 리소스라 이 변경에서는 손대지 않음.
- 위험: 지표 수집마다 DB에 count 쿼리 7개(30초 간격, 데이터가 커지면 비용 증가 → 그때 캐시). 서버 재시작 시 카운터 초기화(rate로 처리). replica 1대 기준.
- 배포(2026-11-12, PR #6 `2e26f8c`): rev 10 `sha-2e26f8c`, `monitoring.enabled=true`, `prometheusRelease=prometheus-stack`.
  - 첫 시도 rev 9는 `--reuse-values`로 새 기본값(`monitoring.alerts.*`)이 빠져 webhook이 규칙 거부 → 리소스 생성 전 실패, API는 rev 8 그대로. `--reset-then-reuse-values`로 재실행해 성공.
  - 확인: 수집 대상 `serviceMonitor/memonest/memonest-api/0` up, `memonest_db_up` 1, 계정 상한 999, 알림 7개 모두 `inactive`/정상 평가, Grafana 사이드카가 `memonest.json` 적재, 외부 `/metrics`는 웹 앱 HTML만(지표 0줄).
  - 배포 중 외부 `/healthz` 60회: 200 57회, 시간 초과 3회(기존 외부 경로 지연과 같은 양상).
- 후속: Alertmanager 복구·알림 받을 곳(메일/메신저) 결정, Postgres 자체 지표(postgres-exporter), 외부 경로 지연 측정(Cloudflare→게이트웨이).

## Unreleased — 로드맵 10: 다운로드 링크 일회용 (`feature/roadmap10-single-use-downloads`)
- 변경 파일: `crates/server/src/downloads.rs`, `db/postgres/007_download_links_single_use.sql`(신규, `claimed_at`·`completed_at` 열 추가), `crates/server/tests/downloads.rs`, OpenAPI 설명, `infra/k8s/README.md`, docs/12 T-26.
- 동작: `GET /dl/{token}`이 링크를 원자적으로 "받는 중"으로 차지 → 파일 끝까지 보내면 `completed_at` 기록, 이후 같은 링크는 404.
  - 받는 중에는 같은 링크의 다른 요청 404(유출된 링크로 동시 수신 차단).
  - 중간에 끊기거나(연결 끊김) HEAD처럼 본문을 보내지 않은 요청은 표시를 풀어 5분 안에 재시도 가능(큰 설치 파일·느린 외부 경로 대비). 다시 받으려면 앱에서 새 링크.
  - 서버가 받는 도중 죽으면 그 링크는 만료(5분)까지 잠김 → 새 링크로 받으면 됨. 웹 화면은 누를 때마다 새 링크를 만들므로 변경 없음.
- 테스트: 다운로드 통합 테스트 갱신 + 1개 추가(T-26). 완료 기록을 빼면 실패하는 것 확인(회귀 검출).
- 배포(2026-11-12, PR #8 `dc1ad15`): rev 11 `sha-dc1ad15`(`--reset-then-reuse-values`, 모니터링 설정 유지). API 로그 `database connected and migrated`(007 적용), `/readyz` ok, 없는 링크 `/dl/...` 404, 지표 수집 up. 배포 중 외부 `/healthz` 60회: 200 56회, 시간 초과 4회(기존 외부 경로 지연).
- 위험: "끝까지 보냄"은 서버가 파일을 다 읽어 보낸 시점 기준(중간 프록시·클라이언트 수신 완료까지는 보장하지 않음). 이어받기(Range)는 지원하지 않음(기존과 같음).

## Unreleased — 로드맵 10: 외부 경로 지연 조사 (문서만)
- `docs/09_OPERATIONS.md`에 측정 결과·원인·선택지 기록. 클러스터·Cloudflare 설정 변경 없음(조회와 `/healthz`·`/cdn-cgi/trace` 요청만).
- 원인: 한국에서 Cloudflare 미국 엣지(SEA/LAX)로 연결되어 사용자↔엣지 구간이 0.4~2초+. 게이트웨이+API는 약 20ms. 배포 중 "시간 초과"는 유실이 아니라 5초 넘는 지연.
- 발견: 게이트웨이 인증서에 `note.studiobasa.com` 없음 → Cloudflare→원본 TLS 미검증.
- 조치(2026-11-13): 공용 게이트웨이 인증서에 `note.studiobasa.com` 추가·재발급, 원본 직접 HTTPS 검증 통과(`ssl_verify_result=0`). Cloudflare SSL Full (strict) 전환은 사용자가 진행.

## Unreleased — 로드맵 10: 서버 응답 대기 한도 10초 → 30초 (`feature/roadmap10-client-timeout`)
- 변경 파일: `apps/web/src/account/api.ts`(`REQUEST_TIMEOUT_MS`), `apps/native/src-tauri/src/remote.rs`(`REQUEST_TIMEOUT`).
- 이유: 국내에서 Cloudflare 미국 엣지(SEA/LAX/PDX)를 거쳐 외부 응답이 최대 9초까지 걸림(2026-11-12 측정) → 10초 한도에서는 느린 순간에 "서버 없음/오프라인"으로 잘못 판단. 프록시 끄기·유료 전환은 보류(사용자 결정), 대신 대기 한도를 늘림.
- 영향: 서버가 정말 멈춘 경우 오프라인 표시까지 최대 30초. 네트워크가 끊긴 경우는 즉시 실패라 그대로. 편집 임대 90초(30초마다 갱신)보다 짧음. 서버 쪽 처리 한도(15초)는 서버 안쪽 시간이라 변경 없음.
- 배포(2026-11-13, PR #12 `0b1339f`): rev 12 `sha-0b1339f`, 외부 `/healthz` 60/60 200(대기 15초). 웹 번들에 30초 반영 확인. 설치 파일 재게시(package 실행 43345087269, 7개, 145.1M) → 앱도 30초.
- 알림 운영 결정(2026-11-13, 사용자): 알림 규칙 7개는 유지, **메일 등 통보(Alertmanager)는 설정하지 않음** — Prometheus Alerts·Grafana "MemoNest" 화면에서 확인.

## Unreleased — 사용 의견 반영 1차 (2026-11-13, `feature/ux-feedback-1`)
- 변경 파일: `apps/web/src/ui/{app.ts,name-dialog.ts(신규),main.css}`, `apps/web/index.html`, `apps/web/src/settings.ts`, `apps/web/src/account/{panel.ts,device-panel.ts,device-name.test.ts(신규)}`, `apps/web/src/storage/tauri.ts`, `apps/native/src-tauri/{src/lib.rs,Cargo.toml}`(`device_hostname` 명령, whoami — sqlx가 이미 쓰는 크레이트), E2E 3개 파일.
- 1. 장비 등록 기본 이름: 호스트 이름을 앞에 (`DESKTOP-AB12 (Windows)`). Android는 호스트 이름이 보통 `localhost`라 기기 모델(`SM-S918N (Android)`), 둘 다 없으면 예전 형식.
- 2. 가입 시 비밀번호 확인 입력(두 번), 다르면 가입하지 않음.
- 3. n8n 토큰: "새 토큰 만들기"를 눌렀을 때만 입력란 표시 (목록은 항상).
- 4. 계정 화면에 "장비 추가 방법" 3단계를 강조 상자로 표시.
- 5. 새 메모/새 폴더: 브라우저 `prompt()` 대신 앱 자체 한 줄 입력 창 — Enter = 만들기(한글 조합 중 Enter 제외), Esc = 취소, 잘못된 폴더는 창 안에 이유 표시(alert 없음). 메모 제목 칸에서 Enter → 본문으로 이동.
- 7. "0-2 DEPTH" → "폴더 2단계까지", 기기 상태 줄 한글화(배터리·네트워크·블루투스, 배터리는 지원 브라우저·Android에서 % 표시), "표시 설명" 펼침 추가. 하단 "Tree (...)" → "연관 그래프: 준비 중"(요구 22, 미구현).
- 8. 트리 강조: 메모를 열면 그 메모와 그 메모의 폴더만 강조(예전엔 루트 `/`가 남아 강조됨).
- 테스트: `make check` 통과(Vitest 84, Playwright 17 — 새 사용 의견 시나리오 1개 추가, 가입 비밀번호 불일치·토큰 입력 접기·장비 추가 안내 검사), `make desktop-sync-check` 7/7, Tauri clippy.
- 후속: 6. 관리자 화면에서 저장공간 변경(2차), 9. 프로젝트(폴더 위 단계, 설계안 먼저 — 3차).

## Unreleased — 사용 의견 6번: 관리자 화면에서 저장공간 변경 (`feature/admin-quota`)
- 변경 파일: `crates/server/src/{admin.rs(신규),accounts.rs,config.rs,lib.rs}`, `db/postgres/008_admin_settings.sql`(신규: `system_settings`, `accounts.quota_custom`), `crates/server/tests/{admin.rs(신규),api.rs}`, OpenAPI(관리자 4개 operation, `Account.isAdmin`), 웹 `account/{api.ts,panel.ts}`·`main.css`, Playwright 설정·E2E, Helm 0.4.0(`admin.existingSecret` → `ADMIN_EMAILS`, optional), 문서(05, 13, infra README).
- 관리자: `ADMIN_EMAILS`의 이메일로 로그인한 계정(자동화 토큰 불가). **관리자 이메일은 저장소·values에 넣지 않고 사용자가 만든 Secret `memonest-admin`에서 읽음**(조직 규칙: 개인정보 저장 금지). Secret이 없으면 아무도 관리자가 아님.
- 웹 계정 화면 **시스템(관리자)**: 계정당 기본 저장공간(MB) 저장 → 따로 정하지 않은 모든 계정에 즉시 적용(기존 계정 포함), 계정 목록(사용량/용량)에서 계정별 용량 적용·기본값으로. 새 가입자는 화면에서 정한 기본값.
- 기본값 우선순위: 화면에서 저장한 값(`system_settings.default_quota_mb`) > `FREE_STORAGE_MB`.
- 테스트: 서버 통합 2개(비관리자 403, 기본값 변경 → 기존 계정 적용, 계정별 유지·되돌리기, 새 가입자, 잘못된 값 400, 없는 계정 404 / ADMIN_EMAILS 없으면 아무도 관리자 아님), 설정 단위 1개, OpenAPI 계약 32개, E2E 2개(관리자 화면, 비관리자에겐 없음).
- 위험: 용량을 사용량보다 작게 정하면 그 계정은 메모를 늘리는 저장이 거절됨(기존 메모는 그대로, 줄이는 저장은 가능).

## Unreleased — 9번 프로젝트 1/3단계: 서버 (ADR-0005 승인, `feature/projects-server`)
- 변경 파일: `docs/decisions/ADR-0005.md`(승인), `db/postgres/009_projects.sql`(신규: `projects`, `notes.project_id`, 경로 유일성에 프로젝트 포함), `crates/server/src/{projects.rs(신규),notes.rs,lib.rs}`, `crates/server/tests/{projects.rs(신규),api.rs}`, OpenAPI(프로젝트 3개 operation, `Note.projectId`, `NoteChanges.projects`, `PutNote.projectId`, `?projects=1`), 문서(01 #30, 05).
- 동작: `GET /projects`, `PUT /projects/{id}`(만들기·이름 바꾸기, 클라이언트가 ID), `DELETE /projects/{id}`(빈 프로젝트만, 묘비). 메모 `projectId`(null = 기본, 필드 없음 = 유지). 같은 경로라도 프로젝트가 다르면 허용.
- 예전 클라이언트 보호: `?projects=1`을 보내지 않으면 기본이 아닌 프로젝트의 메모를 묘비(본문 없음)로 보냄, 예전 클라이언트의 저장은 프로젝트를 바꾸지 않음.
- 테스트: 프로젝트 통합 2개(만들기·이름·중복·잘못된 이름·경로 중복 허용·빈 프로젝트만 삭제·삭제된 프로젝트 저장 거절·다른 계정 격리 / 변경 목록·예전 클라이언트 묘비·프로젝트 유지·limit=1 페이지 나누기), 이름 규칙 단위 1개, OpenAPI 35개.
- 아직: 웹 화면(2/3), 데스크톱 파일 배치(3/3). 이 단계만 배포해도 기존 화면은 그대로 동작(프로젝트를 만들 화면이 아직 없음).

## Unreleased — 9번 프로젝트 2/3단계: 웹 화면 (`feature/projects-web`)
- 변경 파일: `apps/web/src/domain/{projects.ts(신규),types.ts}`(`Note.projectId`, `normalizeNote`가 유지), `apps/web/src/sync/{engine.ts,api.ts,engine-projects.test.ts(신규)}`, `apps/web/src/ui/{app.ts,main.css}`, `apps/web/index.html`, `apps/web/src/settings.ts`, E2E 2개.
- 화면: 정리 영역 맨 위 **프로젝트** 줄(선택·+ 프로젝트·이름 바꾸기·삭제). 트리·검색·메모 수·새 메모·새 폴더는 선택한 프로젝트만. 본문 아래 "프로젝트 이동" 선택으로 메모 옮기기(같은 폴더에 같은 이름이 있으면 거절). 메모가 있는 프로젝트는 지울 수 없음. 마지막 프로젝트 기억. 다른 프로젝트 메모(연관 메모)를 열면 그 프로젝트로 전환.
- 저장: IndexedDB 버전·저장소 그대로 — 프로젝트 목록은 meta `projects`, 프로젝트별 빈 폴더는 folders에 `<프로젝트ID>::<경로>`(`:`는 폴더 이름 금지 문자라 겹치지 않음), 메모에 선택 필드 `projectId`.
- 동기화: `?projects=1`로 프로젝트 변경 수신, 올리기 순서 = 프로젝트 만들기·이름 → 메모 → 프로젝트 지우기(메모를 옮긴 뒤여야 빈 프로젝트). 서버가 지우기를 거절(다른 기기가 메모 추가)하면 되살림. 지우기 대기 중인 프로젝트는 서버 내용으로 덮어쓰지 않음(단위 테스트로 찾은 버그 2건 수정).
- 데스크톱·모바일 앱은 3/3단계 전까지 프로젝트 화면을 숨기고 예전 방식(`projectId`를 보내지 않음 → 서버가 프로젝트 유지, 다른 프로젝트 메모는 받지 않음).
- 테스트: `make check` 통과(Vitest 89 — 프로젝트 동기화 4개·normalizeNote 1개 추가, Playwright 21 — 로컬 프로젝트 시나리오·두 기기 프로젝트 전달 추가), `make desktop-sync-check` 7/7.

## Unreleased — 9번 프로젝트 3/3단계: 데스크톱·모바일 (`feature/projects-desktop`)
- 변경 파일: `crates/core/src/{store.rs,workspace.rs,paths.rs}`, `db/sqlite/006_projects.sql`(신규), `crates/core/tests/workspace.rs`, `packages/protocol/fixtures/paths.json`(`@` 시작 폴더 금지), `apps/web/src/domain/paths.ts`, `apps/web/src/ui/app.ts`(앱에서도 프로젝트 화면, 디스크 다시 읽을 때 프로젝트 목록 갱신), `apps/native/src-tauri/src/remote.rs`(`/projects` 허용), `scripts/desktop_sync_check.py`(8a~8c), 문서(ADR-0005, 01, 04).
- 파일 배치: 기본 프로젝트는 `Notes/` 그대로(기존 파일 이동 없음), 다른 프로젝트는 `Notes/@이름/폴더/폴더/메모.md`. 프로젝트 ID ↔ 이름은 SQLite `projects`, 화면이 목록을 저장할 때 새 프로젝트 등록·이름 바꾸기(폴더 이름도)·빈 프로젝트 폴더 정리.
- 앱 밖: `@이름` 폴더를 만들거나 이름을 바꾸면 새 프로젝트로 등록(메모 ID는 내용으로 유지). 규칙에 맞지 않는 `@` 폴더는 무시.
- 공통 규칙: 폴더 이름은 `@`로 시작할 수 없음(paths.json → TS·Rust·서버).
- 테스트: core 통합 2개 추가(파일 배치·이름 바꾸기·폴더 키·기본으로 옮기기 충돌·빈 프로젝트 정리 / 앱 밖 `@` 폴더 등록·이름 변경 시 ID 유지), `make desktop-sync-check` 10/10 + 콘솔 오류 없음(8a 웹 프로젝트 메모 → `@여행/제주/일정.md`, 8b 앱 밖 `@개인` → 서버 프로젝트, 8c 웹에서 이름 바꾸기 → `@제주 여행`).
- 아직: Windows·Android 실기기 확인(파일 관리자에서 `@` 폴더가 보이는지 등).

## Unreleased — 사용 의견 2차 (2026-11-13, `fix/app-projects-and-feedback-2`)
- 확인 내역 대응
  - **앱에서 만든 프로젝트가 웹에 안 보임**: 리눅스 데스크톱 실기에서는 재현되지 않음(`desktop-sync-check` 8d 추가, 통과). 대신 원인이 될 수 있는 결함을 고침 — 프로젝트를 모르던 예전 버전에서 업데이트된 기기는 그 사이 건너뛴 프로젝트·(묘비로 받아 지운) 다른 프로젝트 메모를 다시 받지 못했음 → **처음 한 번 처음부터 다시 받기**(`syncProjectsAware`). 서버에 못 올린 프로젝트는 목록에 `↑`, 서버가 거절하면 `⚠` + 이유 표시, 상단 "올릴 변경"에 프로젝트도 셈.
  - **기본 프로젝트 이름 바꾸기**: 계정 설정 `defaultProjectName`(서버 마이그레이션 010, `PATCH /api/v1/account`), 다른 기기는 변경 목록 응답으로 받음. 비우면 "기본". 데스크톱 파일 위치는 그대로(루트).
- 기능 추가
  - **메모 삭제**: 본문 아래 "메모 삭제" + 확인 창(기본 선택은 취소). 서버에 올라간 메모는 서버에서도 지움(다른 기기가 편집 중이거나 먼저 고쳤으면 지우지 않고 안내, 서버 연결 필요).
  - **메모 폴더 이동**: 본문 아래 "폴더 이동" 선택(같은 프로젝트 안, 같은 이름이 있으면 거절).
  - **새 메모 기본 제목**: `2026-11-13 1530 Memo` 형식(`:`는 파일 이름에 쓸 수 없어 시분을 붙여 씀).
- 테스트: `make check` 통과(서버 통합 1개·Vitest 93(업데이트 후 재수신 1개)·Playwright 23(로컬 삭제·이동·기본 제목, 두 기기 기본 프로젝트 이름·삭제 전달)), OpenAPI 36, `make desktop-sync-check` 통과(8d 추가).


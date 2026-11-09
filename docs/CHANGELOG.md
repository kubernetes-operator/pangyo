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

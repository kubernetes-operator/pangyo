# 12. 테스트 시나리오

자동화 위치: Web 단위 테스트 `apps/web/src/domain/domain.test.ts`, Web E2E `apps/web/e2e/app.spec.ts` (모두 `make check`에 포함).

| ID | 단계 | 검증 방법 | 성공 조건 |
|---|---|---|---|
| T-01 | 웹 오프라인 | 노트 편집 후 새로고침 / 네트워크 분리 | 내용 보존 (IndexedDB) |
| T-02 | 폴더 | `Work/Project` 생성 후 `Work/Project/Deep` 시도 | 2단계 이하만 허용 |
| T-03 | 파일 | MD/TXT 각각 작성하고 다운로드 | UTF-8 내용 동일 |
| T-04 | Undo | 250회 입력 후 새로고침, Undo | 최대 250회 기록 유지 (용량 조건) |
| T-05 | 편집시간 | 입력 간격 10초 → 2분 방치 → 재입력; 창 숨김 후 복귀 | 작성 시간은 10초만, 열어둔 시간은 방치 포함·숨김 제외, 숨김 복귀 시 새 세션 |
| T-06 | UI | F6 및 편집 버튼 클릭 | 작성/관리 화면 전환 |
| T-07 | 서버 | `curl /healthz` | 200 OK |
| T-08 | 보안 | 서버 `/api/v1/sync` 요청 | 구현 전에는 501 명시 |
| T-09 | 배포 | Helm lint/template (Helm 필요) | 렌더/스키마 오류 없음 |
| T-10 | FreeBSD | 실 ARM64 기기 테스트 | 초기 골격에서는 미검증 |

웹/네이티브를 통틀어 **단일 작성 잠금**은 아직 테스트 대상이 아님(사용자 승인 필요).

## 자동화 현황 (2026-11-04)
| ID | 자동화 | 비고 |
|---|---|---|
| T-01 | E2E | 새로고침 보존 + 네트워크 차단 후 서비스워커로 앱 로드·편집 |
| T-02 | E2E + 단위 | 3단계 폴더 거부, 상대 경로/콜론 거부 |
| T-03 | E2E | MD/TXT 다운로드 파일을 UTF-8로 읽어 원문과 비교 |
| T-04 | E2E + 단위 | 260회 편집 → 새로고침 → Undo 250회 후 `v10`, Redo 확인 |
| T-05 | E2E (가짜 시계) + 단위 | 작성/열어둔 시간 분리 집계, 창 숨김 시 집계 중단과 재개 세션 (T-05, T-05b) |
| T-06 | E2E | F6, 집중 모드 버튼, 2패널 버튼 |
| 호환 | E2E | v0.1 JS가 만든 IndexedDB(undo/redo 없음, 언어 설정)를 그대로 읽고 편집 |
| T-07 | Rust 통합 | `/healthz` 200 + 보안 헤더 + `x-request-id` |
| T-08 | Rust 통합 + 계약 | 미구현 `/api/v1/*` 501, `openapi.json`의 12개 operation 전부 실제 응답과 대조, 1MB 초과 본문 413 |
| T-09 | 수동 | `make helm-lint`, `make image` (CI 미포함) |
| T-10 | 미자동화 | FreeBSD 실기기 |
| T-11 | Rust 통합 | 데스크톱 작업공간: 파일↔인덱스 왕복, 이름 변경 시 ID 유지, 다른 메모 덮어쓰기 금지, 바깥 파일 등록·수정 감지·삭제 반영, 세션 교체, 마이그레이션 반복 적용 (`crates/core/tests/workspace.rs`) |
| T-12 | 단위 | 저장소 계약: IndexedDB(fake-indexeddb)·Tauri(IPC 모의) 어댑터가 같은 동작 (`repository.test.ts`) |
| T-13 | 실행 | `scripts/desktop_smoke.py`: 실제 앱이 화면→Rust→파일/인덱스까지 동작 (Linux Xvfb·Windows·macOS, CI `package.yml`) |
| T-14 | 패키징 | `tauri build` 설치 파일: Linux .deb/.rpm/.AppImage(로컬+CI), Windows .msi/.exe, macOS .dmg(CI). AppImage도 실행 확인 |
| T-15 | 모바일 | Android 디버그 APK 빌드(CI), iOS 시뮬레이터 빌드 시도(CI, 서명 없음). 에뮬레이터 실행 확인은 미자동화 |
| T-16 | Rust 통합(Postgres) | 계정: 가입/중복/입력 검증, 로그인·로그아웃, CSRF, 동시 가입 상한, 가입 차단, 장비 등록·Bearer·해제, 요청 제한, readyz (`crates/server/tests/accounts.rs`) |
| T-17 | E2E(실서버+Postgres) | 웹 계정 화면 가입→용량→등록 코드→로그아웃→재로그인, HttpOnly 쿠키 (`e2e/account.spec.ts`) |
| T-18 | 이미지 | 읽기 전용·권한 없음으로 실행: 웹 앱 제공, 보안 헤더, Secure 쿠키, readyz DB ok |
| T-19 | Rust 통합(Postgres) | 동기화: 버전 충돌 409, 임대 배타·갱신·반납, 만료 후 늦은 저장 fencing 차단, 변경 목록·묘비·용량, 계정 격리, 경로/용량 규칙 (`crates/server/tests/notes.rs`) |
| T-20 | 단위 | 동기화 엔진 두 기기 시나리오 8개 (`src/sync/engine.test.ts`) |
| T-21 | E2E(실서버, 브라우저 2개) | 다른 기기 전달 → 편집 중 읽기 전용(최신 내용 표시) → 반납 후 편집 → 오프라인 초안 충돌 → 둘 다 보관 → 사본 전달 (`e2e/sync.spec.ts`, 3회 반복 통과) |
| T-22 | Rust 통합(Postgres) | 검색: 한국어 제목·본문, 제목 우선, 미리보기, `%` 글자 그대로, 계정 격리 / 연관 메모 / 자동화 토큰: 범위(읽기·쓰기·계정 불가), 토큰으로 토큰 발급 불가, n8n 임대 표시, 원문 비노출, 해제 즉시 401, 입력 검증 (`tests/search_tokens.rs`) |
| T-23 | E2E(실서버) | 계정 화면 토큰 발급→Bearer로 API→해제 후 401, 연관 메모 패널 표시·이동 |
| T-24 | Rust 통합 + 단위 | 앱 밖 변경(ADR-0004): 바깥 편집→올릴 변경, 바깥 이름 변경→ID 유지(애매하면 추측 안 함), 바깥 삭제→보관·다시 받기·잊기, 세션만 저장 시 본문 보존 (`workspace.rs`) / 서버 주소·허용 경로 규칙 (`remote.rs`) / 엔진 서버 삭제·다시 받기(deleted·changed·busy·gone·오프라인) (`engine.test.ts`) |
| T-25 | 실기(Linux Xvfb + 테스트 Postgres) | `make desktop-sync-check`: 실제 데스크톱 앱 화면에서 장비 연결 → 파일↔서버 양방향 → 바깥 편집·이름 변경 반영 → 바깥 삭제 확인 후 서버 삭제 → 토큰이 앱 데이터에 없음 → 웹에서 장비 폐기 시 동기화 중지 안내. 격리된 D-Bus + 임시 키링, 시험 전용 `e2e` feature(배포 빌드 제외). Windows·macOS·Android 실기는 미자동화 |
| T-26 | Rust 통합(Postgres) | 다운로드 링크 일회용: 끝까지 받으면 완료 기록 후 재사용 404, 받는 중 같은 링크 다른 요청 404, 본문을 다 읽지 않고 끊으면 다시 열려 전체 파일 재수신, 완료 뒤 HEAD도 404 (`crates/server/tests/downloads.rs`) |

### 알려진 문제
- **개발 서버 Xvfb에서 데스크톱 앱 시작이 간헐적으로 멈춤 (2026-11-07)**: AMD Ryzen 5 6600H + Mesa 26.0.8(libgallium) + Xvfb 조합에서 GTK가 OpenGL 화면을 만들 때 Mesa 초기화(`driCreateNewScreen3` → `pthread_once` → `pthread_setaffinity`)가 멈춤(gdb 확인, 약 20~50%). 앱 코드 이전 단계이며 `LP_NUM_THREADS=0`, `GDK_GL=disable`, `GALLIUM_DRIVER=softpipe`, Xvfb GLX 끔, DMABUF/컴포지팅 끔 모두 효과 없음. GitHub 러너에서는 재현 안 됨. 대응: 실행 확인 스크립트가 Linux에서 20초 안에 웹뷰가 시작되지 않으면 최대 2회 재실행하고 결과에 표시. **실제 데스크톱(GPU 있는 세션)에서 발생하는지 미확인.**

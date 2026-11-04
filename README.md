# MemoNest v0.1 — Offline-first minimal notes

> **범위:** 장기 개발용 기준 설계 + 웹 오프라인 프로토타입 + Rust(axum) API 헬스체크 + Tauri/FreeBSD/K8s 스캐폴드. **완성 제품이 아닙니다.**

## 0. 이해한 요구
- 1~29번 요구사항은 `docs/01_REQUIREMENTS.md`의 ID로 추적합니다.
- 사용자 데이터는 로컬 우선이며, 동기화·검색·계정은 선택 기능입니다.
- **동시 편집 잠금(#28)은 사용자 승인 전 구현 금지**: `docs/07_LOCK_APPROVAL.md`.
- Git 주소, 이미지 저장소, Kubernetes 주소/컨텍스트는 미정이므로 예제 변수만 제공합니다.

## 1. 프로젝트 실행 순서
1. `cp config/project.env.example .env` 후 **본인 환경**의 변수 기입 (`.env`는 Git 제외).
2. `make setup` (최초 1회: npm 의존성 + 테스트용 Chromium) → `make dev-web` -> 브라우저 `http://127.0.0.1:5173` (Node 22+ 필요).
   - 오프라인(서비스워커) 동작은 빌드 결과에서만: `make build-web && npm -w apps/web run preview` → `http://127.0.0.1:4173`.
3. 선택: `make dev-api` -> `http://127.0.0.1:8080/healthz` (Rust는 `rust-toolchain.toml`로 자동 설치). 이미지: `make image`.
4. `make check` -> JSON, 웹 타입/단위/빌드/E2E, Rust fmt/clippy/test(서버 계약 테스트 포함), 비밀값 가드. 명령 목록은 `make help`.
   - Rust 툴체인은 `rust-toolchain.toml`로 고정되어 rustup이 자동 설치합니다.
5. Git 리모트가 준비되면 `./scripts/git-init.sh`를 **수동**으로 실행.
6. Kubernetes 배포 전 `infra/k8s/README.md` 확인. API의 동기화·로그인 기능은 아직 구현 전.

## 2. 구현 완료와 미구현 구분
| 구분 | 현재 제공 | 아직 제공하지 않음 |
|---|---|---|
| Web (TypeScript + Vite) | IndexedDB 로컬 메모, 폴더 2단계, 에디터, MD/TXT 다운로드, 250개 Undo/Redo 스냅샷, 간단 활동 로그, 1/2패널 전환, 국/영 UI, 오프라인 PWA 구성 | 브라우저 파일시스템에 자동 `.md` 저장, 동기화, 계정/과금/AI/서버 검색 |
| Native | Tauri 프로젝트 골격 | OS별 실파일/SQLite 연결, 듀얼 디스플레이, 모바일 패키징 검증 |
| FreeBSD | Rust 콘솔 편집기(폴더 2단계, md/txt, 원자적 저장 — `crates/core` 공유, Linux에서만 검증)/기기 설정 | 한글 IME, 2개 실제 디스플레이, 블루투스·Wi-Fi 드라이버 검증, 장비 보안 강화 |
| Server | Rust(axum) 헬스체크 API(`crates/server`), OpenAPI 계약·계약 테스트, 컨테이너 이미지, Helm 배포 골격 | 인증/싱크/저장소/검색/요금제의 실제 처리 |

**중요:** 브라우저의 IndexedDB는 로컬 장비에 저장되지만 일반 디렉터리의 `.md` 파일로 자동 저장되지는 않습니다. MD/TXT **내보내기**는 됩니다. 네이티브 앱의 실제 파일 저장은 후속 단계에서 파일 어댑터로 구현합니다.

## 3. 기술 스택 전환 중 (ADR-0002)
화면은 TypeScript 웹 UI(웹·Tauri 공용), 나머지(공통 도메인 `crates/core`·서버 `crates/server`·BSD `apps/bsd`)는 Rust로 통일했습니다(이전 4/4 완료).

## 3-1. 문서 읽는 순서
`01_REQUIREMENTS → 02_ARCHITECTURE → 03_UX → 04_DATA → 05_SYNC_API → 06_SECURITY → 07_LOCK_APPROVAL → 08_ROADMAP → 09_OPERATIONS → 10_BSD → 11_DEV_LOOP`

## 4. 기여와 변경 원칙
- `AGENTS.md`(Claude Code는 `CLAUDE.md`도)를 매 개발 세션마다 먼저 확인합니다.
- 의사결정은 `docs/decisions/`에 ADR로 기록합니다.
- 계약 변경은 `packages/protocol/`과 마이그레이션을 함께 갱신합니다.
- 기능 완료 표기는 반드시 실행 및 테스트 증거가 있을 때만 합니다.

## 5. 외부 접속 변수 위치와 개발 루프
- 각 설정의 실제 적용 여부를 `docs/13_CONFIG_MAP.md`에서 확인합니다.
- `./scripts/loop.sh 21`은 개발 단계 및 검증을 실행하며 **자동 소스 수정/커밋/배포는 하지 않습니다**.
- 참고: Graph나 듀얼 실물 디스플레이는 아직 미구현이며 `diagrams/*.drawio`에서 구조를 수정할 수 있습니다.

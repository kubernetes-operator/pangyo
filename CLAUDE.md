# CLAUDE.md

이 저장소의 공통 규칙은 **`AGENTS.md`** 입니다. 작업 전에 반드시 읽고 따릅니다.

## 빠른 명령
- `make setup` — 최초 1회 (npm 의존성 + E2E용 Chromium)
- `make check` — 전체 검증, E2E 포함 (커밋 전 필수) / `make check-fast` — E2E 제외
- `make test` / `make e2e` / `make fmt` — 단위 테스트 / 웹 E2E / Rust 정렬
- `make dev-web` — 웹 프로토타입 (http://127.0.0.1:5173)
- `make helm-lint` — Helm 차트 정적 검사 (클러스터 접속 없음)

## 기술 스택 (ADR-0002)
- 화면: `apps/web` TypeScript + Vite (웹 PWA + Tauri 공용). 순수 규칙은 `src/domain/`, 저장은 `src/storage/`, DOM은 `src/ui/`
  - IndexedDB 이름/버전/레코드 형식은 v0.1과 동일해야 함 (e2e 호환 테스트가 검사)
- 그 외: Rust 워크스페이스 (`crates/core` 공통 도메인, `crates/server` API, `apps/bsd` TUI)
- 계약: `packages/protocol` (OpenAPI/JSON Schema가 단일 원본)

## 꼭 지킬 것
- 요구 #28 동시 편집 잠금, CRDT 자동 병합은 **승인 전 구현 금지** (`docs/07_LOCK_APPROVAL.md`).
- `config/project.env`, `.env`는 읽거나 수정하지 않음 (실제 값 포함 가능).
- kube context, Git remote, 키를 코드/설정에 하드코딩하지 않음. 배포는 사용자 승인 후.
- 변경 완료 시 `docs/CHANGELOG.md` 갱신, 요구 상태는 테스트 증거가 있을 때만 변경.

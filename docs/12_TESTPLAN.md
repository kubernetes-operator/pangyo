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

### 알려진 문제
- **개발 서버 Xvfb에서 데스크톱 앱 시작이 간헐적으로 멈춤 (2026-11-07)**: AMD Ryzen 5 6600H + Mesa 26.0.8(libgallium) + Xvfb 조합에서 GTK가 OpenGL 화면을 만들 때 Mesa 초기화(`driCreateNewScreen3` → `pthread_once` → `pthread_setaffinity`)가 멈춤(gdb 확인, 약 20~50%). 앱 코드 이전 단계이며 `LP_NUM_THREADS=0`, `GDK_GL=disable`, `GALLIUM_DRIVER=softpipe`, Xvfb GLX 끔, DMABUF/컴포지팅 끔 모두 효과 없음. GitHub 러너에서는 재현 안 됨. 대응: 실행 확인 스크립트가 Linux에서 20초 안에 웹뷰가 시작되지 않으면 최대 2회 재실행하고 결과에 표시. **실제 데스크톱(GPU 있는 세션)에서 발생하는지 미확인.**

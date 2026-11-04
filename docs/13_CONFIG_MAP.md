# 13. 변수와 설정 파일 위치

| 파일 | 역할 | 커밋 가능 | 실제 사용 여부 |
|---|---|---|---|
| `config/project.env.example` | Git/K8s/API/계정 등 모든 외부 접속 자리 | 예 | 복사한 `.env`를 스크립트가 사용 |
| `.env` | 개인 환경 값 / 키 없는 접속 주소 | **아니요** | shell scripts |
| `config/client.example.yaml` | UI·로컬 정책 목표 설정 | 예 | **설계값, 웹 코드는 현재 `apps/web/src/settings.ts`의 SETTINGS 사용** |
| `apps/web/vite.config.ts` | 웹 개발/미리보기 호스트·포트 | 예 | Vite, E2E |
| `apps/web/build/offline-sw.ts` | 오프라인 서비스워커 생성 규칙 | 예 | 빌드 시 `dist/sw.js` 생성 |
| `package.json`, `package-lock.json` | npm 워크스페이스·의존성 고정 | 예 | `npm ci` |
| `config/server.example.yaml` | 서버 계정 등급/스토리지 정책 | 예 | 설계값, 서버 적용 전 |
| `config/cloud.example.yaml` | 개인 클라우드/공통 클라우드 전략 | 예 | 설계값, 동기화 전 |
| `config/integrations.example.yaml` | AI/n8n 연동 프로바이더 | 예 | 설계값, 연결 전 |
| `config/device.freebsd.example.toml` | BSD 단독 장비·터미널·입력 정책 | 예 | 설계값, 실기기 검증 전 |
| `infra/k8s/helm/memonest/values.yaml` | K8s 메모 API Pod 설정 | 예 | Helm 배포 시 사용 |
| `CLAUDE.md` | Claude Code 진입 문서 (AGENTS.md 참조) | 예 | Claude Code 사용 시 |
| `Makefile`, `scripts/check.sh` | 공통 개발/검증 명령 | 예 | 로컬·CI 공통 |
| `Cargo.toml`, `rust-toolchain.toml` | Rust 워크스페이스·툴체인 고정 | 예 | cargo/rustup |
| `.github/workflows/ci.yml` | CI (`make check`) | 예 | GitHub push/PR |
| `config/project.env` | 실제 값이 든 로컬 사본 | **아니요** (.gitignore) | `.env`로 옮겨 사용 권장 |
| `AGENTS.md` | 에이전트 간 공통 정책 | 예 | 개발 시 확인 |

현재 **이중 설정 관리의 중복 위험**을 줄이기 위해 후속 3단계에서 `client.example.yaml → 검증 → 웹/네이티브 공통 생성 설정`으로 통일할 계획입니다. 지금 `client.example.yaml`의 값을 바꾸면 웹 동작이 자동 변경된다고 주장하면 안 됩니다.

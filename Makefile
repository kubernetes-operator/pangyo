# === MemoNest 개발 명령 모음: 모든 사람/CI/에이전트가 같은 진입점을 사용 ===
# 사용법: make help
.PHONY: help setup db-test-up db-test-down check check-fast fmt test e2e dev-web build-web dev-api image bsd desktop desktop-smoke desktop-sync-check helm-lint

IMAGE ?= memonest-api:dev ## make image IMAGE=<레지스트리>/memonest/api:<태그>

help: ## 명령 목록
	@grep -E '^[a-z-]+:.*##' $(MAKEFILE_LIST) | awk -F':.*## ' '{printf "  %-10s %s\n", $$1, $$2}'

setup: ## 최초 1회: npm 의존성 + E2E용 Chromium 설치
	npm ci --no-audit --no-fund
	npx -w apps/web playwright install chromium

db-test-up: ## 테스트 전용 Postgres 실행 (Docker, 127.0.0.1:55432, 비밀번호 없음 — 테스트에만 사용)
	docker run -d --rm --name memonest-test-pg -e POSTGRES_HOST_AUTH_METHOD=trust -p 127.0.0.1:55432:5432 postgres:17-alpine

db-test-down: ## 테스트 Postgres 정리
	docker stop memonest-test-pg

check: ## 전체 검증 (JSON, Web 타입/단위/빌드/E2E, Rust fmt/clippy/test, 비밀값 가드)
	./scripts/check.sh

check-fast: ## E2E를 뺀 빠른 검증
	SKIP_E2E=1 ./scripts/check.sh

fmt: ## Rust 코드 자동 정렬
	cargo fmt --all

test: ## 단위 테스트 (Web Vitest + Rust)
	npm run -s -w apps/web test
	cargo test --workspace

e2e: ## 웹 E2E 회귀 테스트 (Playwright)
	npm run -s -w apps/web e2e

dev-web: ## 웹 개발 서버 (http://127.0.0.1:5173, 서비스워커 비활성)
	./scripts/dev-web.sh

build-web: ## 웹 빌드 → apps/web/dist (오프라인 서비스워커 포함)
	npm run -s -w apps/web build

dev-api: ## API 서버 실행 (http://127.0.0.1:8080/healthz)
	./scripts/dev-api.sh

image: ## API 컨테이너 이미지 빌드 (로컬, push 안 함)
	docker build -f crates/server/Dockerfile -t $(IMAGE) .

desktop: ## 데스크톱 앱 실행 (웹 빌드 포함, 메모 폴더: $$MEMONEST_NOTES_DIR 또는 ~/MemoNest/Notes)
	npm run -s -w apps/web build
	cargo run -p memonest-native --features custom-protocol

desktop-smoke: ## 데스크톱 앱 실행 확인 (Xvfb, 임시 폴더 사용)
	./scripts/desktop-smoke.sh

desktop-sync-check: ## 데스크톱 앱 ↔ 로컬 테스트 서버 동기화 실기 확인 (Linux, make db-test-up 필요)
	npm run -s -w apps/web build >/dev/null
	cargo build -q -p memonest-server
	MEMONEST_BUILD=0000000check cargo build -q -p memonest-native --features custom-protocol,e2e # 앱 안 업데이트 확인용 빌드 표시
	python3 scripts/desktop_sync_check.py

bsd: ## BSD 콘솔 메모 앱 실행 (로컬 테스트)
	cargo run -p memonest-bsd

helm-lint: ## Helm 차트 정적 검사 (클러스터 접속 없음)
	helm lint infra/k8s/helm/memonest --set image.repository=example.invalid/memonest

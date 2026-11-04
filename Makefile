# === MemoNest 개발 명령 모음: 모든 사람/CI/에이전트가 같은 진입점을 사용 ===
# 사용법: make help
.PHONY: help check fmt test dev-web dev-api bsd helm-lint

help: ## 명령 목록
	@grep -E '^[a-z-]+:.*##' $(MAKEFILE_LIST) | awk -F':.*## ' '{printf "  %-10s %s\n", $$1, $$2}'

check: ## 전체 검증 (JSON, JS, Rust fmt/clippy/test, Go, 비밀값 가드)
	./scripts/check.sh

fmt: ## Rust 코드 자동 정렬
	cargo fmt --all

test: ## Rust 테스트만 실행
	cargo test --workspace

dev-web: ## 웹 프로토타입 실행 (http://127.0.0.1:5173)
	./scripts/dev-web.sh

dev-api: ## API 서버 실행 (http://127.0.0.1:8080/healthz)
	./scripts/dev-api.sh

bsd: ## BSD 콘솔 메모 앱 실행 (로컬 테스트)
	cargo run -p memonest-bsd

helm-lint: ## Helm 차트 정적 검사 (클러스터 접속 없음)
	helm lint infra/k8s/helm/memonest --set image.repository=example.invalid/memonest

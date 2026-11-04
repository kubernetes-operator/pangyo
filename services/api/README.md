# API 서비스

- 로컬 실행: `cd services/api && go run .`
- 확인: `curl http://127.0.0.1:8080/healthz`
- 현재: healthz / readyz / info만 제공, `/api/v1/sync/...`는 HTTP 501.
- 동기화가 구현된 것처럼 보이는 가짜 로그인 또는 무인증 업로드 경로는 제공하지 않습니다.
- Kubernetes에서는 API_BIND_ADDRESS=0.0.0.0:8080.

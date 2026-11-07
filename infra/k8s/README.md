# Kubernetes Helm 스캐폴드

**상태:** Rust(axum) API health/info only (`crates/server`). 인증·노트 동기화가 없으므로 외부 HTTPRoute는 기본 비활성화입니다.

1. 이미지를 빌드하여 본인 레지스트리에 Push: `make image IMAGE=<레지스트리>/memonest/api:0.1.0` 후 `docker push`(수동). 빌드 컨텍스트는 저장소 루트이며 `.dockerignore` 허용 목록으로 env 파일은 들어가지 않습니다.
2. `cp config/project.env.example .env` 후 `KUBE_CONTEXT`, `KUBE_NAMESPACE`, `K8S_IMAGE_REPOSITORY`를 채웁니다.
3. `kubectl config get-contexts`, `kubectl --context <name> get ns` 확인.
4. `./scripts/deploy.sh` 실행. 스크립트는 명시된 context를 사용하고 미기입 값이면 중단.
5. `kubectl --context <name> -n <namespace> get deploy,svc` 후 `port-forward` 및 health 확인.

## 운영 주의
- 이 Helm 차트는 Postgres, MinIO/S3, OIDC, 유사 검색 인덱서를 **설치하지 않음**.
- Kubernetes 상 Secret에 접근하는 자동 구성은 포함하지 않음. 현재 API에는 필요 없음.
- 외부 노출이 필요한 경우 TLS, OIDC, rate-limit, API 권한 검사를 갖춘 이후 활성화.
- 에어갭 환경에서는 API 런타임 이미지도 사전 준비하고 digest로 고정 권장.

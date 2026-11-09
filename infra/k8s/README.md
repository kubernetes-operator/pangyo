# Kubernetes 배포 (Helm)

**구성:** API 서버(웹 앱 함께 제공, `crates/server`) + 내장 PostgreSQL(StatefulSet) + HTTPRoute(선택).
계정 기능(가입/로그인/장비 등록/용량)은 동작하고, 메모 동기화는 아직 없습니다(로드맵 06).

## 사전 준비 (사용자가 직접)
1. **DB 비밀번호 Secret** — 차트는 비밀값을 만들지 않습니다. URL에 들어가므로 영문·숫자만:
   ```sh
   kubectl -n <namespace> create secret generic memonest-db \
     --from-literal=password="$(openssl rand -hex 24)"
   ```
   값을 바꾸면 Postgres 내부 비밀번호도 함께 바꿔야 합니다(초기화 후에는 환경변수만 바꿔서는 적용 안 됨).
2. 이미지: `.github/workflows/image.yml`이 GHCR에 게시하거나 `make image IMAGE=<레지스트리>/memonest-api:<태그>` 후 직접 push.

## 설치 / 업그레이드
```sh
helm upgrade --install memonest-api infra/k8s/helm/memonest \
  --kube-context <context> --namespace <namespace> \
  --set image.repository=<레지스트리>/memonest-api --set image.tag=<태그> \
  --set httpRoute.enabled=true --set httpRoute.hostname=<도메인> \
  --set httpRoute.gatewayName=<gateway> --set httpRoute.gatewayNamespace=<gateway 네임스페이스>
```
- `./scripts/deploy.sh`는 위 명령을 `.env` 값으로 실행하며 실행 전 `DEPLOY` 입력을 요구합니다.
- 확인: `kubectl -n <namespace> get pods,svc,pvc,httproute`, `curl https://<도메인>/readyz` → `"database":"ok"`.

## 운영 주의
- 내장 Postgres는 단일 인스턴스, 기본 StorageClass(NFS)에 저장. NFS 마운트 옵션에 `nolock`이 있어 강제 종료 시 손상 위험 → **백업(pg_dump/PITR) 설정 전 중요한 데이터 보관 금지**.
- Postgres는 같은 릴리스의 API Pod만 접속 가능(NetworkPolicy).
- 요청 제한은 서버 메모리 기반이라 `replicaCount: 1` 기준.
- 공용 Gateway의 https 인증서에 도메인이 없어도 Cloudflare가 사용자 TLS를 처리. Cloudflare→원 서버 구간 엄격 검증(Full strict)을 쓰려면 인증서에 도메인 추가 필요(공용 리소스 변경 — 별도 승인).
- Cloudflare 프록시가 HTML에 봇 탐지 스크립트(`/cdn-cgi/challenge-platform`)를 삽입하지만 앱 CSP(`script-src 'self'`)가 차단 → 브라우저 콘솔에 CSP 경고만 남고 앱 동작에는 영향 없음. 없애려면 Cloudflare에서 해당 호스트의 JavaScript Detections를 끄거나 CSP에 예외 추가(보안 검토 필요).
- 첫 설치 때 Postgres 초기화는 이 클러스터 NFS에서 약 10분(공식 이미지의 DB 생성 포함). `helm --wait --timeout 35m` 사용.
- Google 로그인: Google Cloud에서 OAuth 클라이언트를 만든 뒤 Secret으로 주입하는 후속 작업 (`/api/v1/auth/google/start`는 현재 501).

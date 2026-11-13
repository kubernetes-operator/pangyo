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

## 설치 파일 배포
- **관리자(저장공간 설정)**: 관리자 이메일은 저장소·values에 넣지 않고, 사용자가 직접 Secret을 만듭니다.
  `kubectl -n memonest create secret generic memonest-admin --from-literal=emails='<관리자 이메일>'` → API Pod 재시작 후 적용
  (`kubectl -n memonest rollout restart deploy/memonest-api`). 그 이메일로 로그인하면 계정 화면에 **시스템(관리자)** 영역이 보입니다.
  기본 저장공간을 바꾸면 따로 정하지 않은 모든 계정에 바로 적용되고, 계정별로 따로 정할 수도 있습니다.
- **앱 안 업데이트(A안)**: CI가 앱에 빌드 커밋(`MEMONEST_BUILD`)을 넣고, `publish-downloads.sh`가 목록(manifest)에 `build`를 기록합니다.
  연결된 앱은 시작할 때와 6시간마다 비교해 다르면 상단에 **새 버전 있음**을 띄우고, 누르면 그 기기용 설치 파일을 일회용 주소로 받아 OS 설치 화면으로 넘깁니다(서명 키 불필요, 설치는 사용자가 확인).
- 웹 앱 → 계정 → **설치 파일**: 로그인 사용자에게 목록 표시, **받기**를 누르면 5분 유효한 **일회용** 임의 주소(`/dl/<토큰>`)로 내려받음(끝까지 받으면 링크가 닫히고, 받는 중 같은 링크의 다른 요청은 거절, 중간에 끊기면 5분 안에 재시도 가능 — 다시 받으려면 **받기**를 한 번 더). 실제 저장 위치·버전 폴더는 드러나지 않음.
- 저장: PVC `<릴리스>-downloads`(2Gi, RWX, uninstall 후에도 유지), API는 읽기 전용 마운트.
- 게시(운영 변경, 승인 후): `./scripts/publish-downloads.sh <package 워크플로 실행 ID>` — GitHub 결과물 받기 → 임시 Pod로 복사 → 목록 파일은 마지막에 교체(받는 중인 사용자 영향 없음) → 최근 2개 버전만 보관.

## DB 백업과 복원
- **자동 백업**: CronJob `<릴리스>-backup`이 매일 03:00(Asia/Seoul) `pg_dump -Fc`로 백업 PVC `<릴리스>-backup`에 저장하고 14일 보관(최신 1개는 항상 유지). 같은 Job이 **매번 임시 Postgres(노드 로컬 디스크)에 실제로 복원해 테이블·마이그레이션을 검사** — 복원이 안 되는 백업은 Job 실패로 남습니다.
- 확인: `kubectl -n <ns> get cronjob,jobs` / 결과: `kubectl -n <ns> logs job/<job> -c dump` 와 `-c verify-restore`.
- 지금 한 번 실행: `kubectl -n <ns> create job --from=cronjob/<릴리스>-backup <릴리스>-backup-manual-$(date +%s)`
- 백업 PVC는 `helm uninstall` 후에도 남습니다(`helm.sh/resource-policy: keep`).
- 오프사이트: 클러스터 Velero 주간 전체 백업이 이 PVC의 덤프 파일도 MinIO로 가져갑니다. **MinIO도 같은 NAS에 있으므로 NAS 장애 대비로는 부족** — 다른 장비로 복사하는 것은 후속 과제.

### 복원 절차 (운영 데이터를 덮어쓰므로 반드시 승인 후)
1. 쓰기 중단: `kubectl -n <ns> scale deploy/<릴리스> --replicas=0`
2. 복원용 Pod에서 백업 PVC와 DB 연결: `kubectl -n <ns> run pg-restore --rm -it --image=postgres:17-bookworm --overrides='<backup PVC를 /backup에 마운트, 레이블 app.kubernetes.io/component=backup>' -- bash`
   (NetworkPolicy상 DB 접속은 `component=backup` 레이블 Pod만 허용)
3. Pod 안에서: `export PGPASSWORD=...` (Secret 값은 사용자가 직접 입력) 후
   `pg_restore -h <릴리스>-postgres -U memonest -d memonest --clean --if-exists --no-owner /backup/memonest-<시각>.dump`
4. 재개: `kubectl -n <ns> scale deploy/<릴리스> --replicas=1` → `/readyz` 확인

## 운영 주의
- 내장 Postgres는 단일 인스턴스, 기본 StorageClass(NFS)에 저장. NFS 마운트 옵션에 `nolock`이 있어 강제 종료 시 손상 위험 → **백업(pg_dump/PITR) 설정 전 중요한 데이터 보관 금지**.
- Postgres는 같은 릴리스의 API Pod만 접속 가능(NetworkPolicy).
- 요청 제한은 서버 메모리 기반이라 `replicaCount: 1` 기준.
- 공용 Gateway의 https 인증서에 도메인이 없어도 Cloudflare가 사용자 TLS를 처리. Cloudflare→원 서버 구간 엄격 검증(Full strict)을 쓰려면 인증서에 도메인 추가 필요(공용 리소스 변경 — 별도 승인).
- Cloudflare 프록시가 HTML에 봇 탐지 스크립트(`/cdn-cgi/challenge-platform`)를 삽입하지만 앱 CSP(`script-src 'self'`)가 차단 → 브라우저 콘솔에 CSP 경고만 남고 앱 동작에는 영향 없음. 없애려면 Cloudflare에서 해당 호스트의 JavaScript Detections를 끄거나 CSP에 예외 추가(보안 검토 필요).
- 첫 설치 때 Postgres 초기화는 이 클러스터 NFS에서 약 10분(공식 이미지의 DB 생성 포함). `helm --wait --timeout 35m` 사용.
- Google 로그인: Google Cloud에서 OAuth 클라이언트를 만든 뒤 Secret으로 주입하는 후속 작업 (`/api/v1/auth/google/start`는 현재 501).

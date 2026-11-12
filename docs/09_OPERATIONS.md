# 09. Git / Kubernetes / 운영 정책

## Git
- 원격 저장소가 결정되면 `config/project.env.example`을 `.env`로 복사해 `GIT_REMOTE_URL` 입력.
- `scripts/git-init.sh`는 기존 리모트가 있으면 그대로 두며, `push`하지 않음.
- 개인 `.env`, API 키, kubeconfig, PKI, 사본 노트는 절대 추적하지 않음.
- Git 서버 인증은 로컬 OS의 SSH agent/credential helper가 관리하며 프로젝트가 키를 수집하지 않음.

## Kubernetes
- 현재 컨텍스트를 반드시 확인하고 명시적으로 승인된 클러스터에만 배포.
- 서버 API는 **stateless**로 설계, 파일 저장은 별도 DB/Object Storage 사용.
- 서버 인증과 외부 공개 기능은 미구현이므로 Helm 골격은 **내부 서비스**로만 설치.
- Gateway API/HTTPRoute는 인증·TLS·도메인 준비 전 비활성화.
- Postgres, S3, Redis/큐, 외부 secrets는 운영 환경에 이미 있다고 가정하지 않음.

## 모니터링/운영
- 서버: request_id, latency, 4xx/5xx, sync queue depth, quota, indexed backlog.
  - 현재 구현: 모든 응답에 `x-request-id`(없으면 생성), 요청마다 INFO 로그(method, uri, request_id, status, latency). 미구현 501은 WARN. 레벨은 `RUST_LOG`로 조정(헬스 프로브 로그가 많으면 `tower_http=warn`).
  - SIGTERM 수신 시 진행 중 요청을 마치고 종료(graceful shutdown).
  - **지표·알림(로드맵 10, 2026-11-12)**: `METRICS_BIND_ADDRESS`(Helm `monitoring.enabled`)를 주면 **별도 내부 포트** `:9464/metrics`
    (외부 HTTPRoute는 API 포트만 연결). 요청 수·응답 시간(경로 템플릿만 라벨, 메모 ID·토큰 없음), DB 연결, 계정/상한·활성 장비·메모 수·사용 용량·편집 임대 수.
    ServiceMonitor + PrometheusRule(API 다운, DB 장애, 5xx 비율, p95 응답 시간, 계정 상한 근접, 백업 26시간 지연, Pod 재시작) +
    Grafana 대시보드 ConfigMap(`grafana_dashboard=1`). Prometheus가 고르는 `release` 라벨은 클러스터마다 다르므로 `--set monitoring.prometheusRelease=...`.
    **알림 전달은 클러스터 Alertmanager에 달려 있음** — 2026-11-12 확인 시 공용 Alertmanager가 저장소 설정 누락으로 실행되지 않는 상태(별도 조치 필요).
- 데이터: DB PITR, object versioning, 사용자 단위 export/삭제 정책 확정.
  - **구현(2026-11-09)**: 매일 `pg_dump` 백업 + 매번 복원 검증 CronJob, 14일 보관, 백업 PVC 유지 정책. 절차는 `infra/k8s/README.md`. PITR(WAL 보관)과 NAS 밖 사본은 후속.
- Cloud 저장소의 장애 시 메모 편집 지속, 동기화 실패 알림 및 지수 백오프.
- 지역별 법률·개인정보·결제 요구는 상용 서비스 시작 전에 별도 검토.

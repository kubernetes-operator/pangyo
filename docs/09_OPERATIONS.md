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
    **차트에 새 기본값이 추가된 업그레이드는 `--reuse-values` 대신 `--reset-then-reuse-values`**(Helm 3.14+) — `--reuse-values`는 새 차트의
    기본값을 무시해 `monitoring.alerts.*`가 비고, 운영자 webhook이 PrometheusRule을 거부함(2026-11-12 rev 9 실패, 클러스터 변경 없음).
    **알림 전달은 클러스터 Alertmanager에 달려 있음** — 2026-11-12 확인 시 공용 Alertmanager가 저장소 설정 누락으로 실행되지 않는 상태(별도 조치 필요).
- **외부 경로 지연 조사(2026-11-12, 조회·측정만)**: 외부 `/healthz` 응답 0.4~9초, 배포 중 5초 시간 초과가 가끔 나던 원인.
  - 구간별: 서버 내부 p99 5ms / LAN에서 게이트웨이 직접 HTTPS 약 20ms / Cloudflare 경유 TTFB p50 1.4초·p90 4.8초(40회, 모두 게이트웨이 도착 — 유실 아님, 5초 넘게 늦은 것).
  - 한국(`loc=KR`)에서 Cloudflare 엣지가 **미국 SEA·LAX**로 연결됨. 원본을 거치지 않는 `/cdn-cgi/trace`도 0.38~1.9초 → 지연 대부분은 **사용자↔Cloudflare 엣지** 구간(국내 통신사↔Cloudflare 무료/Pro 요금제 연결 경로). 클러스터 설정으로는 개선 불가.
  - LAX 엣지(104.21.80.218)가 SEA(172.67.187.71)보다 느림: 중앙값 2.0초 vs 0.83초, TLS 핸드셰이크 최대 1.8초.
  - 덧붙여 확인: 게이트웨이 인증서(공용 `letsencrypt-issuer-public-tls`)에 `note.studiobasa.com`이 없음(studiobasa.com, test1, test2만) → Cloudflare SSL이 Full(검증 안 함)일 때만 동작하고, Cloudflare→원본 구간 인증서는 검증되지 않음.
  - **조치(2026-11-13, 사용자 승인)**: 공용 Certificate `default/letsencrypt-issuer-public-cert` dnsNames에 `note.studiobasa.com` 추가(`kubectl patch`, 게이트웨이 변경 없음) → DNS-01 재발급(revision 5, 만료 2027-02-11). 4개 호스트 모두 체인·호스트 이름 검증 OK.
    남은 단계(사용자, Cloudflare 대시보드): SSL/TLS 모드 **Full (strict)**. 되돌리기: dnsNames에서 note 한 줄 제거.
    참고: 첫 patch는 cert-manager webhook TLS 핸드셰이크 시간 초과(제어 노드→워커 노드)로 거부, 재시도에서 성공 — 반복되면 노드 간 네트워크 점검.
  - 개선 선택지: (가) Cloudflare 프록시 끄기(DNS only, 원본 직접 — 국내 수십 ms, 대신 원본 IP 노출·DDoS 보호 없음, 인증서에 note 추가 필요) (나) 프록시 유지 + 인증서에 note 추가 후 SSL Full(strict) (다) 국내 POP가 있는 CDN/요금제. 앱은 오프라인 우선이라 지연은 주로 로그인·동기화에 영향.
- 데이터: DB PITR, object versioning, 사용자 단위 export/삭제 정책 확정.
  - **구현(2026-11-09)**: 매일 `pg_dump` 백업 + 매번 복원 검증 CronJob, 14일 보관, 백업 PVC 유지 정책. 절차는 `infra/k8s/README.md`. PITR(WAL 보관)과 NAS 밖 사본은 후속.
- Cloud 저장소의 장애 시 메모 편집 지속, 동기화 실패 알림 및 지수 백오프.
- 지역별 법률·개인정보·결제 요구는 상용 서비스 시작 전에 별도 검토.

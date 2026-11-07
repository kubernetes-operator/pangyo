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
- 데이터: DB PITR, object versioning, 사용자 단위 export/삭제 정책 확정.
- Cloud 저장소의 장애 시 메모 편집 지속, 동기화 실패 알림 및 지수 백오프.
- 지역별 법률·개인정보·결제 요구는 상용 서비스 시작 전에 별도 검토.

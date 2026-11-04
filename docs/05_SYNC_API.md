# 05. 동기화 및 공개 API 계약(설계 단계)

> 서버 v0.1에서 아래 API는 **아직 동작하지 않습니다**(HTTP 501). `/healthz`, `/readyz`, `/api/v1/info`만 현재 활성화입니다.
> **계약 원본은 `packages/protocol/openapi.json`** (`x-memonest-status: implemented|planned`). 이 표와 다르면 OpenAPI가 우선이며, `crates/server/tests/api.rs`가 모든 operation을 실제 서버와 대조합니다.

## 권장 API 경로 v1
| 메서드/경로 | 용도 | 최소 권한 |
|---|---|---|
| `POST /api/v1/pairing/start` | 장비 등록 일회용 코드/QR | 로그인된 사용자 |
| `POST /api/v1/pairing/finish` | 일회용 코드 교환 및 device token 발급 | 유효한 코드 |
| `GET /api/v1/notes?cursor=...` | 소유자 노트 변경 목록 | notes:read |
| `GET /api/v1/notes/{id}` | 단일 노트 읽기 | notes:read |
| `PUT /api/v1/notes/{id}` | 버전 조건으로 노트 변경 | notes:write |
| `POST /api/v1/sync/commit` | idempotency key와 version을 포함한 배치 | notes:write |
| `GET /api/v1/notes/{id}/similar` | 연관 문서 | notes:read |
| `GET /api/v1/account/quota` | 본인 사용량 및 요금제 | account:read |
| `POST /api/v1/integrations/tokens` | n8n 전용 범위/만료 토큰 발급 | owner |

## 클라우드 저장소 연결
- **모드 A: 자체 Kubernetes 서버의 공통 S3 저장소** (계정 등급별 할당량).
- **모드 B: 개인 S3/WebDAV 연결** (향후 별도 adapter; 원격 저장소의 실제 사용량/요금은 사용자가 부담).
- Google Drive/Dropbox 등 OAuth 직접 연동은 플랫폼 약관/토큰 관리 검증 이후 선택적으로 추가.
- 모든 API는 TLS, 재시도 및 중복 커밋 방지에 대한 서버 측 검사가 필요.

## n8n
- OAuth scope 또는 단기 만료 PAT로 **사용자 본인**의 메모만 접근.
- 목록/읽기/쓰기 API를 사용하며, 파일 시스템 직접 마운트 금지.
- rate limit과 감사 로그 적용, 토큰 유출 시 서버에서 즉시 폐기.

## 무선 네트워크 간 전송
- LAN: mDNS는 서버 발견용만 사용, 인증은 pairing 코드/인증서로 동일하게 수행.
- 인터넷: TLS 엔드포인트로 연결, device access token은 OS secure store에 저장.
- 장치 간 계정의 **암호/로그인 정보 자체를 복사하지 않음**. 만료되는 등록 코드로 승인된 인증 정보만 교환.

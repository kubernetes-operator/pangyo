# 06. 보안 · 용량 제한 · AI

## 보안 경계
- 개인 비밀번호, API key, Git key, kubeconfig는 Git 및 Helm values에 저장하지 않음.
- 인증: OIDC/OAuth2 + PKCE(모바일/데스크톱), 웹은 보안 쿠키 + CSRF 방어 또는 BFF.
- 장비 등록: 사용자 승인, 1회용 단기 pairing code, 토큰 회전/폐기, device identity.
- 저장: HTTPS/TLS, Kubernetes Secret 외부 주입, PostgreSQL 백업, S3 객체 버전 또는 복구.
- 계정별 테넌트 검사: 인증된 사용자 UUID 기반으로 모든 노트 요청 및 검색 범위를 서버에서 강제.
- 웹 브라우저에서 외부 AI 서비스에 원시 API 키를 직접 보내지 않음(노출 위험).

## 사용자 수/용량
- 총 활성 사용자 수는 999로 제한하도록 요구. 실제 계정 수 제한은 DB 트랜잭션으로 구현해야 함.
- 기본 예시: free 100MB, plus 1GB, pro 10GB. **임의 설계 기본값이며 확정 상품 가격/정책이 아님.**
- 사용자별 할당량 검사: 업로드 예약(Quota Reservation) → 저장 → 사용량 확정/환불; 동시 요청 및 복구 상황 고려.
- 유료 과금/결제는 별도 서비스 계약/운영 정책이 준비된 이후.

## AI 개인 키(BYOK)
- ChatGPT/OpenAI, Anthropic Claude: 공식 API 가능 여부·과금 정책 확인 후 provider adapter.
- NotebookLM/Gemini Notebook: 개인 계정에 일반 공개 API가 있다고 가정하지 않음; 현재 공식 Enterprise 계열 API와 구분하여 검토.
- Android/iOS/데스크톱: 성능/메모리/권한 요구를 만족할 때에만 사용; 개인 API key는 OS 보안 저장소 사용.
- 웹: 신뢰할 수 있는 사용자별 암호화된 서버 측 비밀 보관소 및 프록시 제공 전에는 비활성화.
- 사용자 노트가 AI에 전송되기 전 **명시적인 본문 선택/전송 동의**가 필요.
- 오프라인 장비와 저사양 BSD 장비에서는 AI 기능 숨김.

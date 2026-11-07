# 검증 계획

- `./scripts/check.sh`: Node JS 문법, JSON, Go 테스트.
- 브라우저 수동 체크리스트: `docs/12_TESTPLAN.md`.
- 추후 Playwright E2E: 편집/새로고침/오프라인/Undo 250회/폴더 3단계 차단.
- 하드웨어 네이티브 테스트: Tauri 각 OS, FreeBSD arm64 실제 보드.
- 서버 보안 테스트: 승인 후 OIDC/BYOK/권한/사용량/동시 수정.

#!/usr/bin/env bash
set -euo pipefail
# === 0. 실행 변수: Vite 개발 서버 (호스트/포트는 apps/web/vite.config.ts 상단 변수) ===
# 오프라인 서비스워커는 빌드 결과에서만 동작합니다: `make build-web && npm -w apps/web run preview`
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}"
[[ -d node_modules ]] || npm ci --no-audit --no-fund
exec npm run -w apps/web dev

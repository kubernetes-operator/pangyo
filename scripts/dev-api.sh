#!/usr/bin/env bash
set -euo pipefail
# === 0. API 기본 변수: 로컬 개발 기본 주소만 허용 ===
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export API_BIND_ADDRESS="${API_BIND_ADDRESS:-127.0.0.1:8080}"
cd "${ROOT}/services/api"
exec go run .

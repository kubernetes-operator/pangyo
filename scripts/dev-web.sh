#!/usr/bin/env bash
set -euo pipefail
# === 0. 실행 변수: root에서 Python 정적 웹 서버로 브라우저 로컬 앱 제공 ===
PORT="${WEB_PORT:-5173}"                      # 로컬 테스트 포트
HOST="${WEB_HOST:-127.0.0.1}"                  # 기본적으로 외부 공개 불가
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# HTTPS 또는 localhost 환경에서 서비스워커/IndexedDB가 동작합니다.
exec python3 -m http.server "${PORT}" --bind "${HOST}" --directory "${ROOT}/apps/web"

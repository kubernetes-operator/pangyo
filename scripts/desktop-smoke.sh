#!/usr/bin/env bash
set -euo pipefail
# === 0. 데스크톱 앱 실행 확인 (빌드 + scripts/desktop_smoke.py). 임시 폴더만 사용 ===
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export PATH="${HOME}/.cargo/bin:${PATH}"
cd "${ROOT}"
npm run -s -w apps/web build >/dev/null
cargo build -q -p memonest-native --features custom-protocol
exec python3 scripts/desktop_smoke.py "${ROOT}/target/debug/memonest-native" "$@"

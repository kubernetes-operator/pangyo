#!/usr/bin/env bash
set -euo pipefail
# === 0. 검증 실행 변수: 외부 인증/배포 없이 로컬과 CI에서 같은 검사를 수행 ===
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REQUIRE_GO="${REQUIRE_GO:-0}"   # 1이면 Go 미설치 시 실패(CI). 4단계에서 서버가 Rust로 이전되면 삭제.
export PATH="${HOME}/.cargo/bin:${PATH}"
cd "${ROOT}"

echo '[1/5] JSON validation'
python3 - <<'PY'
import json
from pathlib import Path
files=[f for d in ('apps','packages') for f in Path(d).rglob('*.json') if 'node_modules' not in f.parts and 'target' not in f.parts]
for f in files: json.loads(f.read_text())
print('OK',len(files),'JSON files')
PY

echo '[2/5] JS syntax'
for f in apps/web/assets/app.js apps/web/assets/store.js apps/web/sw.js; do node --check "$f"; done
echo 'OK'

echo '[3/5] Rust fmt / clippy / test (workspace, Tauri 제외)'
cargo fmt --all --check
cargo clippy --workspace --all-targets --quiet -- -D warnings
cargo test --workspace --quiet

echo '[4/5] Go test (임시: 서버 Rust 이전 전까지)'
if command -v go >/dev/null; then (cd services/api && go test ./...)
elif [[ "${REQUIRE_GO}" == 1 ]]; then echo 'go is required' >&2; exit 1
else echo 'SKIP: go 미설치 (서버는 4단계에서 Rust로 이전 예정)'; fi

echo '[5/5] Secrets guard'
grep -q '^GIT_REMOTE_URL=$' config/project.env.example
grep -q '^KUBE_CONTEXT=$' config/project.env.example
if git ls-files --error-unmatch config/project.env .env >/dev/null 2>&1; then
  echo 'ERROR: 실제 값이 든 env 파일이 Git에 추적되고 있습니다' >&2; exit 1
fi
echo 'OK: checks passed (Tauri/FreeBSD 실기기/Helm 배포는 별도 검증 필요)'

#!/usr/bin/env bash
set -euo pipefail
# === 0. 검증 실행 경로: 외부 인증 및 배포 없이 정적/Go 테스트 ===
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}"
echo '[1/4] JSON validation'
python3 - <<'PY'
import json
from pathlib import Path
files=list(Path('apps').rglob('*.json'))+list(Path('packages').rglob('*.json'))
for f in files: json.loads(f.read_text())
print('OK',len(files),'JSON files')
PY
echo '[2/4] JS syntax'
node --check apps/web/assets/app.js
node --check apps/web/assets/store.js
node --check apps/web/sw.js
echo '[3/4] Go test'
(cd services/api && go test ./...)
echo '[4/4] No real keys in project.env.example'
grep -q '^GIT_REMOTE_URL=$' config/project.env.example
grep -q '^KUBE_CONTEXT=$' config/project.env.example
echo 'OK: initial project checks passed (Tauri/FreeBSD/Helm require separate platform validation)'

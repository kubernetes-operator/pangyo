#!/usr/bin/env bash
set -euo pipefail
# === 0. 검증 실행 변수: 외부 인증/배포 없이 로컬과 CI에서 같은 검사를 수행 ===
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REQUIRE_GO="${REQUIRE_GO:-0}"   # 1이면 Go 미설치 시 실패(CI). 4단계에서 서버가 Rust로 이전되면 삭제.
SKIP_E2E="${SKIP_E2E:-0}"       # 1이면 브라우저 E2E 생략 (빠른 확인용; 커밋 전에는 0 권장)
export PATH="${HOME}/.cargo/bin:${PATH}"
cd "${ROOT}"

echo '[1/6] JSON validation'
python3 - <<'PY'
import json
from pathlib import Path
SKIP={'node_modules','target','dist','test-results','playwright-report'}
files=[f for d in ('apps','packages') for f in Path(d).rglob('*.json') if not SKIP & set(f.parts)]
for f in files: json.loads(f.read_text())
print('OK',len(files),'JSON files')
PY

echo '[2/6] Web: TypeScript 타입 / 단위 테스트 / 빌드'
[[ -d node_modules ]] || npm ci --no-audit --no-fund
npm run -s -w apps/web typecheck
npm run -s -w apps/web test
npm run -s -w apps/web build >/dev/null

echo '[3/6] Web: E2E 회귀 (오프라인, 저장, MD/TXT 내보내기, Undo, 세션, 화면 전환, v0.1 데이터 호환)'
if [[ "${SKIP_E2E}" == 1 ]]; then echo 'SKIP: SKIP_E2E=1'
else npm run -s -w apps/web e2e -- --reporter=line; fi

echo '[4/6] Rust fmt / clippy / test (workspace, Tauri 제외)'
cargo fmt --all --check
cargo clippy --workspace --all-targets --quiet -- -D warnings
cargo test --workspace --quiet

echo '[5/6] Go test (임시: 서버 Rust 이전 전까지)'
if command -v go >/dev/null; then (cd services/api && go test ./...)
elif [[ "${REQUIRE_GO}" == 1 ]]; then echo 'go is required' >&2; exit 1
else echo 'SKIP: go 미설치 (서버는 4단계에서 Rust로 이전 예정)'; fi

echo '[6/6] Secrets guard'
grep -q '^GIT_REMOTE_URL=$' config/project.env.example
grep -q '^KUBE_CONTEXT=$' config/project.env.example
if git ls-files --error-unmatch config/project.env .env >/dev/null 2>&1; then
  echo 'ERROR: 실제 값이 든 env 파일이 Git에 추적되고 있습니다' >&2; exit 1
fi
echo 'OK: checks passed (Tauri/FreeBSD 실기기/Helm 배포는 별도 검증 필요)'

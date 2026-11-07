#!/usr/bin/env bash
set -euo pipefail
# === 0. 검증 실행 변수: 외부 인증/배포 없이 로컬과 CI에서 같은 검사를 수행 ===
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SKIP_E2E="${SKIP_E2E:-0}"       # 1이면 브라우저 E2E 생략 (빠른 확인용; 커밋 전에는 0 권장)
TEST_PG_CONTAINER="memonest-test-pg"   # make db-test-up 이 만드는 테스트 전용 Postgres (trust 인증, 127.0.0.1:55432)
if [[ -z "${TEST_DATABASE_URL:-}" ]] && docker ps --format '{{.Names}}' 2>/dev/null | grep -qx "${TEST_PG_CONTAINER}"; then
  export TEST_DATABASE_URL="postgres://postgres@127.0.0.1:55432/postgres"
fi
export PATH="${HOME}/.cargo/bin:${PATH}"
cd "${ROOT}"

echo '[1/5] JSON validation'
python3 - <<'PY'
import json
from pathlib import Path
SKIP={'node_modules','target','dist','test-results','playwright-report'}
files=[f for d in ('apps','packages') for f in Path(d).rglob('*.json') if not SKIP & set(f.parts)]
for f in files: json.loads(f.read_text())
print('OK',len(files),'JSON files')
PY

echo '[2/5] Web: TypeScript 타입 / 단위 테스트 / 빌드'
[[ -d node_modules ]] || npm ci --no-audit --no-fund
npm run -s -w apps/web typecheck
npm run -s -w apps/web test
npm run -s -w apps/web build >/dev/null

echo '[3/5] Web: E2E 회귀 (오프라인, 저장, MD/TXT 내보내기, Undo, 세션, 화면 전환, v0.1 데이터 호환)'
if [[ "${SKIP_E2E}" == 1 ]]; then echo 'SKIP: SKIP_E2E=1'
else npm run -s -w apps/web e2e -- --reporter=line; fi

echo '[4/5] Rust fmt / clippy / test (core[+workspace], server, bsd, native)'
# 데스크톱 앱(memonest-native)은 WebKitGTK가 있을 때만 포함합니다.
if pkg-config --exists webkit2gtk-4.1 2>/dev/null; then RUST_SCOPE=(--workspace)
else RUST_SCOPE=(--workspace --exclude memonest-native); echo 'SKIP: memonest-native (WebKitGTK 없음)'; fi
cargo fmt --all --check
cargo clippy "${RUST_SCOPE[@]}" --all-targets --quiet -- -D warnings
[[ -n "${TEST_DATABASE_URL:-}" ]] || echo 'SKIP: 서버 계정 DB 테스트 (make db-test-up 으로 테스트 Postgres 실행)'
cargo test "${RUST_SCOPE[@]}" --quiet
# 데스크톱 작업공간(SQLite 인덱스) 기능은 feature로 분리되어 있어 따로 검사
cargo clippy -p memonest-core --features workspace --all-targets --quiet -- -D warnings
cargo test -p memonest-core --features workspace --quiet

echo '[5/5] Secrets guard'
grep -q '^GIT_REMOTE_URL=$' config/project.env.example
grep -q '^KUBE_CONTEXT=$' config/project.env.example
if git ls-files --error-unmatch config/project.env .env >/dev/null 2>&1; then
  echo 'ERROR: 실제 값이 든 env 파일이 Git에 추적되고 있습니다' >&2; exit 1
fi
echo 'OK: checks passed (Tauri/FreeBSD 실기기/Helm 배포는 별도 검증 필요)'

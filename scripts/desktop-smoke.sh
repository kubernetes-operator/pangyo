#!/usr/bin/env bash
set -euo pipefail
# === 0. 데스크톱 앱 실행 확인 변수 (리눅스 + Xvfb). 사용자 실제 메모/홈 폴더는 건드리지 않음 ===
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_SECONDS="${RUN_SECONDS:-15}"   # 앱을 띄워 둘 시간 (열어둔 시간 저장 주기 10초보다 길게)
APP_ID="dev.memonest.notes"        # tauri.conf.json identifier → 앱 데이터 폴더 이름
export PATH="${HOME}/.cargo/bin:${PATH}"
cd "${ROOT}"

command -v xvfb-run >/dev/null || { echo 'xvfb-run 필요 (apt install xvfb)' >&2; exit 1; }
npm run -s -w apps/web build >/dev/null
cargo build -q -p memonest-native --features custom-protocol

WORK="$(mktemp -d)"; trap 'rm -rf "${WORK}"' EXIT
mkdir -p "${WORK}/home" "${WORK}/Notes/BSD"
printf '# 받은 메모\n' > "${WORK}/Notes/Inbox.md"
printf 'from device\n' > "${WORK}/Notes/BSD/device.md"   # BSD 앱 등 바깥에서 만든 파일

echo "[desktop-smoke] ${RUN_SECONDS}초 실행 (Xvfb)"
set +e
HOME="${WORK}/home" XDG_DATA_HOME="${WORK}/home/.local/share" MEMONEST_NOTES_DIR="${WORK}/Notes" \
  MEMONEST_WEBVIEW_CONSOLE=1 timeout "${RUN_SECONDS}" xvfb-run -a "${ROOT}/target/debug/memonest-native" \
  > "${WORK}/app.log" 2>&1
CODE=$?
set -e
grep -v '^libEGL' "${WORK}/app.log" || true
[[ ${CODE} == 124 ]] || { echo "앱이 예상보다 일찍 종료됨 (exit ${CODE})" >&2; exit 1; }
if grep -E 'CONSOLE (ERROR|SECURITY ERROR)' "${WORK}/app.log"; then echo '웹뷰 콘솔 오류 발생' >&2; exit 1; fi

python3 - "${WORK}/home/.local/share/${APP_ID}/index.sqlite" <<'PY'
import sqlite3, sys
db = sqlite3.connect(sys.argv[1])
notes = db.execute('select path from notes order by path').fetchall()
meta = dict(db.execute('select key, value_json from meta').fetchall())
open_ms = db.execute('select coalesce(sum(open_ms),0) from edit_sessions').fetchone()[0]
checks = {
    '바깥 파일 2개가 인덱스에 등록됨 (화면→list_notes)': notes == [('BSD/device.md',), ('Inbox.md',)],
    '화면 언어 설정 저장됨 (화면→set_meta)': meta.get('locale') == '"ko"',
    '열어둔 시간이 기록됨 (화면→save_note)': open_ms >= 5000,
}
for name, ok in checks.items():
    print(('OK  ' if ok else 'FAIL') + ' ' + name)
sys.exit(0 if all(checks.values()) else 1)
PY

#!/bin/sh
# === 키오스크 설치 확인 (VM 검증용): 미리보기 → 설치 2회(멱등) → 계정 셸·자동 로그인 검사 ===
set -eu
BIN="${1:-/root/src/target/release/memonest-bsd}"
OUT="${2:-/root/results}"
cd "$(dirname "$0")/../kiosk"
sh install.sh "${BIN}" --dry-run > "${OUT}/kiosk-dry.log"
sh install.sh "${BIN}" > "${OUT}/kiosk-install.log"
sh install.sh "${BIN}" > "${OUT}/kiosk-install2.log"           # 두 번째 실행: 변경 없어야 함
pw usershow memonest | tee "${OUT}/kiosk-user.txt" | grep -q ':/usr/local/bin/memonest-kiosk$'
[ "$(grep -c '^memonest.autologin:' /etc/gettytab)" = 1 ]          # 항목 중복 없음
grep '^ttyv0' /etc/ttys | tee "${OUT}/kiosk-ttys.txt" | grep -q memonest.autologin
grep -qx /usr/local/bin/memonest-kiosk /etc/shells
[ "$(grep -cx /usr/local/bin/memonest-kiosk /etc/shells)" = 1 ]
echo KIOSK_CHECK_OK

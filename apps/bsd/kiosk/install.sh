#!/bin/sh
# === MemoNest 키오스크 설치 (FreeBSD, root로 실행). 여러 번 실행해도 결과 동일 ===
# 사용: sh install.sh <memonest-bsd 바이너리> [--dry-run]
# 하는 일: 바이너리·실행기 설치, 전용 계정 생성(셸=실행기), ttyv0 자동 로그인, 메모 폴더 권한
set -eu
# === 0. 설치 변수 ===
KIOSK_USER="memonest"                       # 전용 계정 (로그인 셸 = 실행기, 암호 로그인 불가)
KIOSK_TTY="ttyv0"                           # 자동 로그인할 콘솔
BIN_DIR="/usr/local/bin"
GETTY_ENTRY="memonest.autologin"            # /etc/gettytab 항목 이름
HERE="$(cd "$(dirname "$0")" && pwd)"

BIN="${1:-}"; DRY="${2:-}"
[ -n "${BIN}" ] && [ -x "${BIN}" ] || { echo "사용: sh install.sh <memonest-bsd 바이너리> [--dry-run]" >&2; exit 2; }
[ "$(uname -s)" = "FreeBSD" ] || { echo "FreeBSD 전용 스크립트입니다" >&2; exit 2; }
run() { if [ "${DRY}" = "--dry-run" ]; then echo "[dry-run] $*"; else echo "[run] $*"; "$@"; fi; }

run install -m 0555 "${BIN}" "${BIN_DIR}/memonest-bsd"
run install -m 0555 "${HERE}/memonest-kiosk" "${BIN_DIR}/memonest-kiosk"
grep -qx "${BIN_DIR}/memonest-kiosk" /etc/shells || run sh -c "echo ${BIN_DIR}/memonest-kiosk >> /etc/shells"

if ! pw usershow "${KIOSK_USER}" >/dev/null 2>&1; then
  run pw useradd "${KIOSK_USER}" -m -c "MemoNest kiosk" -s "${BIN_DIR}/memonest-kiosk" -w no
else
  run pw usermod "${KIOSK_USER}" -s "${BIN_DIR}/memonest-kiosk"
fi
run install -d -o "${KIOSK_USER}" -m 0700 "/home/${KIOSK_USER}/MemoNest" "/home/${KIOSK_USER}/MemoNest/Notes"

# ttyv0 자동 로그인 (gettytab 항목 추가 + /etc/ttys에서 사용)
if ! grep -q "^${GETTY_ENTRY}:" /etc/gettytab; then
  run sh -c "printf '\n# MemoNest 키오스크 자동 로그인\n${GETTY_ENTRY}:\\\\\n\t:al=${KIOSK_USER}:tc=Pc:\n' >> /etc/gettytab"
fi
if ! grep -q "^${KIOSK_TTY}.*${GETTY_ENTRY}" /etc/ttys; then
  run sed -i '' -E "s|^${KIOSK_TTY}([[:space:]]+)\"/usr/libexec/getty [^\"]*\"|${KIOSK_TTY}\\1\"/usr/libexec/getty ${GETTY_ENTRY}\"|" /etc/ttys
fi
echo "완료. 재부팅하거나 'kill -HUP 1'로 ${KIOSK_TTY}에 적용. 확인: grep ${KIOSK_TTY} /etc/ttys"

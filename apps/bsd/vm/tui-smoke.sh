#!/bin/sh
# === FreeBSD에서 TUI를 실제 터미널(pty)로 실행해 키 입력 → 저장까지 확인 (VM 검증용) ===
# F2(새 메모) → "vmtest" → Enter → "안녕 FreeBSD" → Ctrl+S(저장) → Ctrl+Q(종료)
set -eu
BIN="${1:-/root/src/target/release/memonest-bsd}"
OUT="${2:-/root/results}"
export MEMONEST_NOTES_DIR=/root/MemoNest/Notes
rm -rf /root/MemoNest
mkdir -p "${OUT}"
# 키는 앱이 원시 모드로 바뀐 뒤 보내도록 잠시 기다림. 한글은 UTF-8 바이트(8진수)로 전송
( sleep 8; printf '\033OQ'; sleep 2; printf 'vmtest\r'; sleep 2
  printf '\354\225\210\353\205\225 FreeBSD'; sleep 2; printf '\023'; sleep 3; printf '\021' ) \
  | env TERM=xterm-256color script -q "${OUT}/tui.typescript" "${BIN}"
echo "--- 저장된 파일 ---"
cat "${MEMONEST_NOTES_DIR}/vmtest.md"; echo
[ "$(cat "${MEMONEST_NOTES_DIR}/vmtest.md")" = "$(printf '\354\225\210\353\205\225 FreeBSD')" ] && echo "TUI_SMOKE_OK"

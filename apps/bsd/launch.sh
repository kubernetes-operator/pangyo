#!/bin/sh
# === 0. FreeBSD 전용 앱 런처 변수 ===
APP_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)" # 바이너리가 있는 폴더
APP_BINARY="${MEMONEST_BSD_BINARY:-${APP_DIR}/target/release/memonest-bsd}" # 배포 후 위치에 맞게 변경
# 장치 전용 사용자가 로그인하면 이 앱만 실행하는 방식은 장치 확정 후 보안 검토 필요.
exec "${APP_BINARY}"

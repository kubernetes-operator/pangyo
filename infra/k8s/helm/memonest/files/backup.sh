#!/bin/sh
# === Postgres 논리 백업 (pg_dump -Fc) + 보관 기간 정리 ===
# 환경변수: PGHOST PGUSER PGDATABASE PGPASSWORD BACKUP_DIR RETENTION_DAYS
set -eu
BACKUP_DIR="${BACKUP_DIR:-/backup}"
RETENTION_DAYS="${RETENTION_DAYS:-14}"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
FILE="${BACKUP_DIR}/memonest-${STAMP}.dump"

echo "[backup] ${PGDATABASE}@${PGHOST} → ${FILE}"
chmod 0755 "${BACKUP_DIR}"   # 복원 검증(uid 999)이 읽을 수 있게
umask 022
# 임시 이름으로 쓰고 끝나면 이름 변경: 중간에 실패해도 깨진 백업이 '정상 파일'처럼 남지 않음
pg_dump --format=custom --compress=6 --no-owner --file="${FILE}.partial"
mv "${FILE}.partial" "${FILE}"
ln -sf "$(basename "${FILE}")" "${BACKUP_DIR}/latest.dump"
echo "[backup] 크기: $(du -h "${FILE}" | cut -f1)"

# 보관 기간 지난 백업 삭제 (최신 1개는 항상 남김)
find "${BACKUP_DIR}" -maxdepth 1 -name 'memonest-*.dump' -mtime "+${RETENTION_DAYS}" ! -name "$(basename "${FILE}")" -print -delete
find "${BACKUP_DIR}" -maxdepth 1 -name '*.partial' -mmin +60 -print -delete
echo "[backup] 보관 중: $(ls "${BACKUP_DIR}"/memonest-*.dump | wc -l)개"

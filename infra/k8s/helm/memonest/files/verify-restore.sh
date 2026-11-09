#!/bin/sh
# === 복원 검증: 방금 만든 백업을 임시 Postgres(로컬 디스크)에 실제로 복원해 확인 ===
# 운영 DB는 건드리지 않습니다. postgres 사용자(uid 999)로 실행해야 initdb가 동작합니다.
set -eu
BACKUP_DIR="${BACKUP_DIR:-/backup}"
FILE="$(readlink -f "${BACKUP_DIR}/latest.dump" || true)"
[ -n "${FILE}" ] && [ -r "${FILE}" ] || { echo "[verify] 실패: 읽을 수 있는 최신 백업 없음 (${BACKUP_DIR}/latest.dump)"; exit 1; }
DATA="${VERIFY_DIR:-/verify}/pgdata"
SOCK="${VERIFY_DIR:-/verify}/sock"
EXPECTED_TABLES="accounts sessions devices pairing_codes notes note_leases _sqlx_migrations"

echo "[verify] 복원 대상: ${FILE}"
pg_restore --list "${FILE}" > /dev/null || { echo "[verify] 실패: 백업 아카이브를 읽을 수 없음"; exit 1; }
mkdir -p "${SOCK}"
initdb -D "${DATA}" -U verify --auth=trust --no-sync > /dev/null
pg_ctl -D "${DATA}" -o "-k ${SOCK} -c listen_addresses='' -c fsync=off" -w start > /dev/null
trap 'pg_ctl -D "${DATA}" -m immediate stop > /dev/null 2>&1 || true' EXIT
createdb -h "${SOCK}" -U verify restored
pg_restore -h "${SOCK}" -U verify -d restored --no-owner --exit-on-error "${FILE}"

q() { psql -h "${SOCK}" -U verify -d restored -XAtc "$1"; }
for t in ${EXPECTED_TABLES}; do
  [ "$(q "SELECT to_regclass('public.${t}') IS NOT NULL")" = "t" ] || { echo "[verify] 실패: 테이블 ${t} 없음"; exit 1; }
done
FAILED="$(q "SELECT count(*) FROM _sqlx_migrations WHERE NOT success")"
[ "${FAILED}" = "0" ] || { echo "[verify] 실패: 실패한 마이그레이션 ${FAILED}개"; exit 1; }
echo "[verify] 성공: 마이그레이션 $(q 'SELECT count(*) FROM _sqlx_migrations')개, 계정 $(q 'SELECT count(*) FROM accounts')개, 메모 $(q 'SELECT count(*) FROM notes')개"

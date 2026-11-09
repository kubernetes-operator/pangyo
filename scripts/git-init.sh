#!/usr/bin/env bash
set -euo pipefail
# === 0. Git 초기화 변수: 실제 서버 키는 OS credential helper가 보유 ===
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENV_FILE="${ROOT}/.env"                     # 사용자가 직접 작성하는 로컬 변수 파일
if [[ -f "${ENV_FILE}" ]]; then set -a; source "${ENV_FILE}"; set +a; fi
GIT_REMOTE_URL="${GIT_REMOTE_URL:-}"         # 아직 서버 미결정: 빈 값 허용
GIT_DEFAULT_BRANCH="${GIT_DEFAULT_BRANCH:-main}"
cd "${ROOT}"
if [[ ! -d .git ]]; then git init -b "${GIT_DEFAULT_BRANCH}"; fi
if [[ -n "${GIT_REMOTE_URL}" ]]; then
  if git remote get-url origin >/dev/null 2>&1; then echo 'origin already exists; unchanged';
  else git remote add origin "${GIT_REMOTE_URL}"; fi
else echo 'GIT_REMOTE_URL is empty; local git only'; fi
# 안전을 위해 add/commit/push는 자동으로 실행하지 않습니다.
git status --short
git remote -v

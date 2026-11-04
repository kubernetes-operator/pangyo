#!/usr/bin/env bash
set -euo pipefail
# === 0. 반복 개발 루프 변수: 에이전트 작업과 검증을 명확히 분리 ===
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REQUIREMENT_ID="${1:-}" # 예: 21 (폴더 구조). 입력 없으면 안내만 실행.
cd "${ROOT}"
echo "MemoNest dev loop / requirement ${REQUIREMENT_ID:-not selected}"
echo 'PLAN -> DESIGN -> IMPLEMENT -> VERIFY -> DOCUMENT -> REVIEW'
echo 'Read: AGENTS.md, docs/01_REQUIREMENTS.md, docs/07_LOCK_APPROVAL.md, docs/08_ROADMAP.md'
if [[ -z "${REQUIREMENT_ID}" ]]; then echo 'Usage: ./scripts/loop.sh <requirement-ID>; no code is changed'; exit 0; fi
./scripts/check.sh
# AI 에이전트는 로컬 규칙(.roo/*)을 읽지만 이 스크립트는 자동으로 소스 변경/커밋/배포하지 않습니다.
echo "Verified. Manually update docs/CHANGELOG.md and requirement ${REQUIREMENT_ID} status after review."

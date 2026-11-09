#!/usr/bin/env bash
set -euo pipefail
# === 설치 파일 게시: GitHub Actions 패키징 결과 → 클러스터 다운로드 PVC (운영 변경, 실행 전 승인 필요) ===
# 사용: ./scripts/publish-downloads.sh <package 워크플로 실행 ID>
# 필요: gh(로그인), kubectl(대상 클러스터). 실제 저장 경로는 사용자에게 노출되지 않음(/dl/<토큰>으로만 제공).
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENV_FILE="${ROOT}/.env"; [[ -f "${ENV_FILE}" ]] && { set -a; source "${ENV_FILE}"; set +a; }
RUN_ID="${1:?package 워크플로 실행 ID를 주세요 (gh run list --workflow package)}"
KUBE_CONTEXT="${KUBE_CONTEXT:?KUBE_CONTEXT 필요}"
KUBE_NAMESPACE="${KUBE_NAMESPACE:-memonest}"
RELEASE="${HELM_RELEASE:-memonest-api}"
PVC="${RELEASE}-downloads"
KEEP_VERSIONS="${KEEP_VERSIONS:-2}"              # 보관할 버전 수 (NAS 용량 고려)
REPO="${GITHUB_REPO:-$(gh repo view --json nameWithOwner -q .nameWithOwner)}"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "${ROOT}/Cargo.toml" | head -1)"
WORK="$(mktemp -d)"; trap 'rm -rf "${WORK}"; kubectl --context "${KUBE_CONTEXT}" -n "${KUBE_NAMESPACE}" delete pod memonest-downloads-upload --ignore-not-found --wait=false >/dev/null 2>&1 || true' EXIT

echo "[1/4] 실행 ${RUN_ID}의 설치 파일 받기 (${REPO})"
gh run download "${RUN_ID}" -R "${REPO}" -D "${WORK}/artifacts" -n memonest-windows -n memonest-macos -n memonest-linux -n memonest-android-debug
mkdir -p "${WORK}/out/${VERSION}"
python3 - "${WORK}/artifacts" "${WORK}/out" "${VERSION}" <<'PY'
import hashlib, json, shutil, sys
from pathlib import Path
src, out, ver = Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3]
rules = [  # (id, 플랫폼, 설명, 파일 패턴)
    ("windows-msi", "Windows", "설치 프로그램 (.msi)", "*.msi"),
    ("windows-exe", "Windows", "설치 프로그램 (.exe, 현재 사용자)", "*-setup.exe"),
    ("macos-dmg", "macOS (Apple Silicon)", "디스크 이미지 (.dmg)", "*.dmg"),
    ("linux-deb", "Linux", "Debian/Ubuntu (.deb)", "*.deb"),
    ("linux-rpm", "Linux", "Fedora/RHEL (.rpm)", "*.rpm"),
    ("linux-appimage", "Linux", "AppImage (설치 없이 실행)", "*.AppImage"),
    ("android-apk", "Android", "APK (디버그 빌드, 테스트용)", "*universal-debug.apk"),
]
files = []
for fid, platform, label, pattern in rules:
    found = sorted(src.rglob(pattern))
    if not found:
        print(f"  건너뜀: {fid} ({pattern} 없음)"); continue
    f = found[0]
    dest = out / ver / f.name
    shutil.copy2(f, dest)
    files.append({"id": fid, "platform": platform, "label": label, "file": f"{ver}/{f.name}",
                  "size": dest.stat().st_size, "sha256": hashlib.sha256(dest.read_bytes()).hexdigest()})
    print(f"  {fid}: {f.name} ({dest.stat().st_size // 1024} KB)")
json.dump({"version": ver, "files": files}, open(out / "manifest.json", "w"), ensure_ascii=False, indent=2)
PY

echo "[2/4] 업로드용 임시 Pod (PVC ${PVC} 쓰기 마운트)"
kubectl --context "${KUBE_CONTEXT}" -n "${KUBE_NAMESPACE}" apply -f - >/dev/null <<YAML
apiVersion: v1
kind: Pod
metadata: {name: memonest-downloads-upload, labels: {app.kubernetes.io/name: memonest, app.kubernetes.io/component: downloads-upload}}
spec:
  restartPolicy: Never
  automountServiceAccountToken: false
  containers:
    - name: upload
      image: busybox:1.36
      command: [sh, -c, "sleep 1800"]
      securityContext: {allowPrivilegeEscalation: false, capabilities: {drop: [ALL]}}
      volumeMounts: [{name: d, mountPath: /downloads}]
  volumes: [{name: d, persistentVolumeClaim: {claimName: ${PVC}}}]
YAML
kubectl --context "${KUBE_CONTEXT}" -n "${KUBE_NAMESPACE}" wait --for=condition=Ready pod/memonest-downloads-upload --timeout=180s >/dev/null

echo "[3/4] 파일 복사 (버전 폴더 먼저, 목록 파일은 마지막에 교체 → 받는 중인 사용자 영향 없음)"
K="kubectl --context ${KUBE_CONTEXT} -n ${KUBE_NAMESPACE}"
${K} cp "${WORK}/out/${VERSION}" memonest-downloads-upload:/downloads/.incoming-${VERSION}
${K} exec memonest-downloads-upload -- sh -c "rm -rf /downloads/${VERSION} && mv /downloads/.incoming-${VERSION} /downloads/${VERSION}"
${K} cp "${WORK}/out/manifest.json" memonest-downloads-upload:/downloads/.manifest.json.new
${K} exec memonest-downloads-upload -- sh -c "mv /downloads/.manifest.json.new /downloads/manifest.json && chmod -R a+rX /downloads"

echo "[4/4] 오래된 버전 정리 (최근 ${KEEP_VERSIONS}개 보관)"
${K} exec memonest-downloads-upload -- sh -c "cd /downloads && ls -1d */ 2>/dev/null | sed 's#/##' | sort -V | head -n -${KEEP_VERSIONS} | xargs -r rm -rf; du -sh /downloads; ls /downloads"
echo "완료: 버전 ${VERSION} 게시 (웹 앱 → 계정 → 설치 파일)"

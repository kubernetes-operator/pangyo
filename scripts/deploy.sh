#!/usr/bin/env bash
set -euo pipefail
# === 0. Kubernetes 설정 변수: 기존 클러스터 인증 파일/SSH key 저장 금지 ===
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENV_FILE="${ROOT}/.env"                         # 사용자별 로컬 env 파일
if [[ -f "${ENV_FILE}" ]]; then set -a; source "${ENV_FILE}"; set +a; fi
KUBE_CONTEXT="${KUBE_CONTEXT:-}"                 # 클러스터 오배포 방지: 명시 필수
KUBE_NAMESPACE="${KUBE_NAMESPACE:-memonest}"    # 앱 전용 namespace
HELM_RELEASE="${HELM_RELEASE:-memonest-api}"     # release 이름
K8S_IMAGE_REPOSITORY="${K8S_IMAGE_REPOSITORY:-}" # 사용자의 프라이빗 레지스트리
K8S_IMAGE_TAG="${K8S_IMAGE_TAG:-}"              # 배포 이미지 태그 (예: sha-<커밋>)
[[ -n "${KUBE_CONTEXT}" && -n "${K8S_IMAGE_REPOSITORY}" && -n "${K8S_IMAGE_TAG}" ]] || { echo 'KUBE_CONTEXT, K8S_IMAGE_REPOSITORY, K8S_IMAGE_TAG are required' >&2; exit 1; }
kubectl --context "${KUBE_CONTEXT}" -n "${KUBE_NAMESPACE}" get secret memonest-db >/dev/null 2>&1 || { echo "Secret memonest-db 가 ${KUBE_NAMESPACE}에 없습니다 (infra/k8s/README.md 사전 준비)" >&2; exit 1; }
command -v kubectl >/dev/null;command -v helm >/dev/null
kubectl config get-contexts "${KUBE_CONTEXT}"
echo "Target context=${KUBE_CONTEXT} namespace=${KUBE_NAMESPACE} image=${K8S_IMAGE_REPOSITORY}:${K8S_IMAGE_TAG}"
# 사용자의 명시적 승인: 프로덕션 오배포 방지
read -r -p 'Type DEPLOY to continue: ' approval
[[ "${approval}" == 'DEPLOY' ]] || { echo 'Cancelled'; exit 1; }
kubectl --context "${KUBE_CONTEXT}" create namespace "${KUBE_NAMESPACE}" --dry-run=client -o yaml | kubectl --context "${KUBE_CONTEXT}" apply -f -
helm upgrade --install "${HELM_RELEASE}" "${ROOT}/infra/k8s/helm/memonest" \
  --kube-context "${KUBE_CONTEXT}" --namespace "${KUBE_NAMESPACE}" \
  --set-string "image.repository=${K8S_IMAGE_REPOSITORY}" --set-string "image.tag=${K8S_IMAGE_TAG}" \
  --wait --timeout 10m "$@"   # 추가 옵션 예: --set httpRoute.enabled=true --set httpRoute.hostname=<도메인>

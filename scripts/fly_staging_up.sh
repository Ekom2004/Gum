#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

ORG="${FLY_ORG:-gum}"
REGION="${FLY_REGION:-yyz}"
API_APP="${FLY_API_APP:-gum-api-stg}"
RUNNER_APP="${FLY_RUNNER_APP:-gum-runner-stg}"
PG_APP="${FLY_PG_APP:-gum-pg-stg}"
PG_VM_SIZE="${FLY_PG_VM_SIZE:-shared-cpu-1x}"
PG_VOLUME_GB="${FLY_PG_VOLUME_GB:-20}"
API_URL="${FLY_API_URL:-https://${API_APP}.fly.dev}"

if ! command -v fly >/dev/null 2>&1; then
  echo "fly CLI is required (https://fly.io/docs/flyctl/install/)" >&2
  exit 1
fi

if ! fly auth whoami >/dev/null 2>&1; then
  echo "fly auth is missing. Run: fly auth login" >&2
  exit 1
fi

if ! fly orgs list | awk 'NR>1 {print $(NF-1)}' | grep -qx "${ORG}"; then
  echo "Fly org '${ORG}' not found for this account." >&2
  exit 1
fi

if [[ -n "${GUM_ADMIN_KEY:-}" ]]; then
  ADMIN_KEY="${GUM_ADMIN_KEY}"
else
  ADMIN_KEY="gum_admin_$(openssl rand -hex 24)"
fi

if [[ -n "${GUM_API_KEY:-}" ]]; then
  API_KEY="${GUM_API_KEY}"
else
  API_KEY="gum_live_$(openssl rand -hex 24)"
fi

if [[ -n "${GUM_INTERNAL_KEY:-}" ]]; then
  INTERNAL_KEY="${GUM_INTERNAL_KEY}"
else
  INTERNAL_KEY="gum_internal_$(openssl rand -hex 24)"
fi

echo "org=${ORG} region=${REGION}"
echo "api=${API_APP} runner=${RUNNER_APP} postgres=${PG_APP}"

ensure_app() {
  local app_name="$1"
  if fly apps list -o "${ORG}" -q \
    | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//' \
    | grep -qx "${app_name}"; then
    echo "app exists: ${app_name}"
    return 0
  fi
  echo "creating app: ${app_name}"
  fly apps create "${app_name}" --org "${ORG}"
}

ensure_app "${API_APP}"
ensure_app "${RUNNER_APP}"

if fly postgres list | awk '{print $1}' | grep -qx "${PG_APP}"; then
  echo "postgres exists: ${PG_APP}"
else
  echo "creating postgres: ${PG_APP}"
  fly postgres create \
    --name "${PG_APP}" \
    --org "${ORG}" \
    --region "${REGION}" \
    --initial-cluster-size 1 \
    --vm-size "${PG_VM_SIZE}" \
    --volume-size "${PG_VOLUME_GB}"
fi

echo "attaching postgres to api app (idempotent)"
if ! fly postgres attach --app "${API_APP}" "${PG_APP}" --yes; then
  if fly secrets list -a "${API_APP}" 2>/dev/null | grep -q "DATABASE_URL"; then
    echo "postgres already attached to ${API_APP}; continuing"
  else
    echo "postgres attach failed and DATABASE_URL is missing" >&2
    exit 1
  fi
fi

echo "setting api secrets"
fly secrets set -a "${API_APP}" \
  GUM_API_KEY="${API_KEY}" \
  GUM_ADMIN_KEY="${ADMIN_KEY}" \
  GUM_INTERNAL_KEY="${INTERNAL_KEY}" \
  GUM_API_BIND_ADDR="0.0.0.0" \
  GUM_API_PORT="8080"

echo "setting runner secrets"
fly secrets set -a "${RUNNER_APP}" \
  GUM_API_BASE_URL="${API_URL}" \
  GUM_INTERNAL_KEY="${INTERNAL_KEY}" \
  GUM_RUNNER_COMPUTE_CLASS="standard" \
  GUM_RUNNER_MEMORY_MB="1024" \
  GUM_RUNNER_MAX_CONCURRENT_LEASES="2"

echo "deploying api"
fly deploy -c "${REPO_ROOT}/deploy/fly/api.fly.toml" --app "${API_APP}" --remote-only

echo "deploying runner"
fly deploy -c "${REPO_ROOT}/deploy/fly/runner.fly.toml" --app "${RUNNER_APP}" --remote-only

echo
echo "staging deploy complete"
echo "API URL: ${API_URL}"
echo "API KEY: ${API_KEY}"
echo "ADMIN KEY: ${ADMIN_KEY}"
echo
echo "next:"
echo "  GUM_SMOKE_USE_EXISTING_API=1 GUM_API_BASE_URL=${API_URL} GUM_API_KEY=${API_KEY} GUM_ADMIN_KEY=${ADMIN_KEY} KEEP_SMOKE_ARTIFACTS=1 ./scripts/beta_onboarding_smoke.sh"

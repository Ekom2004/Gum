#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

ORG="${FLY_ORG:-gum}"
REGION="${FLY_REGION:-yyz}"
API_APP="${FLY_API_APP:-gum-api-stg}"
RUNNER_APP="${FLY_RUNNER_APP:-gum-runner-stg}"
SCHEDULER_APP="${FLY_SCHEDULER_APP:-gum-scheduler-stg}"
PG_APP="${FLY_PG_APP:-gum-pg-stg}"
PG_VM_SIZE="${FLY_PG_VM_SIZE:-shared-cpu-1x}"
PG_VOLUME_GB="${FLY_PG_VOLUME_GB:-20}"
API_URL="${FLY_API_URL:-https://${API_APP}.fly.dev}"
RUNNER_COMPUTE_CLASS="${GUM_RUNNER_COMPUTE_CLASS:-standard}"
RUNNER_CPU_CORES="${GUM_RUNNER_CPU_CORES:-1}"
RUNNER_MEMORY_MB="${GUM_RUNNER_MEMORY_MB:-512}"
RUNNER_MAX_CONCURRENT_LEASES="${GUM_RUNNER_MAX_CONCURRENT_LEASES:-2}"
SECRET_BACKEND="${GUM_SECRET_BACKEND:-postgres}"

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

API_KEY_VALUE=""
ADMIN_KEY_VALUE=""
INTERNAL_KEY_VALUE=""
API_KEY_SOURCE="existing"
ADMIN_KEY_SOURCE="existing"
INTERNAL_KEY_SOURCE="existing"

echo "org=${ORG} region=${REGION}"
echo "api=${API_APP} runner=${RUNNER_APP} scheduler=${SCHEDULER_APP} postgres=${PG_APP}"
echo "secret_backend=${SECRET_BACKEND}"

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

app_has_secret() {
  local app_name="$1"
  local secret_name="$2"
  fly secrets list -a "${app_name}" 2>/dev/null \
    | awk 'NR>1 {print $1}' \
    | grep -qx "${secret_name}"
}

first_machine_id() {
  local app_name="$1"
  fly machine list -a "${app_name}" 2>/dev/null \
    | awk 'NR>4 && $1 != "" {print $1; exit}'
}

read_app_env_value() {
  local app_name="$1"
  local env_name="$2"
  local machine_id
  machine_id="$(first_machine_id "${app_name}")"
  if [[ -z "${machine_id}" ]]; then
    return 1
  fi
  fly ssh console -a "${app_name}" --machine "${machine_id}" -C "printenv ${env_name}" 2>/dev/null
}

ensure_app "${API_APP}"
ensure_app "${RUNNER_APP}"
ensure_app "${SCHEDULER_APP}"

if [[ -n "${GUM_ADMIN_KEY:-}" ]]; then
  ADMIN_KEY_VALUE="${GUM_ADMIN_KEY}"
  ADMIN_KEY_SOURCE="provided"
elif ! app_has_secret "${API_APP}" "GUM_ADMIN_KEY"; then
  ADMIN_KEY_VALUE="gum_admin_$(openssl rand -hex 24)"
  ADMIN_KEY_SOURCE="generated"
fi

if [[ -n "${GUM_API_KEY:-}" ]]; then
  API_KEY_VALUE="${GUM_API_KEY}"
  API_KEY_SOURCE="provided"
elif ! app_has_secret "${API_APP}" "GUM_API_KEY"; then
  API_KEY_VALUE="gum_live_$(openssl rand -hex 24)"
  API_KEY_SOURCE="generated"
fi

if [[ -n "${GUM_INTERNAL_KEY:-}" ]]; then
  INTERNAL_KEY_VALUE="${GUM_INTERNAL_KEY}"
  INTERNAL_KEY_SOURCE="provided"
elif ! app_has_secret "${API_APP}" "GUM_INTERNAL_KEY"; then
  INTERNAL_KEY_VALUE="gum_internal_$(openssl rand -hex 24)"
  INTERNAL_KEY_SOURCE="generated"
fi

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
  if app_has_secret "${API_APP}" "DATABASE_URL"; then
    echo "postgres already attached to ${API_APP}; continuing"
  else
    echo "postgres attach failed and DATABASE_URL is missing" >&2
    exit 1
  fi
fi

SHARED_DATABASE_URL="$(read_app_env_value "${API_APP}" "DATABASE_URL" || true)"
if [[ -z "${SHARED_DATABASE_URL}" ]]; then
  echo "failed to read DATABASE_URL from ${API_APP}; scheduler must share the API database" >&2
  exit 1
fi
echo "reusing ${API_APP} DATABASE_URL for ${SCHEDULER_APP}"

API_SECRET_ARGS=(
  GUM_API_BIND_ADDR="0.0.0.0"
  GUM_API_PORT="8080"
  GUM_SECRET_BACKEND="${SECRET_BACKEND}"
)

if [[ -n "${API_KEY_VALUE}" ]]; then
  API_SECRET_ARGS+=(GUM_API_KEY="${API_KEY_VALUE}")
fi

if [[ -n "${ADMIN_KEY_VALUE}" ]]; then
  API_SECRET_ARGS+=(GUM_ADMIN_KEY="${ADMIN_KEY_VALUE}")
fi

if [[ -n "${INTERNAL_KEY_VALUE}" ]]; then
  API_SECRET_ARGS+=(GUM_INTERNAL_KEY="${INTERNAL_KEY_VALUE}")
fi

if [[ "${SECRET_BACKEND}" == "postgres" || "${SECRET_BACKEND}" == "postgresql" ]]; then
  if [[ -n "${GUM_SECRET_MASTER_KEY:-}" ]]; then
    SECRET_MASTER_KEY="${GUM_SECRET_MASTER_KEY}"
    API_SECRET_ARGS+=(GUM_SECRET_MASTER_KEY="${SECRET_MASTER_KEY}")
    echo "using provided GUM_SECRET_MASTER_KEY"
  elif app_has_secret "${API_APP}" "GUM_SECRET_MASTER_KEY"; then
    echo "reusing existing GUM_SECRET_MASTER_KEY from Fly secret state"
  else
    SECRET_MASTER_KEY="$(openssl rand -hex 32)"
    API_SECRET_ARGS+=(GUM_SECRET_MASTER_KEY="${SECRET_MASTER_KEY}")
    echo "generated new GUM_SECRET_MASTER_KEY for first bootstrap"
    echo "store this key outside Fly before production use and pass it explicitly on future fresh bootstraps"
  fi
fi

echo "setting api secrets"
fly secrets set -a "${API_APP}" "${API_SECRET_ARGS[@]}"

echo "setting runner secrets"
RUNNER_SECRET_ARGS=(
  GUM_API_BASE_URL="${API_URL}"
  GUM_RUNNER_COMPUTE_CLASS="${RUNNER_COMPUTE_CLASS}"
  GUM_RUNNER_CPU_CORES="${RUNNER_CPU_CORES}"
  GUM_RUNNER_MEMORY_MB="${RUNNER_MEMORY_MB}"
  GUM_RUNNER_MAX_CONCURRENT_LEASES="${RUNNER_MAX_CONCURRENT_LEASES}"
)

if [[ -n "${INTERNAL_KEY_VALUE}" ]]; then
  RUNNER_SECRET_ARGS+=(GUM_INTERNAL_KEY="${INTERNAL_KEY_VALUE}")
elif ! app_has_secret "${RUNNER_APP}" "GUM_INTERNAL_KEY"; then
  echo "runner is missing GUM_INTERNAL_KEY and no value is available to set" >&2
  exit 1
fi

fly secrets set -a "${RUNNER_APP}" "${RUNNER_SECRET_ARGS[@]}"

echo "setting scheduler secrets"
SCHEDULER_SECRET_ARGS=(
  DATABASE_URL="${SHARED_DATABASE_URL}"
  GUM_SECRET_BACKEND="${SECRET_BACKEND}"
)

if [[ -n "${API_KEY_VALUE}" ]]; then
  SCHEDULER_SECRET_ARGS+=(GUM_API_KEY="${API_KEY_VALUE}")
fi

if [[ -n "${ADMIN_KEY_VALUE}" ]]; then
  SCHEDULER_SECRET_ARGS+=(GUM_ADMIN_KEY="${ADMIN_KEY_VALUE}")
fi

if [[ -n "${INTERNAL_KEY_VALUE}" ]]; then
  SCHEDULER_SECRET_ARGS+=(GUM_INTERNAL_KEY="${INTERNAL_KEY_VALUE}")
fi

if [[ "${SECRET_BACKEND}" == "postgres" || "${SECRET_BACKEND}" == "postgresql" ]]; then
  if [[ -n "${GUM_SECRET_MASTER_KEY:-}" ]]; then
    SCHEDULER_SECRET_ARGS+=(GUM_SECRET_MASTER_KEY="${GUM_SECRET_MASTER_KEY}")
  elif app_has_secret "${SCHEDULER_APP}" "GUM_SECRET_MASTER_KEY"; then
    :
  else
    API_SECRET_MASTER_KEY="$(read_app_env_value "${API_APP}" "GUM_SECRET_MASTER_KEY" || true)"
    if [[ -n "${API_SECRET_MASTER_KEY}" ]]; then
      SCHEDULER_SECRET_ARGS+=(GUM_SECRET_MASTER_KEY="${API_SECRET_MASTER_KEY}")
      echo "copied existing GUM_SECRET_MASTER_KEY from ${API_APP} to ${SCHEDULER_APP}"
    else
      echo "scheduler is missing GUM_SECRET_MASTER_KEY and no value is available to set" >&2
      exit 1
    fi
  fi
fi

fly secrets set -a "${SCHEDULER_APP}" "${SCHEDULER_SECRET_ARGS[@]}"

echo "deploying api"
fly deploy -c "${REPO_ROOT}/deploy/fly/api.fly.toml" --app "${API_APP}" --remote-only

echo "deploying runner"
fly deploy -c "${REPO_ROOT}/deploy/fly/runner.fly.toml" --app "${RUNNER_APP}" --remote-only

echo "deploying scheduler"
fly deploy -c "${REPO_ROOT}/deploy/fly/scheduler.fly.toml" --app "${SCHEDULER_APP}" --remote-only

echo
echo "staging deploy complete"
echo "API URL: ${API_URL}"
if [[ "${API_KEY_SOURCE}" != "existing" ]]; then
  echo "API KEY (${API_KEY_SOURCE}): ${API_KEY_VALUE}"
else
  echo "API KEY: unchanged on Fly"
fi
if [[ "${ADMIN_KEY_SOURCE}" != "existing" ]]; then
  echo "ADMIN KEY (${ADMIN_KEY_SOURCE}): ${ADMIN_KEY_VALUE}"
else
  echo "ADMIN KEY: unchanged on Fly"
fi
echo
echo "next:"
if [[ "${API_KEY_SOURCE}" != "existing" && "${ADMIN_KEY_SOURCE}" != "existing" ]]; then
  echo "  GUM_SMOKE_USE_EXISTING_API=1 GUM_API_BASE_URL=${API_URL} GUM_API_KEY=${API_KEY_VALUE} GUM_ADMIN_KEY=${ADMIN_KEY_VALUE} KEEP_SMOKE_ARTIFACTS=1 ./scripts/beta_onboarding_smoke.sh"
else
  echo "  use existing API/admin keys from Fly to run the onboarding smoke against ${API_URL}"
fi

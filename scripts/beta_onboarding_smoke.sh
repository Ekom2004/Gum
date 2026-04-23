#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export PYTHONPATH="${REPO_ROOT}/sdk${PYTHONPATH:+:${PYTHONPATH}}"
export GUM_API_BASE_URL="${GUM_API_BASE_URL:-http://127.0.0.1:8000}"
export GUM_API_KEY="${GUM_API_KEY:-dev}"
export GUM_ADMIN_KEY="${GUM_ADMIN_KEY:-gum-dev-admin}"

WORKDIR="$(mktemp -d /tmp/gum-beta-onboarding-XXXXXX)"
LOG_DIR="${WORKDIR}/logs"
mkdir -p "${LOG_DIR}"

API_PID=""
RUNNER_PID=""
USE_EXISTING_API="${GUM_SMOKE_USE_EXISTING_API:-0}"

cleanup() {
  local exit_code=$?
  if [[ -n "${RUNNER_PID}" ]] && kill -0 "${RUNNER_PID}" >/dev/null 2>&1; then
    kill "${RUNNER_PID}" >/dev/null 2>&1 || true
    wait "${RUNNER_PID}" >/dev/null 2>&1 || true
  fi
  if [[ -n "${API_PID}" ]] && kill -0 "${API_PID}" >/dev/null 2>&1; then
    kill "${API_PID}" >/dev/null 2>&1 || true
    wait "${API_PID}" >/dev/null 2>&1 || true
  fi
  if [[ "${KEEP_SMOKE_ARTIFACTS:-0}" != "1" ]]; then
    rm -rf "${WORKDIR}"
  else
    echo "smoke artifacts kept at ${WORKDIR}"
  fi
  exit "${exit_code}"
}
trap cleanup EXIT INT TERM

if [[ "${USE_EXISTING_API}" != "1" ]]; then
  if [[ "${GUM_API_BASE_URL}" == "http://127.0.0.1:8000" ]] && command -v lsof >/dev/null 2>&1; then
    if lsof -nP -iTCP:8000 -sTCP:LISTEN >/dev/null 2>&1; then
      echo "port 8000 is already in use; stop the existing API or set GUM_SMOKE_USE_EXISTING_API=1" >&2
      exit 1
    fi
  fi
  echo "starting gum-api..."
  (
    cd "${REPO_ROOT}"
    cargo run -q -p gum-api >"${LOG_DIR}/gum-api.log" 2>&1
  ) &
  API_PID="$!"
else
  echo "using existing API at ${GUM_API_BASE_URL}"
fi

echo "waiting for API readiness..."
python3.11 - <<'PY'
import os
import sys
import time
import urllib.request

base = os.environ["GUM_API_BASE_URL"].rstrip("/")
token = os.environ["GUM_ADMIN_KEY"]
url = f"{base}/internal/admin/runs"
deadline = time.time() + 90
headers = {"Authorization": f"Bearer {token}"}

while time.time() < deadline:
    try:
        req = urllib.request.Request(url, headers=headers)
        with urllib.request.urlopen(req, timeout=2) as resp:
            if resp.status == 200:
                print("api ready")
                raise SystemExit(0)
    except Exception:
        pass
    time.sleep(1)

print("api did not become ready within timeout", file=sys.stderr)
raise SystemExit(1)
PY

if [[ "${USE_EXISTING_API}" != "1" ]]; then
  echo "starting gum-runner..."
  (
    cd "${REPO_ROOT}"
    cargo run -q -p gum-runner >"${LOG_DIR}/gum-runner.log" 2>&1
  ) &
  RUNNER_PID="$!"

  echo "waiting for runner registration..."
  sleep 2
fi

echo "running onboarding flow in ${WORKDIR}..."
cd "${WORKDIR}"

python3.11 -m gum.cli init --project-id proj_dev --api-base-url "${GUM_API_BASE_URL}" \
  >"${LOG_DIR}/01-init.log"

python3.11 -m gum.cli deploy --project-id proj_dev --api-base-url "${GUM_API_BASE_URL}" \
  >"${LOG_DIR}/02-deploy.log"

RUN_ID="$(
python3.11 - <<'PY'
import jobs
run = jobs.hello.enqueue(name="smoke")
print(run.id)
PY
)"

echo "${RUN_ID}" > "${LOG_DIR}/run_id.txt"

python3.11 - "${RUN_ID}" <<'PY' >"${LOG_DIR}/03-wait-run.log"
import sys
import time

from gum.client import default_client

run_id = sys.argv[1]
client = default_client()
deadline = time.time() + 120
terminal = {"succeeded", "failed", "timed_out", "canceled"}

while True:
    run = client.runs.get(run_id)
    status = str(run.status).lower()
    if status in terminal:
        print(f"run {run_id} status={run.status}")
        if status != "succeeded":
            raise SystemExit(1)
        raise SystemExit(0)
    if time.time() > deadline:
        print(f"run {run_id} did not reach terminal state in time", file=sys.stderr)
        raise SystemExit(1)
    time.sleep(1)
PY

python3.11 -m gum.cli get "${RUN_ID}" >"${LOG_DIR}/04-get.log"
python3.11 -m gum.cli logs "${RUN_ID}" >"${LOG_DIR}/05-logs.log"
python3.11 -m gum.cli admin runs get "${RUN_ID}" >"${LOG_DIR}/06-admin-run.log"
python3.11 -m gum.cli admin --once >"${LOG_DIR}/07-admin-once.log"

grep -q "hello smoke" "${LOG_DIR}/05-logs.log"
grep -q "Run:" "${LOG_DIR}/04-get.log"
grep -q "Run:" "${LOG_DIR}/06-admin-run.log"
grep -q "GUM ADMIN" "${LOG_DIR}/07-admin-once.log"

echo "BETA-002 smoke flow passed"
echo "run_id=${RUN_ID}"
echo "logs at ${LOG_DIR}"

#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROJECT_ROOT="${1:-$PWD}"
RUNNER_APP="${FLY_RUNNER_APP:-gum-runner-stg}"
RUNNER_COMPUTE_CLASS="${GUM_RUNNER_COMPUTE_CLASS:-standard}"
RUNNER_PARALLELISM="${GUM_RUNNER_PARALLELISM:-1}"

if ! command -v flyctl >/dev/null 2>&1; then
  echo "flyctl is required" >&2
  exit 1
fi

if ! command -v python3.11 >/dev/null 2>&1; then
  echo "python3.11 is required" >&2
  exit 1
fi

if ! [[ "$RUNNER_PARALLELISM" =~ ^[0-9]+$ ]] || [[ "$RUNNER_PARALLELISM" -eq 0 ]]; then
  echo "GUM_RUNNER_PARALLELISM must be a positive integer" >&2
  exit 1
fi

CAPACITY_VARS="$(
  REPO_ROOT="$REPO_ROOT" PROJECT_ROOT="$PROJECT_ROOT" python3.11 - <<'PY'
import os
import sys
from pathlib import Path

repo_root = Path(os.environ["REPO_ROOT"]).resolve()
project_root = Path(os.environ["PROJECT_ROOT"]).resolve()
sys.path.insert(0, str(repo_root / "sdk"))

from gum.deploy import discover_jobs  # type: ignore

jobs = discover_jobs(project_root)
if not jobs:
    raise SystemExit("no gum jobs found in project")

max_cpu_single = max((job.cpu_cores or 1) for job in jobs)
max_memory_single = max((job.memory_mb or 512) for job in jobs)
max_job_concurrency = max((job.concurrency_limit or 1) for job in jobs)

print(f"MAX_CPU_SINGLE={max_cpu_single}")
print(f"MAX_MEMORY_SINGLE_MB={max_memory_single}")
print(f"MAX_JOB_CONCURRENCY={max_job_concurrency}")
PY
)"

eval "$CAPACITY_VARS"

TARGET_CPU_CORES="$((MAX_CPU_SINGLE * RUNNER_PARALLELISM))"
TARGET_MEMORY_MB="$((MAX_MEMORY_SINGLE_MB * RUNNER_PARALLELISM))"
TARGET_MAX_LEASES="$RUNNER_PARALLELISM"

echo "project_root=${PROJECT_ROOT}"
echo "runner_app=${RUNNER_APP}"
echo "max_single_job_cpu=${MAX_CPU_SINGLE}"
echo "max_single_job_memory_mb=${MAX_MEMORY_SINGLE_MB}"
echo "max_job_concurrency=${MAX_JOB_CONCURRENCY}"
echo "runner_parallelism=${RUNNER_PARALLELISM}"
echo "target_runner_cpu_cores=${TARGET_CPU_CORES}"
echo "target_runner_memory_mb=${TARGET_MEMORY_MB}"
echo "target_runner_max_concurrent_leases=${TARGET_MAX_LEASES}"

echo "updating runner secrets"
flyctl secrets set -a "${RUNNER_APP}" \
  GUM_RUNNER_COMPUTE_CLASS="${RUNNER_COMPUTE_CLASS}" \
  GUM_RUNNER_CPU_CORES="${TARGET_CPU_CORES}" \
  GUM_RUNNER_MEMORY_MB="${TARGET_MEMORY_MB}" \
  GUM_RUNNER_MAX_CONCURRENT_LEASES="${TARGET_MAX_LEASES}"

MACHINE_IDS="$(flyctl machine list -a "${RUNNER_APP}" -q)"
if [[ -z "${MACHINE_IDS}" ]]; then
  echo "no runner machines found for ${RUNNER_APP}" >&2
  exit 1
fi

echo "updating Fly runner machine resources"
while read -r machine_id; do
  [[ -z "${machine_id}" ]] && continue
  flyctl machine update "${machine_id}" \
    -a "${RUNNER_APP}" \
    --vm-cpus "${TARGET_CPU_CORES}" \
    --vm-memory "${TARGET_MEMORY_MB}" \
    --yes
done <<< "${MACHINE_IDS}"

echo "runner capacity sync complete"

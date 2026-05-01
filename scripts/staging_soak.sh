#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

export GUM_API_BASE_URL="${GUM_API_BASE_URL:-https://gum-api-stg.fly.dev}"
export GUM_SMOKE_USE_EXISTING_API=1

if [[ -z "${GUM_API_KEY:-}" ]]; then
  echo "GUM_API_KEY is required" >&2
  exit 1
fi

if [[ -z "${GUM_ADMIN_KEY:-}" ]]; then
  echo "GUM_ADMIN_KEY is required" >&2
  exit 1
fi

DURATION_MINUTES="${SOAK_DURATION_MINUTES:-1440}"
INTERVAL_SECONDS="${SOAK_INTERVAL_SECONDS:-600}"
MAX_FAILURES="${SOAK_MAX_FAILURES:-1}"
FAIL_FAST="${SOAK_FAIL_FAST:-1}"
KEEP_SMOKE_ARTIFACTS_FOR_SOAK="${KEEP_SOAK_SMOKE_ARTIFACTS:-0}"

if ! [[ "$DURATION_MINUTES" =~ ^[0-9]+$ ]] || [[ "$DURATION_MINUTES" -eq 0 ]]; then
  echo "SOAK_DURATION_MINUTES must be a positive integer" >&2
  exit 1
fi
if ! [[ "$INTERVAL_SECONDS" =~ ^[0-9]+$ ]] || [[ "$INTERVAL_SECONDS" -eq 0 ]]; then
  echo "SOAK_INTERVAL_SECONDS must be a positive integer" >&2
  exit 1
fi
if ! [[ "$MAX_FAILURES" =~ ^[0-9]+$ ]]; then
  echo "SOAK_MAX_FAILURES must be a non-negative integer" >&2
  exit 1
fi

SOAK_DIR="${SOAK_DIR:-$(mktemp -d /tmp/gum-staging-soak-XXXXXX)}"
LOG_FILE="${SOAK_DIR}/soak.log"
SUMMARY_FILE="${SOAK_DIR}/summary.txt"

START_EPOCH="$(date +%s)"
END_EPOCH="$((START_EPOCH + DURATION_MINUTES * 60))"
STARTED_AT_UTC="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

attempt=0
pass_count=0
fail_count=0
last_run_id=""

log() {
  local timestamp
  timestamp="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
  echo "[$timestamp] $*" | tee -a "$LOG_FILE"
}

write_summary() {
  local final_status="$1"
  {
    echo "status=${final_status}"
    echo "started_at_utc=${STARTED_AT_UTC}"
    echo "ended_at_utc=$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
    echo "duration_minutes=${DURATION_MINUTES}"
    echo "interval_seconds=${INTERVAL_SECONDS}"
    echo "attempts=${attempt}"
    echo "passes=${pass_count}"
    echo "failures=${fail_count}"
    echo "last_run_id=${last_run_id}"
    echo "soak_log=${LOG_FILE}"
  } >"$SUMMARY_FILE"
}

log "soak_dir=${SOAK_DIR}"
log "target_api=${GUM_API_BASE_URL}"
log "duration_minutes=${DURATION_MINUTES} interval_seconds=${INTERVAL_SECONDS}"
log "max_failures=${MAX_FAILURES} fail_fast=${FAIL_FAST}"

while true; do
  now_epoch="$(date +%s)"
  if [[ "$now_epoch" -ge "$END_EPOCH" ]]; then
    break
  fi

  attempt="$((attempt + 1))"
  attempt_log="${SOAK_DIR}/attempt-${attempt}.log"

  log "attempt=${attempt} started"

  set +e
  KEEP_SMOKE_ARTIFACTS="${KEEP_SMOKE_ARTIFACTS_FOR_SOAK}" \
    "${REPO_ROOT}/scripts/beta_onboarding_smoke.sh" \
    >"${attempt_log}" 2>&1
  rc=$?
  set -e

  if command -v rg >/dev/null 2>&1; then
    run_id="$(rg -o "run_id=[^[:space:]]+" "${attempt_log}" | tail -n 1 | cut -d= -f2 || true)"
  else
    run_id="$(grep -Eo "run_id=[^[:space:]]+" "${attempt_log}" | tail -n 1 | cut -d= -f2 || true)"
  fi
  if [[ -n "${run_id}" ]]; then
    last_run_id="${run_id}"
  fi

  if [[ "$rc" -eq 0 ]]; then
    pass_count="$((pass_count + 1))"
    log "attempt=${attempt} result=pass run_id=${run_id:-unknown} log=${attempt_log}"
  else
    fail_count="$((fail_count + 1))"
    log "attempt=${attempt} result=fail exit_code=${rc} run_id=${run_id:-unknown} log=${attempt_log}"

    if [[ "${MAX_FAILURES}" -gt 0 && "${fail_count}" -ge "${MAX_FAILURES}" ]]; then
      log "failure threshold reached (${fail_count}/${MAX_FAILURES})"
      if [[ "${FAIL_FAST}" == "1" ]]; then
        log "stopping soak due to fail-fast"
        write_summary "failed"
        cat "$SUMMARY_FILE"
        exit 1
      fi
    fi
  fi

  now_epoch="$(date +%s)"
  if [[ "$now_epoch" -ge "$END_EPOCH" ]]; then
    break
  fi
  sleep "$INTERVAL_SECONDS"
done

if [[ "${fail_count}" -gt 0 ]]; then
  write_summary "failed"
  cat "$SUMMARY_FILE"
  exit 1
fi

write_summary "passed"
cat "$SUMMARY_FILE"

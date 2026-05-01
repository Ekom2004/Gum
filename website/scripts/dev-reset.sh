#!/bin/sh
set -eu

PORT="${PORT:-3000}"

PIDS="$(lsof -ti "tcp:${PORT}" -sTCP:LISTEN 2>/dev/null || true)"
if [ -n "$PIDS" ]; then
  echo "Clearing stale listener(s) on port ${PORT}: $PIDS"
  # shellcheck disable=SC2086
  kill $PIDS 2>/dev/null || true
  sleep 0.2
fi

echo "Starting Next.js dev server on port ${PORT}"
exec next dev -p "$PORT"

#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export PATH="/opt/homebrew/opt/postgresql@15/bin:$PATH"

PORT="${OPS_PG_PORT:-55432}"
API_PORT="${OPS_API_PORT:-18080}"
USER_NAME="${OPS_PG_USER:-gumops}"
DB_SRC="${OPS_DB_SRC:-gum_ops_src}"
DB_RESTORE="${OPS_DB_RESTORE:-gum_ops_restore}"
DB_ROLLBACK="${OPS_DB_ROLLBACK:-gum_ops_rollback}"
KEEP_ARTIFACTS="${KEEP_OPS_ARTIFACTS:-0}"

OPS_DIR="${OPS_DIR:-$(mktemp -d /tmp/gum-ops-drill-XXXXXX)}"
PGDATA="$OPS_DIR/pgdata"
PGLOG="$OPS_DIR/postgres.log"
APILOG="$OPS_DIR/api.log"
DUMP_FILE="$OPS_DIR/gum_ops_src.dump"

PG_STARTED=0

cleanup() {
  if [[ "$PG_STARTED" -eq 1 ]]; then
    pg_ctl -D "$PGDATA" stop >/dev/null 2>&1 || true
  fi
  if [[ "$KEEP_ARTIFACTS" != "1" ]]; then
    rm -rf "$OPS_DIR"
  fi
}
trap cleanup EXIT

echo "ops_dir=$OPS_DIR"

initdb -D "$PGDATA" -U "$USER_NAME" -A trust >/dev/null
pg_ctl -D "$PGDATA" -l "$PGLOG" -o "-p $PORT" start >/dev/null
PG_STARTED=1

createdb -h 127.0.0.1 -p "$PORT" -U "$USER_NAME" "$DB_SRC"

(
  cd "$REPO_ROOT"
  DATABASE_URL="postgresql://$USER_NAME@127.0.0.1:$PORT/$DB_SRC" \
  GUM_API_BIND_ADDR="127.0.0.1" \
  GUM_API_PORT="$API_PORT" \
  cargo run -p gum-api >"$APILOG" 2>&1
) &
API_PID=$!

for _ in $(seq 1 30); do
  if rg -n "gum-api listening" "$APILOG" >/dev/null 2>&1; then
    break
  fi
  sleep 1
done

if ! rg -n "gum-api listening" "$APILOG" >/dev/null 2>&1; then
  echo "migration_startup_ok=false"
  echo "api_log_path=$APILOG"
  cat "$APILOG"
  kill "$API_PID" >/dev/null 2>&1 || true
  wait "$API_PID" >/dev/null 2>&1 || true
  exit 1
fi

kill "$API_PID" >/dev/null 2>&1 || true
wait "$API_PID" >/dev/null 2>&1 || true

echo "migration_startup_ok=true"

TABLE_COUNT="$(psql -h 127.0.0.1 -p "$PORT" -U "$USER_NAME" -d "$DB_SRC" -At -c \
  "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema='public';")"
echo "public_table_count=$TABLE_COUNT"

psql -h 127.0.0.1 -p "$PORT" -U "$USER_NAME" -d "$DB_SRC" -v ON_ERROR_STOP=1 -c \
  "INSERT INTO projects (id, name, slug, api_key_hash) VALUES ('proj_backup_probe','Backup Probe','backup-probe','hash') ON CONFLICT (id) DO NOTHING;" >/dev/null
echo "sentinel_inserted=true"

pg_dump -h 127.0.0.1 -p "$PORT" -U "$USER_NAME" -d "$DB_SRC" -Fc -f "$DUMP_FILE"
echo "backup_dump_size_bytes=$(wc -c < "$DUMP_FILE" | tr -d ' ')"

createdb -h 127.0.0.1 -p "$PORT" -U "$USER_NAME" "$DB_RESTORE"
pg_restore -h 127.0.0.1 -p "$PORT" -U "$USER_NAME" -d "$DB_RESTORE" "$DUMP_FILE"
RESTORE_SENTINEL="$(psql -h 127.0.0.1 -p "$PORT" -U "$USER_NAME" -d "$DB_RESTORE" -At -c \
  "SELECT COUNT(*) FROM projects WHERE id='proj_backup_probe';")"
echo "restore_sentinel_count=$RESTORE_SENTINEL"

createdb -h 127.0.0.1 -p "$PORT" -U "$USER_NAME" "$DB_ROLLBACK"
pg_restore -h 127.0.0.1 -p "$PORT" -U "$USER_NAME" -d "$DB_ROLLBACK" "$DUMP_FILE"
psql -h 127.0.0.1 -p "$PORT" -U "$USER_NAME" -d "$DB_ROLLBACK" -v ON_ERROR_STOP=1 -c \
  "INSERT INTO projects (id, name, slug, api_key_hash) VALUES ('proj_rollback_mutation','Rollback Mutation','rollback-mutation','hash') ON CONFLICT (id) DO NOTHING;" >/dev/null

MUTATED_COUNT="$(psql -h 127.0.0.1 -p "$PORT" -U "$USER_NAME" -d "$DB_ROLLBACK" -At -c \
  "SELECT COUNT(*) FROM projects WHERE id='proj_rollback_mutation';")"
echo "rollback_mutated_count_before_restore=$MUTATED_COUNT"

dropdb -h 127.0.0.1 -p "$PORT" -U "$USER_NAME" "$DB_ROLLBACK"
createdb -h 127.0.0.1 -p "$PORT" -U "$USER_NAME" "$DB_ROLLBACK"
pg_restore -h 127.0.0.1 -p "$PORT" -U "$USER_NAME" -d "$DB_ROLLBACK" "$DUMP_FILE"

ROLLBACK_MUTATED_AFTER="$(psql -h 127.0.0.1 -p "$PORT" -U "$USER_NAME" -d "$DB_ROLLBACK" -At -c \
  "SELECT COUNT(*) FROM projects WHERE id='proj_rollback_mutation';")"
ROLLBACK_SENTINEL_AFTER="$(psql -h 127.0.0.1 -p "$PORT" -U "$USER_NAME" -d "$DB_ROLLBACK" -At -c \
  "SELECT COUNT(*) FROM projects WHERE id='proj_backup_probe';")"
echo "rollback_mutated_count_after_restore=$ROLLBACK_MUTATED_AFTER"
echo "rollback_sentinel_count_after_restore=$ROLLBACK_SENTINEL_AFTER"

echo "ops_drill_complete=true"

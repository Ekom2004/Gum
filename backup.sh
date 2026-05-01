#!/bin/bash
set -uo pipefail

# Temporarily disable exit on error so find doesn't kill the script
set +e
SOCK=$(find /var/run /tmp /var/lib -type s -name .s.PGSQL.5432 2>/dev/null | head -n1)
if [ -z "$SOCK" ]; then
  # Fallback to full filesystem search if not in common paths
  SOCK=$(find / -type s -name .s.PGSQL.5432 2>/dev/null | head -n1)
fi
set -e

if [ -z "$SOCK" ]; then
  echo "Could not find PostgreSQL socket."
  exit 1
fi

SOCK_DIR=$(dirname "$SOCK")
D=/data/gum_ops_$(date +%Y%m%d_%H%M%S).dump

echo "Found socket at: $SOCK"
echo "Creating backup dump: $D"

# Run pg_dump as postgres but write the file as root so we have permissions in /data
su -s /bin/bash postgres -c "pg_dump -h $SOCK_DIR -U postgres -d gum_api_stg -Fc" > $D
echo "Backup created. Size:"
ls -lh $D

echo "Dropping/creating restore probe DB..."
su -s /bin/bash postgres -c "dropdb --if-exists -h $SOCK_DIR -U postgres gum_restore_probe"
su -s /bin/bash postgres -c "createdb -h $SOCK_DIR -U postgres gum_restore_probe"

echo "Restoring to probe DB..."
# We pipe the file contents as root to pg_restore running as postgres
cat $D | su -s /bin/bash postgres -c "pg_restore -h $SOCK_DIR -U postgres -d gum_restore_probe"

echo "Database restored. Counting projects:"
su -s /bin/bash postgres -c "psql -h $SOCK_DIR -U postgres -d gum_restore_probe -At -c 'select count(*) from projects;'"

echo "Backup file is at: $D"

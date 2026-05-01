# Gum Ops Runbook

Last updated: April 27, 2026

This runbook covers the beta ops guardrails:

- migrations apply cleanly
- backup and restore drill
- rollback procedure

## Scope

Applies to:

- staging Fly apps: `gum-api-stg`, `gum-runner-stg`, `gum-pg-stg`
- local verification drills from this repository

## 1) Local Ops Drill (repeatable)

Run from repo root:

```bash
KEEP_OPS_ARTIFACTS=1 ./scripts/ops_checks_local.sh
```

What this does:

1. boots a temporary local Postgres cluster
2. starts `gum-api` against it (this applies migrations via `prepare_dev_database`)
3. inserts a sentinel row
4. creates a logical backup (`pg_dump -Fc`)
5. restores into a second DB and verifies sentinel presence
6. simulates rollback by restoring over a mutated DB and verifies mutation removal

Expected success markers:

- `migration_startup_ok=true`
- `restore_sentinel_count=1`
- `rollback_mutated_count_after_restore=0`
- `ops_drill_complete=true`

## 2) Staging Ops Checks

### 2.1 Preflight

```bash
flyctl auth whoami
flyctl status -a gum-api-stg
flyctl status -a gum-pg-stg
flyctl postgres db list -a gum-pg-stg
```

### 2.2 Backup Enablement

Backups must be enabled on `gum-pg-stg` before soak/launch:

```bash
flyctl machine update 8d10e3c5420698 -a gum-pg-stg --vm-memory 512 --yes
flyctl postgres backup enable -a gum-pg-stg
```

If Fly CLI prints:

- `To agree, the --yes flag must be specified when not running interactively`
- and this version does not accept `--yes` for `postgres backup enable`

then this is a CLI interaction/version blocker. Resolve by:

1. running `flyctl postgres backup enable -a gum-pg-stg` interactively in a real TTY shell and accepting terms, or
2. upgrading Fly CLI to a version that supports non-interactive consent on this command.

### 2.3 Backup/Restore Drill (staging)

After backups are enabled:

```bash
flyctl postgres backup create -a gum-pg-stg
flyctl postgres backup list -a gum-pg-stg
```

Optional WAL restore validation into a separate cluster:

```bash
flyctl postgres backup restore -a gum-pg-stg --help
```

Record:

- backup id
- timestamp
- target restore app id (if exercised)
- validation query result

## 3) Rollback Procedure

### 3.1 App rollback (API/runner)

1. Find prior healthy image release in Fly:
   - `flyctl status -a gum-api-stg`
   - `flyctl status -a gum-runner-stg`
2. Deploy prior known-good image/config.
3. Verify health and onboarding smoke.

### 3.2 Data rollback

1. Confirm latest successful backup exists:
   - `flyctl postgres backup list -a gum-pg-stg`
2. Restore backup into a new Postgres cluster (preferred), validate, then repoint apps.
3. If restoring in-place is unavoidable, stop write traffic first and record exact backup id + operator + timestamp.

## 4) Evidence Template

Capture these in `docs/GUM_BETA_7_DAY_CHECKLIST.md`:

- command
- date/time
- pass/fail
- artifact path / backup id
- blocker + owner if failed

## 5) Staging Soak (BETA-009)

Run 24h soak (10-minute cadence) from repo root:

```bash
GUM_API_BASE_URL="https://gum-api-stg.fly.dev" \
GUM_API_KEY="<gum_live_key>" \
GUM_ADMIN_KEY="<gum_admin_key>" \
SOAK_DURATION_MINUTES=1440 \
SOAK_INTERVAL_SECONDS=600 \
./scripts/staging_soak.sh
```

Useful overrides:

- `SOAK_MAX_FAILURES` (default `1`)
- `SOAK_FAIL_FAST` (default `1`)
- `KEEP_SOAK_SMOKE_ARTIFACTS` (default `0`)
- `SOAK_DIR` (explicit output directory instead of auto temp dir)

Expected output includes:

- `status=passed` or `status=failed`
- `attempts=...`
- `passes=...`
- `failures=...`
- `last_run_id=...`
- `soak_log=...`

## 6) Provider Migrations (No-Downtime)

For database-provider and compute-provider moves, use:

- `docs/GUM_ZERO_DOWNTIME_MIGRATION_PLAYBOOK.md`
- `docs/GUM_COMPUTE_PROVIDER_ADAPTERS.md`

That playbook defines:

- go/no-go gates
- phased rollout sequence
- rollback triggers
- evidence requirements

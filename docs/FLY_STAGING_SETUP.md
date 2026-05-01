# Fly Staging Setup

This document wires Gum staging on Fly with:

- `gum-api-stg`
- `gum-runner-stg`
- `gum-pg-stg`

Default org: `gum`  
Default region: `yyz`

## Prerequisites

- Fly CLI installed (`fly version`)
- Logged in (`fly auth login`)
- Access to Fly org `gum`

## One-command staging bootstrap

From repo root:

```bash
./scripts/fly_staging_up.sh
```

This script will:

1. create API and runner apps if missing
2. create Postgres if missing
3. attach Postgres to API (`DATABASE_URL`)
4. set API + admin + internal runner secrets
5. configure the Gum secret backend through environment-driven adapter settings
6. deploy API and runner

Environment overrides:

```bash
FLY_ORG=gum \
FLY_REGION=yyz \
FLY_API_APP=gum-api-stg \
FLY_RUNNER_APP=gum-runner-stg \
FLY_PG_APP=gum-pg-stg \
GUM_RUNNER_COMPUTE_CLASS=standard \
GUM_RUNNER_CPU_CORES=1 \
GUM_RUNNER_MEMORY_MB=512 \
GUM_RUNNER_MAX_CONCURRENT_LEASES=2 \
GUM_SECRET_BACKEND=postgres \
GUM_SECRET_MASTER_KEY=<32-byte key in hex/base64/raw> \
GUM_API_KEY=your_api_key \
GUM_ADMIN_KEY=your_admin_key \
./scripts/fly_staging_up.sh
```

Secret backend notes:

- `GUM_SECRET_BACKEND=postgres` is the staging default and is the recommended durable path.
- If `GUM_SECRET_MASTER_KEY` is omitted on the first bootstrap, the script generates one and stores it in Fly secrets.
- On later reruns, the script preserves the existing Fly-side `GUM_SECRET_MASTER_KEY` unless you explicitly provide a replacement.
- For production or any rebuild where you might recreate the app, provide a stable `GUM_SECRET_MASTER_KEY` from your own secret manager.
- `GUM_SECRET_BACKEND=memory` remains available for disposable dev environments, but it is not durable.

Keep runner capacity aligned with actual VM size. Example:

- `shared-cpu-1x` + `512mb` VM -> `GUM_RUNNER_CPU_CORES=1`, `GUM_RUNNER_MEMORY_MB=512`

## Verify onboarding flow against Fly

After deploy succeeds:

```bash
GUM_SMOKE_USE_EXISTING_API=1 \
GUM_API_BASE_URL=https://gum-api-stg.fly.dev \
GUM_API_KEY=<api_key_from_bootstrap> \
GUM_ADMIN_KEY=<admin_key_from_bootstrap> \
KEEP_SMOKE_ARTIFACTS=1 \
./scripts/beta_onboarding_smoke.sh
```

This validates:

`gum init -> gum deploy -> enqueue -> gum logs -> gum admin`

## Sync runner capacity from job knobs

If your job decorators set `cpu=` / `memory=`, Gum can sync Fly runner capacity automatically during `gum deploy`.

Enable by setting `FLY_RUNNER_APP` in the deploy environment:

```bash
export GUM_COMPUTE_PROVIDER=fly
export FLY_RUNNER_APP=gum-runner-stg
export GUM_RUNNER_PARALLELISM=1
gum deploy
```

Gum deploy will:

1. discover jobs and read max `cpu` / `memory` requirements
2. update runner secrets:
   - `GUM_RUNNER_CPU_CORES`
   - `GUM_RUNNER_MEMORY_MB`
   - `GUM_RUNNER_MAX_CONCURRENT_LEASES`
3. update Fly runner machine CPU/memory to the same values

To disable auto-sync:

```bash
export GUM_AUTO_SYNC_RUNNER_CAPACITY=0
```

Manual fallback (operator workflow):

```bash
cd /path/to/your/gum-python-project
FLY_RUNNER_APP=gum-runner-stg \
GUM_RUNNER_COMPUTE_CLASS=standard \
GUM_RUNNER_PARALLELISM=1 \
/path/to/gum-fresh/scripts/fly_runner_capacity_sync.sh .
```

Set `GUM_RUNNER_PARALLELISM` to reserve capacity for multiple simultaneous heavy attempts per runner.

## Ops follow-up

After staging is up, run the ops checklist in:

- `docs/GUM_OPS_RUNBOOK.md`

At minimum complete:

- backup enablement on `gum-pg-stg`
- backup/restore validation
- rollback readiness notes

## Adapter architecture

Provider adapter details and swap model live in:

- `docs/GUM_COMPUTE_PROVIDER_ADAPTERS.md`

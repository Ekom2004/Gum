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
4. set API + runner secrets
5. deploy API and runner

Environment overrides:

```bash
FLY_ORG=gum \
FLY_REGION=yyz \
FLY_API_APP=gum-api-stg \
FLY_RUNNER_APP=gum-runner-stg \
FLY_PG_APP=gum-pg-stg \
GUM_ADMIN_KEY=your_admin_key \
./scripts/fly_staging_up.sh
```

## Verify onboarding flow against Fly

After deploy succeeds:

```bash
GUM_SMOKE_USE_EXISTING_API=1 \
GUM_API_BASE_URL=https://gum-api-stg.fly.dev \
GUM_ADMIN_KEY=<admin_key_from_bootstrap> \
KEEP_SMOKE_ARTIFACTS=1 \
./scripts/beta_onboarding_smoke.sh
```

This validates:

`gum init -> gum deploy -> enqueue -> gum logs -> gum admin`

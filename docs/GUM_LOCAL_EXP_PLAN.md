# Gum Local EXP Plan

## Goal

Make local onboarding and iteration feel like:

`pip install usegum -> gum login -> gum init -> gum dev up -> gum deploy -> .enqueue() -> gum logs`

without requiring users to run internal scripts manually.

## Why This Exists

We already have reliable script-based local/staging checks, but first-time user flow is still fragmented.
This plan converts that into a productized CLI path.

## Current Building Blocks

- SDK and CLI user commands (`login`, `init`, `deploy`, `get`, `logs`).
- Internal local/staging scripts:
  - `scripts/beta_onboarding_smoke.sh`
  - `scripts/ops_checks_local.sh`
- Stable run/lease/retry semantics in API + runner.

## Scope

### P0: Usable Local DX

1. Add `gum dev up`
- Start local Postgres, API, and runner.
- Write process state under `.gum/dev/`.
- Write logs under `.gum/dev/logs/`.

2. Add `gum dev down`
- Stop all local Gum processes cleanly.
- Clean stale pid/state entries.

3. Add `gum dev status`
- Show service health, ports, and PID state.
- Show quick pointers to log files.

4. Add `gum doctor`
- Preflight checks for required local dependencies and port conflicts.
- Return actionable fix instructions.

5. Auto-target local API in dev mode
- If `gum dev up` is active, CLI commands (`deploy/get/logs`) default to local API base URL unless explicitly overridden.

### P1: Fast Iteration

1. Add `gum deploy --watch`
- Auto redeploy on job file changes.

2. Add `gum run <job> --payload ...` local dry-run
- Validate payload and execution path quickly before cloud execution.

## Implementation Order

1. CLI command scaffolding for `gum dev up/down/status` and `gum doctor`.
2. Shared process/state manager in SDK CLI module.
3. Local bootstrap wiring (Postgres/API/runner process orchestration).
4. Auto-targeting logic for local API.
5. End-to-end tests for local flow.
6. Docs pass and quickstart consolidation.

## Acceptance Criteria

1. New user can complete first successful local run in under 10 minutes.
2. No manual `cargo run` or internal script invocation needed for normal onboarding.
3. CLI-only flow passes in CI:
- `gum init`
- `gum dev up`
- `gum deploy`
- enqueue job
- `gum logs`
- `gum dev down`

## Non-Goals (for this phase)

- Multi-node local clustering.
- Production-grade local observability stack.
- Provider adapter expansion beyond current behavior.

## Notes

- Keep existing scripts as internal reliability/ops checks.
- Product UX should be CLI-first and script-free for standard users.

# Gum Implementation Slice 1

This document locks the first implementation milestone for Gum.

The goal of slice 1 is to prove the core runtime path:

1. define a Python job
2. deploy it
3. enqueue it
4. execute it
5. capture logs
6. inspect run state
7. replay the run

Nothing outside that path is required for the first milestone.

## Scope

Slice 1 includes:
- Python SDK job definition
- deploy registration
- enqueue API
- leased dispatch
- runner execution
- timeout enforcement
- retry handling
- shared rate-limit pools
- enqueue-time duplicate protection
- log retrieval
- replay

Slice 1 does not include:
- scheduler
- backfill
- TypeScript SDK
- dashboard work

## Rust Crates

The first Gum backend crates are:

### `gum-types`

Shared types for:
- run statuses
- trigger types
- attempt statuses
- job policy
- deploy metadata

### `gum-store`

Database-facing layer for:
- projects
- deploys
- jobs
- runs
- attempts
- leases
- logs

### `gum-queue`

Dispatch logic for:
- selecting eligible queued runs
- creating leases
- expiring lost leases
- applying per-function concurrency and rate-limit checks

### `gum-runner`

Execution logic for:
- polling leases
- downloading bundles
- resolving handlers
- enforcing timeout
- streaming logs
- reporting completion

### `gum-api`

Control-plane API for:
- deploy registration
- enqueue
- run lookup
- replay
- log retrieval
- internal lease/attempt endpoints

## First Database Tables

Slice 1 requires these tables:
- `projects`
- `deploys`
- `jobs`
- `runs`
- `attempts`
- `leases`
- `logs`

The exact table semantics are defined in `docs/GUM_ARCHITECTURE.md`.

## First API Endpoints

External:
- `POST /v1/deploys`
- `POST /v1/jobs/{job_id}/runs`
- `GET /v1/runs/{run_id}`
- `POST /v1/runs/{run_id}/replay`
- `GET /v1/runs/{run_id}/logs`

Internal:
- `POST /internal/runs/lease`
- `POST /internal/attempts/{attempt_id}/complete`

## First Runner Loop

The first runner loop is:

1. poll for a lease
2. download bundle
3. resolve handler
4. execute attempt
5. stream logs
6. enforce timeout
7. report success, failure, or timeout

## First Canonical Example

```python
import gum

salesforce_limit = gum.rate_limit("20/m")

@gum.job(retries=5, timeout="5m", rate_limit=salesforce_limit, concurrency=5, key="customer_id")
def sync_customer(customer_id: str):
    ...

sync_customer.enqueue(customer_id="cus_123")
```

If Gum can make this example real end to end, slice 1 is successful.

## Deferred Work

Defer until after slice 1:
- `every` scheduling runtime
- backfill
- per-key concurrency
- long-lived orchestration
- artifacts/output management

## Exit Criteria

Slice 1 is complete when:
- a deployed Python job can be enqueued
- the runner executes it through a lease
- logs are stored and retrievable
- retries happen automatically
- timeouts are enforced
- a failed run can be replayed

# Gum Architecture

This document locks the v1 system design for Gum.

Gum is built around one central object:
- `Run`

Everything else exists to:
- create runs
- decide when they may start
- execute them
- record what happened

## Services

Gum v1 is made of these core services:

1. API / control plane
2. scheduler
3. dispatch queue
4. runner fleet
5. Postgres
6. object storage
7. log storage

## 1. API / Control Plane

The API service is the source of truth.

It owns:
- projects
- API keys
- deploys
- jobs
- runs
- replay
- backfill requests
- secrets metadata
- policy validation

It does not execute jobs directly.

## 2. Scheduler

The scheduler decides when a run should exist.

It owns:
- `every=...`
- delayed retries
- delayed scheduled runs

Its job is simple:
- determine whether a job should produce a run now

The scheduler does not execute runs.

## 3. Dispatch Queue

The queue is the boundary between control plane and execution.

It owns:
- queued runs waiting for execution
- leasing runs to runners
- redelivery when a runner dies or a lease expires

The queue must support:
- lease owner
- lease expiry
- acknowledge on completion
- requeue on lost lease

For v1, the queue can be backed by Postgres.

## 4. Runner Fleet

The runner fleet is the execution plane.

It owns:
- fetching deploy bundles
- resolving job handlers
- injecting environment and payload
- executing the function
- enforcing timeout
- streaming logs
- reporting attempt results

The runner must be:
- bounded
- isolated
- disposable

Gum runners are for bounded job execution, not general app hosting.

## 5. Postgres

Postgres is the system of record.

It stores:
- projects
- deploys
- jobs
- runs
- attempts
- leases
- logs metadata
- replay lineage

For v1, Postgres also backs queue state.

## 6. Object Storage

Object storage is used for:
- deploy bundles
- large logs later if needed
- future artifacts if needed

Deploy bundles should not live in Postgres.

## 7. Log Storage

Every run gets:
- stdout/stderr logs
- structured log events
- attempt-aware retrieval

For v1, simple per-run log storage is enough.

## Core Data Model

### `projects`

Fields:
- `id`
- `name`
- `slug`
- `api_key_hash`
- `created_at`
- `updated_at`

Purpose:
- tenant boundary for jobs, deploys, and runs

### `deploys`

Fields:
- `id`
- `project_id`
- `version`
- `bundle_url`
- `bundle_sha256`
- `sdk_language`
- `entrypoint`
- `status`
- `created_at`

Purpose:
- immutable uploaded code bundle
- stable execution target for runs and replay

### `jobs`

Fields:
- `id`
- `project_id`
- `deploy_id`
- `name`
- `handler_ref`
- `trigger_mode`
- `schedule_expr`
- `retries`
- `timeout_secs`
- `rate_limit_spec`
- `concurrency_limit`
- `enabled`
- `created_at`
- `updated_at`

Purpose:
- durable job definition and policy

### `runs`

Fields:
- `id`
- `project_id`
- `job_id`
- `deploy_id`
- `trigger_type`
- `status`
- `input_json`
- `attempt_count`
- `max_attempts`
- `scheduled_at`
- `started_at`
- `finished_at`
- `failure_reason`
- `replay_of_run_id`
- `created_at`
- `updated_at`

Purpose:
- logical execution lineage for one job input

### `attempts`

Fields:
- `id`
- `run_id`
- `attempt_number`
- `status`
- `lease_id`
- `runner_id`
- `started_at`
- `finished_at`
- `failure_reason`
- `created_at`

Purpose:
- concrete execution tries within one run

### `leases`

Fields:
- `id`
- `attempt_id`
- `runner_id`
- `leased_at`
- `expires_at`
- `acked_at`
- `released_at`

Purpose:
- dispatch ownership contract between queue and runner

### `logs`

Fields:
- `id`
- `run_id`
- `attempt_id`
- `ts`
- `stream`
- `message`

Purpose:
- per-run and per-attempt log retrieval

## Run Lifecycle

Public run states:
- `queued`
- `running`
- `succeeded`
- `failed`
- `timed_out`
- `canceled`

Internal flow:

1. API or scheduler creates a run
2. run enters `queued`
3. runner leases work
4. attempt enters `running`
5. attempt succeeds, fails, or times out
6. run becomes terminal or is requeued for retry

Replay creates a new run with:
- `trigger_type = replay`
- `replay_of_run_id = prior run`

Backfill creates many normal runs.

## Retry Semantics

Gum uses this rule:
- `retries = N` means `N` retry attempts after the first attempt

Example:
- `retries = 5`
- maximum total attempts = `6`

Retry behavior:
- if an attempt fails and attempts remain, the run is requeued
- if attempts are exhausted, the run becomes terminal

Retry is automatic.
Replay is operator-driven.

## Timeout Semantics

Timeout is enforced by the runner.

If an attempt exceeds `timeout`:
- the process is terminated
- the attempt is marked `timed_out`
- retry policy is evaluated

Timeout must be a hard kill, not just a status update.

## Concurrency Semantics

V1 concurrency is per job.

Meaning:
- at most `N` attempts for that job may be actively running at once

If the limit is reached:
- further runs remain queued

Per-key concurrency is out of scope for v1.

## Rate Limit Semantics

V1 rate limits are per job.

Example:
- `20/m`

Meaning:
- that job may start at most 20 runs per minute

If the rate limit is exhausted:
- the run remains queued

Shared rate-limit pools are out of scope for v1.

## Lease Semantics

Lease flow:

1. runner asks for eligible work
2. control plane selects a queued run
3. next attempt is created if needed
4. lease is created for that attempt
5. runner receives payload + deploy metadata
6. runner executes and completes or loses lease

If a runner dies:
- lease expires
- the attempt is considered lost
- the run becomes eligible for recovery

This is how Gum avoids orphaned work.

## Deploy Model

V1 deploy flow:

1. user writes Python jobs
2. `gum deploy`
3. Gum packages code into a bundle
4. bundle is uploaded to object storage
5. jobs are registered against the deploy
6. deploy becomes active for future runs

Runs point to deploys directly.

This keeps:
- replay stable
- execution reproducible
- operator behavior understandable

## V1 Execution Path

The first path Gum must support end to end is:

1. define Python job
2. deploy bundle
3. enqueue run
4. lease run
5. execute attempt
6. capture logs
7. succeed or fail
8. retry if needed
9. replay manually if desired

This path is the product core.

## V1 Non-Goals

Out of scope for this architecture:
- workflows
- step graphs
- waits as a first-class execution primitive
- shared rate-limit pools
- per-key concurrency
- multi-language execution
- advanced artifact/output systems

# Gum Contract

Gum runs bounded background jobs.

A job can be:
- scheduled
- enqueued

Every run has:
- one input
- one policy
- one status
- one log stream
- one replay path

This document defines the v1 contract for Gum.

## Product Boundary

Gum owns:
- scheduling
- run dispatch
- retries
- timeout enforcement
- rate limits
- concurrency limits
- run state
- logs
- replay
- managed execution

Users own:
- function code
- business logic
- integrations
- payloads

Gum is not:
- a workflow engine
- an event bus
- an agent framework

## Core Objects

### Job

A durable definition of:
- what code to run
- how Gum should run it

Required fields:
- `id`
- `run`

Optional fields:
- `input`
- `every`
- `retries`
- `timeout`
- `memory`
- `rate_limit`
- `concurrency`
- `key`

Canonical Python shape:

```python
import gum

salesforce_limit = gum.rate_limit("20/m")

@gum.job(retries=8, timeout="15m", memory="1gb", rate_limit=salesforce_limit, concurrency=5)
def sync_customer(customer_id: str):
    salesforce.upsert_customer(customer_id)
```

### Run

A single execution lineage of a job.

Fields:
- `id`
- `jobId`
- `status`
- `input`
- `attempt`
- `triggerType`
- `scheduledAt`
- `startedAt`
- `finishedAt`
- `failureReason`
- `replayOf`

### Backfill

A bulk enqueue request for one job.

Fields:
- `id`
- `jobId`
- `status`
- `itemCount`
- `createdAt`

Backfill creates normal runs.
It does not bypass retries, rate limits, or concurrency limits.

## Trigger Contract

There are two primary trigger modes in v1:

### Scheduled

Gum creates runs automatically from `every`.

Example:

```python
@gum.job(every="1d", retries=5, timeout="10m")
def nightly_sync():
    sync_everything()
```

### Enqueued

Application code explicitly requests a run.

Example:

```python
sync_customer.enqueue(customer_id="cus_123")
```

## Job Definition Contract

### `id`

Stable job identifier within a project.

### `input`

Optional input schema.

v1 recommendation:
- Python type hints first
- optional Pydantic-style validation later if we need stricter payload contracts

If present:
- enqueue payloads must validate against it
- `run` receives the typed payload

### `run`

The actual function body.

This is the business logic Gum executes.

### `every`

Schedule expression for automatic runs.

v1:
- support simple duration-style scheduling first
- example: `"1d"`, `"20m"`, `"7d"`

### `retries`

Maximum retry attempts after the first run.

Example:
- `retries=4`
- total possible attempts = `1 initial + 4 retries`

### `timeout`

Hard wall-clock limit for one attempt.

If exceeded:
- the attempt is terminated
- the run becomes `timed_out`
- retry policy is evaluated

### `memory`

Memory required by one execution attempt.

Example:

```python
@gum.job(memory="4gb")
def render_video(video_id: str):
    ...
```

Meaning:
- Gum treats memory as a placement requirement
- a runner must have enough remaining memory capacity before leasing the run
- memory is per attempt, not per logical run

### `rate_limit`

Maximum run starts allowed in a time window.

Two forms are supported:

#### Per-job

```python
@gum.job(rate_limit="20/m")
def sync_customer(...):
    ...
```

Meaning:
- this function cannot start more than 20 runs per minute

#### Shared pool

```python
openai_limit = gum.rate_limit("60/m")

@gum.job(rate_limit=openai_limit)
def summarize(...):
    ...

@gum.job(rate_limit=openai_limit)
def embed(...):
    ...
```

Meaning:
- all jobs in the same project using `openai_limit` share one 60/min budget

Shared pools are:
- project-scoped
- inferred from module-level `gum.rate_limit(...)` binding names in the Python SDK
- rejected if the same pool name has conflicting definitions

### `concurrency`

Maximum simultaneous active runs for this job.

Example:

```python
@gum.job(concurrency=5)
def sync_customer(...):
    ...
```

Meaning:
- no more than 5 runs of this function may be active at once

v1:
- concurrency is per function
- per-key concurrency is out of scope

### `key`

Field name used for enqueue-time duplicate protection.

Example:

```python
@gum.job(key="event_id")
def process_webhook(event_id: str, event: dict):
    ...
```

Meaning:
- a duplicate enqueue with the same `event_id` returns the existing run
- retries stay inside the same keyed run
- replay intentionally bypasses dedupe and creates new work

## Run Status Contract

Public run statuses:
- `queued`
- `running`
- `succeeded`
- `failed`
- `timed_out`
- `canceled`

Internal states may exist, but these are the product statuses.

## Retry Contract

When an attempt fails:
- if retry attempts remain, Gum schedules another attempt
- all attempts belong to the same logical run lineage

Needed run metadata:
- `attempt`
- `maxAttempts`
- `nextRetryAt`

Retry is automatic.

Replay is separate.

## Replay Contract

Replay is operator-triggered.

Replay creates:
- a new run
- with the same input
- under the same job
- with `triggerType = "replay"`
- with `replayOf = <original_run_id>`

v1 supports:
- replay one run by `runId`

v1.1 target:
- replay filtered failed subset

## Backfill Contract

Backfill is bulk enqueue for a single job.

Canonical shape:

```python
sync_customer.backfill([
    {"customer_id": "cus_1"},
    {"customer_id": "cus_2"},
    {"customer_id": "cus_3"},
])
```

Semantics:
- creates one run per item
- each run obeys the job's retry policy
- each run obeys the job's rate limit
- each run obeys the job's concurrency cap

Backfill is not a separate execution model.

## Logs Contract

Every run gets:
- run metadata
- stdout/stderr log stream
- timestamped log events

Minimum v1 behavior:
- logs visible per run
- logs retained for the plan retention window

## SDK Contract

The v1 SDK surface should stay small and Python-first:

- `@gum.job(...)`
- `gum.rate_limit(...)`
- `job.enqueue(...)`
- `job.backfill(...)`
- `gum.runs.get(...)`
- `gum.runs.replay(...)`

## v1 / v1.1 Split

### v1

Support:
- scheduled jobs
- enqueued jobs
- retries
- timeout
- memory sizing
- per-function and shared-pool rate limits
- per-function concurrency
- enqueue-time duplicate protection with `key`
- logs
- replay one run

### v1.1

Add:
- bulk backfill as a first-class UX
- replay failed subset

## Non-Goals

Out of scope for v1:
- DAGs
- step workflows
- agent orchestration
- budget accounting
- per-key concurrency
- cross-job fairness scheduling

## Decision Rules

If a feature makes Gum feel like:
- a workflow engine
- a queue toolkit
- or a general event platform

do not add it to v1.

If a feature makes a job definition feel like a stronger operational contract, it is a better fit.

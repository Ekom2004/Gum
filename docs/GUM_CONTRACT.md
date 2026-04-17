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
- `rateLimit`
- `concurrency`

Canonical TypeScript shape:

```ts
const syncCustomer = job("sync-customer", {
  input: z.object({
    customerId: z.string(),
  }),
  retries: 8,
  timeout: "15m",
  rateLimit: "salesforce:20/m",
  concurrency: 5,
  run: async ({ customerId }) => {
    await salesforce.upsertCustomer(customerId);
  },
});
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

```ts
const nightlySync = job("nightly-sync", {
  every: "1d",
  retries: 5,
  timeout: "10m",
  run: async () => {
    await syncEverything();
  },
});
```

### Enqueued

Application code explicitly requests a run.

Example:

```ts
await syncCustomer.enqueue({
  customerId: "cus_123",
});
```

## Job Definition Contract

### `id`

Stable job identifier within a project.

### `input`

Optional input schema.

v1 recommendation:
- `zod` in TypeScript

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
- `retries: 4`
- total possible attempts = `1 initial + 4 retries`

### `timeout`

Hard wall-clock limit for one attempt.

If exceeded:
- the attempt is terminated
- the run becomes `timed_out`
- retry policy is evaluated

### `rateLimit`

Maximum run starts allowed in a time window.

Two forms are supported:

#### Per-job

```ts
rateLimit: "20/m"
```

Meaning:
- this job cannot start more than 20 runs per minute

#### Shared pool

```ts
rateLimit: "openai:60/m"
```

Meaning:
- all jobs in the same project using pool `openai` share one 60/min budget

Shared pools are:
- project-scoped in v1.1

### `concurrency`

Maximum simultaneous active runs for this job.

Example:

```ts
concurrency: 5
```

Meaning:
- no more than 5 runs of this job may be active at once

v1:
- concurrency is per job
- per-key concurrency is out of scope

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

```ts
await syncCustomer.backfill([
  { customerId: "cus_1" },
  { customerId: "cus_2" },
  { customerId: "cus_3" },
]);
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

The v1 SDK surface should stay small.

TypeScript:
- `job(...)`
- `job.enqueue(...)`
- `job.backfill(...)`
- `gum.runs.get(...)`
- `gum.runs.replay(...)`

Python:
- `@gum.job(...)`
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
- per-job rate limits
- per-job concurrency
- logs
- replay one run

### v1.1

Add:
- shared rate-limit pools
- bulk backfill as a first-class UX
- replay failed subset

## Non-Goals

Out of scope for v1:
- DAGs
- step workflows
- agent orchestration
- dedupe as a primary product feature
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


# Gum Duplication Invariants

This document defines the anti-duplication rules for Gum.

The goal is:

- one logical unit of work should map to one `run_id`
- retries, holds, and recovery should create new `attempt_id`s, not new runs
- knobs must never accidentally clone work
- `key` should be the only enqueue-time dedupe mechanism
- users should have a clear way to make downstream side effects idempotent

## Core Distinction

Gum needs to separate two different problems:

- duplicate Gum work
- duplicate user side effects

These are not the same.

### Duplicate Gum Work

This is Gum’s responsibility.

Examples:

- a retry path accidentally creates a second run
- a schedule tick enqueues twice
- lost-runner recovery causes sibling runs
- a held run is re-enqueued instead of resumed

Gum should prevent these by design.

### Duplicate User Side Effects

This is only partially Gum’s responsibility.

Examples:

- the function calls Stripe successfully
- the runner crashes before Gum records success
- Gum retries later
- Stripe sees the call twice unless the user supplied downstream idempotency

Gum cannot promise exactly-once external side effects on its own.

So the product contract should be:

- Gum guarantees stable logical run identity
- Gum helps the user carry identity into downstream systems
- true exactly-once side effects still depend on downstream idempotency

## Identity Model

Gum should treat these identities separately:

### `run_id`

The identity of one logical unit of work.

This is what should remain stable across:

- retries
- timeout retries
- rate-limit holds
- concurrency waits
- health holds

### `attempt_id`

The identity of one execution attempt for a run.

This changes across:

- retries
- timeout retry
- lost-runner recovery retry

### `key`

Optional enqueue-time dedupe identity.

This answers:

- should this enqueue create a new run?

It should not change:

- retry identity
- timeout identity
- hold identity

## Hard Invariants

These should hold regardless of knob combination.

### Invariant 1: Retry Never Forks Work

Retries must never create a new `run_id`.

Retry logic should only:

- keep the existing run
- schedule another attempt later

### Invariant 2: Timeout Never Forks Work

A timeout is an attempt failure.

It may:

- finish the current attempt
- schedule a later retry

It must not:

- create a sibling run

### Invariant 3: Rate-Limit Hold Never Forks Work

If a run is over budget:

- it waits
- it does not duplicate

Parking a run behind rate limits must preserve the same `run_id`.

### Invariant 4: Concurrency Wait Never Forks Work

If concurrency is full:

- the run remains queued
- it does not duplicate

Opening a slot later should lease the same run, not a clone.

### Invariant 5: Health Hold Never Forks Work

If function health is degraded or down:

- queued work is held
- retry budget may be preserved

The held run remains the same logical run.

### Invariant 6: Replay Is The Explicit New-Work Path

Replay is allowed to create a new `run_id`.

This is intentional, user-requested new work.

### Invariant 7: Schedule Ticks Create New Work, But Only Once Per Tick

A fresh schedule tick may create a new run.

But Gum must never:

- enqueue the same due tick twice
- create phantom overlap runs for the same tick

### Invariant 8: Duplicate Enqueue Suppression Belongs To `key`

Without `key`, a second enqueue is a new logical request.

With `key`, a second enqueue may resolve to the existing run.

This keeps dedupe behavior narrow and predictable.

## What Each Knob Must Never Do

### `retries`

Must not:

- create a new run
- create a second dedupe record

May:

- create a new attempt
- delay that attempt

### `timeout`

Must not:

- fork a run

May:

- fail the current attempt
- trigger a retry attempt

### `rate_limit`

Must not:

- clone work while parked
- spend budget for duplicate enqueue that resolves to an existing run

### `concurrency`

Must not:

- clone work while waiting for a slot

### `schedule`

Must not:

- double-enqueue the same due tick

May:

- create a fresh run for each distinct due tick

### `priority`

Must not:

- create or duplicate work

May:

- reorder waiting work

### `key`

Must only control:

- whether a new enqueue creates a new run

It must not control:

- retry identity
- timeout identity
- hold identity

## Default Behavior Without `key`

Even when `key` is not configured, Gum should still avoid accidental duplication.

That means:

- one enqueue request creates one run
- retries stay inside that run
- holds stay inside that run
- recovery stays inside that run

So `key` is not the thing that makes Gum “safe.”

It is the thing that makes enqueue-time duplicate suppression explicit.

## `key` Semantics

When `key` is configured:

- dedupe is scoped to `(team_id, job_id, key_value)`
- duplicate enqueue returns the existing run
- retries remain inside that run
- replay bypasses dedupe and creates a new run

### `key` Is Enqueue Dedupe, Not Run Mutation

This is the most important rule.

`key` should only decide:

- should a new enqueue create a new run?

It should not decide:

- whether a retry is the same run
- whether a timeout is the same run
- whether a held run is the same run

Those must already be stable by Gum’s core model.

## Downstream Idempotency

Gum should expose enough execution identity for user code to protect side effects.

At minimum, user code should be able to access:

- `run_id`
- `attempt_id`
- resolved `key` if present
- replay lineage if relevant

This lets users pass identity into downstream systems:

- Stripe idempotency keys
- database dedupe keys
- provider request ids

## Recommended Product Contract

The honest contract should be:

- Gum guarantees no accidental duplicate runs from retries, holds, or recovery
- `key` lets Gum suppress duplicate enqueues for the same logical work
- downstream exactly-once side effects still require downstream idempotency

## Testing Strategy

This should be enforced with explicit invariants tests.

### Invariant Tests

For each knob combination, assert:

- one enqueue produces at most one run unless replay or a distinct schedule tick occurs

### High-Risk Combination Tests

Need direct coverage for:

- retry + timeout
- retry + lost-runner recovery
- retry + function health hold
- rate_limit + retry hold
- concurrency + queued wait
- schedule + concurrency=1
- cancel + replay
- key + retry
- key + cancel
- key + schedule

### Property-Style Direction

Longer term, Gum should generate sequences of:

- enqueue
- lease
- fail
- timeout
- lose runner
- hold by rate limit
- hold by concurrency
- replay
- schedule tick

And assert:

- no extra run ids appear unexpectedly
- only attempt ids grow for existing logical work

## Next Design Step

Before implementing `key`, Gum should keep this rule in mind:

- first make run identity stable across all current knobs
- then add enqueue-time dedupe on top

That keeps `key` small, correct, and easy to reason about.

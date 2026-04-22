# Gum Knob Design

This document defines Gum's public job configuration surface.

The goal is:

- expose only decisions users understand
- keep protective runtime behavior built into Gum
- avoid flat configuration growth

## Core Principle

Every exposed knob must answer:

- what user problem does this solve?
- can the user set it correctly without understanding Gum internals?
- does it compose cleanly with the other knobs?
- is it a contract Gum should support long-term?

If the answer is weak, the behavior stays internal.

## Public Job Knobs

The beta public surface is:

```python
@gum.job(
    retries=...,
    timeout=...,
    memory=...,
    rate_limit=...,
    concurrency=...,
    every=...,
    key=...,
)
def work(...):
    ...
```

These are the only job-level knobs users should see in public docs.

### `retries`

User intent:

- how many additional attempts Gum may spend after the first attempt fails

Contract:

- retries stay inside the same `run_id`
- each retry creates a new `attempt_id`
- Gum owns backoff, jitter, and health-aware retry timing internally

### `timeout`

User intent:

- how long one execution attempt is allowed to run

Contract:

- timeout is per attempt, not per logical run
- a timed-out attempt may retry if budget remains
- stale or expired runners cannot commit completion after ownership is lost

### `memory`

User intent:

- choose how much memory one cloud function attempt needs

Example:

```python
@gum.job(memory="4gb", timeout="30m")
def render_video(video_id: str):
    ...
```

Contract:

- memory is per attempt
- Gum only leases the run to a runner with enough remaining memory capacity
- memory composes with concurrency as `memory * active slots`
- compute placement remains internal

### `rate_limit`

User intent:

- limit how quickly a function or shared external dependency is called

Per-function form:

```python
@gum.job(rate_limit="20/m")
def sync_customer(...):
    ...
```

Shared-pool form:

```python
openai_limit = gum.rate_limit("60/m")

@gum.job(rate_limit=openai_limit)
def summarize(...):
    ...

@gum.job(rate_limit=openai_limit)
def embed(...):
    ...
```

Contract:

- inline strings are function-scoped by default
- module-level `gum.rate_limit("60/m")` definitions infer the pool name from the binding, e.g. `openai_limit`
- jobs using the same pool share one budget
- conflicting definitions for the same pool are rejected

### `concurrency`

User intent:

- bound how many executions of one function may run simultaneously

Contract:

- concurrency is per function
- active usage is derived from durable running attempts, not an in-memory counter
- queued runs can wait on `waiting_on_concurrency`
- retries compete for slots like normal work

### `every`

User intent:

- run this function repeatedly on an interval

Contract:

- scheduled ticks create normal Gum runs
- scheduled runs use the same retries, timeout, rate limit, concurrency, key, logs, cancel, and replay behavior
- `concurrency=1` naturally prevents overlapping scheduled runs

### `key`

User intent:

- define duplicate identity for enqueue-time duplicate protection

Example:

```python
@gum.job(retries=3, key="event_id")
def process_stripe_webhook(event_id: str, event: dict):
    ...
```

Contract:

- same function plus same resolved key returns the existing run
- different functions do not collide
- retries stay inside the same keyed run
- replay bypasses dedupe and intentionally creates new work
- canceled keyed runs keep their key claim until retention expiry

## Internal System Behavior

These are intentionally not public knobs:

- function health
- provider health
- circuit breakers / outage guards
- retry preservation timing
- probe cadence
- lease expiry
- stale-runner fencing
- runner recovery
- internal execution context
- compute placement

Users should experience these as Gum being reliable, not as knobs they must configure.

## Interaction Rules

Knobs must not behave as isolated flat flags.

### `retries` x `timeout`

- timeout fails one attempt
- retry may create a later attempt on the same run
- timeout does not fork run identity

### `retries` x function health

- retries are the user budget
- function health may delay when a retry is spent
- function health must not silently reduce the total retry budget

### `rate_limit` x `concurrency`

- concurrency is checked before rate-limit budget is spent
- work that cannot acquire a slot should not consume rate-limit capacity

### `memory` x `concurrency`

- memory is per active attempt
- total pressure is `memory * active slots`
- work waits if no runner has enough remaining memory capacity

### `key` x `retries`

- retries stay inside the same run
- no new dedupe record is created for retry attempts

### `key` x `rate_limit`

- duplicate enqueue returns the existing run
- duplicate enqueue does not spend extra rate-limit budget

### `key` x `concurrency`

- duplicate enqueue returns the existing run
- duplicate enqueue creates no extra slot pressure

### `key` x `replay`

- replay bypasses dedupe
- replay creates new work intentionally

### `every` x `concurrency`

- `concurrency=1` provides no-overlap scheduled execution
- later scheduled ticks wait behind active work instead of creating parallel overlap

## Rule For Adding New Knobs

Before adding a knob, ask:

1. Is this truly a user decision?
2. Can users explain it in plain language?
3. Will most users set it correctly?
4. Does it compose cleanly with existing knobs?
5. Can Gum make this built in instead?

If the answer is weak, do not add the knob.

## Summary

Gum exposes a small public policy surface:

- `retries`
- `timeout`
- `memory`
- `rate_limit`
- `concurrency`
- `every`
- `key`

Everything else is internal runtime behavior.

# Gum Knob Design

This document defines how Gum should think about public knobs.

The goal is:

- expose only what users really understand and need
- keep advanced protective behavior inside Gum
- avoid flat, unstructured configuration growth

## Core Principle

Every exposed knob must answer:

- what problem does the user recognize?
- what decision are they actually qualified to make?
- is this a stable contract Gum can support long-term?

If the answer is weak, the behavior should stay internal.

## Types of Controls

### 1. User policy knobs

These belong in the public API because users understand the intent.

Examples:

- `retries`
- `timeout`
- `rate_limit`
- `concurrency`
- `every`
- `compute`
- `key`

### 2. Internal system behaviors

These should generally stay inside Gum.

Examples:

- provider health inference
- outage guards
- probe cadence
- circuit open/close logic
- retry preservation when a provider is down
- stale-runner recovery rules

## Current Public Knobs

### `retries`

User intent:

- how many times Gum should retry after failure

Why exposed:

- clear and expected

### `timeout`

User intent:

- how long this work is allowed to run

Why exposed:

- tied directly to workload shape

### `rate_limit`

User intent:

- do not exceed this provider or job throughput budget

Why exposed:

- common operational concern

### `concurrency`

User intent:

- bound parallel execution for this job

Why exposed:

- resource and ordering concern users understand

### `every`

User intent:

- run this on a schedule

Why exposed:

- directly describes trigger semantics

### `compute`

User intent:

- choose the compute class this work needs

Why exposed:

- workload size and cost decision

### `key`

User intent:

- define duplicate identity for this work

Why exposed:

- duplicate delivery is a real application concern
- users often know the stable external id

## Planned Public Knob: `key`

Use `key` for duplicate protection.

Example:

```python
@gum.job(retries=3, key="event_id")
def process_stripe_webhook(event_id: str, event: dict):
    ...
```

Meaning:

- Gum treats `event_id` as duplicate identity
- repeated enqueue of the same identity should not produce duplicate work

Why not `unique=True`:

- too vague
- unclear identity source
- too much hidden magic

## What Should Stay Internal

### Provider outage handling

Keep internal:

- provider health checks
- degraded/down inference
- outage-aware retry preservation
- probe-based recovery

Users should not need:

- `circuit_breaker=True`

### Retry preservation timing

Keep internal:

- whether Gum should spend the next retry now or wait

Users care about:

- retry budget

They do not need:

- fine-grained retry spend timing knobs

### Recovery logic

Keep internal:

- lease expiry rules
- stale-runner fencing
- control lease timing

## Knob Interaction Principles

Knobs must not behave as isolated flat flags.

They need clear interaction rules.

### `retries` × provider health

Rule:

- `retries` is the user budget
- provider health may affect when retries are spent
- provider health should not silently reduce the total retry budget

### `rate_limit` × provider health

Rule:

- rate limit controls throughput
- provider health controls whether downstream is usable at all
- these are separate dimensions

### `concurrency` × `compute`

Rule:

- `compute` chooses machine class
- `concurrency` chooses parallelism on that work
- Gum should enforce both

### `key` × `replay`

Rule:

- replay should not be accidentally blocked by duplicate protection
- replay semantics should be explicit

This will need a dedicated contract later.

## Rule For Adding New Knobs

Before adding a knob, ask:

1. Is this a user decision or Gum’s internal decision?
2. Can the user explain it in plain language?
3. Will most users set it correctly?
4. Does it compose cleanly with existing knobs?
5. Is there a simpler internal default instead?

If the answer is weak, do not add the knob.

## Near-Term Public Surface

Public user surface should stay close to:

- `retries`
- `timeout`
- `rate_limit`
- `concurrency`
- `every`
- `compute`
- `key`

Everything else should be heavily questioned.

## Summary

Gum should expose a small set of meaningful policy knobs and keep smarter reliability behavior internal.

That gives users:

- enough control
- clear intent

And it gives Gum:

- room to be opinionated
- room to improve system behavior without inflating the API

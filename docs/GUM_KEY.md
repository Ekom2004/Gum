# Gum Key

This document defines the `key` knob in Gum.

The goal is:

- make enqueue-time duplicate suppression explicit
- keep run identity stable across retries, timeouts, and holds
- avoid accidental extra slot pressure or rate-limit spend
- keep replay as the explicit new-work path

## Surface

```python
@job(key="event_id")
async def process_stripe_webhook(event_id: str, event: dict):
    ...
```

Meaning:

- Gum resolves `event_id` from the enqueue payload
- for this function, that value identifies the logical work
- duplicate enqueue returns the existing run instead of creating a new one

## Core Model

`key` is an enqueue-time dedupe mechanism.

It should only answer:

- should this enqueue create a new run?

It should not answer:

- is this retry the same run?
- is this timeout retry the same run?
- is this held run the same run?

Those must already be stable under Gum’s core run model.

## Scope

Deduplication is scoped to:

- `team_id`
- `job_id`
- `key_value`

Different teams never collide.
Different functions never collide.

## Resolution

At enqueue time:

1. Gum reads the configured key field from the input payload
2. Gum resolves the key value as a string
3. Gum checks whether an active dedupe record already exists for:
   - same `team_id`
   - same `job_id`
   - same `key_value`

If the key field is missing:

- enqueue fails with a clear error

Example:

```text
key field "event_id" missing from input
```

## Behavior

### Fresh enqueue

If there is no active dedupe record:

- create a new run
- store a dedupe record pointing to that run
- return `deduped=false`

### Duplicate enqueue

If an active dedupe record exists:

- do not create a new run
- return the existing run
- return `deduped=true`

## Dedupe Record

Gum should store a durable dedupe record with at least:

- `team_id`
- `job_id`
- `key_value`
- `run_id`
- `created_at`
- `expires_at`

This is the source of truth for duplicate suppression.

## Retention

`key` dedupe must be time-bounded.

Reason:

- dedupe should suppress duplicate delivery
- not block all future legitimate work forever

Default model:

- dedupe record expires after a retention window
- after expiry, the same key may create a fresh run again

The exact default window can be decided later, but the model should assume a bounded lifetime.

## Interaction Rules

### `key + retries`

Retries stay inside the same run.

They do not:

- create a new run
- create a new dedupe record

For a keyed run:

- `run_id` remains stable
- only `attempt_id` changes across retries

### `key + timeout`

Timeout does not fork identity.

If a timed-out run retries:

- it remains the same run under the same key

### `key + function health`

Function health holds apply to the same keyed run.

If a keyed run is held for health:

- it remains the same run
- it is not replaced by a new run

### `key + provider health`

Provider-aware retry preservation also applies to the same keyed run.

Health-aware holding must never create replacement work for the same key.

### `key + replay`

Replay bypasses dedupe and creates a new run intentionally.

Example:

```text
enqueue(evt_123) -> run_1
replay(run_1) -> run_2
```

This is important because replay is the explicit:

- do this work again

path.

### `key + cancel`

Cancel does not free the key early.

The dedupe record remains active until retention expiry.

So:

- enqueue with the same key during the retention window returns the canceled run
- replay remains the explicit way to intentionally create new work

### `key + concurrency`

Duplicate enqueue must not create extra slot pressure.

Because no new run is created:

- no extra queued run competes for concurrency
- no extra active run can consume a slot

### `key + rate_limit`

Duplicate enqueue must not spend additional rate-limit budget.

Only real execution attempts count against rate limits.

So:

- original enqueue may eventually lead to execution
- duplicate enqueue that resolves to the same run does not spend more budget

### `key + schedule`

Scheduled runs are independent by default.

That means:

- separate schedule ticks should create separate runs
- they should only dedupe if the scheduler intentionally resolves the same key value for them

### `key + priority`

Dedupe returns the existing run.

It does not create a second run that could introduce a new priority decision.

The existing run keeps its configured priority semantics.

## Response Shape

When enqueue returns a run, Gum should make dedupe explicit.

Recommended response fields:

- `id`
- `status`
- `deduped`

Examples:

```json
{ "id": "run_123", "status": "queued", "deduped": false }
```

```json
{ "id": "run_123", "status": "running", "deduped": true }
```

## What `key` Must Never Do

`key` must never:

- create a new run during retry
- change timeout semantics
- change health-hold semantics
- change whether replay creates new work
- reserve concurrency slots
- spend rate-limit budget on duplicate enqueue

## Product Contract

The honest contract for `key` should be:

- Gum guarantees enqueue-time duplicate suppression for the same function and key value within the retention window
- Gum preserves a stable run identity across retries, timeouts, and holds
- Gum does not claim exactly-once external side effects on its own

For true downstream idempotency, user code should still be able to pass identity into external systems using:

- `run_id`
- `attempt_id`
- resolved `key`

## Testing

Need direct coverage for:

- same key, same function returns same run
- different key creates a new run
- same key, different function does not collide
- expired key allows a new run
- missing key field errors clearly
- replay bypasses dedupe
- canceled run still holds the key until expiry
- retries stay within the same run
- duplicate enqueue creates no extra concurrency pressure
- duplicate enqueue creates no extra rate-limit spend

## Next Implementation Step

Implementation should follow this order:

1. add `key` to job policy and deploy metadata
2. add durable dedupe storage
3. resolve key at enqueue time
4. return existing run on duplicate enqueue
5. expose `deduped` in enqueue responses
6. add test coverage for all settled interactions

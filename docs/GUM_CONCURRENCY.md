# Gum Concurrency

This document defines the Gum-native concurrency model.

The goal is:

- per-function concurrency limits
- zero slot leaks
- crash-safe execution
- no second in-memory authority for slot ownership

## Overview

Concurrency controls how many executions of a function may run at the same time.

Example:

```python
@job(concurrency=5)
async def sync_to_crm(user_id: str):
    await hubspot.upsert(user_id)
```

This means:

- at most 5 active executions of `sync_to_crm`
- additional runs remain queued until a slot opens

If `concurrency` is omitted:

- the function has no per-function concurrency cap

## Gum Design Principle

**Track running attempts durably.**

Do not make an in-memory slot tracker the primary source of truth.

Gum already has:

- runs
- attempts
- leases
- runner heartbeats
- lease expiry and recovery

That means Gum can derive concurrency directly from durable execution state.

## Execution Model

In Gum:

- `job` = function definition
- `run` = one invocation of that function
- `attempt` = one execution attempt of that run

Concurrency applies to:

- **running attempts for a job**

So the real rule is:

> `concurrency = N` means Gum will not lease a new attempt for `job_id` if there are already `N` running attempts for that job.

## Source of Truth

The source of truth is the database.

Active slot usage is derived from:

- `attempts.status = 'running'`
- joined to `runs.job_id`

Conceptually:

```sql
SELECT COUNT(*)
FROM attempts
JOIN runs ON runs.id = attempts.run_id
WHERE attempts.status = 'running'
  AND runs.job_id = $1;
```

That count is the active concurrency usage for the function.

## Why Not a Mutable Counter?

A simple counter like:

- `active = 3`

is fragile.

One missed decrement can permanently leak capacity.

A separate in-memory set of slot owners is better than a raw counter, but still creates a second authority that can drift away from durable state.

For Gum, the durable execution model already exists.
So concurrency should be derived from that durable model rather than duplicated in memory.

## What a Slot Means

A slot is occupied when there is a running attempt for the function.

A slot is released when that attempt is no longer running.

That includes:

- success
- failure
- timeout
- cancel
- lost-runner recovery

This is stronger than “remembering to decrement a counter.”

## Zero Slot Leaks

The hard requirement is:

**there must never be a permanently-running attempt that is no longer actually owned by a healthy runner.**

Gum achieves that through:

- runner heartbeats
- lease expiry
- stale-runner recovery
- attempt finalization

So the real “no slot leak” invariant is:

- no stale attempt should remain `running` forever

## Lease-Time Concurrency Gate

Concurrency is enforced when Gum chooses whether to lease the next queued run.

The leasing path should check:

1. function health hold
2. concurrency
3. rate limit
4. compute class / runner placement
5. lease dispatch

Important:

- concurrency should be checked before rate-limit spending
- if a function cannot run because all concurrency slots are full, Gum should not waste any rate-limit budget

## Current Query Shape

The current Gum leasing logic already does this in spirit:

- find queued runs
- join to jobs
- count running attempts for the job
- skip jobs whose running count is already at the limit

That is the correct base model.

## Slot Release Paths

There are five important release paths.

### 1. Success

When an attempt succeeds:

- attempt becomes terminal
- it is no longer counted as running

### 2. Failure

When an attempt fails:

- attempt becomes terminal
- it is no longer counted as running
- if retries remain, a later attempt may be requeued

The retry does **not** inherit the old slot.
It must compete for a slot normally.

### 3. Timeout

When a job times out:

- the running attempt becomes terminal timed out
- it no longer counts as running
- any retry is a future attempt, not a continued slot ownership

### 4. Cancel / Revoke

When a running job is canceled:

- revoke is requested
- runner stops execution
- attempt becomes canceled
- it no longer counts as running

### 5. Crash / Heartbeat Lost

When the runner dies or stops heartbeating:

- Gum’s lost-runner recovery marks the running attempt failed
- the stale lease is released
- the attempt no longer counts as running

This is how Gum prevents leaked concurrency slots after runner death.

## Restart Recovery

Gum does not need to rebuild a separate slot tracker as the primary mechanism.

On restart:

- running attempts are already in the database
- stale attempts are detected through heartbeat and lease recovery
- concurrency can be derived again directly from current attempt state

So restart recovery is:

- query durable state
- recover stale attempts
- continue leasing

No separate mutable slot authority is required.

## Concurrency and Retries

Retries do not reserve capacity.

Example:

```text
concurrency = 2

1. run_A leased
2. run_B leased
3. run_A fails
4. attempt_A released
5. retry_A becomes queued
6. run_C is also queued
7. whichever queued run wins normal scheduling gets the next slot
```

This is important:

- retries must compete fairly with other queued work
- no hidden “reserved retry slot”

## Concurrency and Schedule

This is a very useful property:

```python
@job(every="5m", concurrency=1)
async def cleanup():
    ...
```

This naturally guarantees no overlap.

If the previous scheduled run is still active:

- the next scheduled run stays queued
- it cannot acquire a slot yet

So `concurrency=1` gives clean serial scheduled execution by default.

## Concurrency and Rate Limits

Both gates must pass.

Examples:

### Case A

- concurrency usage: `3 / 5`
- rate limit budget: available

Result:

- dispatch allowed

### Case B

- concurrency usage: `5 / 5`
- rate limit budget: available

Result:

- concurrency blocks
- do not spend rate-limit budget

### Case C

- concurrency usage: `3 / 5`
- rate limit budget: exhausted

Result:

- rate limit blocks
- run stays queued until budget returns

## Concurrency and Function Health

Function health should gate before concurrency.

If a function is currently in a held/degraded/down state:

- Gum should not lease more work for it even if concurrency slots are technically free

That means the effective gating order is:

1. function health hold
2. concurrency slots
3. rate limit

This keeps retry preservation and concurrency behavior aligned.

## Admin Visibility

Gum should eventually expose concurrency clearly in the admin surface.

Useful operator data:

- function name
- concurrency limit
- active running attempts
- queued runs waiting on concurrency

The key operator need is:

- see which functions are saturated
- see how much work is queued behind saturation

## Better Queued Reasons

Right now a blocked run is just `queued`.

Eventually Gum should expose a reason like:

- `waiting_on_concurrency`

That would make the queue state much easier to reason about in `gum admin`.

## Audit Queries

### Active slots by function

```sql
SELECT runs.job_id, COUNT(*) AS active_slots
FROM attempts
JOIN runs ON runs.id = attempts.run_id
WHERE attempts.status = 'running'
GROUP BY runs.job_id;
```

### Potential stale running attempts

```sql
SELECT attempts.id,
       runs.job_id,
       attempts.runner_id,
       leases.expires_at
FROM attempts
JOIN runs ON runs.id = attempts.run_id
LEFT JOIN leases ON leases.id = attempts.lease_id
WHERE attempts.status = 'running';
```

### Queued work behind a function

```sql
SELECT job_id, COUNT(*) AS queued_runs
FROM runs
WHERE status = 'queued'
GROUP BY job_id;
```

## Reconciliation

Because the database is the source of truth, reconciliation is simpler:

- compare durable running attempts to expected healthy runner ownership
- recover stale attempts
- continue leasing

If Gum ever adds an in-memory cache for performance, that cache must be:

- derived
- disposable
- rebuildable from DB truth

Not authoritative.

## Guarantees

1. **Per-function concurrency.** The limit applies to running attempts for a function.
2. **No slot leaks.** A slot disappears when the attempt is no longer running, including stale-runner recovery.
3. **Crash-safe.** Durable attempt state survives scheduler restarts.
4. **Retry-safe.** Retries compete for slots like any other queued work.
5. **No over-count.** There is no separate mutable slot counter to drift.
6. **Composes cleanly.** Function health, concurrency, and rate limits all act as separate lease gates.

## Summary

The Gum-native concurrency model is:

- per-function
- enforced at lease time
- derived from durable running attempts
- crash-safe through lease recovery
- not based on a separate in-memory slot authority

That gives Gum the same practical guarantees as a slot tracker, but with fewer moving parts and a better fit for the architecture Gum already has.

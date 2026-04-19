# Gum Execution Semantics

This document defines the execution model Gum should promise.

The goal is:

- keep logical work identity stable
- survive worker and scheduler crashes cleanly
- make timeout semantics coherent
- prevent accidental duplicate Gum work
- be honest about the external side-effect boundary

## Core Principle

A Gum run should survive infrastructure failure without changing identity.

That means:

- worker crash
- runner restart
- scheduler restart
- lease expiry
- lost heartbeat

should not create:

- a new `run_id`
- confusing user-visible duplication
- silent work forking

They may create:

- a new `attempt_id`
- internal recovery activity
- a later retry on the same run

## Execution Model

Gum should treat these as separate concepts:

### Run

The logical unit of work.

`run_id` is the stable identity for:

- one enqueue
- one schedule tick
- one replay request

A run is what the user/operator thinks of as:

- "this job invocation"

### Attempt

One execution try for a run.

`attempt_id` changes across:

- retries
- timeout retries
- lost-runner recovery
- infrastructure interruptions

An attempt is what a runner actually owns and executes.

### Lease

The current ownership claim for an attempt.

A lease gives a runner the right to:

- execute the attempt
- append logs
- report completion

Only the current valid lease holder should be allowed to commit completion.

### Durable Completion

An attempt is only considered finished when Gum durably records completion.

Not when:

- user code returns locally
- stdout prints a success message
- the runner thinks it is done

This durable completion boundary is the most important rule in the system.

## Public Identity Contract

### Stable Run Identity

The following should preserve the same `run_id`:

- retry
- timeout retry
- rate-limit hold
- concurrency wait
- function-health hold
- worker crash recovery
- scheduler restart recovery

### New Run Identity

The following should create a new `run_id`:

- new enqueue
- new schedule tick
- replay

## Attempt Lifecycle

Conceptually:

```text
queued run
  -> leased attempt
  -> running attempt
  -> terminal attempt or recoverable attempt failure
```

An attempt can end in one of these ways:

- succeeded
- failed
- timed_out
- canceled
- lost / crashed / expired ownership

If the run still has retry budget and policy allows:

- Gum creates a new attempt later
- on the same run

## Lease Ownership Rules

### Single Owner

At any moment, one attempt should have at most one active lease owner.

### Fenced Completion

If a runner loses ownership because of:

- lease expiry
- revoke/cancel
- lost heartbeat recovery

then that runner must no longer be able to:

- complete the attempt
- mutate run outcome

Stale completion must be rejected.

### Lost Ownership

If Gum cannot confirm a runner is still alive within the heartbeat/lease window:

- the attempt becomes recoverable
- Gum may retry the run with a new attempt

## Timeout Semantics

## Definition

`timeout` is a hard wall-clock limit for one attempt.

It is not:

- a lifetime limit for the run
- a global workflow deadline

Example:

```python
@gum.job(timeout="5m", retries=3)
def export_workspace(...):
    ...
```

Meaning:

- each attempt may run for up to 5 minutes
- if an attempt exceeds that limit, Gum terminates that attempt
- the run may retry on a new attempt if budget remains

### Why Per-Attempt

This keeps the contract clean:

- retries are still meaningful
- crashes and recovery do not distort identity
- timeout remains local to one execution try

## Timeout Outcomes

### If retry budget remains

- old attempt ends as timed out
- run remains the same run
- Gum schedules a later attempt

### If retry budget is exhausted

- run becomes terminal `timed_out`

### Failure Classification

Timeout should remain a distinct class:

- `job_timeout`

This is different from:

- worker crash
- heartbeat loss
- provider timeout

Because the user code really did exceed the allowed attempt runtime.

## Crash And Recovery Semantics

### Worker Crash

If the worker dies before durable completion:

- Gum must assume the attempt outcome is uncertain
- Gum must recover conservatively

That means:

- stale ownership is fenced out
- the run may retry on a new attempt
- the `run_id` stays the same

### Runner Heartbeat Loss

If the runner stops heartbeating beyond the grace window:

- Gum treats the attempt as lost
- Gum recovers the run

This is an infrastructure interruption, not a user-code timeout.

### Scheduler Restart

If the scheduler/control plane restarts:

- Gum rebuilds truth from durable store
- running attempts with healthy ownership continue
- stale/lost attempts are recovered

This should not change user-visible work identity.

## Public Status Contract

Public run statuses should stay simple:

- `queued`
- `running`
- `succeeded`
- `failed`
- `timed_out`
- `canceled`

Internal recovery states may exist, but Gum should not require the user to understand:

- lease repair
- stale-owner fencing
- heartbeat reaping

### Attempt Visibility

Operators should still be able to inspect:

- attempt history
- failure class
- logs per attempt

So the run view stays simple, while attempt detail remains available.

## The Ambiguity Window

This is the hard edge:

1. user code performs an external side effect
2. the worker crashes before Gum records durable completion
3. Gum does not know whether the side effect happened zero times or one time

This ambiguity cannot be eliminated by Gum alone for arbitrary external systems.

### What Gum Can Guarantee

Gum can guarantee:

- stable run identity
- stable replay lineage
- strict lease ownership
- stale completion fencing
- at-least-once recovery of logical work

### What Gum Cannot Guarantee Alone

Gum cannot guarantee:

- universal exactly-once external side effects

That requires cooperation from:

- `key`
- downstream idempotency keys
- user-side idempotent design

## Side-Effect Boundary Contract

Gum should state this explicitly:

- Gum guarantees stable logical work identity
- Gum may retry if durable completion was not recorded
- Gum preserves run identity across recovery
- external side effects may be repeated unless the downstream operation is idempotent

### Recommended Identity Surface For User Code

User code should be able to access:

- `run_id`
- `attempt_id`
- `key` if configured
- replay lineage if relevant

That lets the user pass stable identity into:

- Stripe idempotency keys
- database uniqueness constraints
- external request ids

## Idempotency Relationship

### Without `key`

Gum should still prevent accidental duplicate runs caused by:

- retries
- timeout recovery
- lease recovery
- rate-limit holds
- concurrency waits

But Gum will not dedupe separate enqueues.

### With `key`

Gum can additionally prevent duplicate enqueue for the same logical work.

That helps narrow the ambiguity window for user-facing event processing, but it still does not replace downstream idempotency.

## Logging Contract

Logs from prior attempts must remain queryable after:

- timeout
- worker crash recovery
- retry

This is required for:

- debugging
- trust
- support

Users/operators need to understand:

- what happened on attempt 1
- why attempt 2 exists

## Retry Contract

Retry must remain run-stable:

- retry creates a new attempt
- retry never creates a new run

### Retry After Timeout

If a timed-out attempt is retryable:

- next attempt runs on the same `run_id`
- old attempt remains visible in history

### Retry After Crash

If a crashed/lost attempt is recoverable:

- next attempt runs on the same `run_id`
- old attempt remains visible as lost/interrupted internally

## Cancellation Contract

Cancel targets the run.

Effects:

- active attempt receives revoke/cancel request
- stale owners must be fenced
- no later retry should proceed after terminal cancel

If cancel races with completion:

- only the valid lease holder and durable commit order should decide final state

## Desired Internal States

Internally, Gum may want more detail than the public run status surface.

Useful internal states/reasons:

- `attempt_lost`
- `attempt_timed_out`
- `waiting_for_retry`
- `waiting_for_function_health`
- `waiting_on_rate_limit`
- `waiting_on_concurrency`
- `stale_completion_rejected`

But these should mostly map back to the simple public run model.

## Test Matrix

Gum should have explicit tests for:

### Timeout

- timed-out attempt with no retries -> terminal `timed_out`
- timed-out attempt with retries -> same run, new attempt

### Crash Recovery

- worker crash before completion -> same run recovered
- heartbeat loss -> same run recovered
- scheduler restart with healthy attempts -> attempts continue
- scheduler restart with stale attempts -> attempts recovered

### Ownership

- stale runner cannot complete after recovery
- revoked runner cannot commit late completion

### Side-Effect Boundary

Gum cannot prove external effect uniqueness, but tests should ensure:

- run identity remains stable
- retries do not fork runs
- replay creates new work intentionally

### Logs

- logs from timed-out attempt remain queryable
- logs from recovered attempts remain queryable

## Product Summary

The Gum execution promise should be:

- one logical run identity
- many attempts if needed
- infrastructure failure is mostly invisible to the user
- timeout is per-attempt
- completion only counts when durably recorded
- external exactly-once side effects still require idempotency at the downstream boundary

That is the clean execution contract for Gum.

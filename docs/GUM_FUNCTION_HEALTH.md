# Gum Function Health

This document defines the next health model for Gum retries.

The goal is:

- zero-config health protection by default
- no dependence on `rate_limit` for basic health behavior
- smarter retry spending
- shared health only when Gum has an explicit reason to share it

## Core Idea

Gum should track execution health at a **health scope**.

By default, that scope is:

- the function/job itself

Later, Gum can also support:

- shared service health across multiple functions

So the base model is:

- no config: per-function health
- shared key: shared health

## Health Scopes

Gum should support two internal scope types:

- `function`
- `shared`

Conceptually:

```rust
enum HealthScope {
    Function { job_id: String },
    Shared { key: String },
}
```

### Function Scope

Default.

Every function has its own health state.

Example:

```python
@job(retries=5)
async def sync_to_crm(user_id):
    await hubspot.upsert(...)
```

Gum does not need to know this is HubSpot.
It only needs to know:

- this function is repeatedly failing in an infrastructure-like way

### Shared Scope

Optional later.

If multiple functions intentionally share the same downstream, Gum can let them share health.

That could eventually be driven by:

- a shared rate-limit pool
- an explicit internal service key

But shared scope should be an upgrade, not a dependency for the default behavior.

## Health States

Each health scope should have one of:

- `healthy`
- `degraded`
- `down`

These are Gum-internal states.

### Healthy

- recent infrastructure failures are low
- recent successes are normal
- no hold on retries

### Degraded

- recent infrastructure failures are elevated
- Gum should retry more cautiously
- Gum may hold retries briefly

### Down

- repeated infrastructure failures strongly suggest the dependency is unavailable
- Gum should preserve retry budget
- Gum should hold queued retries for this scope

## Failure Classification

Health should only be affected by the right kind of failures.

The runtime/runner should classify failures into two high-level buckets:

- `InfrastructureError`
- `ApplicationError`

Conceptually:

```rust
enum FailureKind {
    InfrastructureError,
    ApplicationError,
}
```

In practice Gum can keep the more detailed classes already added:

- `provider_timeout`
- `provider_connect_error`
- `provider_5xx`
- `provider_429`
- `provider_auth_error`
- `user_code_error`
- `gum_internal_error`
- `job_timeout`

### Health-Affecting Classes

These should count against scope health:

- `provider_timeout`
- `provider_connect_error`
- `provider_5xx`
- `provider_429`

Maybe later:

- some `gum_internal_error` cases, if clearly downstream-related

### Non-Health-Affecting Classes

These should not degrade scope health:

- `user_code_error`
- `provider_auth_error`
- data/validation style bad requests

Reason:

- a code bug is not a downstream outage
- bad auth is configuration/user error, not health degradation

## Data Model

Gum should add durable health state for function scopes.

### `health_scopes`

```sql
CREATE TABLE health_scopes (
    id TEXT PRIMARY KEY,
    scope_type TEXT NOT NULL,
    scope_key TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(scope_type, scope_key)
);
```

Examples:

- `scope_type = 'function', scope_key = 'job_sync_to_crm'`
- `scope_type = 'shared', scope_key = 'hubspot'`

### `health_signals`

Append-only evidence log.

```sql
CREATE TABLE health_signals (
    id TEXT PRIMARY KEY,
    health_scope_id TEXT NOT NULL REFERENCES health_scopes(id),
    signal_kind TEXT NOT NULL,
    failure_class TEXT,
    source TEXT NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL,
    details_json JSONB NOT NULL DEFAULT '{}'::jsonb
);
```

Examples:

- function attempt failed with `provider_503`
- function attempt succeeded
- optional future probe succeeded/failed

### `health_state`

Current rolled-up state.

```sql
CREATE TABLE health_state (
    health_scope_id TEXT PRIMARY KEY REFERENCES health_scopes(id),
    state TEXT NOT NULL,
    consecutive_infra_failures INTEGER NOT NULL DEFAULT 0,
    degraded_score INTEGER NOT NULL DEFAULT 0,
    down_score INTEGER NOT NULL DEFAULT 0,
    last_success_at TIMESTAMPTZ,
    last_failure_at TIMESTAMPTZ,
    last_changed_at TIMESTAMPTZ NOT NULL,
    reason TEXT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

This is the table the retry logic should read.

## State Machine

Start simple.

### Healthy -> Degraded

Trigger when:

- `3` consecutive infrastructure-like failures

or:

- degraded score crosses threshold

### Degraded -> Down

Trigger when:

- `5` consecutive infrastructure-like failures

or:

- repeated failures continue without an intervening success

### Degraded/Down -> Healthy

Trigger when:

- a real successful run completes for that health scope

For v1, use real successes.
Later, Gum can add active probes on top.

## Retry Behavior

Retry logic should read health state before scheduling the next retry.

### Healthy

- retry normally
- use backoff + jitter

### Degraded

- retry with stronger backoff
- preserve retries more cautiously

Example:

- double the normal retry delay

### Down

- do not spend the next retry immediately
- keep the run queued
- set a future retry check time
- preserve remaining retry budget
- surface that the run is waiting on downstream recovery

This is the key behavior:

- retries are preserved
- not burned while a dependency is obviously unhealthy

## Run Surface

When Gum holds retries because a health scope is down, the run should show:

- `status = queued`
- `failure_class = blocked_by_downstream`
- `failure_reason = waiting for function health recovery`
- `retry_after_epoch_ms = ...`
- maybe `waiting_for_scope_key = ...`

That makes the behavior understandable without exposing a circuit-breaker knob.

## Transition Rules From Attempt Outcomes

When an attempt completes:

1. classify failure
2. determine health scope
3. update health state if the failure is infrastructure-like
4. decide retry disposition based on:
   - attempt count
   - retry budget
   - failure class
   - current health state

### Success

- reset consecutive infrastructure failures
- move toward `healthy`

### Infrastructure Failure

- increment consecutive failures
- increment degraded/down scores
- potentially move `healthy -> degraded -> down`

### Application Failure

- do not affect health state
- likely fail terminally or retry only under normal policy

## Scope Resolution

### Default Resolution

Every run should resolve to:

- `HealthScope::Function(job_id)`

That gives zero-config protection.

### Shared Resolution Later

If Gum has an explicit shared key for a function, resolve to:

- `HealthScope::Shared(key)`

That lets multiple functions share health signals.

The default should never depend on this.

## Interaction With Provider Health

The provider-health slice already built should not be thrown away.

Instead:

- `function health` becomes the default retry-protection model
- `provider health` becomes an optional/global signal layer

### Suggested coexistence

#### Function health

- zero-config
- always available
- drives retries by default

#### Provider health

- optional richer signal
- useful for:
  - admin visibility
  - provider-wide incidents
  - later shared scope upgrades

### Merge rule

For v1:

- function health decides retry behavior
- provider health is visibility plus future expansion

Later:

- if both exist, Gum can take the more conservative state

Example:

- function = healthy
- provider = down

Then Gum could treat the effective state as:

- `down`

But that does not need to be the first implementation.

## Suggested Rollout

### Phase 1

- add function-scope health state
- update it from classified attempt outcomes
- use it to hold retries when scope is `down`

### Phase 2

- add stronger degraded behavior
- use success to recover state more smoothly

### Phase 3

- optionally merge provider health and function health
- optionally add shared scopes

## Public API Impact

This should stay mostly internal.

Users should not need:

- `circuit_breaker=True`
- provider naming
- extra health knobs

The public knobs remain:

- `retries`
- `timeout`
- `memory`
- `rate_limit`
- `concurrency`
- `every`
- `key`

Health-aware retry preservation should be Gum behavior, not user ceremony.

## Summary

The right model is:

- classify failures
- default health scope is the function
- preserve retries when function health says the downstream path is unhealthy
- keep provider health as a richer parallel signal, not the default dependency

That gives Gum:

- zero-config smart retries
- fewer wasted attempts
- clearer run behavior
- a clean path to shared health later

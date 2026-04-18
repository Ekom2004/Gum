# Gum Provider Health

This document defines how Gum should detect downstream provider health and how that health should influence execution.

The goal is simple:

- know when a provider is likely healthy
- know when it is likely degraded
- know when it is likely down
- explain that state clearly to operators and users
- avoid burning retries blindly when a dependency is unhealthy

This is an internal Gum capability.

It is not a decorator flag.
It is not exposed as `circuit_breaker=True`.

## Why This Exists

External systems fail in ways that normal retry logic does not handle well:

- provider outages
- intermittent 5xxs
- long timeout periods
- auth failures
- regional incidents
- degraded latency before full outage

If Gum only has:

- retries
- static rate limits
- static concurrency

then it can still waste:

- retry budget
- runner capacity
- operator attention

Provider health gives Gum a way to reason about downstream systems as first-class dependencies.

## Design Principles

### 1. Do not trust a single signal

Provider health should not depend on one thing alone.

Do not rely only on:

- synthetic probes
- real failures
- status pages

Use multiple signals and reconcile them.

### 2. Real traffic matters more than synthetic checks

Synthetic probes are useful, but actual request outcomes are more trustworthy.

If a probe succeeds but real requests are failing, Gum should bias toward real failures.

### 3. Health should be inferred, not guessed

Gum cannot know the future.
It should infer health state from recent evidence.

### 4. Internal behavior first

Provider health is an internal system.

Users should see:

- better status
- clearer messages
- smarter behavior

They should not have to configure a circuit breaker knob to benefit from it.

### 5. Start with visibility before aggressive automation

Phase 1 should focus on:

- accurate health state
- clear admin visibility
- clear user-facing status

Then Gum can layer in retry preservation and outage-aware scheduling.

## Provider Health States

Gum should maintain one of three states for each provider target:

- `healthy`
- `degraded`
- `down`

These are Gum-internal classifications.

### Healthy

Characteristics:

- probes succeeding
- latency near normal
- real requests mostly succeeding
- no recent strong outage signal

### Degraded

Characteristics:

- probes succeeding inconsistently
- latency materially worse than baseline
- some real traffic failing
- elevated timeout or 5xx rate

### Down

Characteristics:

- recent probes failing consistently
- or real requests failing almost completely
- or strong external incident signal plus local confirmation

## What Is a Provider Target?

Provider health should not be global across all external systems.

It should be tracked per provider target.

Examples:

- `openai`
- `stripe`
- `resend`
- `hubspot`
- `salesforce`

In the future, Gum can support finer-grained targets:

- `openai:chat`
- `openai:embeddings`
- `stripe:webhooks`

For v1, keep it simple:

- one named provider target per downstream

## Signals

Gum should use three signal classes.

### 1. Active Probe Signal

Gum runs a lightweight authenticated check on an interval.

Examples:

- OpenAI: tiny low-cost request
- Stripe: cheap authenticated read
- generic HTTP provider: configured health endpoint

Probe result fields:

- `success`
- `latency_ms`
- `error_class`
- `status_code`
- `checked_at`

Use probes for:

- fast detection
- degraded/down inference
- recovery confirmation

Do not use probes as the only signal.

### 2. Passive Real Request Signal

Gum should classify real attempt outcomes.

Useful classes:

- `provider_timeout`
- `provider_connect_error`
- `provider_5xx`
- `provider_429`
- `provider_auth_error`
- `user_code_error`
- `gum_internal_error`

Only provider-related classes should feed provider health.

This matters because:

- user code errors should not mark a provider unhealthy
- provider failures should

### 3. External Incident Hint Signal

If a provider publishes machine-readable incident signals, Gum may ingest them.

Examples:

- status-page feed
- incident webhook

Treat these as hints, not truth.

Why:

- they can lag
- they can be too coarse
- they may not reflect the exact API path Gum uses

## Health Evaluation Rules

Start with simple rules.

### Transition to `down`

Mark provider `down` if either:

- 3 consecutive probes fail
- or recent real provider requests strongly fail within a short window

Example v1 rule:

- 5 provider-classified failures in the last 60s with no successes

### Transition to `degraded`

Mark provider `degraded` if:

- probe success rate drops below threshold
- or probe latency rises above threshold
- or real request failure ratio is elevated but not total

Example v1 rule:

- 2 of last 5 probes fail
- or p95 latency is over threshold
- or 20%+ of recent provider-classified requests fail

### Transition back to `healthy`

Mark provider `healthy` when:

- consecutive successful probes reach threshold
- and recent real request outcomes stabilize

Example v1 rule:

- 3 successful probes in a row

## Data Model

Gum should add dedicated provider-health records.

### `provider_targets`

Fields:

- `id`
- `name`
- `slug`
- `probe_kind`
- `probe_config_json`
- `enabled`
- `created_at`
- `updated_at`

Purpose:

- durable registry of monitored downstreams

### `provider_checks`

Fields:

- `id`
- `provider_target_id`
- `kind`
- `status`
- `latency_ms`
- `error_class`
- `status_code`
- `checked_at`

Purpose:

- append-only probe history

### `provider_health`

Fields:

- `provider_target_id`
- `state`
- `reason`
- `last_changed_at`
- `last_success_at`
- `last_failure_at`
- `degraded_score`
- `down_score`
- `metadata_json`

Purpose:

- current derived provider health state

### Optional later: `provider_events`

Fields:

- `id`
- `provider_target_id`
- `event_kind`
- `payload_json`
- `created_at`

Purpose:

- external status-page or incident ingestion

## Where Health Checks Run

Add a small background service or scheduler task:

- `gum-provider-health`

Responsibilities:

- run probes on interval
- classify probe outcomes
- update `provider_checks`
- update `provider_health`

It should be:

- stateless
- restart-safe
- easy to run on Fly or bare metal

## Operator UX

Provider health must show up in the admin experience.

Add a provider-health view later to `gum admin`.

At minimum show:

- provider name
- current health state
- last probe time
- last success
- last failure
- short reason

Examples:

- `openai   degraded   probe timeout + high 5xx rate`
- `stripe   healthy    last probe ok 12s ago`
- `resend   down       3 consecutive probe failures`

## User-Facing Messaging

Even before retry behavior changes, Gum should improve run messaging.

Good statuses or reasons:

- `downstream_unavailable`
- `provider_degraded`
- `provider_down`
- `waiting_for_provider_recovery`

Good operator/system log lines:

- `Gum marked provider openai as degraded after repeated timeouts.`
- `Gum marked provider stripe as down after probe failures.`

This matters because users need to know:

- their code is probably not the problem
- Gum understands the failure domain

## Retry Interaction

Phase 1:

- provider health does not change retry counts
- provider health improves messaging and operator visibility

Phase 2:

- if provider is `down`, Gum may preserve retries instead of spending them immediately
- runs remain queued
- Gum retries again after provider recovery or probe success

Important:

- user retry budget is still respected
- Gum is choosing when to spend retries, not removing them

## Circuit Breaker Relationship

Provider health is the foundation.

Circuit-breaker-like behavior is a later policy built on top of it.

That later policy could be:

- if provider health is `down`, stop leasing more runs for that provider target
- periodically allow a probe or limited resume

Do not expose this first as a user knob.

Build the health model first.

## Rollout Plan

### Phase 1

- add provider-target registry
- add probe runner
- add provider health state
- add admin visibility
- add better downstream failure reasons

### Phase 2

- classify real request failures into provider failure classes
- blend real traffic into provider health scoring

### Phase 3

- preserve retries when provider is down
- hold runs queued behind provider health state
- resume on recovery

### Phase 4

- optional incident feed ingestion
- optional richer provider-specific probe libraries

## What Not To Do First

Do not start with:

- AIMD
- user-exposed circuit-breaker flags
- complicated health scoring
- full query language for provider health
- dozens of provider-specific adapters

Keep v1:

- simple
- provider-target based
- interval probe driven
- operator visible

## Summary

Gum should build provider health as an internal system that:

- probes downstreams
- learns from real failures
- tracks `healthy / degraded / down`
- improves user messaging
- later enables smarter retry preservation

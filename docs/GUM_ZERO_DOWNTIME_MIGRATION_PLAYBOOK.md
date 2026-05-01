# Gum Zero-Downtime Migration Playbook

Last updated: April 27, 2026

This document defines how Gum migrates:

- database provider
- compute provider (runner fleet)

without API downtime and without losing logical work.

## Success Criteria

A migration is successful only if all are true:

- no sustained API outage (no extended 5xx window)
- no lost runs
- no duplicate `run_id` creation for the same logical trigger
- no stale-lease completion commits
- rollback path remains available until post-cutover soak passes

## Non-Negotiable Invariants

1. `run_id` stays stable across retries and recovery.
2. Attempts may change; run identity must not.
3. Only valid lease owner can commit completion.
4. All schema/runtime changes are forward+backward compatible during cutover.
5. One authoritative writer path at any point in time.

## Pre-Migration Requirements

Complete these before any production move:

- backup/restore drill is passing
- rollback procedure is documented and rehearsed
- staging migration rehearsal is passing for the same change class
- 24h soak is green on current baseline
- dashboards are available for:
  - API success rate and latency
  - enqueue rate and queue depth
  - running attempts and lease recovery events
  - runner heartbeat health
  - retry and timeout rates

## Global Gates, Abort Rules, and Rollback Trigger

Use explicit thresholds for go/no-go:

- abort if API 5xx > 1% for 5 continuous minutes
- abort if enqueue or get p95 latency > 2x baseline for 10 minutes
- abort if queue depth grows continuously for 15 minutes without recovery
- abort if lease recovery/lost-runner events spike > 3x baseline for 10 minutes
- abort if any correctness mismatch is found in validation checks

When an abort condition triggers:

1. stop progressing rollout percentage
2. revert to last known-good route/config
3. verify health + onboarding smoke
4. log incident timeline and blocker owner

## Standard Rollout Phases

Use this sequence for both DB and compute migrations:

1. `phase-0`: staging rehearsal with production-like load profile
2. `phase-1`: shadow validation (no user-facing cutover)
3. `phase-2`: canary (small traffic slice or specific compute class)
4. `phase-3`: progressive ramp with hold points and metric checks
5. `phase-4`: full cutover with rollback switch still armed
6. `phase-5`: post-cutover soak window (minimum 24h)
7. `phase-6`: decommission old path only after soak is green

---

## Database Provider Migration

### Scope

Move Gum system-of-record from DB provider A to provider B with no API downtime.

### DB-1: Expand/Contract First

- deploy additive schema changes only
- do not remove/rename required columns in same window
- keep old code and new code both valid against the transition schema

### DB-2: Build Target DB and Sync

- create target cluster
- apply current schema/migrations
- load base snapshot from source
- start continuous change sync from source to target

### DB-3: Validation Before Canary

Validate parity on critical entities:

- projects
- deploys
- jobs
- runs
- attempts
- leases

Run read-parity checks on:

- row counts
- recent-window checksums (for example last 24h runs/attempts)
- key correctness queries:
  - active running attempts
  - active leases
  - retry-eligible queued runs

### DB-4: Canary Cutover

- route a small read path first (admin/read-only endpoints)
- hold and monitor
- then route a small write slice (enqueue/register/lease flow) if architecture allows
- if partial write slicing is not available, perform short control-plane failover with instant rollback switch ready

### DB-5: Full Cutover

- flip main API/scheduler store config to target DB
- keep source DB in hot-rollback state
- run onboarding smoke immediately
- monitor against abort thresholds

### DB-6: Rollback Plan

If gates fail:

1. revert API/scheduler store config to source DB
2. restart/reload services
3. confirm health + smoke
4. keep target sync artifacts for forensic diff

Only decommission source DB after 24h soak on target passes.

---

## Compute Provider Migration

### Scope

Move runner fleet from compute provider A to provider B while keeping the same API/control plane.

### CP-1: Keep Control Plane Constant

- keep Gum API and durable store stable during compute move
- migrate runner fleet only
- keep lease protocol unchanged

### CP-2: Create Provider-B Runner Pool

- deploy new runners in provider B
- register with same Gum API/internal auth model
- configure explicit runner capacity:
  - `GUM_RUNNER_CPU_CORES`
  - `GUM_RUNNER_MEMORY_MB`
  - `GUM_RUNNER_MAX_CONCURRENT_LEASES`

### CP-3: Route by Compute Class

Use `compute_class` as migration control:

- create/assign jobs for new class (for example `provider_b_standard`)
- canary selected jobs to provider B class
- validate run correctness, latency, and recovery behavior

### CP-4: Progressive Ramp

- move job groups to provider B class in steps
- hold each step and verify metrics
- keep provider A runners active for rollback safety

### CP-5: Drain Provider A

When provider B is stable:

- stop assigning new work to provider A classes
- wait until provider A active leases reach zero
- scale down provider A runners after lease drain

### CP-6: Rollback Plan

If gates fail:

1. move affected jobs back to provider A compute class
2. scale provider A runner capacity back up
3. keep provider B runners available for debugging only
4. verify health + smoke

Only fully retire provider A after 24h green soak on provider B.

---

## Operational Checklist Template

Use this checklist for each migration event:

- migration id
- operator
- start time / end time
- baseline metric snapshot link
- phase-by-phase timestamps
- gate pass/fail at each hold point
- rollback trigger observed (if any)
- rollback executed (yes/no)
- final status
- artifact links (logs, parity reports, smoke outputs)

## Minimum Evidence to Archive

- pre-cutover smoke pass output
- parity check output (DB move)
- canary + ramp metric snapshots
- post-cutover smoke pass output
- soak summary (24h)
- incident notes for any abort/retry

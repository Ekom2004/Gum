# Gum Compute Inheritance

This document captures the compute-plane ideas Gum should inherit from the MX8 architecture and codebase.

The goal is not to copy MX8 wholesale.

The goal is to reuse the parts that make managed execution durable, recoverable, and operationally honest.

## Why This Exists

Gum already has a good product surface:

- write the function
- deploy it
- enqueue or schedule it
- Gum runs it

What makes that credible in production is the compute plane underneath it.

MX8 already contains good patterns for leased compute, failure recovery, and execution control. Gum should adopt those patterns in a simpler form.

## What Gum Should Take

### 1. Leader fencing

Source:
- `mx8-media/crates/mx8-coordinator/src/leader_lease.rs`

What it is:
- one active mutator for control-plane work
- explicit term ownership
- followers can observe but not mutate

Why Gum should take it:
- the scheduler and dispatcher should not be allowed to double-fire
- mutating operations need a single owner during failover

What problem it solves:
- duplicate schedule ticks
- split-brain dispatch
- two instances both trying to own queue mutation

How Gum should apply it:
- scheduler leadership
- dispatch leadership
- any future lease expiry / requeue sweeper

Priority:
- high

### 2. Durable execution state

Source:
- `mx8-media/crates/mx8-coordinator/src/state_store.rs`

What it is:
- explicit durable state for nodes, leases, progress, completed work, and metrics

Why Gum should take it:
- managed compute needs durable execution ownership
- in-memory state is not enough once runners, long-running work, and recovery matter

What problem it solves:
- restart ambiguity
- orphaned attempts
- unclear ownership after process death

How Gum should apply it:
- runner registry
- active leases
- lease expiry / requeue bookkeeping
- execution progress metadata where needed

Notes:
- Gum does not need the MX8 schema shape
- Gum does need the same explicitness

Priority:
- high

### 3. Revocable leases

Source:
- `mx8-media/crates/mx8d-agent/src/main.rs`

What it is:
- execution checks whether the coordinator has revoked the lease
- work can stop cleanly when ownership changes

Why Gum should take it:
- Gum should not only start work
- Gum should also be able to stop work

What problem it solves:
- canceled jobs continuing to run
- work running past control-plane intent
- no clean preemption or revoke path

How Gum should apply it:
- cancel run
- timeout escalation
- lease revocation when runner ownership is lost

Priority:
- high

### 4. Heartbeats and dead-runner recovery

Source:
- MX8 coordinator + agent behavior around heartbeats, departed nodes, and replacement

What it is:
- runners report liveness
- the coordinator detects dead workers
- work can be reassigned

Why Gum should take it:
- long-running managed execution is not real without liveness
- a dead runner must not leave a run stuck forever

What problem it solves:
- orphaned attempts
- permanently running jobs after a crash
- no safe way to requeue lost work

How Gum should apply it:
- runner heartbeats
- lease expiry
- dead-runner detection
- requeue or failover policy for leased attempts

Priority:
- highest

### 5. Capacity-aware execution controls

Source:
- `mx8-media/crates/mx8-runtime/src/pipeline.rs`

What it is:
- explicit inflight limits
- bounded memory
- backpressure
- controlled concurrency

Why Gum should take it:
- autoscaling is not enough
- each runner still needs local execution discipline

What problem it solves:
- overcommitted runners
- memory blowups
- fake elasticity that collapses under load

How Gum should apply it:
- max concurrent executions per runner
- runner capability classes
- future placement based on resource needs
- bounded local queues

Notes:
- Gum should take the discipline, not the media-specific batching logic

Priority:
- medium-high

## What Gum Should Not Copy Directly

These are real in MX8, but they are not Gum’s core problem:

- dataset range splitting
- manifest-specific replay semantics
- segment cursor logic
- world-size and rank assignment
- media transform sink logic

Those belong to MX8’s distributed data-processing model.

Gum’s unit is simpler:

- run
- attempt
- runner
- lease

## The Translation To Gum

MX8 is a leased compute fabric for distributed data work.

Gum should become a leased compute fabric for production function execution.

The translation is:

- MX8 node -> Gum runner
- MX8 lease -> Gum attempt lease
- MX8 progress -> Gum attempt liveness / execution progress
- MX8 coordinator -> Gum scheduler / dispatcher / runner registry
- MX8 revoke -> Gum cancel / timeout / lease-loss handling

## Recommended Adoption Order

1. Heartbeats and dead-runner recovery
2. Revocable leases
3. Leader fencing
4. Durable execution state
5. Capacity-aware runner limits

This order matters because it moves Gum from:

- can run work

to:

- can own execution safely

## Why This Matters

Without these pieces, Gum risks becoming:

- a nice SDK
- a basic runner loop
- managed in branding more than in system design

With these pieces, Gum becomes:

- a real managed execution platform
- capable of long-running work
- capable of recovering from runner failure
- capable of honest operational ownership

That is the difference between a product that demos well and a product teams trust in production.

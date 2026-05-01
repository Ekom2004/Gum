# Gum Compute Provider Adapters

Last updated: April 28, 2026

This document defines how Gum maps job resource knobs (`cpu`, `memory`, `compute`) to compute providers.

## Goal

Keep user DX stable while making provider swaps operator-simple.

User flow remains:

1. write `@gum.job(...)`
2. run `gum deploy`
3. call `.enqueue(...)`

Provider-specific capacity wiring happens behind `gum deploy`.

## Current Adapter Contract

Implemented in `sdk/gum/provisioning.py`.

- `RunnerCapacityPlan`:
  - `compute_class`
  - `cpu_cores`
  - `memory_mb`
  - `max_concurrent_leases`
- `CapacityProvisioner.sync(plan)`:
  - applies provider capacity state from the normalized plan

## Provider Selection

Environment:

- `GUM_COMPUTE_PROVIDER=fly|noop|none`
- if unset:
  - `fly` is selected when `FLY_RUNNER_APP` is set
  - otherwise `noop`

Controls:

- `GUM_AUTO_SYNC_RUNNER_CAPACITY=0|1` (default behavior: enabled when provider is available)
- `GUM_RUNNER_PARALLELISM` (default `1`)
- `GUM_RUNNER_COMPUTE_CLASS` (default `standard`)

## Fly Adapter

When provider is `fly`, `gum deploy`:

1. discovers max per-job CPU/memory requirements
2. multiplies by `GUM_RUNNER_PARALLELISM`
3. updates runner secrets:
   - `GUM_RUNNER_CPU_CORES`
   - `GUM_RUNNER_MEMORY_MB`
   - `GUM_RUNNER_MAX_CONCURRENT_LEASES`
4. updates Fly runner machine CPU/memory accordingly

## How This Enables Smooth Provider Swaps

To add a new provider:

1. implement `CapacityProvisioner` for that provider
2. wire selector in `provisioner_from_env()`
3. map `RunnerCapacityPlan` -> provider API calls

No change to:

- job decorator API
- deploy payload model
- enqueue/runtime API

## Spillover Design Direction

Adapter layer should support class-based spillover without changing user DX:

- primary class
- fallback classes
- wait-threshold trigger
- max spillover percentage

This belongs in provider/operator config, not user job code.

# Gum Secret Store Adapters

## Decision

Gum should use a `SecretStore` adapter boundary.

This gives us:

- fast initial shipping with one managed backend
- ability to switch providers later without user-facing downtime
- a stable Gum UX (`gum setup resend`, `gum secrets set`) regardless of backend

## Recommendation

1. Build adapter boundary now.
2. Start with one backend implementation (AWS KMS + Secrets Manager recommended for long-term trust/compliance).
3. Keep provider details behind Gum API and runtime.

## Non-Goals

- exposing backend-specific setup to end users
- coupling job execution code directly to any one secrets provider

## Product UX Contract

Users should only interact with Gum commands:

- `gum setup resend`
- `gum secrets set NAME`
- `gum secrets list`
- `gum secrets delete NAME`

Users should not need to know whether Gum uses AWS, Infisical, or another provider.

## Core Adapter Interface

Define a single internal interface:

- `set(project_id, env, name, value) -> SecretVersionRef`
- `resolve(project_id, env, name) -> SecretValue`
- `list(project_id, env) -> SecretMetadata[]`
- `delete(project_id, env, name) -> void`
- `rotate(project_id, env, name, value) -> SecretVersionRef`

Notes:

- `resolve` is runtime/internal only.
- User-facing APIs should never return plaintext secret values.

## Data Model (Gum DB)

Store metadata and pointers, not plaintext:

- `project_id`
- `environment` (`stg`, `prod`, etc.)
- `name` (e.g. `RESEND_API_KEY`)
- `backend` (e.g. `aws_secrets_manager`)
- `secret_ref` (provider reference/ARN/path)
- `active_version`
- `created_at`, `updated_at`, `last_used_at`
- `created_by`

Optional event/audit table:

- `event_type` (`set`, `delete`, `rotate`, `resolve`)
- `actor`
- `project_id`, `environment`, `name`, `version`
- `timestamp`
- `run_id` (for runtime resolve events)

## Runtime Flow

1. Runner leases run.
2. Runner asks Gum API for required secret names for that run.
3. Gum resolves via configured `SecretStore` backend.
4. Runner injects values in-memory only for process execution.
5. Runner clears secret material after run completion.

## Security Rules

- no plaintext secret readback in CLI/dashboard
- redact secret values in logs and error output
- scoped auth: project/env boundaries enforced
- TLS in transit
- audit all mutation and resolve events

## Backend Swap Strategy

To migrate from backend A to backend B:

1. Add backend B adapter.
2. Bulk-copy secrets from A to B and verify checksums/lengths.
3. Update metadata pointers (`backend`, `secret_ref`, `active_version`) per secret.
4. Dual-read fallback window:
   - read B first
   - fallback to A only if missing
5. Monitor resolve failures and audit logs.
6. Remove fallback and decommission A.

This avoids downtime during migration.

## Immediate Build Plan

1. Implement `SecretStore` interface + one concrete backend.
2. Add API endpoints for `set/list/delete` with metadata-only responses.
3. Add runner runtime `resolve` path.
4. Add audit events + log redaction.
5. Add `gum setup resend` as guided secret onboarding.

## Current Implementation (Apr 30, 2026)

- `SecretStore` now has two backends:
  - `memory` (dev fallback)
  - `postgres_aes256_gcm_v1` (persistent, encrypted at rest)
- Secret metadata is stored in Postgres table `project_secrets`.
- Secret values are encrypted/decrypted with AES-256-GCM in Gum API.
- Runner receives resolved values only for required secret names at lease time.
- Runner redacts resolved secret values from stdout/stderr and failure messages before logs persist.

## Backend Config

Set these in the Gum API environment:

1. `GUM_SECRET_BACKEND=postgres`
2. `GUM_SECRET_MASTER_KEY=<32-byte key in hex/base64/raw>`
3. `DATABASE_URL=<gum postgres url>`

Notes:

- If `GUM_SECRET_BACKEND` is unset:
  - Gum auto-selects `postgres` if `GUM_SECRET_MASTER_KEY` exists.
  - Otherwise Gum uses `memory`.
- `GUM_SECRET_MASTER_KEY` accepted formats:
  - 64-char hex
  - base64 value decoding to 32 bytes
  - raw 32-byte string

Example key generation:

```bash
openssl rand -hex 32
```

## Bootstrap Guidance

- Infra bootstrap scripts should treat the secret backend as an adapter choice, not a provider-specific assumption.
- Pass `GUM_SECRET_BACKEND` explicitly from the environment when bringing up an API stack.
- For durable backends like `postgres`, preserve the same `GUM_SECRET_MASTER_KEY` across restarts and redeploys.
- Auto-generating a master key is acceptable only for first-time disposable environments; once secrets exist, rotating that key without migration will make existing secret values unreadable.

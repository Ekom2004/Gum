# Gum CLI-First Secret Flow

## Goal

Let customers use Gum end-to-end without depending on the web UI for runtime setup.

Primary flow should stay:

1. `gum login`
2. `gum deploy`
3. monitor with `gum list`, `gum get`, `gum logs`

## Product Contract

- `gum deploy` detects required provider secrets from job code.
- If required secrets are missing, `gum deploy` prompts in terminal with hidden input.
- Gum stores secrets encrypted server-side.
- Deploy continues automatically after secrets are saved.
- Secrets are never returned in plaintext after save.

## Customer Flow (Interactive)

1. User writes job using provider SDK (example: Resend).
2. User runs `gum deploy`.
3. Gum scans and detects required secret names (example: `RESEND_API_KEY`).
4. Gum checks secret presence for current project/env.
5. Missing secret path:
   - prompt: `Enter RESEND_API_KEY (input hidden):`
   - store encrypted in Gum secret backend
   - show metadata-only confirmation
6. Deploy registers successfully.
7. Scheduler/enqueue creates runs normally.
8. Runner resolves required secrets at run start and injects in-memory only.

## Customer Flow (CI / Non-Interactive)

1. CI runs `gum deploy`.
2. If required secret missing, deploy fails with explicit message:
   - missing secret name(s)
   - exact remediation path
3. CI supplies secret via CI secret store integration.
4. Re-run deploy, succeeds.

## Runtime Flow

1. Runner leases run.
2. Runner requests required secrets for that run via internal auth.
3. Gum resolves via configured `SecretStore` adapter.
4. Runner injects values into process env in memory only.
5. Runner clears secret material after execution.

## Security Requirements

- no plaintext secret readback (CLI or API)
- hidden-input prompt for interactive entry
- TLS in transit
- encrypted at rest via managed backend
- strict project/env scoping
- secret redaction in logs/errors
- audit trail for `set`, `resolve`, `delete`, `rotate`

## UX Rules

- Keep default onboarding to minimal commands:
  - `gum login`
  - `gum deploy`
- No required provider-specific command for common paths.
- Advanced secret commands can exist but are not required for first success.

## API + CLI Surface (MVP)

### CLI

- `gum deploy` (with missing-secret detection + hidden prompt)
- `gum secrets list` (metadata only, optional for advanced users)
- `gum secrets delete NAME` (optional for advanced users)

### API

- set secret (write only)
- list secret metadata
- delete secret
- resolve secret (internal runtime path only)

## Error Contracts

Interactive deploy:

- prompt for missing secrets and continue.

Non-interactive deploy:

- fail with:
  - missing secret names
  - command hint to fix

Runtime failure:

- if resolve fails, run fails with clear class/reason (`missing_secret:<NAME>`), no value leakage.

## Build Plan

1. Implement secret detection in deploy scanner.
2. Add deploy-time missing-secret check API call.
3. Add hidden-input CLI prompt + write path.
4. Implement runner resolve/injection path.
5. Add redaction middleware + audit events.
6. Add CI error/remediation behavior.
7. Validate with E2E scheduled Resend scenario.

## Acceptance Criteria

- User can go from `gum login` to successful `gum deploy` without web dashboard.
- Missing provider secret is handled inline during deploy.
- Scheduled Resend job executes and sends email from Gum cloud.
- No secret value appears in logs, CLI output, or API responses.

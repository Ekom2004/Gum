# Gum Beta 7-Day Checklist

Launch window: April 23, 2026 -> April 30, 2026

## Beta goal

Ship a reliable, usable beta for serverless background jobs with one clear onboarding flow:

`gum init -> gum deploy -> enqueue -> gum logs -> gum admin`

## Launch criteria (must be true by April 30, 2026)

- [ ] Core flow works end-to-end on a fresh machine with no local assumptions.
- [ ] Retry, timeout, key dedupe, and concurrency behavior are verified in tests.
- [ ] Admin auth works and no secrets are leaked in logs/docs/examples.
- [ ] Docs are complete for first deploy and troubleshooting.
- [ ] We can support beta users through one clear channel with response SLA.

## Execution board (P0/P1)

Status legend: `todo`, `in_progress`, `done`

| Pri | Work item | Owner | Due date | Status |
| --- | --- | --- | --- | --- |
| P0 | Freeze beta scope and publish explicit "not in beta" list | Ekomotu (Product) | 2026-04-23 | `done` |
| P0 | Lock onboarding path acceptance checks (`init -> deploy -> enqueue -> logs -> admin`) | Ekomotu (Product/Runtime) | 2026-04-23 | `in_progress` |
| P0 | Retry reliability matrix (provider/transient/user-code/timeout) | Ekomotu (Runtime) | 2026-04-24 | `done` |
| P0 | Lease-loss and scheduler-recovery verification | Ekomotu (Runtime) | 2026-04-24 | `done` |
| P0 | Key/idempotency matrix (dedupe, replay semantics, retention) | Ekomotu (Runtime) | 2026-04-25 | `done` |
| P0 | Concurrency slot lifecycle matrix (all release paths + restart) | Ekomotu (Runtime) | 2026-04-25 | `todo` |
| P0 | Admin auth baseline + secrets hygiene pass | Ekomotu (Runtime/Security) | 2026-04-25 | `todo` |
| P0 | Ops checks (migration, backup/restore drill, rollback runbook) | Ekomotu (Ops) | 2026-04-27 | `todo` |
| P0 | 24h staging soak and blocker triage | Ekomotu (Runtime/Ops) | 2026-04-27 | `todo` |
| P0 | Beta support channel, owner, and response window published | Ekomotu (Support) | 2026-04-28 | `todo` |
| P1 | Observability pass (waiting reasons, retry_after, failure_class clarity) | Ekomotu (Runtime) | 2026-04-26 | `todo` |
| P1 | Docs pass (quickstart, deploy, knobs, troubleshooting, known limits) | Ekomotu (Docs) | 2026-04-26 | `in_progress` |
| P1 | Site/docs/CLI example consistency pass | Ekomotu (Product/Docs) | 2026-04-26 | `in_progress` |
| P1 | Optional `gum run <function>` DX shortcut | Ekomotu (Runtime/SDK) | 2026-04-29 | `todo` |

## Current snapshot (as of April 23, 2026)

- `done`: scope freeze and "not in beta" list published in this doc.
- `in_progress`: onboarding path is implemented in product surface (`gum init`, `gum deploy`, run inspection/admin) and needs fresh-machine validation.
- `done`: retry reliability matrix core paths validated by targeted tests (transient, health-hold, user-code terminal, timeout).
- `done`: lease-loss and scheduler-recovery behavior validated by targeted tests.
- `done`: key/idempotency matrix validated for dedupe semantics, replay bypass, scalar key validation, and numeric key support; retention behavior documented.
- `in_progress`: docs foundation exists (quickstart/deploy/environment) and needs final troubleshooting/known-limits pass.
- `in_progress`: website hero example consistency work is active and should be locked before docs freeze.

## Issue list (execution IDs)

| ID | Pri | Work item | Owner | Due date | Status |
| --- | --- | --- | --- | --- | --- |
| BETA-001 | P0 | Freeze beta scope and publish "not in beta" list | Ekomotu | 2026-04-23 | `done` |
| BETA-002 | P0 | Lock onboarding path acceptance checks | Ekomotu | 2026-04-23 | `in_progress` |
| BETA-003 | P0 | Retry reliability matrix execution | Ekomotu | 2026-04-24 | `done` |
| BETA-004 | P0 | Lease-loss and scheduler-recovery verification | Ekomotu | 2026-04-24 | `done` |
| BETA-005 | P0 | Key/idempotency matrix execution | Ekomotu | 2026-04-25 | `done` |
| BETA-006 | P0 | Concurrency slot lifecycle matrix execution | Ekomotu | 2026-04-25 | `todo` |
| BETA-007 | P0 | Admin auth + secrets hygiene pass | Ekomotu | 2026-04-25 | `todo` |
| BETA-008 | P0 | Ops checks: migration, backup/restore, rollback | Ekomotu | 2026-04-27 | `todo` |
| BETA-009 | P0 | 24h staging soak + blocker triage | Ekomotu | 2026-04-27 | `todo` |
| BETA-010 | P0 | Support channel + response window published | Ekomotu | 2026-04-28 | `todo` |
| BETA-011 | P1 | Observability pass | Ekomotu | 2026-04-26 | `todo` |
| BETA-012 | P1 | Docs pass (quickstart/deploy/knobs/troubleshooting) | Ekomotu | 2026-04-26 | `in_progress` |
| BETA-013 | P1 | Site/docs/CLI example consistency pass | Ekomotu | 2026-04-26 | `in_progress` |
| BETA-014 | P1 | Optional `gum run <function>` DX shortcut | Ekomotu | 2026-04-29 | `todo` |

## Not in beta (explicitly deferred)

- TypeScript SDK/runtime support.
- Managed multi-region control plane.
- Automatic autoscaling policies (AIMD/PID style knobs).
- Public provider health probing configuration surface.
- Advanced enterprise controls (SSO, RBAC, audit export, fine-grained org policy).
- Turnkey cloud billing and usage dashboard.
- Dedicated managed database offering.
- Non-core cloud provider integrations beyond current stack.
- New public knobs that are not fully covered by tests/docs.

## BETA-003 evidence (retry reliability matrix)

Executed on April 23, 2026.

| Scenario | Test command | Result |
| --- | --- | --- |
| Transient provider failure requeues with retry_after | `cargo test -p gum-api failed_attempt_requeues_when_retries_remain` | pass |
| Function health blocks retries after repeated infra failures | `cargo test -p gum-api function_health_blocks_retry_without_provider_config` | pass |
| User-code failure is terminal (no retry requeue) | `cargo test -p gum-api user_code_failures_do_not_consume_retry_budget_as_requeues` | pass |
| Timeout marks run timed_out and keeps logs | `cargo test -p gum-runner timed_out_execution_marks_run_timed_out_and_keeps_logs` | pass |

## BETA-004 evidence (lease-loss and scheduler-recovery)

Executed on April 23, 2026.

| Scenario | Test command | Result |
| --- | --- | --- |
| Expired lease is recovered; heartbeat keeps active lease alive | `cargo test -p gum-api expired_lease_is_recovered_and_heartbeat_keeps_active_lease_alive` | pass |
| Expired lease cannot commit stale completion before recovery | `cargo test -p gum-api expired_lease_cannot_commit_completion_before_recovery_runs` | pass |

## BETA-005 evidence (key/idempotency matrix)

Executed on April 23, 2026.

| Scenario | Test command | Result |
| --- | --- | --- |
| Same key and same payload returns existing run | `cargo test -p gum-api keyed_enqueue_returns_existing_run_for_duplicate_payload` | pass |
| Same key and different payload still dedupes by key | `cargo test -p gum-api keyed_enqueue_dedupes_by_key_even_when_payload_differs` | pass |
| Replay intentionally bypasses key dedupe | `cargo test -p gum-api replay_bypasses_key_dedupe` | pass |
| Missing configured key field returns clear error | `cargo test -p gum-api keyed_enqueue_requires_the_configured_field` | pass |
| Non-scalar key values are rejected | `cargo test -p gum-api keyed_enqueue_requires_scalar_key_value` | pass |
| Numeric key values resolve and dedupe correctly | `cargo test -p gum-api keyed_enqueue_accepts_numeric_key_values` | pass |
| Key retention/expiry behavior is documented | `docs-site/knobs/key.mdx` | pass |

## Must-ship checklist

- [x] Freeze beta scope and publish a "not in beta" list.
- [ ] Lock one hero product path and keep examples consistent across site/docs/CLI.
- [x] Run reliability matrix for retries:
- [x] transient provider failure retries with backoff and jitter
- [x] user-code failure is terminal when non-retryable
- [x] timeout path retries/terminal behavior
- [x] runner lease loss and scheduler recovery path
- [x] Run idempotency/key matrix:
- [x] duplicate enqueue with same key dedupes correctly
- [x] replay behavior with key is explicit and tested
- [x] key retention/expiry behavior is documented
- [ ] Run concurrency matrix:
- [ ] slot acquire/release on success, failure, timeout, cancel
- [ ] slot recovery after runner loss and scheduler restart
- [ ] Auth/security baseline:
- [ ] admin auth and local key storage flow verified
- [ ] no secrets checked into config/docs/examples
- [ ] Observability minimum:
- [ ] run status, failure class, retry_after, waiting reason visible
- [ ] function health visibility in API/admin path
- [ ] Ops guardrails:
- [ ] backup and restore drill done once
- [ ] migrations apply cleanly on staging
- [ ] rollback procedure documented
- [ ] Docs readiness:
- [ ] quickstart
- [ ] deploy guide
- [ ] knob semantics (retry/timeout/rate_limit/concurrency/key)
- [ ] troubleshooting and known limits
- [ ] Error messaging pass:
- [ ] provider down / retry held / deduped run / auth errors are actionable
- [ ] Support readiness:
- [ ] beta support channel linked from docs/site
- [ ] owner and response window defined

## Should-ship if time remains

- [ ] Add `gum run <function>` CLI shortcut for first-run DX.
- [ ] Improve admin empty/loading states.
- [ ] Add polished Stripe and AI examples in docs.

## Day-by-day execution plan

### Day 1 - April 23, 2026

- [ ] Freeze scope and publish beta feature list + excluded items.
- [ ] Lock hero onboarding path and acceptance criteria.
- [ ] Create issue board for P0/P1 beta blockers only.

### Day 2 - April 24, 2026

- [ ] Execute retry/timeout/failure-class matrix.
- [ ] Fix all P0 reliability defects found.

### Day 3 - April 25, 2026

- [ ] Execute key/idempotency + concurrency slot matrix.
- [ ] Fix all P0/P1 correctness defects found.

### Day 4 - April 26, 2026

- [ ] Complete docs pass (quickstart, deploy, knobs, troubleshooting).
- [ ] Align website examples with actual runtime behavior.

### Day 5 - April 27, 2026

- [ ] Run staging ops checks: migrations, backup/restore, rollback drill.
- [ ] Run 24h soak on staging with synthetic traffic.

### Day 6 - April 28, 2026

- [ ] Invite first beta users (design partners).
- [ ] Run live onboarding sessions and capture friction points.

### Day 7 - April 29, 2026

- [ ] Fix only P0/P1 launch blockers.
- [ ] Freeze release candidate and prep launch communication.

### Launch day - April 30, 2026

- [ ] Tag beta release.
- [ ] Open beta access window.
- [ ] Monitor support + incident channel continuously.

## Known out-of-scope candidates (explicitly defer if unstable)

- [ ] Non-core cloud provider integrations not required for beta.
- [ ] Non-essential UI polish that does not affect onboarding or operations.
- [ ] New knobs or major SDK surface changes without full test coverage.

## Owner map

- Product owner: scope freeze, launch criteria, beta user messaging.
- Runtime owner: retries, key, concurrency, recovery behavior.
- Docs owner: quickstart and troubleshooting.
- Ops owner: migrations, backup/restore, rollback, staging health.
- Support owner: beta channel triage and response.

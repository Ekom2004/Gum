# Gum Ready For Customers Checklist

Use this checklist before handing Gum to external users.

## P0: Must Be Ready

- [ ] Onboarding flow works cleanly: `pip install usegum` -> `gum login` -> `gum deploy` -> `.enqueue()` -> inspect run/logs.
- [ ] Secrets are cloud-managed: set/list/delete/rotate, runtime injection, redaction in logs/UI.
- [ ] CI-first deploy is default (no founder laptop dependency).
- [ ] Scheduler reliability is proven (cron/every fire on time, restart-safe recovery).
- [ ] Execution semantics are solid under failure (retries, timeout, concurrency, idempotency key).
- [ ] CPU/memory knobs are validated in real placement/execution behavior.
- [ ] Dashboard covers core ops (runs, attempts, logs, replay, cancel, filters).
- [ ] Auth is production-safe (scoped keys, revocation, admin isolation).
- [ ] Observability + alerts are live (API/scheduler/runner health, failure alerts).
- [ ] Backups + restore drill are passing (persistence and rollback confidence).
- [ ] Billing minimum is live (usage metering, limits, invoice path).
- [ ] Docs match reality exactly (commands, knobs, errors, troubleshooting).
- [ ] Security baseline is complete (TLS, encrypted secrets, audit trail, no secret leakage).
- [ ] Support/incident runbook is ready (known failures, response steps, SLA).
- [ ] One full real scenario is green in staging and prod (scheduled Resend email end-to-end).

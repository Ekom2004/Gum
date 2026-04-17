# Gum V1

This document locks the v1 product boundary for Gum.

## What Gum Is

Gum is managed execution for background jobs.

The product promise is simple:
- define a background job
- schedule it or enqueue it
- Gum runs it reliably

Gum is not:
- a workflow engine
- an event bus
- an agent framework
- a generic serverless platform

## Who Gum Is For

The first target users are:
- Python backend teams
- SaaS teams with recurring operational jobs
- teams replacing cron, Celery, or custom worker glue

The first strong use cases are:
- CRM and billing syncs
- transactional and lifecycle emails
- exports and reports
- file or document processing
- AI enrichment batches
- scheduled lifecycle jobs

## V1 Product Surface

Gum v1 supports two trigger modes:
- scheduled jobs
- enqueued jobs

Gum v1 supports these policy fields:
- `every`
- `retries`
- `timeout`
- `rate_limit`
- `concurrency`

Gum v1 owns:
- deploys
- scheduling
- run dispatch
- retries
- timeout enforcement
- per-job rate limits
- per-job concurrency limits
- logs
- replay
- managed execution

Users own:
- function code
- business logic
- integrations
- payloads

## First End-to-End Path

The first end-to-end path Gum must make real is:

1. define a Python job
2. deploy it
3. enqueue it
4. execute it
5. show logs and run status
6. replay a failed run

This is the first product bar.

Scheduled jobs matter for the homepage and product story, but enqueue is the first execution path to prove.

## V1 Non-Goals

These are explicitly out of scope for v1:
- workflow steps
- waits and long-lived orchestration
- shared rate-limit pools
- per-key concurrency
- dedupe
- budgets
- TypeScript-first SDK work
- visual builders
- multi-language runtimes

## Canonical Examples

### Scheduled

```python
import gum
import resend

@gum.job(every="20d", retries=5, timeout="5m")
def send_followup():
    resend.emails.send(
        from_="Acme <hello@acme.com>",
        to="alex@example.com",
        subject="Checking in",
        html="<p>Hey Alex, just checking in.</p>",
    )
```

### Enqueued

```python
import gum

@gum.job(retries=5, timeout="5m", rate_limit="20/m", concurrency=5)
def sync_customer(customer_id: str):
    ...

sync_customer.enqueue(customer_id="cus_123")
```

## Architecture Direction

The v1 implementation direction is:
- Python SDK
- Rust control plane
- Rust runner
- Postgres as the system of record
- object storage for deploy bundles

The architecture should stay centered on one object:
- `Run`

## Success Criteria

Gum v1 is real when a team can:
- define a Python job in minutes
- deploy it once
- enqueue work from app code
- trust retries and timeouts
- inspect logs
- replay failures without writing glue

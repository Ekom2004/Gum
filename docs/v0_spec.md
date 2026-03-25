# MX8 v0 Specification

MX8 v0 is about shipping fast, reliable media work from a single package (`mx8`) with a high-level `input/work/output` job shape. The first release must balance responsiveness (start time), throughput (PB-scale jobs), and controls (quotas, billing, and pause/resume) so early partners can test with confidence.

## Release goals

1. **Single-entry SDK** — teams should only need the `mx8` package, not multiple package names. The SDK exposes `run` plus top-level operations like `find(...)` and `extract_frames(...)` inside an ordered `work=[...]` list. `docs/api_shape.md` is the source of truth for the public job DSL.
2. **Async job lifecycle** — every job is async from day one. Customers can pause, resume, cancel, and watch progress updates through the SDK, webhooks, or CLI. We should be able to restart jobs mid-run without recomputing successful slices.
3. **Throughput targets** — large jobs (30–100+ TB) should complete in hours, not days. That means the coordinator must be able to spin up tens of thousands of vCPUs (or a few hundred NVDEC-enabled GPUs) rapidly (target: <8 minutes to get compute warmed). Throughput monitoring (>5 GB/s per job) will be visible in the job API so customers can verify we are beating their homegrown stacks.

## Transform guarantees

- `extract` enumerates frames, clips, or scenes and emits typed metadata (duration, format, codec, resolution).
- `filter` accepts simple boolean expressions through the `expr` argument (`duration > 5`, `format == 'mp4'`). Expressions can compare duration, format, codec, width, height, fps, byte_size, checksum/hash, stream_id/media_type, bitrate, sample_rate, and channels, so clients can keep only the slices that matter without writing SQL.
- `sink` is part of job submission, so output location is declared once rather than repeated as a transform step.
- corrupted assets are already handled by the ingest/runtime path and do not need a public `corrupt == false` example in the default UX.

## Quotas & governance

Per-team controls are essential:

- `max_concurrent_jobs` (default 3) keeps runaway fleets from starving other teams.
- `spend_cap` (e.g., $2,000/month) triggers soft stops; the client receives `QuotaExceeded` events before a job is admitted.
- `max_workers_total` limits how many nodes a single team can own (useful when they want to stay under 40 workers).
- pool selection and worker concurrency stay in the control plane. They matter operationally, but they are not part of the default public SDK surface.

## Data & cost control

- S3 is the canonical source/sink, but the engine is storage-agnostic; connectors for Azure, GCS, on-prem, and even HTTP/FTP can be added later.
- We monitor egress costs by region, and the billing pipeline can refund egress line items when the customer keeps data inside the region. If the job spans regions we pass a `egress_cost` field back so the user can offset it from their invoice, keeping the user’s experience at `.20/GB` consistent.

## Search & analytics

The search layer indexes transformed metadata and optionally user-provided embeddings. It ships with:

- `POST /v1/search` with a query and optional vector payload.
- Pricing per query (target $0.02/query in v0) with usage reported alongside job costs.
- Rate limiting tied to `max_queries_per_minute` so we can support embed-first workloads without spinning up a new compute plane.

## Pricing & revenue examples

- At `$0.20/GB` the 50 TB cleanup job in the API example brings in `$10,240`. Even with a 30% margin earmarked for compute + egress reimbursements, that leaves room for enterprise margins on top of search revenue (e.g., 500K queries at `$0.02/query` is another `$10K`).
- Keep a `cloud_costs` line item per job (compute, storage I/O, egress) so clients can trace the `profit = revenue - refunds` number themselves. We will refund egress only when the customer chooses not to pay it directly, keeping the net price on the invoicing screen at `.20/GB`.

## Next steps

1. Document `job` telemetry and webhook payloads so Mintlify can show live dashboards.
2. Build CLI prototypes for `mx8 job pause/resume` that talk to the same API surface as the SDK.
3. Add sample data references (S3 paths, public datasets) so design partners can reproduce the workflows without needing private buckets.

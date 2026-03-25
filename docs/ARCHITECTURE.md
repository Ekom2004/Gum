MX8 Architecture

MX8 is a distributed media transform platform built for teams that need fast, predictable, and affordable processing across massive datasets. The platform splits into two tightly coupled halves: a **control plane** that owns intent (API, CLI, policy) and a **data plane** that materializes work on compute nodes that live close to data sources such as S3.

## High-level components

1. **API & SDK control plane** — accepts authenticated requests from clients and tooling (including Mintlify docs, CLI, or enterprise portals). It stores job templates, quota rules, and billing caps, and it emits webhooks/events when jobs pause, resume, or finish. Importantly, the SDK surface is slim so teams work from one package (`mx8`) without juggling multiple packages or nested imports.

2. **Coordinator** — lives inside the control plane but runs on the compute pool. Its job is to allocate workers, pull metadata from the customer’s bucket, split the workload, and boot each slice as soon as resources are available. The coordinator links to reserve pools (cold/spot/priority) so extremely large workloads (e.g., tens of petabytes) can start within minutes instead of hours.

3. **Distributed compute pool** — a swarm of worker nodes (GPUs, CPU, and NVDEC-equipped machines). Nodes pull work from the coordinator via a lease system, which lets the system adjust concurrency by job, by team, or by billing plan. Each worker handles a portion of the dataset, streams results back to the storage tier, and writes telemetry so the coordinator can throttle, pause, or scale on demand.

4. **Storage connectors** — the default sink is S3, but the architecture anticipates new sinks (Azure, GCS, on-prem). Transform steps run close to the data (zero-copy design) so the only egress that happens is the final export, which can be paid for privately or refunded by MX8 depending on the contract.

5. **Search & analytics plane** — optional service that indexes transformed metadata and unstructured media. It powers search-as-a-service pricing (per query) and allows customers to explore their data without repeatedly running full transforms.

6. **Billing, monitoring, and quotas** — job-level throttles, per-team concurrency limits, worker budgets, and monthly spend caps are defined in the control plane and enforced before a job even launches. Every request is annotated with a customer ID so billing, ingestion, and compliance can track compute/egress separately.

## Data flow

1. Client describes a job through the SDK or REST API (example in `docs/api_shape.md`).
2. The request lands in the control plane, which applies policy: quota rules, spend limits, and compliance checks.
3. The coordinator segments the job and awakens the compute pool. Work starts on whichever nodes are available (spot, reserved, or priority) so enormous workloads can begin even if the customer has only a handful of nodes on paper.
4. Each worker executes the transform chain (extract, filter, deduplicate, export) against the metadata stream, writes outputs back to S3, and publishes progress to the job’s channel.
5. Once the job completes, the control plane ships logs, search indexes, and billing usage to the customer dashboard.

## Operational durability

- **Fault isolation:** Workers report heartbeats; the coordinator can reassign leases if a node dies mid-job.
- **Async control:** Jobs can be paused, resumed, or canceled via the CLI and SDK so teams can treat MX8 like a Kubernetes cluster and react to supply fluctuations.
- **Observability hooks:** Every job publishes metrics (throughput, errors, egress, compute seconds) so teams can spot runaway spend before it hits a billing cap.

This architecture document should be referenced by Mintlify to explain the flow from the `mx8` SDK through to finished outputs and search/analytics layers.

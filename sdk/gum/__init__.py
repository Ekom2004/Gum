from __future__ import annotations

from .client import BackfillRef, DeployRef, GumAPIError, GumClient, RunRecord, RunRef, default_client
from .job import JobDefinition, JobPolicy, job

Client = GumClient


class _RunsProxy:
    def get(self, run_id: str, *, client: GumClient | None = None) -> RunRecord:
        active_client = client or default_client()
        return active_client.runs.get(run_id)

    def replay(self, run_id: str, *, client: GumClient | None = None) -> RunRef:
        active_client = client or default_client()
        return active_client.runs.replay(run_id)


runs = _RunsProxy()

__all__ = [
    "BackfillRef",
    "Client",
    "DeployRef",
    "GumAPIError",
    "GumClient",
    "JobDefinition",
    "JobPolicy",
    "RunRecord",
    "RunRef",
    "job",
    "runs",
]

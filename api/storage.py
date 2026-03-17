from __future__ import annotations

from collections.abc import Iterable
from typing import Protocol

from .models import CreateJobRequest, JobRecord, JobStatus, utc_now


class JobStore(Protocol):
    def create_job(self, request: CreateJobRequest) -> JobRecord: ...

    def get_job(self, job_id: str) -> JobRecord | None: ...

    def list_jobs(self) -> Iterable[JobRecord]: ...

    def update_job_status(self, job_id: str, status: JobStatus) -> JobRecord | None: ...


class InMemoryJobStore:
    def __init__(self) -> None:
        self._jobs: dict[str, JobRecord] = {}

    def create_job(self, request: CreateJobRequest) -> JobRecord:
        record = JobRecord(
            source=request.source,
            sink=request.sink,
            transforms=request.transforms,
        )
        self._jobs[record.id] = record
        return record

    def get_job(self, job_id: str) -> JobRecord | None:
        return self._jobs.get(job_id)

    def list_jobs(self) -> list[JobRecord]:
        return sorted(
            self._jobs.values(),
            key=lambda job: job.created_at,
            reverse=True,
        )

    def update_job_status(self, job_id: str, status: JobStatus) -> JobRecord | None:
        record = self._jobs.get(job_id)
        if record is None:
            return None
        updated = record.model_copy(
            update={
                "status": status,
                "updated_at": utc_now(),
            }
        )
        self._jobs[job_id] = updated
        return updated

from __future__ import annotations

from datetime import datetime, timezone
from enum import Enum
from typing import Any
from uuid import uuid4

from pydantic import BaseModel, Field


def utc_now() -> datetime:
    return datetime.now(timezone.utc)


class JobStatus(str, Enum):
    PENDING = "PENDING"
    QUEUED = "QUEUED"
    RUNNING = "RUNNING"
    COMPLETE = "COMPLETE"
    FAILED = "FAILED"


class TransformSpec(BaseModel):
    type: str
    params: dict[str, Any] = Field(default_factory=dict)


class CreateJobRequest(BaseModel):
    source: str
    sink: str
    transforms: list[TransformSpec]


class JobRecord(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid4()))
    status: JobStatus = JobStatus.PENDING
    source: str
    sink: str
    transforms: list[TransformSpec]
    created_at: datetime = Field(default_factory=utc_now)
    updated_at: datetime = Field(default_factory=utc_now)


class JobStatusUpdate(BaseModel):
    status: JobStatus


class JobCompletionWebhook(BaseModel):
    job_id: str
    total_bytes_processed: int = 0

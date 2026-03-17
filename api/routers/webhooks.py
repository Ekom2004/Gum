from __future__ import annotations

from fastapi import APIRouter, HTTPException, Request, status

from ..models import JobCompletionWebhook, JobRecord, JobStatus, JobStatusUpdate

router = APIRouter(prefix="/internal", tags=["internal"])


@router.post("/job-complete", response_model=JobRecord)
def complete_job(payload: JobCompletionWebhook, request: Request) -> JobRecord:
    store = request.app.state.store
    record = store.update_job_status(payload.job_id, JobStatus.COMPLETE)
    if record is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="job not found")
    return record


@router.post("/job-status/{job_id}", response_model=JobRecord)
def update_job_status(job_id: str, payload: JobStatusUpdate, request: Request) -> JobRecord:
    store = request.app.state.store
    record = store.update_job_status(job_id, payload.status)
    if record is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="job not found")
    return record

from __future__ import annotations

from fastapi import APIRouter, HTTPException, Request, status

from ..models import (
    JobCompletionWebhook,
    JobProgressUpdate,
    JobRecord,
    JobStatus,
    JobStatusUpdate,
)

router = APIRouter(prefix="/internal", tags=["internal"])


@router.post("/job-complete", response_model=JobRecord)
def complete_job(payload: JobCompletionWebhook, request: Request) -> JobRecord:
    store = request.app.state.store
    scaler = request.app.state.scaler
    record = store.update_job_progress(
        JobProgressUpdate(
            job_id=payload.job_id,
            status=JobStatus.COMPLETE,
            completed_objects=payload.total_objects_processed,
            completed_bytes=payload.total_bytes_processed,
        )
    )
    if record is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="job not found")
    scaler.wake()
    return record


@router.post("/job-status/{job_id}", response_model=JobRecord)
def update_job_status(job_id: str, payload: JobStatusUpdate, request: Request) -> JobRecord:
    store = request.app.state.store
    scaler = request.app.state.scaler
    record = store.update_job_status(job_id, payload.status)
    if record is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="job not found")
    scaler.wake()
    return record


@router.post("/job-progress", response_model=JobRecord)
def update_job_progress(payload: JobProgressUpdate, request: Request) -> JobRecord:
    store = request.app.state.store
    scaler = request.app.state.scaler
    record = store.update_job_progress(payload)
    if record is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="job not found")
    scaler.wake()
    return record

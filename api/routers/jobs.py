from __future__ import annotations

from fastapi import APIRouter, HTTPException, Request, status

from ..models import JobView, SubmitJobRequest

router = APIRouter(prefix="/v1/jobs", tags=["jobs"])


@router.post("", response_model=JobView, status_code=status.HTTP_201_CREATED)
def create_job(payload: SubmitJobRequest, request: Request) -> JobView:
    store = request.app.state.store
    finder = request.app.state.finder
    scaler = request.app.state.scaler
    record = store.create_job(payload.to_internal())
    finder.wake()
    scaler.wake()
    return JobView.from_record(record)


@router.get("", response_model=list[JobView])
def list_jobs(request: Request) -> list[JobView]:
    store = request.app.state.store
    return [JobView.from_record(record) for record in store.list_jobs()]


@router.get("/{job_id}", response_model=JobView)
def get_job(job_id: str, request: Request) -> JobView:
    store = request.app.state.store
    record = store.get_job(job_id)
    if record is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="job not found")
    return JobView.from_record(record)

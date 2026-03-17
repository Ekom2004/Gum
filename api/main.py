from __future__ import annotations

from fastapi import FastAPI

from .routers import jobs, webhooks
from .storage import InMemoryJobStore


def create_app() -> FastAPI:
    app = FastAPI(title="mx8-media api", version="0.1.0")
    app.state.store = InMemoryJobStore()
    app.include_router(jobs.router)
    app.include_router(webhooks.router)

    @app.get("/healthz", tags=["system"])
    def healthz() -> dict[str, str]:
        return {"status": "ok"}

    return app


app = create_app()

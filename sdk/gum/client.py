from __future__ import annotations

import json
import os
from dataclasses import dataclass
from typing import Any
from urllib import error, request


class GumAPIError(RuntimeError):
    pass


@dataclass(slots=True)
class RunRef:
    id: str
    status: str
    deduped: bool = False


@dataclass(slots=True)
class BackfillRef:
    id: str
    status: str
    enqueued: int | None = None


@dataclass(slots=True)
class RunRecord:
    id: str
    job_id: str
    status: str
    attempt: int
    trigger_type: str | None = None
    failure_reason: str | None = None
    failure_class: str | None = None
    retry_after_epoch_ms: int | None = None
    waiting_reason: str | None = None
    replay_of: str | None = None


@dataclass(slots=True)
class LogLine:
    attempt_id: str
    stream: str
    message: str


@dataclass(slots=True)
class DeployRef:
    id: str
    registered_jobs: int


@dataclass(slots=True)
class RunnerStatus:
    id: str
    compute_class: str
    memory_mb: int
    active_memory_mb: int
    max_concurrent_leases: int
    last_heartbeat_at_epoch_ms: int
    active_lease_count: int


@dataclass(slots=True)
class LeaseStatus:
    lease_id: str
    run_id: str
    attempt_id: str
    runner_id: str
    expires_at_epoch_ms: int
    cancel_requested: bool


@dataclass(slots=True)
class ConcurrencyStatus:
    job_id: str
    job_name: str
    concurrency_limit: int
    active_count: int
    queued_count: int
    active_run_ids: list[str]
    queued_run_ids: list[str]


class RunsAPI:
    def __init__(self, client: "GumClient") -> None:
        self._client = client

    def get(self, run_id: str) -> RunRecord:
        body = self._client._request("GET", f"/v1/runs/{run_id}")
        return _run_record_from_payload(body)

    def replay(self, run_id: str) -> RunRef:
        body = self._client._request("POST", f"/v1/runs/{run_id}/replay")
        return _run_ref_from_payload(body)

    def cancel(self, run_id: str) -> RunRef:
        body = self._client._request("POST", f"/v1/runs/{run_id}/cancel", {"reason": None})
        return _run_ref_from_payload(body)

    def logs(self, run_id: str) -> list[LogLine]:
        body = self._client._request("GET", f"/v1/runs/{run_id}/logs")
        return [_log_line_from_payload(item) for item in body]

    def list(self) -> list[RunRecord]:
        body = self._client._request("GET", "/internal/admin/runs", use_admin_auth=True)
        return [_run_record_from_payload(item) for item in body.get("runs", [])]


class AdminAPI:
    def __init__(self, client: "GumClient") -> None:
        self._client = client

    def runners(self) -> list[RunnerStatus]:
        body = self._client._request("GET", "/internal/admin/runners", use_admin_auth=True)
        return [_runner_status_from_payload(item) for item in body.get("runners", [])]

    def leases(self) -> list[LeaseStatus]:
        body = self._client._request("GET", "/internal/admin/leases", use_admin_auth=True)
        return [_lease_status_from_payload(item) for item in body.get("leases", [])]

    def concurrency(self) -> list[ConcurrencyStatus]:
        body = self._client._request("GET", "/internal/admin/concurrency", use_admin_auth=True)
        return [_concurrency_status_from_payload(item) for item in body.get("concurrency", [])]


@dataclass(slots=True)
class GumClient:
    base_url: str
    api_key: str | None = None
    admin_key: str | None = None
    timeout_secs: float = 30.0

    @property
    def runs(self) -> RunsAPI:
        return RunsAPI(self)

    @property
    def admin(self) -> AdminAPI:
        return AdminAPI(self)

    def enqueue(self, job_id: str, payload: dict[str, Any], *, delay: str | None = None) -> RunRef:
        request_body: dict[str, Any] = {"input": payload}
        if delay is not None:
            request_body["delay"] = delay
        body = self._request("POST", f"/v1/jobs/{job_id}/runs", request_body)
        return _run_ref_from_payload(body)

    def register_deploy(self, payload: dict[str, Any]) -> DeployRef:
        body = self._request("POST", "/v1/deploys", payload)
        return DeployRef(
            id=body["id"],
            registered_jobs=body.get("registered_jobs", 0),
        )

    def backfill(self, job_id: str, items: list[dict[str, Any]]) -> BackfillRef:
        raise GumAPIError(
            f"backfill for {job_id} is not implemented yet; slice 1 only supports deploy, enqueue, run, logs, and replay"
        )

    def _request(
        self,
        method: str,
        path: str,
        payload: dict[str, Any] | None = None,
        *,
        use_admin_auth: bool = False,
    ) -> Any:
        url = f"{self.base_url.rstrip('/')}{path}"
        headers = {"Accept": "application/json"}
        token = self.admin_key if use_admin_auth else (self.api_key or self.admin_key)
        if token:
            headers["Authorization"] = f"Bearer {token}"

        data = None
        if payload is not None:
            headers["Content-Type"] = "application/json"
            data = json.dumps(payload).encode("utf-8")

        req = request.Request(url, data=data, headers=headers, method=method)
        try:
            with request.urlopen(req, timeout=self.timeout_secs) as resp:
                raw = resp.read()
        except error.HTTPError as exc:
            detail = exc.read().decode("utf-8", errors="replace")
            raise GumAPIError(f"{method} {path} failed: {exc.code} {detail}") from exc
        except error.URLError as exc:
            raise GumAPIError(f"{method} {path} failed: {exc.reason}") from exc

        if not raw:
            return None
        return json.loads(raw.decode("utf-8"))


def default_client() -> GumClient:
    return GumClient(
        base_url=os.environ.get("GUM_API_BASE_URL", "http://127.0.0.1:8000"),
        api_key=os.environ.get("GUM_API_KEY"),
        admin_key=os.environ.get("GUM_ADMIN_KEY"),
    )


def _run_ref_from_payload(payload: dict[str, Any]) -> RunRef:
    return RunRef(
        id=payload["id"],
        status=payload.get("status", "queued"),
        deduped=payload.get("deduped", False),
    )


def _run_record_from_payload(payload: dict[str, Any]) -> RunRecord:
    return RunRecord(
        id=payload["id"],
        job_id=payload["job_id"],
        status=payload["status"],
        attempt=payload.get("attempt", 1),
        trigger_type=payload.get("trigger_type"),
        failure_reason=payload.get("failure_reason"),
        failure_class=payload.get("failure_class"),
        retry_after_epoch_ms=payload.get("retry_after_epoch_ms"),
        waiting_reason=payload.get("waiting_reason"),
        replay_of=payload.get("replay_of"),
    )


def _log_line_from_payload(payload: dict[str, Any]) -> LogLine:
    return LogLine(
        attempt_id=payload["attempt_id"],
        stream=payload["stream"],
        message=payload["message"],
    )


def _runner_status_from_payload(payload: dict[str, Any]) -> RunnerStatus:
    return RunnerStatus(
        id=payload["id"],
        compute_class=payload["compute_class"],
        memory_mb=payload.get("memory_mb", 1024),
        active_memory_mb=payload.get("active_memory_mb", 0),
        max_concurrent_leases=payload["max_concurrent_leases"],
        last_heartbeat_at_epoch_ms=payload["last_heartbeat_at_epoch_ms"],
        active_lease_count=payload["active_lease_count"],
    )


def _lease_status_from_payload(payload: dict[str, Any]) -> LeaseStatus:
    return LeaseStatus(
        lease_id=payload["lease_id"],
        run_id=payload["run_id"],
        attempt_id=payload["attempt_id"],
        runner_id=payload["runner_id"],
        expires_at_epoch_ms=payload["expires_at_epoch_ms"],
        cancel_requested=payload["cancel_requested"],
    )


def _concurrency_status_from_payload(payload: dict[str, Any]) -> ConcurrencyStatus:
    return ConcurrencyStatus(
        job_id=payload["job_id"],
        job_name=payload["job_name"],
        concurrency_limit=payload["concurrency_limit"],
        active_count=payload["active_count"],
        queued_count=payload["queued_count"],
        active_run_ids=list(payload.get("active_run_ids", [])),
        queued_run_ids=list(payload.get("queued_run_ids", [])),
    )

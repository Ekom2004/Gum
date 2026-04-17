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
    replay_of: str | None = None


@dataclass(slots=True)
class DeployRef:
    id: str
    registered_jobs: int


class RunsAPI:
    def __init__(self, client: "GumClient") -> None:
        self._client = client

    def get(self, run_id: str) -> RunRecord:
        body = self._client._request("GET", f"/v1/runs/{run_id}")
        return _run_record_from_payload(body)

    def replay(self, run_id: str) -> RunRef:
        body = self._client._request("POST", f"/v1/runs/{run_id}/replay")
        return _run_ref_from_payload(body)


@dataclass(slots=True)
class GumClient:
    base_url: str
    api_key: str | None = None
    timeout_secs: float = 30.0

    @property
    def runs(self) -> RunsAPI:
        return RunsAPI(self)

    def enqueue(self, job_id: str, payload: dict[str, Any]) -> RunRef:
        body = self._request("POST", f"/v1/jobs/{job_id}/runs", {"input": payload})
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
    ) -> Any:
        url = f"{self.base_url.rstrip('/')}{path}"
        headers = {"Accept": "application/json"}
        if self.api_key:
            headers["Authorization"] = f"Bearer {self.api_key}"

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
    )


def _run_ref_from_payload(payload: dict[str, Any]) -> RunRef:
    return RunRef(
        id=payload["id"],
        status=payload.get("status", "queued"),
    )


def _run_record_from_payload(payload: dict[str, Any]) -> RunRecord:
    return RunRecord(
        id=payload["id"],
        job_id=payload["job_id"],
        status=payload["status"],
        attempt=payload.get("attempt", 1),
        trigger_type=payload.get("trigger_type"),
        failure_reason=payload.get("failure_reason"),
        replay_of=payload.get("replay_of"),
    )

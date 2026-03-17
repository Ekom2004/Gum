from __future__ import annotations

from dataclasses import dataclass
from time import sleep
from typing import TYPE_CHECKING


TERMINAL_STATUSES = {"COMPLETE", "FAILED"}


@dataclass(slots=True)
class Job:
    client: "MX8Client"
    id: str
    status: str
    source: str
    sink: str

    def poll(self) -> "Job":
        latest = self.client.get_job(self.id)
        self.status = latest.status
        self.source = latest.source
        self.sink = latest.sink
        return self

    def wait(self, *, poll_interval_secs: float = 2.0) -> "Job":
        while self.status not in TERMINAL_STATUSES:
            sleep(poll_interval_secs)
            self.poll()
        return self


if TYPE_CHECKING:
    from .client import MX8Client

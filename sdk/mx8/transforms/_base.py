from __future__ import annotations

from dataclasses import dataclass
from typing import Any


@dataclass(frozen=True, slots=True)
class Transform:
    kind: str
    params: dict[str, Any]

    def to_payload(self) -> dict[str, Any]:
        return {
            "type": self.kind,
            "params": dict(self.params),
        }

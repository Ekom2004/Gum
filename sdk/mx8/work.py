from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True, slots=True)
class FindWork:
    query: str

    def __post_init__(self) -> None:
        normalized = self.query.strip()
        if not normalized:
            raise ValueError("find query must be non-empty")
        object.__setattr__(self, "query", normalized)

    def to_payload(self) -> dict[str, object]:
        return {
            "type": "find",
            "params": {
                "query": self.query,
            },
        }


def find(query: str) -> FindWork:
    return FindWork(query=query)

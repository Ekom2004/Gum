from __future__ import annotations

from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass
from functools import update_wrapper
from typing import Any, ParamSpec, TypeVar

from .client import BackfillRef, GumClient, RunRef, default_client

P = ParamSpec("P")
R = TypeVar("R")


@dataclass(frozen=True, slots=True)
class JobPolicy:
    id: str | None = None
    every: str | None = None
    retries: int | None = None
    timeout: str | None = None
    rate_limit: str | None = None
    concurrency: int | None = None
    name: str | None = None


class JobDefinition(Callable[P, R]):
    def __init__(
        self,
        fn: Callable[P, R],
        *,
        every: str | None = None,
        retries: int | None = None,
        timeout: str | None = None,
        rate_limit: str | None = None,
        concurrency: int | None = None,
        name: str | None = None,
        client: GumClient | None = None,
    ) -> None:
        self._fn = fn
        self._client = client
        self.name = name or fn.__name__
        self.id = f"job_{self.name}"
        self.policy = JobPolicy(
            id=self.id,
            every=every,
            retries=retries,
            timeout=timeout,
            rate_limit=rate_limit,
            concurrency=concurrency,
            name=self.name,
        )
        update_wrapper(self, fn)

    def __call__(self, *args: P.args, **kwargs: P.kwargs) -> R:
        return self._fn(*args, **kwargs)

    def enqueue(
        self,
        payload: Mapping[str, Any] | None = None,
        /,
        *,
        client: GumClient | None = None,
        **kwargs: Any,
    ) -> RunRef:
        normalized = _normalize_payload(payload, kwargs)
        active_client = client or self._client or default_client()
        return active_client.enqueue(self.id, normalized)

    def backfill(
        self,
        items: Sequence[Mapping[str, Any]],
        *,
        client: GumClient | None = None,
    ) -> BackfillRef:
        normalized = [dict(item) for item in items]
        active_client = client or self._client or default_client()
        return active_client.backfill(self.id, normalized)


def job(
    *,
    every: str | None = None,
    retries: int | None = None,
    timeout: str | None = None,
    rate_limit: str | None = None,
    concurrency: int | None = None,
    name: str | None = None,
    client: GumClient | None = None,
) -> Callable[[Callable[P, R]], JobDefinition[P, R]]:
    def wrap(fn: Callable[P, R]) -> JobDefinition[P, R]:
        return JobDefinition(
            fn,
            every=every,
            retries=retries,
            timeout=timeout,
            rate_limit=rate_limit,
            concurrency=concurrency,
            name=name,
            client=client,
        )

    return wrap


def _normalize_payload(
    payload: Mapping[str, Any] | None,
    kwargs: Mapping[str, Any],
) -> dict[str, Any]:
    if payload is not None and kwargs:
        raise ValueError("use either a payload mapping or keyword arguments, not both")
    if payload is None:
        return dict(kwargs)
    return dict(payload)

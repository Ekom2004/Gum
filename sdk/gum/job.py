from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Callable, Generic, ParamSpec, TypeVar

from .client import BackfillRef, GumClient, RunRef, default_client

P = ParamSpec("P")
R = TypeVar("R")


@dataclass(frozen=True, slots=True)
class RateLimit:
    spec: str


@dataclass(slots=True)
class JobPolicy:
    id: str
    name: str
    handler_ref: str
    every: str | None = None
    cron: str | None = None
    timezone: str | None = None
    retries: int = 0
    timeout: str = "5m"
    rate_limit: str | None = None
    concurrency: int | None = None
    cpu: int | None = None
    memory: str | None = None
    key: str | None = None
    compute_class: str | None = None


class GumJob(Generic[P, R]):
    def __init__(
        self,
        func: Callable[P, R],
        *,
        every: str | None,
        cron: str | None,
        timezone: str | None,
        retries: int,
        timeout: str,
        rate_limit: str | RateLimit | None,
        concurrency: int | None,
        cpu: int | None,
        memory: str | None,
        key: str | None,
        compute: str | None,
        client: GumClient | None,
    ) -> None:
        self._func = func
        self._client = client
        self.name = func.__name__
        self.id = f"job_{func.__name__}"
        self.policy = JobPolicy(
            id=self.id,
            name=self.name,
            handler_ref=f"{func.__module__}:{func.__name__}",
            every=every,
            cron=cron,
            timezone=timezone,
            retries=retries,
            timeout=timeout,
            rate_limit=_normalize_rate_limit(rate_limit),
            concurrency=concurrency,
            cpu=cpu,
            memory=memory,
            key=key,
            compute_class=compute,
        )
        self.__name__ = func.__name__
        self.__doc__ = getattr(func, "__doc__")
        self.__module__ = func.__module__
        self.__gum_policy__ = self.policy

    def __call__(self, *args: P.args, **kwargs: P.kwargs) -> R:
        return self._func(*args, **kwargs)

    def enqueue(self, *, delay: str | None = None, **payload: object) -> RunRef:
        return self._gum_client().enqueue(self.id, payload, delay=delay)

    def backfill(self, items: list[dict[str, object]]) -> BackfillRef:
        return self._gum_client().backfill(self.id, items)

    def _gum_client(self) -> GumClient:
        if self._client is not None:
            return self._client
        return default_client()


def job(
    *,
    every: str | None = None,
    cron: str | None = None,
    timezone: str | None = None,
    retries: int = 0,
    timeout: str = "5m",
    rate_limit: str | RateLimit | None = None,
    concurrency: int | None = None,
    cpu: int | None = None,
    memory: str | None = None,
    key: str | None = None,
    compute_class: str | None = None,
    compute: str | None = None,
    client: GumClient | None = None,
) -> Callable[[Callable[P, R]], GumJob[P, R]]:
    """Register a function as a Gum job.

    IDE autocomplete should surface supported keyword knobs:
    `every`, `cron`, `timezone`, `retries`, `timeout`, `rate_limit`,
    `concurrency`, `cpu`, `memory`, `key`, `compute_class`, `client`.
    """
    if every is not None and cron is not None:
        raise ValueError("only one of every or cron may be set")
    if cron is not None and not str(cron).strip():
        raise ValueError("cron must not be empty")
    if timezone is not None and cron is None:
        raise ValueError("timezone requires cron")
    if timezone is not None and not str(timezone).strip():
        raise ValueError("timezone must not be empty")
    if compute_class is not None and compute is not None:
        raise ValueError("only one of compute_class or compute may be set")
    resolved_compute = compute_class if compute_class is not None else compute

    def decorator(func: Callable[P, R]) -> GumJob[P, R]:
        return GumJob(
            func,
            every=every,
            cron=cron,
            timezone=timezone,
            retries=retries,
            timeout=timeout,
            rate_limit=rate_limit,
            concurrency=concurrency,
            cpu=cpu,
            memory=memory,
            key=key,
            compute=resolved_compute,
            client=client,
        )

    return decorator


def rate_limit(spec: str) -> RateLimit:
    return RateLimit(spec=spec)


def _normalize_rate_limit(value: str | RateLimit | None) -> str | None:
    if value is None:
        return None
    if isinstance(value, RateLimit):
        return value.spec
    return value

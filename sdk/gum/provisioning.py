from __future__ import annotations

import os
import shutil
import subprocess
from dataclasses import dataclass
from typing import Protocol


@dataclass(slots=True)
class RunnerCapacityPlan:
    compute_class: str
    cpu_cores: int
    memory_mb: int
    max_concurrent_leases: int


class CapacityProvisioner(Protocol):
    name: str

    def sync(self, plan: RunnerCapacityPlan) -> None:
        ...


class NoopProvisioner:
    name = "noop"

    def sync(self, plan: RunnerCapacityPlan) -> None:
        _ = plan
        return None


class FlyProvisioner:
    name = "fly"

    def __init__(self, *, runner_app: str, flyctl_bin: str) -> None:
        self.runner_app = runner_app
        self.flyctl_bin = flyctl_bin

    def sync(self, plan: RunnerCapacityPlan) -> None:
        _run(
            [
                self.flyctl_bin,
                "secrets",
                "set",
                "-a",
                self.runner_app,
                f"GUM_RUNNER_COMPUTE_CLASS={plan.compute_class}",
                f"GUM_RUNNER_CPU_CORES={plan.cpu_cores}",
                f"GUM_RUNNER_MEMORY_MB={plan.memory_mb}",
                f"GUM_RUNNER_MAX_CONCURRENT_LEASES={plan.max_concurrent_leases}",
            ]
        )
        machine_ids_proc = _run(
            [self.flyctl_bin, "machine", "list", "-a", self.runner_app, "-q"],
            capture_output=True,
        )
        machine_ids = [line.strip() for line in machine_ids_proc.stdout.splitlines() if line.strip()]
        if not machine_ids:
            raise RuntimeError(f"no runner machines found for Fly app '{self.runner_app}'")

        for machine_id in machine_ids:
            _run(
                [
                    self.flyctl_bin,
                    "machine",
                    "update",
                    machine_id,
                    "-a",
                    self.runner_app,
                    "--vm-cpus",
                    str(plan.cpu_cores),
                    "--vm-memory",
                    str(plan.memory_mb),
                    "--yes",
                ]
            )


def build_runner_capacity_plan(
    jobs: list[object],
    *,
    compute_class: str,
    parallelism: int,
) -> RunnerCapacityPlan:
    if parallelism <= 0:
        raise RuntimeError("GUM_RUNNER_PARALLELISM must be a positive integer")

    max_cpu_single = max((_job_cpu_cores(job) for job in jobs), default=1)
    max_memory_single = max((_job_memory_mb(job) for job in jobs), default=512)

    return RunnerCapacityPlan(
        compute_class=compute_class,
        cpu_cores=max_cpu_single * parallelism,
        memory_mb=max_memory_single * parallelism,
        max_concurrent_leases=parallelism,
    )


def provisioner_from_env() -> CapacityProvisioner:
    provider = (os.environ.get("GUM_COMPUTE_PROVIDER") or "").strip().lower()
    if not provider:
        provider = "fly" if os.environ.get("FLY_RUNNER_APP") else "noop"

    if provider in {"noop", "none"}:
        return NoopProvisioner()

    if provider == "fly":
        runner_app = (os.environ.get("FLY_RUNNER_APP") or "").strip()
        if not runner_app:
            raise RuntimeError("FLY_RUNNER_APP is required when GUM_COMPUTE_PROVIDER=fly")
        flyctl = shutil.which("flyctl") or shutil.which("fly")
        if flyctl is None:
            raise RuntimeError("flyctl is required for Fly runner capacity sync")
        return FlyProvisioner(runner_app=runner_app, flyctl_bin=flyctl)

    raise RuntimeError(f"unsupported compute provider: {provider}")


def _job_cpu_cores(job: object) -> int:
    value = getattr(job, "cpu_cores", None)
    return value if isinstance(value, int) and value > 0 else 1


def _job_memory_mb(job: object) -> int:
    value = getattr(job, "memory_mb", None)
    return value if isinstance(value, int) and value > 0 else 512


def _run(args: list[str], *, capture_output: bool = False) -> subprocess.CompletedProcess[str]:
    try:
        return subprocess.run(
            args,
            check=True,
            text=True,
            capture_output=capture_output,
        )
    except subprocess.CalledProcessError as error:
        stderr = (error.stderr or "").strip()
        stdout = (error.stdout or "").strip()
        detail = stderr or stdout or str(error)
        raise RuntimeError(detail) from error

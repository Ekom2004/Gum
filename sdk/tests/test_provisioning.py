from __future__ import annotations

import os
import sys
import unittest
from dataclasses import dataclass
from pathlib import Path
from unittest.mock import patch

if sys.version_info < (3, 10):
    raise unittest.SkipTest("gum sdk tests require Python 3.10+")

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "sdk"))

from gum.provisioning import (  # noqa: E402
    FlyProvisioner,
    NoopProvisioner,
    build_runner_capacity_plan,
    provisioner_from_env,
)


@dataclass
class _Job:
    cpu_cores: int | None
    memory_mb: int | None


class ProvisioningTests(unittest.TestCase):
    def test_build_runner_capacity_plan_uses_max_resource_and_parallelism(self) -> None:
        jobs = [_Job(cpu_cores=1, memory_mb=512), _Job(cpu_cores=2, memory_mb=2048)]
        plan = build_runner_capacity_plan(jobs, compute_class="standard", parallelism=3)
        self.assertEqual(plan.compute_class, "standard")
        self.assertEqual(plan.cpu_cores, 6)
        self.assertEqual(plan.memory_mb, 6144)
        self.assertEqual(plan.max_concurrent_leases, 3)

    def test_build_runner_capacity_plan_defaults_when_job_limits_missing(self) -> None:
        jobs = [_Job(cpu_cores=None, memory_mb=None)]
        plan = build_runner_capacity_plan(jobs, compute_class="standard", parallelism=1)
        self.assertEqual(plan.cpu_cores, 1)
        self.assertEqual(plan.memory_mb, 512)

    def test_provisioner_from_env_defaults_to_noop_without_provider_hints(self) -> None:
        old_provider = os.environ.pop("GUM_COMPUTE_PROVIDER", None)
        old_runner_app = os.environ.pop("FLY_RUNNER_APP", None)
        try:
            provisioner = provisioner_from_env()
            self.assertIsInstance(provisioner, NoopProvisioner)
        finally:
            if old_provider is not None:
                os.environ["GUM_COMPUTE_PROVIDER"] = old_provider
            if old_runner_app is not None:
                os.environ["FLY_RUNNER_APP"] = old_runner_app

    def test_provisioner_from_env_builds_fly_adapter(self) -> None:
        old_provider = os.environ.get("GUM_COMPUTE_PROVIDER")
        old_runner_app = os.environ.get("FLY_RUNNER_APP")
        os.environ["GUM_COMPUTE_PROVIDER"] = "fly"
        os.environ["FLY_RUNNER_APP"] = "gum-runner-stg"
        try:
            with patch("gum.provisioning.shutil.which", side_effect=["flyctl", None]):
                provisioner = provisioner_from_env()
                self.assertIsInstance(provisioner, FlyProvisioner)
        finally:
            if old_provider is None:
                os.environ.pop("GUM_COMPUTE_PROVIDER", None)
            else:
                os.environ["GUM_COMPUTE_PROVIDER"] = old_provider
            if old_runner_app is None:
                os.environ.pop("FLY_RUNNER_APP", None)
            else:
                os.environ["FLY_RUNNER_APP"] = old_runner_app

    def test_fly_provisioner_sync_executes_machine_updates(self) -> None:
        calls: list[list[str]] = []

        def fake_run(args, check, text, capture_output):  # type: ignore[no-untyped-def]
            calls.append(list(args))

            class Result:
                stdout = ""
                stderr = ""

            if args[:4] == ["flyctl", "machine", "list", "-a"]:
                Result.stdout = "m1\nm2\n"
            return Result()

        provisioner = FlyProvisioner(runner_app="gum-runner-stg", flyctl_bin="flyctl")
        with patch("gum.provisioning.subprocess.run", side_effect=fake_run):
            plan = build_runner_capacity_plan(
                [_Job(cpu_cores=2, memory_mb=2048)],
                compute_class="standard",
                parallelism=2,
            )
            provisioner.sync(plan)

        self.assertTrue(any(cmd[:4] == ["flyctl", "secrets", "set", "-a"] for cmd in calls))
        self.assertTrue(any(cmd[:4] == ["flyctl", "machine", "list", "-a"] for cmd in calls))
        update_calls = [cmd for cmd in calls if cmd[:3] == ["flyctl", "machine", "update"]]
        self.assertEqual(len(update_calls), 2)


if __name__ == "__main__":
    unittest.main()

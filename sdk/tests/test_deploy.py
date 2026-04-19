from __future__ import annotations

import sys
import tarfile
import tempfile
import textwrap
import unittest
from dataclasses import dataclass
from pathlib import Path

if sys.version_info < (3, 10):
    raise unittest.SkipTest("gum sdk tests require Python 3.10+")

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "sdk"))

from gum.client import DeployRef
from gum.deploy import DeployError, deploy_project, discover_jobs, package_project


@dataclass
class FakeClient:
    payload: dict | None = None

    def register_deploy(self, payload: dict) -> DeployRef:
        self.payload = payload
        return DeployRef(id="dep_test", registered_jobs=len(payload.get("jobs", [])))


class DeployDiscoveryTests(unittest.TestCase):
    def test_discover_jobs_extracts_policy_and_handler(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = Path(tmp_dir)
            (root / "pyproject.toml").write_text("[project]\nname='demo'\n", encoding="utf-8")
            (root / "jobs.py").write_text(
                textwrap.dedent(
                    """
                    import gum

                    @gum.job(every="20d", retries=5, timeout="5m", rate_limit="20/m", concurrency=5, key="event_id", compute="high-mem")
                    def send_followup():
                        return None
                    """
                ).strip()
                + "\n",
                encoding="utf-8",
            )

            jobs = discover_jobs(root)

        self.assertEqual(len(jobs), 1)
        self.assertEqual(jobs[0].id, "job_send_followup")
        self.assertEqual(jobs[0].handler_ref, "jobs:send_followup")
        self.assertEqual(jobs[0].schedule_expr, "20d")
        self.assertEqual(jobs[0].timeout_secs, 300)
        self.assertEqual(jobs[0].key_field, "event_id")
        self.assertEqual(jobs[0].compute_class, "high-mem")

    def test_package_project_creates_bundle(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = Path(tmp_dir)
            (root / "pyproject.toml").write_text("[project]\nname='demo'\n", encoding="utf-8")
            (root / "jobs.py").write_text("print('hello')\n", encoding="utf-8")

            bundle_path = package_project(root)

            self.assertTrue(bundle_path.exists())
            with tarfile.open(bundle_path, "r:gz") as archive:
                self.assertIn("jobs.py", archive.getnames())

    def test_missing_manifest_fails(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = Path(tmp_dir)
            with self.assertRaises(DeployError):
                package_project(root)

    def test_deploy_project_registers_bundle_and_jobs(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = Path(tmp_dir)
            (root / "pyproject.toml").write_text("[project]\nname='demo'\n", encoding="utf-8")
            (root / "jobs.py").write_text(
                textwrap.dedent(
                    """
                    import gum

                    @gum.job(retries=3, timeout="30s", rate_limit="10/m", concurrency=2, key="customer_id", compute="gpu")
                    def sync_signup(user_id: str):
                        return user_id
                    """
                ).strip()
                + "\n",
                encoding="utf-8",
            )
            client = FakeClient()

            result = deploy_project(root, client=client, project_id="proj_dev")
            self.assertTrue(result.bundle_path.exists())

        self.assertEqual(result.deploy.id, "dep_test")
        self.assertEqual(len(result.jobs), 1)
        self.assertIsNotNone(client.payload)
        assert client.payload is not None
        self.assertEqual(client.payload["project_id"], "proj_dev")
        self.assertEqual(client.payload["jobs"][0]["id"], "job_sync_signup")
        self.assertEqual(client.payload["jobs"][0]["key_field"], "customer_id")
        self.assertEqual(client.payload["jobs"][0]["compute_class"], "gpu")


if __name__ == "__main__":
    unittest.main()

from __future__ import annotations

import os
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
from gum.deploy import (
    DeployError,
    deploy_project,
    discover_jobs,
    init_project,
    load_project_config,
    package_project,
    resolve_api_base_url,
    resolve_project_id,
)


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

                    openai_limit = gum.rate_limit("60/m")

                    @gum.job(every="20d", retries=5, timeout="5m", rate_limit=openai_limit, concurrency=5, memory="2gb", key="event_id")
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
        self.assertEqual(jobs[0].rate_limit_spec, "openai_limit:60/m")
        self.assertEqual(jobs[0].memory_mb, 2048)
        self.assertEqual(jobs[0].key_field, "event_id")
        self.assertIsNone(jobs[0].compute_class)

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
            (root / "gum.toml").write_text(
                'project_id = "proj_file"\napi_base_url = "https://gum.example"\n',
                encoding="utf-8",
            )
            (root / "pyproject.toml").write_text("[project]\nname='demo'\n", encoding="utf-8")
            (root / "jobs.py").write_text(
                textwrap.dedent(
                    """
                    import gum

                    openai_limit = gum.rate_limit("60/m")

                    @gum.job(retries=3, timeout="30s", rate_limit=openai_limit, concurrency=2, memory="512mb", key="customer_id")
                    def sync_signup(user_id: str):
                        return user_id
                    """
                ).strip()
                + "\n",
                encoding="utf-8",
            )
            client = FakeClient()

            result = deploy_project(root, client=client)
            self.assertTrue(result.bundle_path.exists())

        self.assertEqual(result.deploy.id, "dep_test")
        self.assertEqual(result.project_id, "proj_file")
        self.assertEqual(result.api_base_url, "https://gum.example")
        self.assertEqual(len(result.jobs), 1)
        self.assertIsNotNone(client.payload)
        assert client.payload is not None
        self.assertEqual(client.payload["project_id"], "proj_file")
        self.assertEqual(client.payload["jobs"][0]["id"], "job_sync_signup")
        self.assertEqual(client.payload["jobs"][0]["rate_limit_spec"], "openai_limit:60/m")
        self.assertEqual(client.payload["jobs"][0]["memory_mb"], 512)
        self.assertEqual(client.payload["jobs"][0]["key_field"], "customer_id")
        self.assertIsNone(client.payload["jobs"][0]["compute_class"])

    def test_shared_rate_limit_pool_conflict_fails_discovery(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = Path(tmp_dir)
            (root / "pyproject.toml").write_text("[project]\nname='demo'\n", encoding="utf-8")
            (root / "summarize.py").write_text(
                textwrap.dedent(
                    """
                    import gum

                    openai_limit = gum.rate_limit("60/m")

                    @gum.job(rate_limit=openai_limit)
                    def summarize():
                        return None
                    """
                ).strip()
                + "\n",
                encoding="utf-8",
            )
            (root / "embed.py").write_text(
                textwrap.dedent(
                    """
                    import gum

                    openai_limit = gum.rate_limit("100/m")

                    @gum.job(rate_limit=openai_limit)
                    def embed():
                        return None
                    """
                ).strip()
                + "\n",
                encoding="utf-8",
            )

            with self.assertRaisesRegex(
                DeployError,
                'rate limit pool "openai_limit" has conflicting definitions: openai_limit:100/m and openai_limit:60/m',
            ):
                discover_jobs(root)

    def test_init_project_writes_cloud_project_files(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = Path(tmp_dir)

            result = init_project(
                root,
                project_id="proj_live",
                api_base_url="https://api.gum.example",
            )

            self.assertEqual(
                {path.name for path in result.created},
                {"gum.toml", ".env.example", "pyproject.toml", "jobs.py"},
            )
            self.assertEqual(result.kept, [])
            self.assertIn('project_id = "proj_live"', (root / "gum.toml").read_text(encoding="utf-8"))
            self.assertIn('GUM_API_KEY="gum_live_..."', (root / ".env.example").read_text(encoding="utf-8"))
            self.assertIn("@gum.job", (root / "jobs.py").read_text(encoding="utf-8"))

            second = init_project(root, project_id="proj_other")

            self.assertEqual(second.created, [])
            self.assertEqual(
                {path.name for path in second.kept},
                {"gum.toml", ".env.example", "pyproject.toml", "jobs.py"},
            )
            self.assertIn('project_id = "proj_live"', (root / "gum.toml").read_text(encoding="utf-8"))

    def test_project_config_resolution_prefers_explicit_env_then_file(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = Path(tmp_dir)
            (root / "gum.toml").write_text(
                'project_id = "proj_file"\napi_base_url = "https://file.example"\n',
                encoding="utf-8",
            )
            old_project_id = os.environ.pop("GUM_PROJECT_ID", None)
            old_api_base_url = os.environ.pop("GUM_API_BASE_URL", None)
            try:
                config = load_project_config(root)
                self.assertEqual(config.project_id, "proj_file")
                self.assertEqual(config.api_base_url, "https://file.example")
                self.assertEqual(resolve_project_id(root), "proj_file")
                self.assertEqual(resolve_api_base_url(root), "https://file.example")

                os.environ["GUM_PROJECT_ID"] = "proj_env"
                os.environ["GUM_API_BASE_URL"] = "https://env.example"
                self.assertEqual(resolve_project_id(root), "proj_env")
                self.assertEqual(resolve_api_base_url(root), "https://env.example")
                self.assertEqual(resolve_project_id(root, "proj_flag"), "proj_flag")
                self.assertEqual(resolve_api_base_url(root, "https://flag.example"), "https://flag.example")
            finally:
                if old_project_id is not None:
                    os.environ["GUM_PROJECT_ID"] = old_project_id
                else:
                    os.environ.pop("GUM_PROJECT_ID", None)
                if old_api_base_url is not None:
                    os.environ["GUM_API_BASE_URL"] = old_api_base_url
                else:
                    os.environ.pop("GUM_API_BASE_URL", None)


if __name__ == "__main__":
    unittest.main()

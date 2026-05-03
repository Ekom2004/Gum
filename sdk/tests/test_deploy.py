from __future__ import annotations

import os
import sys
import tarfile
import tempfile
import textwrap
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

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
import gum.deploy as gum_deploy
from gum.provisioning import RunnerCapacityPlan


class FakeClient:
    def __init__(self, *, existing_secrets: list[str] | None = None) -> None:
        self.payload: dict | None = None
        self.prepared_deploy_id: str | None = None
        self.secrets = FakeSecretsAPI(existing=existing_secrets or [])

    def register_deploy(self, payload: dict) -> DeployRef:
        self.payload = payload
        return DeployRef(id="dep_test", registered_jobs=len(payload.get("jobs", [])))

    def prepare_deploy_runtime(self, deploy_id: str):
        self.prepared_deploy_id = deploy_id
        return SimpleNamespace(id=deploy_id, status="warming")


class FakeSecretsAPI:
    def __init__(self, *, existing: list[str]) -> None:
        self._by_env: dict[str, set[str]] = {"prod": set(existing)}
        self.set_calls: list[tuple[str, str, str | None]] = []

    def set(self, name: str, value: str, *, environment: str | None = None):
        env = environment or "prod"
        self._by_env.setdefault(env, set()).add(name)
        self.set_calls.append((name, value, environment))
        return SimpleNamespace(name=name, environment=env)

    def list(self, *, environment: str | None = None):
        env = environment or "prod"
        return [SimpleNamespace(name=name) for name in sorted(self._by_env.get(env, set()))]


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

                    @gum.job(every="20d", retries=5, timeout="5m", rate_limit=openai_limit, concurrency=5, cpu=2, memory="2gb", key="event_id")
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
        self.assertEqual(jobs[0].cpu_cores, 2)
        self.assertEqual(jobs[0].memory_mb, 2048)
        self.assertEqual(jobs[0].key_field, "event_id")
        self.assertIsNone(jobs[0].compute_class)

    def test_discover_jobs_supports_compute_class(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = Path(tmp_dir)
            (root / "pyproject.toml").write_text("[project]\nname='demo'\n", encoding="utf-8")
            (root / "jobs.py").write_text(
                textwrap.dedent(
                    """
                    import gum

                    @gum.job(compute_class="gpu")
                    def train_model():
                        return None
                    """
                ).strip()
                + "\n",
                encoding="utf-8",
            )

            jobs = discover_jobs(root)

        self.assertEqual(len(jobs), 1)
        self.assertEqual(jobs[0].compute_class, "gpu")

    def test_discover_jobs_rejects_compute_alias_conflict(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = Path(tmp_dir)
            (root / "pyproject.toml").write_text("[project]\nname='demo'\n", encoding="utf-8")
            (root / "jobs.py").write_text(
                textwrap.dedent(
                    """
                    import gum

                    @gum.job(compute="standard", compute_class="gpu")
                    def conflict():
                        return None
                    """
                ).strip()
                + "\n",
                encoding="utf-8",
            )

            with self.assertRaisesRegex(DeployError, "only one of compute_class or compute may be set"):
                discover_jobs(root)

    def test_discover_jobs_supports_cron_schedule(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = Path(tmp_dir)
            (root / "pyproject.toml").write_text("[project]\nname='demo'\n", encoding="utf-8")
            (root / "jobs.py").write_text(
                textwrap.dedent(
                    """
                    import gum

                    @gum.job(cron="*/15 * * * *", timezone="America/New_York", retries=1, timeout="30s")
                    def refresh_index():
                        return None
                    """
                ).strip()
                + "\n",
                encoding="utf-8",
            )

            jobs = discover_jobs(root)

        self.assertEqual(len(jobs), 1)
        self.assertEqual(jobs[0].trigger_mode, "schedule")
        self.assertEqual(jobs[0].schedule_expr, "cron:tz=America/New_York;*/15 * * * *")

    def test_discover_jobs_rejects_every_and_cron_together(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = Path(tmp_dir)
            (root / "pyproject.toml").write_text("[project]\nname='demo'\n", encoding="utf-8")
            (root / "jobs.py").write_text(
                textwrap.dedent(
                    """
                    import gum

                    @gum.job(every="5m", cron="*/5 * * * *")
                    def invalid_schedule():
                        return None
                    """
                ).strip()
                + "\n",
                encoding="utf-8",
            )

            with self.assertRaisesRegex(DeployError, "only one of every or cron may be set"):
                discover_jobs(root)

    def test_discover_jobs_rejects_timezone_without_cron(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = Path(tmp_dir)
            (root / "pyproject.toml").write_text("[project]\nname='demo'\n", encoding="utf-8")
            (root / "jobs.py").write_text(
                textwrap.dedent(
                    """
                    import gum

                    @gum.job(timezone="America/New_York")
                    def invalid_timezone():
                        return None
                    """
                ).strip()
                + "\n",
                encoding="utf-8",
            )

            with self.assertRaisesRegex(DeployError, "timezone requires cron"):
                discover_jobs(root)

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

    def test_deploy_requests_remote_runtime_prepare_when_enabled(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = Path(tmp_dir)
            (root / "pyproject.toml").write_text(
                "[project]\nname='demo'\ndependencies=['usegum']\n",
                encoding="utf-8",
            )
            (root / "uv.lock").write_text("version = 1\n", encoding="utf-8")
            (root / "jobs.py").write_text(
                textwrap.dedent(
                    """
                    import gum

                    @gum.job()
                    def hello():
                        return None
                    """
                ).strip()
                + "\n",
                encoding="utf-8",
            )
            client = FakeClient()
            with patch.dict(os.environ, {"GUM_PREWARM_RUNTIME": "1"}, clear=False):
                deploy_project(root, client=client, project_id="proj_test")
            self.assertEqual(client.prepared_deploy_id, "dep_test")

    def test_deploy_project_registers_bundle_and_jobs(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = Path(tmp_dir)
            (root / "gum.toml").write_text(
                'project_id = "proj_file"\napi_base_url = "https://gum.example"\n',
                encoding="utf-8",
            )
            (root / "pyproject.toml").write_text(
                "[project]\nname='demo'\ndependencies=['usegum']\n",
                encoding="utf-8",
            )
            (root / "jobs.py").write_text(
                textwrap.dedent(
                    """
                    import gum

                    openai_limit = gum.rate_limit("60/m")

                    @gum.job(retries=3, timeout="30s", rate_limit=openai_limit, concurrency=2, cpu=1, memory="512mb", key="customer_id")
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
        self.assertEqual(client.payload["jobs"][0]["cpu_cores"], 1)
        self.assertEqual(client.payload["jobs"][0]["memory_mb"], 512)
        self.assertEqual(client.payload["jobs"][0]["key_field"], "customer_id")
        self.assertIsNone(client.payload["jobs"][0]["compute_class"])
        self.assertEqual(client.payload["python_version"], "3.11")
        self.assertIsNone(client.payload["deps_mode"])
        self.assertIsNone(client.payload["deps_hash"])

    def test_deploy_project_auto_syncs_fly_runner_capacity_when_configured(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = Path(tmp_dir)
            (root / "gum.toml").write_text(
                'project_id = "proj_file"\napi_base_url = "https://gum.example"\n',
                encoding="utf-8",
            )
            (root / "pyproject.toml").write_text(
                "[project]\nname='demo'\ndependencies=['usegum']\n",
                encoding="utf-8",
            )
            (root / "jobs.py").write_text(
                textwrap.dedent(
                    """
                    import gum

                    @gum.job(cpu=2, memory="2gb")
                    def process():
                        return None
                    """
                ).strip()
                + "\n",
                encoding="utf-8",
            )
            client = FakeClient()
            plans: list[RunnerCapacityPlan] = []

            class FakeProvisioner:
                name = "fly"

                def sync(self, plan: RunnerCapacityPlan) -> None:
                    plans.append(plan)

            old_runner_app = os.environ.get("FLY_RUNNER_APP")
            old_parallelism = os.environ.get("GUM_RUNNER_PARALLELISM")
            old_auto_sync = os.environ.get("GUM_AUTO_SYNC_RUNNER_CAPACITY")
            os.environ["FLY_RUNNER_APP"] = "gum-runner-stg"
            os.environ["GUM_RUNNER_PARALLELISM"] = "2"
            os.environ["GUM_AUTO_SYNC_RUNNER_CAPACITY"] = "1"
            try:
                with patch.object(gum_deploy, "provisioner_from_env", return_value=FakeProvisioner()):
                    deploy_project(root, client=client)
            finally:
                if old_runner_app is None:
                    os.environ.pop("FLY_RUNNER_APP", None)
                else:
                    os.environ["FLY_RUNNER_APP"] = old_runner_app
                if old_parallelism is None:
                    os.environ.pop("GUM_RUNNER_PARALLELISM", None)
                else:
                    os.environ["GUM_RUNNER_PARALLELISM"] = old_parallelism
                if old_auto_sync is None:
                    os.environ.pop("GUM_AUTO_SYNC_RUNNER_CAPACITY", None)
                else:
                    os.environ["GUM_AUTO_SYNC_RUNNER_CAPACITY"] = old_auto_sync

        self.assertEqual(len(plans), 1)
        self.assertEqual(plans[0].cpu_cores, 4)  # cpu=2 * parallelism=2
        self.assertEqual(plans[0].memory_mb, 4096)  # memory=2gb * parallelism=2
        self.assertEqual(plans[0].max_concurrent_leases, 2)

    def test_deploy_project_fails_when_provider_is_invalid_and_auto_sync_explicit(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = Path(tmp_dir)
            (root / "pyproject.toml").write_text(
                "[project]\nname='demo'\ndependencies=['usegum']\n",
                encoding="utf-8",
            )
            (root / "jobs.py").write_text(
                textwrap.dedent(
                    """
                    import gum

                    @gum.job()
                    def process():
                        return None
                    """
                ).strip()
                + "\n",
                encoding="utf-8",
            )
            client = FakeClient()
            old_provider = os.environ.get("GUM_COMPUTE_PROVIDER")
            old_auto_sync = os.environ.get("GUM_AUTO_SYNC_RUNNER_CAPACITY")
            os.environ["GUM_COMPUTE_PROVIDER"] = "unknown"
            os.environ["GUM_AUTO_SYNC_RUNNER_CAPACITY"] = "1"
            try:
                with self.assertRaisesRegex(DeployError, "unsupported compute provider"):
                    deploy_project(root, client=client)
            finally:
                if old_provider is None:
                    os.environ.pop("GUM_COMPUTE_PROVIDER", None)
                else:
                    os.environ["GUM_COMPUTE_PROVIDER"] = old_provider
                if old_auto_sync is None:
                    os.environ.pop("GUM_AUTO_SYNC_RUNNER_CAPACITY", None)
                else:
                    os.environ["GUM_AUTO_SYNC_RUNNER_CAPACITY"] = old_auto_sync

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

    def test_deploy_prompts_for_missing_resend_secret_in_tty(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = Path(tmp_dir)
            (root / "pyproject.toml").write_text(
                "[project]\nname='demo'\ndependencies=['usegum','resend']\n",
                encoding="utf-8",
            )
            (root / "jobs.py").write_text(
                textwrap.dedent(
                    """
                    import gum
                    import resend

                    @gum.job()
                    def send_email():
                        return None
                    """
                ).strip()
                + "\n",
                encoding="utf-8",
            )
            client = FakeClient()
            with patch.object(gum_deploy, "_stdin_is_tty", return_value=True), patch.object(
                gum_deploy.getpass, "getpass", return_value="re_test_123"
            ):
                deploy_project(root, client=client)

        self.assertEqual(client.secrets.set_calls, [("RESEND_API_KEY", "re_test_123", "prod")])

    def test_deploy_fails_when_missing_secret_non_interactive(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = Path(tmp_dir)
            (root / "pyproject.toml").write_text(
                "[project]\nname='demo'\ndependencies=['usegum','resend']\n",
                encoding="utf-8",
            )
            (root / "jobs.py").write_text(
                textwrap.dedent(
                    """
                    import gum
                    import resend

                    @gum.job()
                    def send_email():
                        return None
                    """
                ).strip()
                + "\n",
                encoding="utf-8",
            )
            client = FakeClient()
            with patch.object(gum_deploy, "_stdin_is_tty", return_value=False):
                with self.assertRaisesRegex(DeployError, "missing required secrets"):
                    deploy_project(root, client=client)

    def test_deploy_fails_when_imported_dependency_is_missing_from_pyproject(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = Path(tmp_dir)
            (root / "pyproject.toml").write_text("[project]\nname='demo'\n", encoding="utf-8")
            (root / "jobs.py").write_text(
                textwrap.dedent(
                    """
                    import gum
                    import resend

                    @gum.job()
                    def send_email():
                        return None
                    """
                ).strip()
                + "\n",
                encoding="utf-8",
            )
            client = FakeClient()

            with self.assertRaisesRegex(
                DeployError,
                r"jobs\.py imports resend but pyproject\.toml does not include resend",
            ):
                deploy_project(root, client=client)


if __name__ == "__main__":
    unittest.main()

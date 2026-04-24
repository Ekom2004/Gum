from __future__ import annotations

import io
import os
import sys
import tempfile
import unittest
from contextlib import redirect_stdout, redirect_stderr
from pathlib import Path

if sys.version_info < (3, 10):
    raise unittest.SkipTest("gum sdk tests require Python 3.10+")

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "sdk"))

import gum.cli as gum_cli
import gum.deploy as gum_deploy
from gum.client import DeployRef, GumAPIError, LeaseStatus, LogLine, RunRecord, RunRef, RunnerStatus
from gum.deploy import DeployResult, DiscoveredJob


class _FakeRunsAPI:
    def __init__(self) -> None:
        self.cancelled: list[str] = []
        self.replayed: list[str] = []
        self.logs_requested: list[str] = []
        self.get_requested: list[str] = []
        self._run = RunRecord(
            id="run_123",
            job_id="job_export",
            status="running",
            attempt=1,
            trigger_type="enqueue",
            failure_reason=None,
            replay_of=None,
        )
        self._logs = [
            LogLine(attempt_id="att_1", stream="stdout", message="starting export"),
            LogLine(attempt_id="att_1", stream="stdout", message="building csv"),
        ]

    def get(self, run_id: str) -> RunRecord:
        self.get_requested.append(run_id)
        return self._run

    def replay(self, run_id: str) -> RunRef:
        self.replayed.append(run_id)
        return RunRef(id="run_999", status="queued")

    def cancel(self, run_id: str) -> RunRef:
        self.cancelled.append(run_id)
        return RunRef(id=run_id, status="canceled")

    def logs(self, run_id: str) -> list[LogLine]:
        self.logs_requested.append(run_id)
        return list(self._logs)

    def list(self) -> list[RunRecord]:
        return [self._run]


class _FakeClient:
    def __init__(self) -> None:
        self.runs = _FakeRunsAPI()
        self.admin = _FakeAdminAPI()


class _FakeAdminAPI:
    def runners(self) -> list[RunnerStatus]:
        return [
            RunnerStatus(
                id="runner_1",
                compute_class="high-mem",
                cpu_cores=4,
                memory_mb=4096,
                active_cpu_cores=2,
                active_memory_mb=2048,
                max_concurrent_leases=2,
                last_heartbeat_at_epoch_ms=123456,
                active_lease_count=1,
            )
        ]

    def leases(self) -> list[LeaseStatus]:
        return [
            LeaseStatus(
                lease_id="lease_1",
                run_id="run_123",
                attempt_id="att_1",
                runner_id="runner_1",
                expires_at_epoch_ms=999999,
                cancel_requested=False,
            )
        ]


class CliTests(unittest.TestCase):
    def setUp(self) -> None:
        self.client = _FakeClient()
        self._old_default_client = gum_cli.default_client
        self._old_store_admin_key = gum_cli.store_admin_key
        self._old_clear_admin_key = gum_cli.clear_admin_key
        self._old_load_admin_key = gum_cli.load_admin_key
        self._old_default_admin_key = gum_cli.default_admin_key
        self._old_getpass = gum_cli.getpass.getpass
        self._old_deploy_project = gum_deploy.deploy_project
        gum_cli.default_client = lambda: self.client
        gum_cli.default_admin_key = lambda: None

    def tearDown(self) -> None:
        gum_cli.default_client = self._old_default_client
        gum_cli.store_admin_key = self._old_store_admin_key
        gum_cli.clear_admin_key = self._old_clear_admin_key
        gum_cli.load_admin_key = self._old_load_admin_key
        gum_cli.default_admin_key = self._old_default_admin_key
        gum_cli.getpass.getpass = self._old_getpass
        gum_deploy.deploy_project = self._old_deploy_project

    def test_get_command_prints_run_details(self) -> None:
        stdout = io.StringIO()
        with redirect_stdout(stdout):
            exit_code = gum_cli.main(["get", "run_123"])
        self.assertEqual(exit_code, 0)
        output = stdout.getvalue()
        self.assertIn("Run:      run_123", output)
        self.assertIn("Status:   running", output)

    def test_list_command_prints_recent_runs(self) -> None:
        gum_cli.load_admin_key = lambda _: "admin-secret"
        gum_cli.getpass.getpass = lambda _: "passphrase"
        stdout = io.StringIO()
        with redirect_stdout(stdout):
            exit_code = gum_cli.main(["list"])
        self.assertEqual(exit_code, 0)
        output = stdout.getvalue()
        self.assertIn("STATUS", output)
        self.assertIn("run_123", output)

    def test_logs_command_prints_log_lines(self) -> None:
        stdout = io.StringIO()
        with redirect_stdout(stdout):
            exit_code = gum_cli.main(["logs", "run_123"])
        self.assertEqual(exit_code, 0)
        output = stdout.getvalue()
        self.assertIn("[stdout] starting export", output)
        self.assertIn("[stdout] building csv", output)

    def test_cancel_command_calls_api(self) -> None:
        stdout = io.StringIO()
        with redirect_stdout(stdout):
            exit_code = gum_cli.main(["cancel", "run_123"])
        self.assertEqual(exit_code, 0)
        self.assertEqual(self.client.runs.cancelled, ["run_123"])
        self.assertIn("Canceled run_123 (canceled)", stdout.getvalue())

    def test_live_once_renders_live_frame(self) -> None:
        stdout = io.StringIO()
        with redirect_stdout(stdout):
            exit_code = gum_cli.main(["live", "run_123", "--once", "--lines", "1"])
        self.assertEqual(exit_code, 0)
        output = stdout.getvalue()
        self.assertIn("GUM LIVE", output)
        self.assertIn("run:      run_123", output)
        self.assertIn("[stdout] building csv", output)

    def test_live_without_run_id_renders_admin_dashboard(self) -> None:
        gum_cli.load_admin_key = lambda _: "admin-secret"
        gum_cli.getpass.getpass = lambda _: "passphrase"
        stdout = io.StringIO()
        with redirect_stdout(stdout):
            exit_code = gum_cli.main(["live", "--once"])
        self.assertEqual(exit_code, 0)
        output = stdout.getvalue()
        self.assertIn("GUM ADMIN", output)
        self.assertIn("runner_1", output)
        self.assertIn("lease_1", output)

    def test_api_errors_go_to_stderr(self) -> None:
        def boom(_: str) -> RunRecord:
            raise GumAPIError("GET /v1/runs/run_123 failed")

        self.client.runs.get = boom  # type: ignore[method-assign]
        stdout = io.StringIO()
        stderr = io.StringIO()
        with redirect_stdout(stdout), redirect_stderr(stderr):
            exit_code = gum_cli.main(["get", "run_123"])
        self.assertEqual(exit_code, 1)
        self.assertIn("GET /v1/runs/run_123 failed", stderr.getvalue())

    def test_admin_login_stores_encrypted_credentials(self) -> None:
        stored: list[tuple[str, str]] = []
        gum_cli.store_admin_key = lambda admin_key, passphrase: stored.append((admin_key, passphrase))
        prompts = iter(["real-admin-key", "1234", "1234"])
        gum_cli.getpass.getpass = lambda _: next(prompts)
        stdout = io.StringIO()
        with redirect_stdout(stdout):
            exit_code = gum_cli.main(["admin", "login"])
        self.assertEqual(exit_code, 0)
        self.assertEqual(stored, [("real-admin-key", "1234")])
        self.assertIn("Stored admin credentials for Gum.", stdout.getvalue())

    def test_admin_logout_clears_stored_credentials(self) -> None:
        gum_cli.clear_admin_key = lambda: True
        stdout = io.StringIO()
        with redirect_stdout(stdout):
            exit_code = gum_cli.main(["admin", "logout"])
        self.assertEqual(exit_code, 0)
        self.assertIn("Cleared stored admin credentials.", stdout.getvalue())

    def test_admin_dashboard_unlocks_before_requesting_admin_data(self) -> None:
        gum_cli.load_admin_key = lambda passphrase: "admin-secret" if passphrase == "1234" else ""
        gum_cli.getpass.getpass = lambda _: "1234"
        stdout = io.StringIO()
        with redirect_stdout(stdout):
            exit_code = gum_cli.main(["admin", "--once"])
        self.assertEqual(exit_code, 0)
        output = stdout.getvalue()
        self.assertIn("GUM ADMIN", output)
        self.assertIn("runner_1", output)

    def test_admin_runs_list_uses_unlocked_admin_key(self) -> None:
        gum_cli.load_admin_key = lambda _: "admin-secret"
        gum_cli.getpass.getpass = lambda _: "1234"
        stdout = io.StringIO()
        with redirect_stdout(stdout):
            exit_code = gum_cli.main(["admin", "runs", "list"])
        self.assertEqual(exit_code, 0)
        self.assertIn("run_123", stdout.getvalue())

    def test_admin_runs_list_uses_env_admin_key_without_unlock_prompt(self) -> None:
        gum_cli.default_admin_key = lambda: "env-admin-secret"
        gum_cli.load_admin_key = lambda _: (_ for _ in ()).throw(
            AssertionError("load_admin_key should not be called when env key exists")
        )
        stdout = io.StringIO()
        with redirect_stdout(stdout):
            exit_code = gum_cli.main(["admin", "runs", "list"])
        self.assertEqual(exit_code, 0)
        self.assertIn("run_123", stdout.getvalue())

    def test_admin_runs_list_fails_with_missing_stored_credentials(self) -> None:
        gum_cli.load_admin_key = lambda _: (_ for _ in ()).throw(
            gum_cli.AdminAuthError("no stored admin credentials; run `gum admin login` first")
        )
        gum_cli.getpass.getpass = lambda _: "1234"
        stdout = io.StringIO()
        stderr = io.StringIO()
        with redirect_stdout(stdout), redirect_stderr(stderr):
            exit_code = gum_cli.main(["admin", "runs", "list"])
        self.assertEqual(exit_code, 1)
        self.assertIn("no stored admin credentials", stderr.getvalue())
        self.assertNotIn("admin-secret", stderr.getvalue())

    def test_admin_runs_list_fails_with_invalid_passphrase(self) -> None:
        gum_cli.load_admin_key = lambda _: (_ for _ in ()).throw(
            gum_cli.AdminAuthError("invalid passphrase")
        )
        gum_cli.getpass.getpass = lambda _: "wrong-passphrase"
        stdout = io.StringIO()
        stderr = io.StringIO()
        with redirect_stdout(stdout), redirect_stderr(stderr):
            exit_code = gum_cli.main(["admin", "runs", "list"])
        self.assertEqual(exit_code, 1)
        self.assertIn("invalid passphrase", stderr.getvalue())
        self.assertNotIn("wrong-passphrase", stderr.getvalue())

    def test_filter_runs_matches_job_id_and_status(self) -> None:
        filtered = gum_cli.filter_runs([self.client.runs._run], "export")
        self.assertEqual(len(filtered), 1)
        filtered = gum_cli.filter_runs([self.client.runs._run], "running")
        self.assertEqual(len(filtered), 1)
        filtered = gum_cli.filter_runs([self.client.runs._run], "missing")
        self.assertEqual(filtered, [])

    def test_render_view_tabs_marks_active_view(self) -> None:
        tabs = gum_cli.render_view_tabs("runners")
        self.assertIn("[2:runners]", tabs)
        self.assertIn("1:runs", tabs)
        self.assertIn("3:leases", tabs)

    def test_render_runs_panel_uses_status_symbol_for_selected_run(self) -> None:
        lines = gum_cli.render_runs_panel([self.client.runs._run], 0)
        self.assertIn("> ●", lines[1])

    def test_init_command_writes_project_files(self) -> None:
        old_cwd = os.getcwd()
        with tempfile.TemporaryDirectory() as tmp_dir:
            os.chdir(tmp_dir)
            stdout = io.StringIO()
            try:
                with redirect_stdout(stdout):
                    exit_code = gum_cli.main(
                        [
                            "init",
                            "--project-id",
                            "proj_live",
                            "--api-base-url",
                            "https://api.gum.example",
                        ]
                    )
            finally:
                os.chdir(old_cwd)

            root = Path(tmp_dir)
            self.assertEqual(exit_code, 0)
            self.assertTrue((root / "gum.toml").exists())
            self.assertTrue((root / ".env.example").exists())
            self.assertIn("Gum init", stdout.getvalue())
            self.assertIn("gum deploy", stdout.getvalue())

    def test_deploy_command_prints_cloud_summary(self) -> None:
        calls: list[tuple[str | None, str | None]] = []

        def fake_deploy_project(*, project_id: str | None = None, api_base_url: str | None = None) -> DeployResult:
            calls.append((project_id, api_base_url))
            return DeployResult(
                project_root=Path("/tmp/demo"),
                project_id=project_id or "proj_live",
                api_base_url=api_base_url or "https://api.gum.example",
                bundle_path=Path("/tmp/bundle.tar.gz"),
                jobs=[
                    DiscoveredJob(
                        id="job_sync_customer",
                        name="sync_customer",
                        handler_ref="jobs:sync_customer",
                        trigger_mode="manual",
                        schedule_expr=None,
                        retries=3,
                        timeout_secs=300,
                        rate_limit_spec="openai_limit:60/m",
                        concurrency_limit=5,
                        cpu_cores=2,
                        memory_mb=1024,
                        key_field="customer_id",
                        compute_class=None,
                        module_path="jobs.py",
                    )
                ],
                deploy=DeployRef(id="dep_test", registered_jobs=1),
            )

        gum_deploy.deploy_project = fake_deploy_project
        stdout = io.StringIO()
        with redirect_stdout(stdout):
            exit_code = gum_cli.main(
                [
                    "deploy",
                    "--project-id",
                    "proj_live",
                    "--api-base-url",
                    "https://api.gum.example",
                ]
            )

        self.assertEqual(exit_code, 0)
        self.assertEqual(calls, [("proj_live", "https://api.gum.example")])
        output = stdout.getvalue()
        self.assertIn("Gum deploy", output)
        self.assertIn("Project: proj_live", output)
        self.assertIn("sync_customer [manual, retries=3, timeout=300s", output)
        self.assertIn("cpu=2", output)
        self.assertIn("memory=1024mb", output)
        self.assertIn("Deploy:   dep_test", output)


if __name__ == "__main__":
    unittest.main()

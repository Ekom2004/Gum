from __future__ import annotations

import io
import sys
import unittest
from contextlib import redirect_stdout, redirect_stderr
from pathlib import Path

if sys.version_info < (3, 10):
    raise unittest.SkipTest("gum sdk tests require Python 3.10+")

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "sdk"))

import gum.cli as gum_cli
from gum.client import GumAPIError, LeaseStatus, LogLine, RunRecord, RunRef, RunnerStatus


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
        gum_cli.default_client = lambda: self.client

    def tearDown(self) -> None:
        gum_cli.default_client = self._old_default_client

    def test_get_command_prints_run_details(self) -> None:
        stdout = io.StringIO()
        with redirect_stdout(stdout):
            exit_code = gum_cli.main(["get", "run_123"])
        self.assertEqual(exit_code, 0)
        output = stdout.getvalue()
        self.assertIn("Run:      run_123", output)
        self.assertIn("Status:   running", output)

    def test_list_command_prints_recent_runs(self) -> None:
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
        stdout = io.StringIO()
        with redirect_stdout(stdout):
            exit_code = gum_cli.main(["live", "--once"])
        self.assertEqual(exit_code, 0)
        output = stdout.getvalue()
        self.assertIn("GUM LIVE", output)
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


if __name__ == "__main__":
    unittest.main()

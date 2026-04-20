from __future__ import annotations

import sys
import types
import unittest
from pathlib import Path

if sys.version_info < (3, 10):
    raise unittest.SkipTest("gum sdk tests require Python 3.10+")

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "sdk"))

from gum.runtime import _classify_exception, _context, main


class _HttpError(Exception):
    def __init__(self, status_code: int) -> None:
        super().__init__(f"http {status_code}")
        self.status_code = status_code


class RuntimeClassificationTests(unittest.TestCase):
    def test_http_429_is_provider_rate_limit(self) -> None:
        self.assertEqual(_classify_exception(_HttpError(429)), "provider_429")

    def test_timeout_maps_to_provider_timeout(self) -> None:
        self.assertEqual(_classify_exception(TimeoutError("slow upstream")), "provider_timeout")

    def test_connection_error_maps_to_provider_connect_error(self) -> None:
        self.assertEqual(
            _classify_exception(ConnectionError("connection refused")),
            "provider_connect_error",
        )

    def test_generic_value_error_is_user_code_error(self) -> None:
        self.assertEqual(_classify_exception(ValueError("bad payload")), "user_code_error")


class RuntimeContextTests(unittest.TestCase):
    def setUp(self) -> None:
        self.module = types.ModuleType("gum_test_runtime_handlers")
        self.module.captured = None

        def sync_handler(name: str) -> None:
            current = _context()
            self.module.captured = {
                "name": name,
                "run_id": current.run_id,
                "attempt_id": current.attempt_id,
                "job_id": current.job_id,
                "key": current.key,
                "replay_of": current.replay_of,
            }

        async def async_handler(name: str) -> None:
            current = _context()
            self.module.captured = {
                "name": name,
                "run_id": current.run_id,
                "attempt_id": current.attempt_id,
                "job_id": current.job_id,
                "key": current.key,
                "replay_of": current.replay_of,
                "async": True,
            }

        self.module.sync_handler = sync_handler
        self.module.async_handler = async_handler
        sys.modules[self.module.__name__] = self.module
        self.addCleanup(sys.modules.pop, self.module.__name__, None)

    def test_context_is_available_inside_sync_handler(self) -> None:
        exit_code = main(
            [
                "--handler",
                "gum_test_runtime_handlers:sync_handler",
                "--payload-json",
                '{"name": "Ada"}',
                "--run-id",
                "run_123",
                "--attempt-id",
                "att_123",
                "--job-id",
                "job_sync",
                "--key",
                "evt_123",
                "--replay-of",
                "run_122",
            ]
        )

        self.assertEqual(exit_code, 0)
        self.assertEqual(
            self.module.captured,
            {
                "name": "Ada",
                "run_id": "run_123",
                "attempt_id": "att_123",
                "job_id": "job_sync",
                "key": "evt_123",
                "replay_of": "run_122",
            },
        )

    def test_context_is_available_inside_async_handler(self) -> None:
        exit_code = main(
            [
                "--handler",
                "gum_test_runtime_handlers:async_handler",
                "--payload-json",
                '{"name": "Grace"}',
                "--run-id",
                "run_async",
                "--attempt-id",
                "att_async",
                "--job-id",
                "job_async",
            ]
        )

        self.assertEqual(exit_code, 0)
        self.assertEqual(
            self.module.captured,
            {
                "name": "Grace",
                "run_id": "run_async",
                "attempt_id": "att_async",
                "job_id": "job_async",
                "key": None,
                "replay_of": None,
                "async": True,
            },
        )

    def test_context_is_cleared_after_handler_returns(self) -> None:
        exit_code = main(
            [
                "--handler",
                "gum_test_runtime_handlers:sync_handler",
                "--payload-json",
                '{"name": "Linus"}',
                "--run-id",
                "run_reset",
                "--attempt-id",
                "att_reset",
                "--job-id",
                "job_reset",
            ]
        )

        self.assertEqual(exit_code, 0)
        with self.assertRaisesRegex(RuntimeError, "only available while a Gum job is running"):
            _context()


if __name__ == "__main__":
    unittest.main()

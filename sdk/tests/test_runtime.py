from __future__ import annotations

import sys
import unittest
from pathlib import Path

if sys.version_info < (3, 10):
    raise unittest.SkipTest("gum sdk tests require Python 3.10+")

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "sdk"))

from gum.runtime import _classify_exception


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


if __name__ == "__main__":
    unittest.main()

from __future__ import annotations

import os
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

if sys.version_info < (3, 10):
    raise unittest.SkipTest("gum sdk tests require Python 3.10+")

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "sdk"))

from gum.auth import clear_admin_key, load_admin_key, store_admin_key


@unittest.skipIf(shutil.which("openssl") is None, "openssl is required for auth storage tests")
class AdminAuthTests(unittest.TestCase):
    def test_store_load_and_clear_admin_key(self) -> None:
        original_home = os.environ.get("GUM_HOME")
        with tempfile.TemporaryDirectory() as tmpdir:
            os.environ["GUM_HOME"] = tmpdir
            store_admin_key("admin-secret", "1234")
            self.assertEqual(load_admin_key("1234"), "admin-secret")
            self.assertTrue(clear_admin_key())
            self.assertFalse(clear_admin_key())
        if original_home is None:
            os.environ.pop("GUM_HOME", None)
        else:
            os.environ["GUM_HOME"] = original_home

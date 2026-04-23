from __future__ import annotations

import re
import sys
import unittest
from pathlib import Path

if sys.version_info < (3, 10):
    raise unittest.SkipTest("gum sdk tests require Python 3.10+")


class SecretsHygieneTests(unittest.TestCase):
    def test_docs_and_examples_do_not_embed_live_secrets(self) -> None:
        root = Path(__file__).resolve().parents[2]
        scan_roots = [
            root / "docs",
            root / "docs-site",
            root / "website" / "src",
            root / "sdk" / "gum",
        ]
        extensions = {".md", ".mdx", ".py", ".ts", ".tsx", ".js", ".jsx", ".toml", ".yml", ".yaml", ".json"}
        secret_patterns = [
            re.compile(r"\bgum_live_[A-Za-z0-9]{8,}\b"),
            re.compile(r"\bgum_admin_[A-Za-z0-9]{8,}\b"),
            re.compile(r"\bsk-[A-Za-z0-9]{20,}\b"),
        ]

        findings: list[str] = []
        for scan_root in scan_roots:
            if not scan_root.exists():
                continue
            for path in scan_root.rglob("*"):
                if not path.is_file() or path.suffix.lower() not in extensions:
                    continue
                contents = path.read_text(encoding="utf-8", errors="replace")
                for pattern in secret_patterns:
                    match = pattern.search(contents)
                    if match:
                        relpath = path.relative_to(root)
                        findings.append(f"{relpath}: {match.group(0)}")

        self.assertEqual(
            findings,
            [],
            "Found possible embedded live secrets in docs/examples:\n" + "\n".join(findings),
        )

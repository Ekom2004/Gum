#!/usr/bin/env python3
"""Scrape YC startups for 2024-2026 from yc-oss public batch APIs.

Usage:
  python3 scripts/scrape_yc_2024_2026.py
  python3 scripts/scrape_yc_2024_2026.py --out-dir docs

Outputs:
  - yc_startups_2024_2026.csv
  - yc_startups_2024_2026.json
  - yc_startups_2024_2026_summary.md
"""

from __future__ import annotations

import argparse
import csv
import json
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable
from urllib.error import URLError, HTTPError
from urllib.request import Request, urlopen


@dataclass(frozen=True)
class BatchSpec:
    label: str
    slug: str
    expected_count: int


# Verified from yc-oss/api README metadata page (snapshot Feb 8, 2026).
BATCHES: list[BatchSpec] = [
    BatchSpec("Winter 2024", "winter-2024", 251),
    BatchSpec("Summer 2024", "summer-2024", 249),
    BatchSpec("Fall 2024", "fall-2024", 93),
    BatchSpec("Winter 2025", "winter-2025", 167),
    BatchSpec("Spring 2025", "spring-2025", 145),
    BatchSpec("Summer 2025", "summer-2025", 168),
    BatchSpec("Fall 2025", "fall-2025", 151),
    BatchSpec("Winter 2026", "winter-2026", 132),
    BatchSpec("Spring 2026", "spring-2026", 1),
    BatchSpec("Summer 2026", "summer-2026", 1),
]

BASE = "https://yc-oss.github.io/api/batches"


def fetch_json(url: str, timeout: int = 30) -> Any:
    req = Request(url, headers={"User-Agent": "gum-yc-scraper/1.0"})
    with urlopen(req, timeout=timeout) as resp:
        return json.loads(resp.read().decode("utf-8"))


def normalize_company(batch: str, item: dict[str, Any]) -> dict[str, Any]:
    return {
        "batch": batch,
        "name": item.get("name"),
        "slug": item.get("slug"),
        "yc_url": item.get("url"),
        "website": item.get("website"),
        "all_locations": item.get("all_locations"),
        "industry": item.get("industry"),
        "subindustry": item.get("subindustry"),
        "regions": "; ".join(item.get("regions") or []),
        "industries": "; ".join(item.get("industries") or []),
        "status": item.get("status"),
        "stage": item.get("stage"),
        "is_hiring": item.get("isHiring"),
        "team_size": item.get("team_size"),
        "nonprofit": item.get("nonprofit"),
    }


def write_csv(path: Path, rows: Iterable[dict[str, Any]]) -> None:
    rows = list(rows)
    if not rows:
        raise ValueError("No rows to write")
    fields = list(rows[0].keys())
    with path.open("w", newline="", encoding="utf-8") as f:
        w = csv.DictWriter(f, fieldnames=fields)
        w.writeheader()
        w.writerows(rows)


def write_summary(path: Path, batch_counts: list[tuple[str, int, int]]) -> None:
    total = sum(actual for _, _, actual in batch_counts)
    with path.open("w", encoding="utf-8") as f:
        f.write("# YC Startups 2024-2026\n\n")
        f.write("Source: https://yc-oss.github.io/api (batch endpoints)\n\n")
        f.write("| Batch | Expected | Fetched |\n")
        f.write("|---|---:|---:|\n")
        for label, expected, actual in batch_counts:
            f.write(f"| {label} | {expected} | {actual} |\n")
        f.write(f"\nTotal fetched: **{total}**\n")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out-dir", default="docs", help="Output directory (default: docs)")
    parser.add_argument("--sleep-ms", type=int, default=150, help="Sleep between requests")
    args = parser.parse_args()

    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    all_rows: list[dict[str, Any]] = []
    batch_counts: list[tuple[str, int, int]] = []

    for b in BATCHES:
        url = f"{BASE}/{b.slug}.json"
        try:
            payload = fetch_json(url)
        except (URLError, HTTPError, TimeoutError) as e:
            print(f"ERROR fetching {b.label} ({url}): {e}", file=sys.stderr)
            return 2

        if not isinstance(payload, list):
            print(f"ERROR: unexpected payload for {b.label}: {type(payload).__name__}", file=sys.stderr)
            return 2

        rows = [normalize_company(b.label, item) for item in payload if isinstance(item, dict)]
        all_rows.extend(rows)
        batch_counts.append((b.label, b.expected_count, len(rows)))
        time.sleep(args.sleep_ms / 1000)

    csv_path = out_dir / "yc_startups_2024_2026.csv"
    json_path = out_dir / "yc_startups_2024_2026.json"
    summary_path = out_dir / "yc_startups_2024_2026_summary.md"

    write_csv(csv_path, all_rows)
    with json_path.open("w", encoding="utf-8") as jf:
        json.dump(all_rows, jf, ensure_ascii=False, indent=2)
    write_summary(summary_path, batch_counts)

    print(f"Wrote {len(all_rows)} rows to {csv_path}")
    print(f"Wrote JSON to {json_path}")
    print(f"Wrote summary to {summary_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

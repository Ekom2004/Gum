# YC Startups 2024-2026 Batch Manifest

Primary source: `yc-oss/api` public batch endpoints (GitHub + yc-oss host).

## Batch endpoints

| Batch | Count (source snapshot) | Endpoint |
|---|---:|---|
| Winter 2024 | 251 | https://yc-oss.github.io/api/batches/winter-2024.json |
| Summer 2024 | 249 | https://yc-oss.github.io/api/batches/summer-2024.json |
| Fall 2024 | 93 | https://yc-oss.github.io/api/batches/fall-2024.json |
| Winter 2025 | 167 | https://yc-oss.github.io/api/batches/winter-2025.json |
| Spring 2025 | 145 | https://yc-oss.github.io/api/batches/spring-2025.json |
| Summer 2025 | 168 | https://yc-oss.github.io/api/batches/summer-2025.json |
| Fall 2025 | 151 | https://yc-oss.github.io/api/batches/fall-2025.json |
| Winter 2026 | 132 | https://yc-oss.github.io/api/batches/winter-2026.json |
| Spring 2026 | 1 | https://yc-oss.github.io/api/batches/spring-2026.json |
| Summer 2026 | 1 | https://yc-oss.github.io/api/batches/summer-2026.json |

Total expected rows from these endpoints: **1358**

## Scraper script

`python3 scripts/scrape_yc_2024_2026.py --out-dir docs`

This writes:

- `docs/yc_startups_2024_2026.csv`
- `docs/yc_startups_2024_2026.json`
- `docs/yc_startups_2024_2026_summary.md`

## Notes

- The terminal in this session cannot resolve external DNS, so I couldn’t execute the network fetch from here.
- The script is ready and uses the same verified endpoints above.

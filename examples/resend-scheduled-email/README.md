# Gum + Resend Scheduled Email (Customer Flow)

This example runs like a real customer:

1. `pip install usegum resend`
2. `gum login`
3. `gum init`
4. `gum deploy`
5. watch scheduled runs execute

## Quick start

```bash
python -m venv .venv
source .venv/bin/activate
pip install usegum resend
gum login
gum init --project-id proj_live
cp .env.example .env
```

Edit `.env`:

```bash
RESEND_API_KEY=re_xxx
RESEND_FROM=ops@yourdomain.com
RESEND_TO=you@yourdomain.com
```

Load env and deploy:

```bash
set -a
source .env
set +a
gum deploy
```

Inspect scheduled runs:

```bash
gum list --limit 20
gum get <run_id>
gum logs <run_id>
```

## Schedule choice

- Fast verification:
  - `cron="*/5 * * * *"` (every 5 minutes)
- Production weekly digest:
  - `cron="0 9 * * 1"` + `timezone="America/New_York"`

If timezone is omitted, Gum evaluates cron in UTC.

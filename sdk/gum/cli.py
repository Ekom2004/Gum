from __future__ import annotations

import argparse
import getpass
import sys
import time
from typing import Iterable

from .auth import AdminAuthError, clear_admin_key, default_admin_key, load_admin_key, store_admin_key
from .client import GumAPIError, GumClient, LeaseStatus, LogLine, RunRecord, RunnerStatus, default_client


TERMINAL_RUN_STATUSES = {"succeeded", "failed", "timed_out", "canceled"}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="gum")
    subparsers = parser.add_subparsers(dest="command", required=True)

    deploy_parser = subparsers.add_parser("deploy", help="package and register the current project")
    deploy_parser.add_argument("--project-id", default="proj_dev")

    list_parser = subparsers.add_parser("list", help="show recent runs")
    list_parser.add_argument("--limit", type=int, default=20)

    get_parser = subparsers.add_parser("get", help="show one run")
    get_parser.add_argument("run_id")

    logs_parser = subparsers.add_parser("logs", help="show logs for one run")
    logs_parser.add_argument("run_id")
    logs_parser.add_argument("--attempt")

    cancel_parser = subparsers.add_parser("cancel", help="cancel one run")
    cancel_parser.add_argument("run_id")

    replay_parser = subparsers.add_parser("replay", help="replay one run")
    replay_parser.add_argument("run_id")

    live_parser = subparsers.add_parser("live", help="live terminal dashboard")
    live_parser.add_argument("run_id", nargs="?")
    live_parser.add_argument("--interval", type=float, default=1.0)
    live_parser.add_argument("--lines", type=int, default=20)
    live_parser.add_argument("--once", action="store_true")

    admin_parser = subparsers.add_parser("admin", help="admin and support commands")
    admin_parser.add_argument("--interval", type=float, default=1.0)
    admin_parser.add_argument("--lines", type=int, default=20)
    admin_parser.add_argument("--once", action="store_true")
    admin_subparsers = admin_parser.add_subparsers(dest="admin_command")

    admin_subparsers.add_parser("login", help="store an encrypted admin key locally")
    admin_subparsers.add_parser("logout", help="remove stored admin credentials")

    admin_runs_parser = admin_subparsers.add_parser("runs", help="inspect runs")
    admin_runs_subparsers = admin_runs_parser.add_subparsers(dest="admin_runs_command", required=True)
    admin_runs_list_parser = admin_runs_subparsers.add_parser("list", help="show recent runs")
    admin_runs_list_parser.add_argument("--limit", type=int, default=20)
    admin_runs_get_parser = admin_runs_subparsers.add_parser("get", help="show one run")
    admin_runs_get_parser.add_argument("run_id")

    admin_logs_parser = admin_subparsers.add_parser("logs", help="show logs for one run")
    admin_logs_parser.add_argument("run_id")
    admin_logs_parser.add_argument("--attempt")

    admin_runners_parser = admin_subparsers.add_parser("runners", help="inspect runners")
    admin_runners_parser.add_subparsers(dest="admin_runners_command", required=True).add_parser(
        "list", help="show runners"
    )

    admin_leases_parser = admin_subparsers.add_parser("leases", help="inspect active leases")
    admin_leases_parser.add_subparsers(dest="admin_leases_command", required=True).add_parser(
        "list", help="show leases"
    )

    args = parser.parse_args(argv)

    if args.command == "deploy":
        from .deploy import DeployError, deploy_project

        try:
            result = deploy_project(project_id=args.project_id)
        except DeployError as exc:
            print(str(exc), file=sys.stderr)
            return 1

        print(f'Deploying project "{result.project_root.name}"...')
        print("")
        print(f"Found {len(result.jobs)} jobs:")
        for job in result.jobs:
            print(f"  - {job.name}")
        print("")
        print(f"Packaged bundle {result.bundle_path.name}")
        print(f"Registered deploy {result.deploy.id}")
        print(f"Activated deploy {result.deploy.id}")
        return 0

    client = default_client()

    try:
        if args.command == "list":
            admin_client = require_admin_client(client)
            runs = admin_client.runs.list()[: args.limit]
            print(render_run_table(runs))
            return 0

        if args.command == "get":
            run = client.runs.get(args.run_id)
            print(render_run_record(run))
            return 0

        if args.command == "logs":
            logs = client.runs.logs(args.run_id)
            print(render_logs(logs, attempt_id=args.attempt))
            return 0

        if args.command == "cancel":
            run = client.runs.cancel(args.run_id)
            print(f"Canceled {run.id} ({run.status})")
            return 0

        if args.command == "replay":
            run = client.runs.replay(args.run_id)
            print(f"Replayed {run.id} ({run.status})")
            return 0

        if args.command == "live":
            if args.run_id:
                return run_live_view(
                    client=client,
                    run_id=args.run_id,
                    interval_secs=args.interval,
                    log_lines=args.lines,
                    once=args.once,
                )
            admin_client = require_admin_client(client)
            return admin_live_view(
                client=admin_client,
                interval_secs=args.interval,
                once=args.once,
            )

        if args.command == "admin":
            return handle_admin_command(args, client)
    except (GumAPIError, AdminAuthError) as exc:
        print(str(exc), file=sys.stderr)
        return 1

    parser.print_help()
    return 1


def handle_admin_command(args: argparse.Namespace, client: GumClient) -> int:
    if args.admin_command == "login":
        admin_key = getpass.getpass("Admin key: ").strip()
        passphrase = getpass.getpass("Set local passphrase: ")
        confirm = getpass.getpass("Confirm passphrase: ")
        if passphrase != confirm:
            raise AdminAuthError("passphrases do not match")
        store_admin_key(admin_key, passphrase)
        print("Stored admin credentials for Gum.")
        return 0

    if args.admin_command == "logout":
        if clear_admin_key():
            print("Cleared stored admin credentials.")
        else:
            print("No stored admin credentials.")
        return 0

    admin_client = require_admin_client(client)

    if args.admin_command is None:
        return admin_live_view(
            client=admin_client,
            interval_secs=args.interval,
            once=args.once,
        )

    if args.admin_command == "runs":
        if args.admin_runs_command == "list":
            runs = admin_client.runs.list()[: args.limit]
            print(render_run_table(runs))
            return 0
        if args.admin_runs_command == "get":
            run = admin_client.runs.get(args.run_id)
            print(render_run_record(run))
            return 0

    if args.admin_command == "logs":
        logs = admin_client.runs.logs(args.run_id)
        print(render_logs(logs, attempt_id=args.attempt))
        return 0

    if args.admin_command == "runners":
        print(render_runner_table(admin_client.admin.runners()))
        return 0

    if args.admin_command == "leases":
        print(render_lease_table(admin_client.admin.leases()))
        return 0

    raise AdminAuthError("unsupported admin command")


def require_admin_client(client: GumClient) -> GumClient:
    admin_key = getattr(client, "admin_key", None) or default_admin_key()
    if admin_key is None:
        passphrase = getpass.getpass("Unlock passphrase: ")
        admin_key = load_admin_key(passphrase)
    if not isinstance(client, GumClient):
        return client
    return GumClient(
        base_url=client.base_url,
        api_key=client.api_key,
        admin_key=admin_key,
        timeout_secs=client.timeout_secs,
    )


def run_live_view(*, client: GumClient, run_id: str, interval_secs: float, log_lines: int, once: bool) -> int:
    while True:
        run = client.runs.get(run_id)
        logs = client.runs.logs(run_id)
        frame = render_live_frame(run, logs, log_lines=log_lines, interval_secs=interval_secs)
        sys.stdout.write("\x1b[2J\x1b[H")
        sys.stdout.write(frame)
        sys.stdout.flush()

        if once or run.status in TERMINAL_RUN_STATUSES:
            return 0

        try:
            time.sleep(interval_secs)
        except KeyboardInterrupt:
            return 0


def render_run_record(run: RunRecord) -> str:
    lines = [
        f"Run:      {run.id}",
        f"Job:      {run.job_id}",
        f"Status:   {run.status}",
        f"Attempt:  {run.attempt}",
        f"Trigger:  {run.trigger_type or '--'}",
        f"Replay:   {run.replay_of or '--'}",
        f"Failure:  {run.failure_reason or '--'}",
    ]
    return "\n".join(lines)


def render_logs(logs: Iterable[LogLine], *, attempt_id: str | None = None) -> str:
    filtered = [log for log in logs if attempt_id is None or log.attempt_id == attempt_id]
    if not filtered:
        return "No logs found."
    return "\n".join(f"[{log.stream}] {log.message}" for log in filtered)


def render_run_table(runs: list[RunRecord]) -> str:
    if not runs:
        return "No runs found."
    lines = ["STATUS       JOB                  RUN ID               ATTEMPT  TRIGGER    FAILURE"]
    for run in runs:
        failure = (run.failure_reason or "--")[:28]
        lines.append(
            f"{run.status:<12} {run.job_id:<20} {run.id:<20} {run.attempt:<8} {(run.trigger_type or '--'):<10} {failure}"
        )
    return "\n".join(lines)


def render_runner_table(runners: list[RunnerStatus]) -> str:
    if not runners:
        return "No runners found."
    lines = ["RUNNERS", "ID                   CLASS       ACTIVE/MAX  HEARTBEAT(ms)"]
    for runner in runners:
        lines.append(
            f"{runner.id:<20} {runner.compute_class:<11} {runner.active_lease_count}/{runner.max_concurrent_leases:<9} {runner.last_heartbeat_at_epoch_ms}"
        )
    return "\n".join(lines)


def render_lease_table(leases: list[LeaseStatus]) -> str:
    if not leases:
        return "No active leases."
    lines = ["LEASES", "LEASE ID             RUN ID               RUNNER               EXPIRES(ms)      CANCEL"]
    for lease in leases:
        cancel = "yes" if lease.cancel_requested else "no"
        lines.append(
            f"{lease.lease_id:<20} {lease.run_id:<20} {lease.runner_id:<20} {lease.expires_at_epoch_ms:<16} {cancel}"
        )
    return "\n".join(lines)


def render_live_frame(run: RunRecord, logs: list[LogLine], *, log_lines: int, interval_secs: float) -> str:
    recent_logs = logs[-log_lines:] if log_lines > 0 else logs
    body = [
        "GUM LIVE",
        "",
        f"run:      {run.id}",
        f"job:      {run.job_id}",
        f"status:   {run.status}",
        f"attempt:  {run.attempt}",
        f"trigger:  {run.trigger_type or '--'}",
        f"replay:   {run.replay_of or '--'}",
        f"failure:  {run.failure_reason or '--'}",
        "",
        "LOGS",
        *[f"[{log.stream}] {log.message}" for log in recent_logs],
        "",
        f"refreshing every {interval_secs:.1f}s  ctrl+c to exit",
    ]
    return "\n".join(body) + "\n"


def admin_live_view(*, client: GumClient, interval_secs: float, once: bool) -> int:
    while True:
        runs = client.runs.list()
        runners = client.admin.runners()
        leases = client.admin.leases()
        frame = render_admin_live_frame(
            runs=runs,
            runners=runners,
            leases=leases,
            interval_secs=interval_secs,
        )
        sys.stdout.write("\x1b[2J\x1b[H")
        sys.stdout.write(frame)
        sys.stdout.flush()

        if once:
            return 0

        try:
            time.sleep(interval_secs)
        except KeyboardInterrupt:
            return 0


def render_admin_live_frame(
    *, runs: list[RunRecord], runners: list[RunnerStatus], leases: list[LeaseStatus], interval_secs: float
) -> str:
    queued = sum(1 for run in runs if run.status == "queued")
    running = sum(1 for run in runs if run.status == "running")
    failed = sum(1 for run in runs if run.status in {"failed", "timed_out"})
    header = (
        f"GUM ADMIN  queued: {queued}   running: {running}   failed: {failed}   "
        f"runners: {len(runners)}   active leases: {len(leases)}"
    )
    body = [
        header,
        "",
        render_run_table(runs[:12]),
        "",
        render_runner_table(runners),
        "",
        render_lease_table(leases),
        "",
        f"refreshing every {interval_secs:.1f}s  ctrl+c to exit",
    ]
    return "\n".join(body) + "\n"


if __name__ == "__main__":
    raise SystemExit(main())

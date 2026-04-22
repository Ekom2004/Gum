from __future__ import annotations

import argparse
import curses
import getpass
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time
from dataclasses import dataclass
from typing import Iterable

from .auth import AdminAuthError, clear_admin_key, default_admin_key, load_admin_key, store_admin_key
from .client import GumAPIError, GumClient, LeaseStatus, LogLine, RunRecord, RunnerStatus, default_client


TERMINAL_RUN_STATUSES = {"succeeded", "failed", "timed_out", "canceled"}
RUN_FAILURE_STATUSES = {"failed", "timed_out"}
CONSOLE_VIEWS = ("runs", "runners", "leases")
STATUS_SYMBOLS = {
    "queued": "○",
    "running": "●",
    "failed": "×",
    "timed_out": "!",
    "canceled": "■",
    "succeeded": "✓",
}


@dataclass
class AdminConsoleState:
    view: str = "runs"
    selected_index: int = 0
    filter_query: str = ""
    filter_mode: bool = False
    message: str = ""
    message_level: str = "info"


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
        if not args.once and sys.stdin.isatty() and sys.stdout.isatty():
            return launch_ratatui_admin(client=admin_client)
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


def launch_ratatui_admin(*, client: GumClient) -> int:
    command, workdir = resolve_admin_tui_command()
    env = {
        **os.environ,
        "GUM_API_BASE_URL": client.base_url,
    }
    if client.admin_key is not None:
        env["GUM_ADMIN_KEY"] = client.admin_key
    try:
        completed = subprocess.run(command, cwd=workdir, env=env, check=False)
    except OSError as exc:
        raise AdminAuthError(f"failed to launch gum-admin: {exc}") from exc
    return completed.returncode


def resolve_admin_tui_command() -> tuple[list[str], str | None]:
    explicit = os.environ.get("GUM_ADMIN_TUI_BIN")
    if explicit:
        return [explicit], None
    installed = shutil.which("gum-admin")
    if installed:
        return [installed], None

    repo_root = Path(__file__).resolve().parents[2]
    cargo_toml = repo_root / "Cargo.toml"
    crate_dir = repo_root / "crates" / "gum-admin"
    if cargo_toml.exists() and crate_dir.exists():
        return ["cargo", "run", "-q", "-p", "gum-admin", "--"], str(repo_root)

    raise AdminAuthError(
        "gum-admin binary not found; set GUM_ADMIN_TUI_BIN or run inside the Gum repo"
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
        f"Waiting:  {run.waiting_reason or '--'}",
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
    lines = ["RUNNERS", "ID                   CLASS       MEMORY       ACTIVE/MAX  HEARTBEAT(ms)"]
    for runner in runners:
        lines.append(
            f"{runner.id:<20} {runner.compute_class:<11} {runner.active_memory_mb}/{runner.memory_mb:<8} {runner.active_lease_count}/{runner.max_concurrent_leases:<9} {runner.last_heartbeat_at_epoch_ms}"
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


def run_admin_console(*, client: GumClient, interval_secs: float) -> int:
    state = AdminConsoleState()
    return curses.wrapper(lambda screen: _admin_console_loop(screen, client, interval_secs, state))


def _admin_console_loop(screen, client: GumClient, interval_secs: float, state: AdminConsoleState) -> int:
    init_console_colors()
    curses.curs_set(0)
    screen.nodelay(True)
    screen.timeout(max(100, int(interval_secs * 1000)))
    while True:
        try:
            snapshot = fetch_admin_snapshot(client, state)
        except GumAPIError as exc:
            state.message = str(exc)
            state.message_level = "error"
            snapshot = AdminSnapshot(runs=[], runners=[], leases=[], logs=[])
        draw_admin_console(screen, state, snapshot, interval_secs)
        key = screen.getch()
        if key == -1:
            continue
        if state.filter_mode:
            if handle_filter_key(state, key):
                continue
            if key == 27:
                state.filter_mode = False
                state.message = "Filter canceled."
                continue
        if key in (ord("q"),):
            return 0
        if key in (ord("1"),):
            state.view = "runs"
            state.selected_index = 0
        elif key in (ord("2"),):
            state.view = "runners"
            state.selected_index = 0
        elif key in (ord("3"),):
            state.view = "leases"
            state.selected_index = 0
        elif key in (ord("j"), curses.KEY_DOWN):
            move_selection(state, snapshot, 1)
        elif key in (ord("k"), curses.KEY_UP):
            move_selection(state, snapshot, -1)
        elif key == ord("/"):
            state.filter_mode = True
            state.message = "Type to filter runs. Enter applies. Esc cancels."
        elif key in (ord("c"),):
            if state.view == "runs":
                run = selected_run(snapshot.runs, state)
                if run is not None:
                    try:
                        client.runs.cancel(run.id)
                        state.message = f"Canceled {run.id}."
                    except GumAPIError as exc:
                        state.message = str(exc)
        elif key in (ord("r"),):
            if state.view == "runs":
                run = selected_run(snapshot.runs, state)
                if run is not None:
                    try:
                        replayed = client.runs.replay(run.id)
                        state.message = f"Replayed {replayed.id}."
                    except GumAPIError as exc:
                        state.message = str(exc)
        elif key in (10, 13, curses.KEY_ENTER):
            state.message = "Selected item shown in detail pane."


@dataclass
class AdminSnapshot:
    runs: list[RunRecord]
    runners: list[RunnerStatus]
    leases: list[LeaseStatus]
    logs: list[LogLine]


def fetch_admin_snapshot(client: GumClient, state: AdminConsoleState) -> AdminSnapshot:
    runs = filter_runs(client.runs.list(), state.filter_query)
    runners = client.admin.runners()
    leases = client.admin.leases()
    logs: list[LogLine] = []
    run = selected_run(runs, state)
    if run is not None:
        logs = client.runs.logs(run.id)
    clamp_selection(state, runs, runners, leases)
    return AdminSnapshot(runs=runs, runners=runners, leases=leases, logs=logs)


def filter_runs(runs: list[RunRecord], query: str) -> list[RunRecord]:
    query = query.strip().lower()
    if not query:
        return runs
    return [
        run
        for run in runs
        if query in run.id.lower()
        or query in run.job_id.lower()
        or query in run.status.lower()
        or query in (run.trigger_type or "").lower()
    ]


def selected_run(runs: list[RunRecord], state: AdminConsoleState) -> RunRecord | None:
    if not runs:
        return None
    if state.selected_index < 0:
        state.selected_index = 0
    if state.selected_index >= len(runs):
        state.selected_index = len(runs) - 1
    return runs[state.selected_index]


def clamp_selection(
    state: AdminConsoleState,
    runs: list[RunRecord],
    runners: list[RunnerStatus],
    leases: list[LeaseStatus],
) -> None:
    items = {
        "runs": len(runs),
        "runners": len(runners),
        "leases": len(leases),
    }[state.view]
    if items == 0:
        state.selected_index = 0
        return
    state.selected_index = max(0, min(state.selected_index, items - 1))


def move_selection(state: AdminConsoleState, snapshot: AdminSnapshot, delta: int) -> None:
    items = {
        "runs": len(snapshot.runs),
        "runners": len(snapshot.runners),
        "leases": len(snapshot.leases),
    }[state.view]
    if items == 0:
        state.selected_index = 0
        return
    state.selected_index = (state.selected_index + delta) % items


def handle_filter_key(state: AdminConsoleState, key: int) -> bool:
    if key in (10, 13, curses.KEY_ENTER):
        state.filter_mode = False
        state.message = f'Run filter: "{state.filter_query or "all"}"'
        return True
    if key in (curses.KEY_BACKSPACE, 127, 8):
        state.filter_query = state.filter_query[:-1]
        return True
    if 32 <= key <= 126:
        state.filter_query += chr(key)
        return True
    return False


def draw_admin_console(screen, state: AdminConsoleState, snapshot: AdminSnapshot, interval_secs: float) -> None:
    screen.erase()
    height, width = screen.getmaxyx()
    status_line = render_admin_header(snapshot)
    screen.addnstr(0, 0, status_line, width - 1, color_attr("header"))
    screen.hline(1, 0, curses.ACS_HLINE, width)
    screen.addnstr(2, 0, render_view_tabs(state.view), width - 1, color_attr("tabs"))
    if state.filter_mode:
        screen.addnstr(3, 0, f"Filter: {state.filter_query}", width - 1, color_attr("accent"))
    elif state.message:
        screen.addnstr(3, 0, state.message, width - 1, color_attr("message", state.message_level))

    top_start = 5
    top_height = max(10, height // 2)
    left_width = max(48, width // 2)
    draw_boxed_panel(
        screen,
        start_y=top_start,
        start_x=0,
        width=left_width,
        height=top_height - top_start,
        title=panel_title(state.view),
        lines=render_primary_panel(state, snapshot),
    )
    draw_boxed_panel(
        screen,
        start_y=top_start,
        start_x=left_width + 1,
        width=width - left_width - 1,
        height=top_height - top_start,
        title="DETAIL",
        lines=render_detail_panel(state, snapshot),
    )
    draw_boxed_panel(
        screen,
        start_y=top_height + 1,
        start_x=0,
        width=width,
        height=height - top_height - 3,
        title="LOGS",
        lines=render_logs_panel(snapshot.logs),
    )
    footer = "j/k move  / filter  enter inspect  c cancel  r replay  1 runs  2 runners  3 leases  q quit"
    screen.addnstr(height - 1, 0, footer[: width - 1], width - 1, color_attr("footer"))
    screen.refresh()


def render_admin_header(snapshot: AdminSnapshot) -> str:
    queued = sum(1 for run in snapshot.runs if run.status == "queued")
    running = sum(1 for run in snapshot.runs if run.status == "running")
    failed = sum(1 for run in snapshot.runs if run.status in RUN_FAILURE_STATUSES)
    return (
        f"GUM ADMIN  queued: {queued}   running: {running}   failed: {failed}   "
        f"runners: {len(snapshot.runners)}   active leases: {len(snapshot.leases)}"
    )


def render_view_tabs(active_view: str) -> str:
    parts: list[str] = []
    for index, view in enumerate(CONSOLE_VIEWS, start=1):
        label = f"{index}:{view}"
        if view == active_view:
            label = f"[{label}]"
        parts.append(label)
    return "  ".join(parts)


def render_primary_panel(state: AdminConsoleState, snapshot: AdminSnapshot) -> list[str]:
    if state.view == "runs":
        return render_runs_panel(snapshot.runs, state.selected_index)
    if state.view == "runners":
        return render_runners_panel(snapshot.runners, state.selected_index)
    return render_leases_panel(snapshot.leases, state.selected_index)


def render_runs_panel(runs: list[RunRecord], selected_index: int) -> list[str]:
    lines = ["status   job                  run id               attempt  trigger"]
    if not runs:
        return lines + ["No runs found."]
    for index, run in enumerate(runs[:20]):
        marker = ">" if index == selected_index else " "
        symbol = status_symbol(run.status)
        lines.append(
            f"{marker} {symbol:<2} {run.job_id:<20} {run.id:<20} {run.attempt:<8} {(run.trigger_type or '--'):<10}"
        )
    return lines


def render_runners_panel(runners: list[RunnerStatus], selected_index: int) -> list[str]:
    lines = ["id                   class       memory      active/max  heartbeat(ms)"]
    if not runners:
        return lines + ["No runners found."]
    for index, runner in enumerate(runners[:20]):
        marker = ">" if index == selected_index else " "
        lines.append(
            f"{marker} {runner.id:<20} {runner.compute_class:<11} {runner.active_memory_mb}/{runner.memory_mb:<8} {runner.active_lease_count}/{runner.max_concurrent_leases:<9} {runner.last_heartbeat_at_epoch_ms}"
        )
    return lines


def render_leases_panel(leases: list[LeaseStatus], selected_index: int) -> list[str]:
    lines = ["lease id             run id               runner               cancel"]
    if not leases:
        return lines + ["No active leases."]
    for index, lease in enumerate(leases[:20]):
        marker = ">" if index == selected_index else " "
        cancel = "yes" if lease.cancel_requested else "no"
        lines.append(
            f"{marker} {lease.lease_id:<20} {lease.run_id:<20} {lease.runner_id:<20} {cancel:<6}"
        )
    return lines


def render_detail_panel(state: AdminConsoleState, snapshot: AdminSnapshot) -> list[str]:
    if state.view == "runs":
        run = selected_run(snapshot.runs, state)
        if run is None:
            return ["No run selected."]
        return [
            f"run:      {run.id}",
            f"job:      {run.job_id}",
            f"status:   {status_symbol(run.status)} {run.status}",
            f"attempt:  {run.attempt}",
            f"trigger:  {run.trigger_type or '--'}",
            f"replay:   {run.replay_of or '--'}",
            f"failure:  {run.failure_reason or '--'}",
        ]
    if state.view == "runners":
        if not snapshot.runners:
            return ["No runner selected."]
        runner = snapshot.runners[state.selected_index]
        return [
            f"runner:   {runner.id}",
            f"class:    {runner.compute_class}",
            f"memory:   {runner.active_memory_mb}/{runner.memory_mb} MB",
            f"active:   {runner.active_lease_count}/{runner.max_concurrent_leases}",
            f"seen:     {runner.last_heartbeat_at_epoch_ms}",
        ]
    if not snapshot.leases:
        return ["No lease selected."]
    lease = snapshot.leases[state.selected_index]
    return [
        f"lease:    {lease.lease_id}",
        f"run:      {lease.run_id}",
        f"attempt:  {lease.attempt_id}",
        f"runner:   {lease.runner_id}",
        f"expires:  {lease.expires_at_epoch_ms}",
        f"cancel:   {'yes' if lease.cancel_requested else 'no'}",
    ]


def render_logs_panel(logs: list[LogLine]) -> list[str]:
    lines: list[str] = []
    if not logs:
        return ["No logs for selected run."]
    for log in logs[-12:]:
        lines.append(f"[{log.stream}] {log.message}")
    return lines


def draw_boxed_panel(screen, *, start_y: int, start_x: int, width: int, height: int, title: str, lines: list[str]) -> None:
    if width <= 4 or height <= 2:
        return
    bottom = start_y + height - 1
    right = start_x + width - 1
    screen.addch(start_y, start_x, curses.ACS_ULCORNER)
    screen.hline(start_y, start_x + 1, curses.ACS_HLINE, max(0, width - 2))
    screen.addch(start_y, right, curses.ACS_URCORNER)
    screen.vline(start_y + 1, start_x, curses.ACS_VLINE, max(0, height - 2))
    screen.vline(start_y + 1, right, curses.ACS_VLINE, max(0, height - 2))
    screen.addch(bottom, start_x, curses.ACS_LLCORNER)
    screen.hline(bottom, start_x + 1, curses.ACS_HLINE, max(0, width - 2))
    screen.addch(bottom, right, curses.ACS_LRCORNER)
    screen.addnstr(start_y, start_x + 2, f" {title} ", max(1, width - 4), color_attr("panel_title"))
    for offset, line in enumerate(lines[: max(0, height - 2)]):
        screen.addnstr(start_y + 1 + offset, start_x + 1, line, max(1, width - 2))


def panel_title(view: str) -> str:
    return {
        "runs": "RUNS",
        "runners": "RUNNERS",
        "leases": "LEASES",
    }[view]


def status_symbol(status: str) -> str:
    return STATUS_SYMBOLS.get(status, "?")


def init_console_colors() -> None:
    if not curses.has_colors():
        return
    curses.start_color()
    curses.use_default_colors()
    curses.init_pair(1, curses.COLOR_WHITE, -1)
    curses.init_pair(2, curses.COLOR_CYAN, -1)
    curses.init_pair(3, curses.COLOR_YELLOW, -1)
    curses.init_pair(4, curses.COLOR_RED, -1)
    curses.init_pair(5, curses.COLOR_GREEN, -1)


def color_attr(kind: str, level: str | None = None) -> int:
    if not curses.has_colors():
        return curses.A_NORMAL
    if kind == "header":
        return curses.color_pair(2) | curses.A_BOLD
    if kind == "tabs":
        return curses.color_pair(1) | curses.A_BOLD
    if kind == "panel_title":
        return curses.color_pair(2) | curses.A_BOLD
    if kind == "footer":
        return curses.color_pair(1)
    if kind == "accent":
        return curses.color_pair(3) | curses.A_BOLD
    if kind == "message":
        if level == "error":
            return curses.color_pair(4) | curses.A_BOLD
        return curses.color_pair(5) | curses.A_BOLD
    return curses.A_NORMAL


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

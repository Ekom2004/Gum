from __future__ import annotations

import argparse
import sys

from .deploy import DeployError, deploy_project


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="gum")
    subparsers = parser.add_subparsers(dest="command", required=True)

    deploy_parser = subparsers.add_parser("deploy", help="package and register the current project")
    deploy_parser.add_argument("--project-id", default="proj_dev")

    args = parser.parse_args(argv)

    if args.command == "deploy":
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

    parser.print_help()
    return 1

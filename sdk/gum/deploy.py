from __future__ import annotations

import ast
import os
import tarfile
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from .client import DeployRef, GumClient, default_client


class DeployError(RuntimeError):
    pass


@dataclass(frozen=True, slots=True)
class DiscoveredJob:
    id: str
    name: str
    handler_ref: str
    trigger_mode: str
    schedule_expr: str | None
    retries: int
    timeout_secs: int
    rate_limit_spec: str | None
    concurrency_limit: int | None


@dataclass(frozen=True, slots=True)
class DeployResult:
    project_root: Path
    bundle_path: Path
    jobs: list[DiscoveredJob]
    deploy: DeployRef


def deploy_project(
    project_root: str | os.PathLike[str] | None = None,
    *,
    client: GumClient | None = None,
    project_id: str = "proj_dev",
) -> DeployResult:
    root = Path(project_root or os.getcwd()).resolve()
    _validate_project_root(root)
    jobs = discover_jobs(root)
    if not jobs:
        raise DeployError("No Gum jobs found in this project.")

    bundle_path = package_project(root)
    active_client = client or default_client()
    payload = {
        "project_id": project_id,
        "version": bundle_path.stem,
        "bundle_url": bundle_path.resolve().as_uri(),
        "bundle_sha256": "local-dev-bundle",
        "sdk_language": "python",
        "entrypoint": "python",
        "jobs": [
            {
                "id": job.id,
                "name": job.name,
                "handler_ref": job.handler_ref,
                "trigger_mode": job.trigger_mode,
                "schedule_expr": job.schedule_expr,
                "retries": job.retries,
                "timeout_secs": job.timeout_secs,
                "rate_limit_spec": job.rate_limit_spec,
                "concurrency_limit": job.concurrency_limit,
            }
            for job in jobs
        ],
    }
    deploy_ref = active_client.register_deploy(payload)
    return DeployResult(
        project_root=root,
        bundle_path=bundle_path,
        jobs=jobs,
        deploy=deploy_ref,
    )


def discover_jobs(project_root: Path) -> list[DiscoveredJob]:
    jobs: list[DiscoveredJob] = []
    for path in sorted(project_root.rglob("*.py")):
        if _should_skip(path):
            continue
        module_name = _module_name(project_root, path)
        jobs.extend(_discover_jobs_in_file(path, module_name))
    return jobs


def package_project(project_root: Path) -> Path:
    _validate_project_root(project_root)
    build_dir = project_root / ".gum" / "builds"
    build_dir.mkdir(parents=True, exist_ok=True)
    bundle_name = f"deploy-{_safe_timestamp()}.tar.gz"
    bundle_path = build_dir / bundle_name

    with tarfile.open(bundle_path, "w:gz") as archive:
        for path in sorted(project_root.rglob("*")):
            if not path.is_file() or _should_skip(path):
                continue
            archive.add(path, arcname=path.relative_to(project_root))

    return bundle_path


def _validate_project_root(project_root: Path) -> None:
    if not project_root.exists():
        raise DeployError(f"Project root does not exist: {project_root}")
    if not project_root.is_dir():
        raise DeployError(f"Project root is not a directory: {project_root}")
    has_manifest = (project_root / "pyproject.toml").exists() or (project_root / "requirements.txt").exists()
    if not has_manifest:
        raise DeployError("No supported dependency manifest found. Expected pyproject.toml or requirements.txt.")


def _discover_jobs_in_file(path: Path, module_name: str) -> list[DiscoveredJob]:
    try:
        tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
    except SyntaxError as exc:
        raise DeployError(f"Failed to parse {path}: {exc}") from exc

    jobs: list[DiscoveredJob] = []
    for node in tree.body:
        if isinstance(node, ast.FunctionDef):
            job_call = _gum_job_decorator(node)
            if job_call is None:
                continue

            kwargs = {kw.arg: _literal_value(kw.value) for kw in job_call.keywords if kw.arg is not None}
            job_name = str(kwargs.get("name") or node.name)
            every = _optional_string(kwargs.get("every"))
            retries = _optional_int(kwargs.get("retries")) or 0
            timeout_secs = _timeout_secs(kwargs.get("timeout"))
            rate_limit_spec = _optional_string(kwargs.get("rate_limit"))
            concurrency_limit = _optional_int(kwargs.get("concurrency"))
            trigger_mode = "both" if every else "manual"

            jobs.append(
                DiscoveredJob(
                    id=f"job_{job_name}",
                    name=job_name,
                    handler_ref=f"{module_name}:{node.name}",
                    trigger_mode=trigger_mode,
                    schedule_expr=every,
                    retries=retries,
                    timeout_secs=timeout_secs,
                    rate_limit_spec=rate_limit_spec,
                    concurrency_limit=concurrency_limit,
                )
            )
    return jobs


def _gum_job_decorator(node: ast.FunctionDef) -> ast.Call | None:
    for decorator in node.decorator_list:
        if isinstance(decorator, ast.Call) and _is_job_call(decorator.func):
            return decorator
    return None


def _is_job_call(func: ast.expr) -> bool:
    if isinstance(func, ast.Attribute):
        return isinstance(func.value, ast.Name) and func.value.id == "gum" and func.attr == "job"
    if isinstance(func, ast.Name):
        return func.id == "job"
    return False


def _literal_value(node: ast.AST) -> Any:
    if isinstance(node, ast.Constant):
        return node.value
    raise DeployError("Only literal Gum job policy values are supported in local deploy for now.")


def _optional_string(value: Any) -> str | None:
    if value is None:
        return None
    if isinstance(value, str):
        return value
    raise DeployError("Expected a string policy value.")


def _optional_int(value: Any) -> int | None:
    if value is None:
        return None
    if isinstance(value, int):
        return value
    raise DeployError("Expected an integer policy value.")


def _timeout_secs(value: Any) -> int:
    if value is None:
        raise DeployError("Every Gum job must declare timeout in local deploy for now.")
    if not isinstance(value, str):
        raise DeployError("Expected timeout to be a string like '5m'.")
    if value.endswith("m"):
        return int(value[:-1]) * 60
    if value.endswith("s"):
        return int(value[:-1])
    if value.endswith("h"):
        return int(value[:-1]) * 3600
    raise DeployError("Unsupported timeout format. Use values like '30s', '5m', or '1h'.")


def _module_name(project_root: Path, path: Path) -> str:
    relative = path.relative_to(project_root)
    parts = list(relative.with_suffix("").parts)
    if parts[-1] == "__init__":
        parts = parts[:-1]
    return ".".join(parts) if parts else path.stem


def _should_skip(path: Path) -> bool:
    parts = set(path.parts)
    return any(
        marker in parts
        for marker in {
            ".git",
            ".venv",
            ".gum",
            "__pycache__",
            "node_modules",
            "target",
            ".pytest_cache",
        }
    )


def _safe_timestamp() -> str:
    return datetime.now(timezone.utc).strftime("%Y%m%d%H%M%S")

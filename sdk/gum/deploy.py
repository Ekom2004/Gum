from __future__ import annotations

import ast
import hashlib
import os
import tarfile
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path

from .client import DeployRef, GumClient, default_client


class DeployError(RuntimeError):
    pass


@dataclass(slots=True)
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
    memory_mb: int | None
    key_field: str | None
    compute_class: str | None
    module_path: str


@dataclass(slots=True)
class DeployResult:
    project_root: Path
    bundle_path: Path
    jobs: list[DiscoveredJob]
    deploy: DeployRef


@dataclass(slots=True)
class _AstJobConfig:
    every: str | None = None
    retries: int = 0
    timeout: str = "5m"
    rate_limit: str | None = None
    concurrency: int | None = None
    memory: str | None = None
    key: str | None = None
    compute: str | None = None


@dataclass(slots=True)
class _ModuleBindings:
    values: dict[str, object]


def discover_jobs(project_root: Path) -> list[DiscoveredJob]:
    project_root = project_root.resolve()
    jobs: list[DiscoveredJob] = []
    for path in sorted(project_root.rglob("*.py")):
        if path.name.startswith("."):
            continue
        module_path = path.relative_to(project_root).as_posix()
        module_name = module_path[:-3].replace("/", ".")
        source = path.read_text(encoding="utf-8")
        tree = ast.parse(source, filename=str(path))
        bindings = _collect_module_bindings(tree)
        for node in tree.body:
            if not isinstance(node, ast.FunctionDef):
                continue
            config = _extract_job_config(node, bindings)
            if config is None:
                continue
            jobs.append(
                DiscoveredJob(
                    id=f"job_{node.name}",
                    name=node.name,
                    handler_ref=f"{module_name}:{node.name}",
                    trigger_mode="schedule" if config.every else "manual",
                    schedule_expr=config.every,
                    retries=config.retries,
                    timeout_secs=_parse_timeout_secs(config.timeout),
                    rate_limit_spec=config.rate_limit,
                    concurrency_limit=config.concurrency,
                    memory_mb=_parse_memory_mb(config.memory) if config.memory else None,
                    key_field=config.key,
                    compute_class=config.compute,
                    module_path=module_path,
                )
            )
    _validate_rate_limit_pools(jobs)
    return jobs


def package_project(project_root: Path) -> Path:
    project_root = project_root.resolve()
    if not (project_root / "pyproject.toml").exists():
        raise DeployError("pyproject.toml not found")

    bundle_dir = Path(tempfile.mkdtemp(prefix="gum-deploy-"))
    bundle_path = bundle_dir / "bundle.tar.gz"
    with tarfile.open(bundle_path, "w:gz") as archive:
        for path in sorted(project_root.rglob("*")):
            if not path.is_file():
                continue
            if ".git" in path.parts or "__pycache__" in path.parts:
                continue
            archive.add(path, arcname=path.relative_to(project_root))
    return bundle_path


def deploy_project(
    project_root: Path | str | None = None,
    *,
    client: GumClient | None = None,
    project_id: str = "proj_dev",
) -> DeployResult:
    root = Path(project_root or os.getcwd()).resolve()
    jobs = discover_jobs(root)
    if not jobs:
        raise DeployError("no gum jobs found")

    bundle_path = package_project(root)
    deploy_client = client or default_client()
    version = f"dev-{int(time.time())}"
    bundle_sha256 = hashlib.sha256(bundle_path.read_bytes()).hexdigest()
    entrypoint = jobs[0].module_path
    payload = {
        "project_id": project_id,
        "version": version,
        "bundle_url": f"file://{bundle_path}",
        "bundle_sha256": bundle_sha256,
        "sdk_language": "python",
        "entrypoint": entrypoint,
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
                "memory_mb": job.memory_mb,
                "key_field": job.key_field,
                "compute_class": job.compute_class,
            }
            for job in jobs
        ],
    }
    deploy = deploy_client.register_deploy(payload)
    return DeployResult(project_root=root, bundle_path=bundle_path, jobs=jobs, deploy=deploy)


def _extract_job_config(
    node: ast.FunctionDef,
    bindings: _ModuleBindings,
) -> _AstJobConfig | None:
    for decorator in node.decorator_list:
        if isinstance(decorator, ast.Call):
            target = decorator.func
            if isinstance(target, ast.Attribute) and isinstance(target.value, ast.Name):
                if target.value.id == "gum" and target.attr == "job":
                    return _parse_decorator_keywords(decorator, bindings)
            if isinstance(target, ast.Name) and target.id == "job":
                return _parse_decorator_keywords(decorator, bindings)
    return None


def _parse_decorator_keywords(node: ast.Call, bindings: _ModuleBindings) -> _AstJobConfig:
    config = _AstJobConfig()
    for keyword in node.keywords:
        if keyword.arg is None:
            continue
        value = _literal_value(keyword.value, bindings)
        if keyword.arg == "every":
            config.every = value
        elif keyword.arg == "retries":
            config.retries = int(value)
        elif keyword.arg == "timeout":
            config.timeout = str(value)
        elif keyword.arg == "rate_limit":
            config.rate_limit = value
        elif keyword.arg == "concurrency":
            config.concurrency = int(value)
        elif keyword.arg == "memory":
            config.memory = str(value)
        elif keyword.arg == "key":
            config.key = str(value)
        elif keyword.arg == "compute":
            config.compute = value
    return config


def _collect_module_bindings(tree: ast.Module) -> _ModuleBindings:
    values: dict[str, object] = {}
    bindings = _ModuleBindings(values=values)
    for node in tree.body:
        if not isinstance(node, ast.Assign) or len(node.targets) != 1:
            continue
        target = node.targets[0]
        if not isinstance(target, ast.Name):
            continue
        try:
            values[target.id] = _literal_value(node.value, bindings, binding_name=target.id)
        except Exception:
            continue
    return bindings


def _literal_value(
    node: ast.AST,
    bindings: _ModuleBindings,
    *,
    binding_name: str | None = None,
):
    if isinstance(node, ast.Name):
        if node.id in bindings.values:
            return bindings.values[node.id]
        raise ValueError(f"unknown name: {node.id}")
    if isinstance(node, ast.Call):
        target = node.func
        is_rate_limit_call = False
        if isinstance(target, ast.Attribute) and isinstance(target.value, ast.Name):
            is_rate_limit_call = target.value.id == "gum" and target.attr == "rate_limit"
        elif isinstance(target, ast.Name):
            is_rate_limit_call = target.id == "rate_limit"
        if is_rate_limit_call:
            if len(node.args) != 1 or node.keywords:
                raise ValueError("rate_limit() expects exactly one positional spec")
            spec = str(_literal_value(node.args[0], bindings))
            if ":" in spec or binding_name is None:
                return spec
            return f"{binding_name}:{spec}"
    return ast.literal_eval(node)


def _parse_timeout_secs(raw: str) -> int:
    if raw.isdigit():
        value = int(raw)
        if value <= 0:
            raise DeployError("timeout must be positive")
        return value

    amount = raw[:-1]
    unit = raw[-1]
    value = int(amount)
    if value <= 0:
        raise DeployError("timeout must be positive")
    multiplier = {
        "s": 1,
        "m": 60,
        "h": 3600,
    }.get(unit)
    if multiplier is None:
        raise DeployError(f"unsupported timeout value: {raw}")
    return value * multiplier


def _parse_memory_mb(raw: str) -> int:
    normalized = raw.strip().lower()
    if not normalized:
        raise DeployError("memory must not be empty")
    units = {
        "mb": 1,
        "m": 1,
        "gb": 1024,
        "g": 1024,
    }
    for suffix, multiplier in units.items():
        if normalized.endswith(suffix):
            amount = normalized[: -len(suffix)]
            break
    else:
        raise DeployError(f"unsupported memory value: {raw}")
    try:
        value = int(amount)
    except ValueError as exc:
        raise DeployError(f"unsupported memory value: {raw}") from exc
    if value <= 0:
        raise DeployError("memory must be positive")
    return value * multiplier


def _validate_rate_limit_pools(jobs: list[DiscoveredJob]) -> None:
    pools: dict[str, str] = {}
    for job in jobs:
        if job.rate_limit_spec is None:
            continue
        pool_name = _rate_limit_pool_name(job.rate_limit_spec)
        if pool_name is None:
            continue
        existing = pools.get(pool_name)
        if existing is not None and existing != job.rate_limit_spec:
            raise DeployError(
                f'rate limit pool "{pool_name}" has conflicting definitions: '
                f"{existing} and {job.rate_limit_spec}"
            )
        pools[pool_name] = job.rate_limit_spec


def _rate_limit_pool_name(spec: str) -> str | None:
    if ":" not in spec:
        return None
    pool_name, _quota = spec.rsplit(":", 1)
    if not pool_name:
        raise DeployError(f"rate limit pool must not be empty: {spec}")
    return pool_name

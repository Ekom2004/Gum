from __future__ import annotations

import ast
import base64
import getpass
import hashlib
import os
import re
import sys
import tarfile
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path

import tomllib

from .auth import default_admin_key, default_api_key
from .client import DeployRef, GumClient, default_client
from .provisioning import build_runner_capacity_plan, provisioner_from_env


class DeployError(RuntimeError):
    pass


CONFIG_FILE_NAME = "gum.toml"
DEFAULT_PROJECT_ID = "proj_dev"
DEFAULT_API_BASE_URL = "https://api.gum.cloud"
DEFAULT_SECRET_ENV = "prod"

_PROVIDER_SECRET_MAP = {
    "anthropic": "ANTHROPIC_API_KEY",
    "openai": "OPENAI_API_KEY",
    "resend": "RESEND_API_KEY",
    "stripe": "STRIPE_API_KEY",
}
_SECRET_NAME_PATTERN = re.compile(r"^[A-Z0-9_]+$")


@dataclass(slots=True)
class GumProjectConfig:
    project_id: str | None = None
    api_base_url: str | None = None


@dataclass(slots=True)
class InitResult:
    project_root: Path
    config_path: Path
    env_example_path: Path
    pyproject_path: Path
    jobs_path: Path
    created: list[Path]
    kept: list[Path]


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
    cpu_cores: int | None
    memory_mb: int | None
    key_field: str | None
    compute_class: str | None
    required_secret_names: list[str]
    module_path: str


@dataclass(slots=True)
class DeployResult:
    project_root: Path
    project_id: str
    api_base_url: str
    bundle_path: Path
    jobs: list[DiscoveredJob]
    deploy: DeployRef


@dataclass(slots=True)
class RuntimeSpec:
    python_version: str
    deps_mode: str | None
    deps_hash: str | None


@dataclass(slots=True)
class _AstJobConfig:
    every: str | None = None
    cron: str | None = None
    timezone: str | None = None
    retries: int = 0
    timeout: str = "5m"
    rate_limit: str | None = None
    concurrency: int | None = None
    cpu: int | None = None
    memory: str | None = None
    key: str | None = None
    compute: str | None = None
    compute_class: str | None = None


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
        module_required_secrets = sorted(
            _discover_provider_required_secrets(tree) | _discover_env_secret_names(tree)
        )
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
                    trigger_mode="schedule" if (config.every or config.cron) else "manual",
                    schedule_expr=_resolve_schedule_expr(config),
                    retries=config.retries,
                    timeout_secs=_parse_timeout_secs(config.timeout),
                    rate_limit_spec=config.rate_limit,
                    concurrency_limit=config.concurrency,
                    cpu_cores=_parse_cpu_cores(config.cpu) if config.cpu is not None else None,
                    memory_mb=_parse_memory_mb(config.memory) if config.memory else None,
                    key_field=config.key,
                    compute_class=config.compute_class or config.compute,
                    required_secret_names=list(module_required_secrets),
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
    project_id: str | None = None,
    api_base_url: str | None = None,
) -> DeployResult:
    root = Path(project_root or os.getcwd()).resolve()
    resolved_project_id = resolve_project_id(root, project_id)
    resolved_api_base_url = resolve_api_base_url(root, api_base_url)
    deploy_client = client or client_from_project(root, api_base_url=api_base_url)
    jobs = discover_jobs(root)
    if not jobs:
        raise DeployError("no gum jobs found")

    _ensure_required_secrets(
        root,
        jobs,
        deploy_client,
        project_id=resolved_project_id,
    )
    runtime_spec = discover_runtime_spec(root)

    bundle_path = package_project(root)
    version = f"dev-{int(time.time())}"
    bundle_sha256 = hashlib.sha256(bundle_path.read_bytes()).hexdigest()
    entrypoint = jobs[0].module_path
    payload = {
        "project_id": resolved_project_id,
        "version": version,
        "bundle_url": _inline_bundle_url(bundle_path),
        "bundle_sha256": bundle_sha256,
        "sdk_language": "python",
        "entrypoint": entrypoint,
        "python_version": runtime_spec.python_version,
        "deps_mode": runtime_spec.deps_mode,
        "deps_hash": runtime_spec.deps_hash,
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
                "cpu_cores": job.cpu_cores,
                "memory_mb": job.memory_mb,
                "key_field": job.key_field,
                "compute_class": job.compute_class,
                "required_secret_names": job.required_secret_names,
            }
            for job in jobs
        ],
    }
    deploy = deploy_client.register_deploy(payload)
    _maybe_auto_sync_runner_capacity(jobs)
    _maybe_request_runtime_prepare(deploy_client, deploy.id, runtime_spec)
    return DeployResult(
        project_root=root,
        project_id=resolved_project_id,
        api_base_url=resolved_api_base_url,
        bundle_path=bundle_path,
        jobs=jobs,
        deploy=deploy,
    )


def discover_runtime_spec(project_root: Path) -> RuntimeSpec:
    pyproject_path = project_root / "pyproject.toml"
    pyproject_text = pyproject_path.read_text(encoding="utf-8")
    pyproject = tomllib.loads(pyproject_text)
    python_version = _resolve_python_version(pyproject)

    uv_lock_path = project_root / "uv.lock"
    if uv_lock_path.exists():
        return RuntimeSpec(
            python_version=python_version,
            deps_mode="uv_lock",
            deps_hash=hashlib.sha256(uv_lock_path.read_bytes()).hexdigest(),
        )

    requirements_path = project_root / "requirements.txt"
    if requirements_path.exists():
        return RuntimeSpec(
            python_version=python_version,
            deps_mode="requirements_txt",
            deps_hash=hashlib.sha256(requirements_path.read_bytes()).hexdigest(),
        )

    return RuntimeSpec(
        python_version=python_version,
        deps_mode=None,
        deps_hash=None,
    )


def _resolve_python_version(pyproject: dict[str, object]) -> str:
    project = pyproject.get("project")
    if not isinstance(project, dict):
        return "3.11"
    requires_python = project.get("requires-python")
    if not isinstance(requires_python, str):
        return "3.11"
    match = re.search(r"([0-9]+)\\.([0-9]+)", requires_python)
    if not match:
        return "3.11"
    return f"{match.group(1)}.{match.group(2)}"


def _maybe_request_runtime_prepare(
    client: GumClient, deploy_id: str, runtime_spec: RuntimeSpec
) -> None:
    enabled = os.environ.get("GUM_PREWARM_RUNTIME", "").strip().lower()
    if enabled not in {"1", "true", "yes", "on"}:
        return
    if runtime_spec.deps_mode is None or runtime_spec.deps_hash is None:
        return
    try:
        client.prepare_deploy_runtime(deploy_id)
        print("remote runtime prepare requested")
    except Exception as exc:
        print(f"remote runtime prepare skipped: {exc}")

def _ensure_required_secrets(
    project_root: Path,
    jobs: list[DiscoveredJob],
    client: GumClient,
    *,
    project_id: str,
) -> None:
    required = discover_required_secrets(project_root, [job.module_path for job in jobs])
    if not required:
        return
    environment = resolve_secret_environment()
    existing_names = {
        metadata.name for metadata in client.secrets.list(environment=environment)
    }
    missing = [name for name in required if name not in existing_names]
    if not missing:
        return

    unresolved: list[str] = []
    for name in missing:
        env_name = f"GUM_SECRET_{name}"
        env_value = os.environ.get(env_name)
        if env_value is None or not env_value.strip():
            unresolved.append(name)
            continue
        client.secrets.set(name, env_value.strip(), environment=environment)

    if not unresolved:
        return

    if _stdin_is_tty():
        for name in unresolved:
            value = getpass.getpass(
                f"Missing secret {name} (env={environment}). Enter value (input hidden): "
            ).strip()
            if not value:
                raise DeployError(f"secret {name} cannot be empty")
            client.secrets.set(name, value, environment=environment)
        return

    joined = ", ".join(unresolved)
    hints = "\n".join(
        [f"  - export GUM_SECRET_{name}='<value>'" for name in unresolved]
    )
    raise DeployError(
        f"missing required secrets for project={project_id} env={environment}: {joined}\n"
        "Provide them in CI before deploy, for example:\n"
        f"{hints}"
    )


def discover_required_secrets(project_root: Path, module_paths: list[str]) -> list[str]:
    root = project_root.resolve()
    required: set[str] = set()
    for module_path in sorted(set(module_paths)):
        path = root / module_path
        if not path.exists():
            continue
        tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
        required.update(_discover_provider_required_secrets(tree))
        required.update(_discover_env_secret_names(tree))
    return sorted(required)


def resolve_secret_environment() -> str:
    raw = os.environ.get("GUM_SECRET_ENV", DEFAULT_SECRET_ENV).strip()
    if not raw:
        return DEFAULT_SECRET_ENV
    return raw


def _discover_provider_required_secrets(tree: ast.Module) -> set[str]:
    required: set[str] = set()
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            for alias in node.names:
                provider = alias.name.split(".", 1)[0]
                secret_name = _PROVIDER_SECRET_MAP.get(provider)
                if secret_name:
                    required.add(secret_name)
        elif isinstance(node, ast.ImportFrom):
            if node.module is None:
                continue
            provider = node.module.split(".", 1)[0]
            secret_name = _PROVIDER_SECRET_MAP.get(provider)
            if secret_name:
                required.add(secret_name)
    return required


def _discover_env_secret_names(tree: ast.Module) -> set[str]:
    names: set[str] = set()
    for node in ast.walk(tree):
        name = _secret_name_from_environ_subscript(node)
        if name and _looks_like_secret_name(name):
            names.add(name)
            continue
        name = _secret_name_from_environ_get(node)
        if name and _looks_like_secret_name(name):
            names.add(name)
            continue
        name = _secret_name_from_getenv(node)
        if name and _looks_like_secret_name(name):
            names.add(name)
    return names


def _secret_name_from_environ_subscript(node: ast.AST) -> str | None:
    if not isinstance(node, ast.Subscript):
        return None
    if not _is_os_environ(node.value):
        return None
    return _literal_string(node.slice)


def _secret_name_from_environ_get(node: ast.AST) -> str | None:
    if not isinstance(node, ast.Call):
        return None
    target = node.func
    if not isinstance(target, ast.Attribute):
        return None
    if target.attr != "get":
        return None
    if not _is_os_environ(target.value):
        return None
    if not node.args:
        return None
    return _literal_string(node.args[0])


def _secret_name_from_getenv(node: ast.AST) -> str | None:
    if not isinstance(node, ast.Call):
        return None
    target = node.func
    if not isinstance(target, ast.Attribute):
        return None
    if target.attr != "getenv":
        return None
    if not isinstance(target.value, ast.Name) or target.value.id != "os":
        return None
    if not node.args:
        return None
    return _literal_string(node.args[0])


def _is_os_environ(node: ast.AST) -> bool:
    return (
        isinstance(node, ast.Attribute)
        and node.attr == "environ"
        and isinstance(node.value, ast.Name)
        and node.value.id == "os"
    )


def _literal_string(node: ast.AST) -> str | None:
    if isinstance(node, ast.Constant) and isinstance(node.value, str):
        return node.value
    return None


def _looks_like_secret_name(name: str) -> bool:
    if not _SECRET_NAME_PATTERN.fullmatch(name):
        return False
    return (
        name.endswith("_KEY")
        or name.endswith("_TOKEN")
        or name.endswith("_SECRET")
        or name.endswith("_PASSWORD")
    )


def _stdin_is_tty() -> bool:
    return bool(getattr(sys.stdin, "isatty", lambda: False)())


def _maybe_auto_sync_runner_capacity(jobs: list[DiscoveredJob]) -> None:
    auto_raw = os.environ.get("GUM_AUTO_SYNC_RUNNER_CAPACITY")
    auto_enabled = auto_raw is not None and auto_raw.strip().lower() in {
        "1",
        "true",
        "yes",
        "on",
    }
    auto_disabled = auto_raw is not None and auto_raw.strip().lower() in {
        "0",
        "false",
        "no",
        "off",
    }
    if auto_disabled:
        return

    try:
        provisioner = provisioner_from_env()
    except RuntimeError as exc:
        if auto_enabled:
            raise DeployError(f"runner capacity auto-sync failed: {exc}") from exc
        return

    parallelism_raw = os.environ.get("GUM_RUNNER_PARALLELISM", "1").strip()
    if not parallelism_raw.isdigit() or int(parallelism_raw) <= 0:
        raise DeployError("GUM_RUNNER_PARALLELISM must be a positive integer")
    parallelism = int(parallelism_raw)
    compute_class = os.environ.get("GUM_RUNNER_COMPUTE_CLASS", "standard")

    plan = build_runner_capacity_plan(
        jobs,
        compute_class=compute_class,
        parallelism=parallelism,
    )
    try:
        provisioner.sync(plan)
    except RuntimeError as exc:
        raise DeployError(f"runner capacity auto-sync failed: {exc}") from exc


def _inline_bundle_url(bundle_path: Path) -> str:
    encoded = base64.urlsafe_b64encode(bundle_path.read_bytes()).decode("ascii")
    return f"inline://{encoded.rstrip('=')}"


def init_project(
    project_root: Path | str | None = None,
    *,
    project_id: str = DEFAULT_PROJECT_ID,
    api_base_url: str = DEFAULT_API_BASE_URL,
    force: bool = False,
) -> InitResult:
    root = Path(project_root or os.getcwd()).resolve()
    root.mkdir(parents=True, exist_ok=True)
    created: list[Path] = []
    kept: list[Path] = []

    config_path = root / CONFIG_FILE_NAME
    env_example_path = root / ".env.example"
    pyproject_path = root / "pyproject.toml"
    jobs_path = root / "jobs.py"

    _write_file(
        config_path,
        _render_gum_toml(project_id=project_id, api_base_url=api_base_url),
        force=force,
        created=created,
        kept=kept,
    )
    _write_file(
        env_example_path,
        _render_env_example(api_base_url=api_base_url, project_id=project_id),
        force=force,
        created=created,
        kept=kept,
    )
    _write_file(
        pyproject_path,
        _render_pyproject(project_name=root.name),
        force=False,
        created=created,
        kept=kept,
    )
    _write_file(
        jobs_path,
        _render_jobs_py(),
        force=False,
        created=created,
        kept=kept,
    )
    return InitResult(
        project_root=root,
        config_path=config_path,
        env_example_path=env_example_path,
        pyproject_path=pyproject_path,
        jobs_path=jobs_path,
        created=created,
        kept=kept,
    )


def load_project_config(project_root: Path | str | None = None) -> GumProjectConfig:
    root = Path(project_root or os.getcwd()).resolve()
    config_path = root / CONFIG_FILE_NAME
    if not config_path.exists():
        return GumProjectConfig()
    values: dict[str, str] = {}
    for raw_line in config_path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, raw_value = line.split("=", 1)
        key = key.strip()
        value = raw_value.strip().strip('"').strip("'")
        values[key] = value
    return GumProjectConfig(
        project_id=values.get("project_id") or None,
        api_base_url=values.get("api_base_url") or None,
    )


def resolve_project_id(
    project_root: Path | str | None = None,
    explicit_project_id: str | None = None,
) -> str:
    if explicit_project_id:
        return explicit_project_id
    env_project_id = os.environ.get("GUM_PROJECT_ID")
    if env_project_id:
        return env_project_id
    config = load_project_config(project_root)
    return config.project_id or DEFAULT_PROJECT_ID


def resolve_api_base_url(
    project_root: Path | str | None = None,
    explicit_api_base_url: str | None = None,
) -> str:
    if explicit_api_base_url:
        return explicit_api_base_url
    env_base_url = os.environ.get("GUM_API_BASE_URL")
    if env_base_url:
        return env_base_url
    config = load_project_config(project_root)
    return config.api_base_url or DEFAULT_API_BASE_URL


def client_from_project(
    project_root: Path | str | None = None,
    *,
    api_base_url: str | None = None,
) -> GumClient:
    if api_base_url is None and os.environ.get("GUM_API_BASE_URL") is not None:
        return default_client()
    return GumClient(
        base_url=resolve_api_base_url(project_root, api_base_url),
        api_key=default_api_key(),
        admin_key=default_admin_key(),
    )


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
        elif keyword.arg == "cron":
            config.cron = str(value)
        elif keyword.arg == "timezone":
            config.timezone = str(value)
        elif keyword.arg == "retries":
            config.retries = int(value)
        elif keyword.arg == "timeout":
            config.timeout = str(value)
        elif keyword.arg == "rate_limit":
            config.rate_limit = value
        elif keyword.arg == "concurrency":
            config.concurrency = int(value)
        elif keyword.arg == "cpu":
            config.cpu = int(value)
        elif keyword.arg == "memory":
            config.memory = str(value)
        elif keyword.arg == "key":
            config.key = str(value)
        elif keyword.arg == "compute_class":
            config.compute_class = str(value)
        elif keyword.arg == "compute":
            config.compute = str(value)
    if config.compute is not None and config.compute_class is not None:
        raise DeployError("only one of compute_class or compute may be set")
    return config


def _resolve_schedule_expr(config: _AstJobConfig) -> str | None:
    if config.every and config.cron:
        raise DeployError("only one of every or cron may be set")
    if config.timezone and not config.cron:
        raise DeployError("timezone requires cron")
    if config.cron:
        cron = str(config.cron).strip()
        if not cron:
            raise DeployError("cron must not be empty")
        if config.timezone is not None:
            timezone = str(config.timezone).strip()
            if not timezone:
                raise DeployError("timezone must not be empty")
            return f"cron:tz={timezone};{cron}"
        return f"cron:{cron}"
    return config.every


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


def _parse_cpu_cores(raw: int) -> int:
    if raw <= 0:
        raise DeployError("cpu must be positive")
    return raw


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


def _write_file(
    path: Path,
    content: str,
    *,
    force: bool,
    created: list[Path],
    kept: list[Path],
) -> None:
    if path.exists() and not force:
        kept.append(path)
        return
    path.write_text(content, encoding="utf-8")
    created.append(path)


def _render_gum_toml(*, project_id: str, api_base_url: str) -> str:
    return (
        "# Gum project config\n"
        f'project_id = "{project_id}"\n'
        f'api_base_url = "{api_base_url}"\n'
    )


def _render_env_example(*, api_base_url: str, project_id: str) -> str:
    return (
        f'# GUM_API_BASE_URL="{api_base_url}"\n'
        f'GUM_PROJECT_ID="{project_id}"\n'
        'GUM_API_KEY="gum_live_..."\n'
    )


def _render_pyproject(*, project_name: str) -> str:
    normalized_name = project_name.replace("_", "-").lower() or "gum-project"
    return (
        "[project]\n"
        f'name = "{normalized_name}"\n'
        'version = "0.1.0"\n'
        'requires-python = ">=3.10"\n'
        'dependencies = ["usegum"]\n'
    )


def _render_jobs_py() -> str:
    return (
        "import gum\n\n\n"
        '@gum.job(retries=3, timeout="5m", memory="512mb")\n'
        "def hello(name: str) -> str:\n"
        '    print(f"hello {name}")\n'
        "    return name\n"
    )

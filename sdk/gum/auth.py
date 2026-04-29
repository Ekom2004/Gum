from __future__ import annotations

import os
import shutil
import subprocess
import json
from pathlib import Path


class AdminAuthError(RuntimeError):
    pass


class UserAuthError(RuntimeError):
    pass


def default_admin_key() -> str | None:
    return os.environ.get("GUM_ADMIN_KEY")


def default_api_key() -> str | None:
    value = os.environ.get("GUM_API_KEY")
    if value:
        return value
    return load_stored_api_key()


def default_api_base_url(default_base_url: str) -> str:
    value = os.environ.get("GUM_API_BASE_URL")
    if value:
        return value
    profile = _load_profile()
    stored = profile.get("api_base_url")
    if isinstance(stored, str) and stored.strip():
        return stored.strip()
    return default_base_url


def store_api_credentials(api_key: str, *, api_base_url: str | None = None) -> None:
    normalized_key = api_key.strip()
    if not normalized_key:
        raise UserAuthError("api key cannot be empty")
    profile = _load_profile()
    profile["api_key"] = normalized_key
    if api_base_url is not None and api_base_url.strip():
        profile["api_base_url"] = api_base_url.strip()
    _store_profile(profile)


def clear_api_credentials() -> bool:
    profile = _load_profile()
    had_key = "api_key" in profile
    if not had_key:
        return False
    profile.pop("api_key", None)
    _store_profile(profile)
    return True


def store_admin_key(admin_key: str, passphrase: str) -> None:
    if not admin_key:
        raise AdminAuthError("admin key cannot be empty")
    if not passphrase:
        raise AdminAuthError("passphrase cannot be empty")

    openssl = _openssl_path()
    credentials_path = admin_credentials_path()
    credentials_path.parent.mkdir(parents=True, exist_ok=True)
    env = {**os.environ, "GUM_ADMIN_PASSPHRASE": passphrase}
    result = subprocess.run(
        [
            openssl,
            "enc",
            "-aes-256-cbc",
            "-pbkdf2",
            "-salt",
            "-pass",
            "env:GUM_ADMIN_PASSPHRASE",
            "-out",
            str(credentials_path),
        ],
        input=admin_key.encode("utf-8"),
        capture_output=True,
        env=env,
        check=False,
    )
    if result.returncode != 0:
        stderr = result.stderr.decode("utf-8", errors="replace").strip()
        raise AdminAuthError(f"failed to store admin key: {stderr or 'openssl error'}")
    credentials_path.chmod(0o600)


def load_admin_key(passphrase: str) -> str:
    if not passphrase:
        raise AdminAuthError("passphrase cannot be empty")
    credentials_path = admin_credentials_path()
    if not credentials_path.exists():
        raise AdminAuthError("no stored admin credentials; run `gum admin login` first")

    openssl = _openssl_path()
    env = {**os.environ, "GUM_ADMIN_PASSPHRASE": passphrase}
    result = subprocess.run(
        [
            openssl,
            "enc",
            "-d",
            "-aes-256-cbc",
            "-pbkdf2",
            "-pass",
            "env:GUM_ADMIN_PASSPHRASE",
            "-in",
            str(credentials_path),
        ],
        capture_output=True,
        env=env,
        check=False,
    )
    if result.returncode != 0:
        raise AdminAuthError("invalid passphrase")

    admin_key = result.stdout.decode("utf-8", errors="strict").strip()
    if not admin_key:
        raise AdminAuthError("stored admin key is empty")
    return admin_key


def clear_admin_key() -> bool:
    credentials_path = admin_credentials_path()
    if credentials_path.exists():
        credentials_path.unlink()
        return True
    return False


def admin_credentials_path() -> Path:
    return gum_config_root() / "admin.enc"


def gum_config_root() -> Path:
    root = os.environ.get("GUM_HOME")
    if root:
        return Path(root).expanduser()
    return Path.home() / ".config" / "gum"


def _openssl_path() -> str:
    openssl = shutil.which("openssl")
    if openssl is None:
        raise AdminAuthError("openssl is required for admin credential storage")
    return openssl


def profile_path() -> Path:
    return gum_config_root() / "profile.json"


def _load_profile() -> dict[str, object]:
    path = profile_path()
    if not path.exists():
        return {}
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        raise UserAuthError("stored user profile is invalid JSON") from exc
    if not isinstance(data, dict):
        raise UserAuthError("stored user profile is invalid")
    return data


def _store_profile(profile: dict[str, object]) -> None:
    path = profile_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(profile, indent=2) + "\n", encoding="utf-8")
    path.chmod(0o600)


def load_stored_api_key() -> str | None:
    profile = _load_profile()
    value = profile.get("api_key")
    if isinstance(value, str) and value.strip():
        return value.strip()
    return None

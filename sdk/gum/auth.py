from __future__ import annotations

import os
import shutil
import subprocess
from pathlib import Path


class AdminAuthError(RuntimeError):
    pass


def default_admin_key() -> str | None:
    return os.environ.get("GUM_ADMIN_KEY")


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
    root = os.environ.get("GUM_HOME")
    if root:
        return Path(root).expanduser() / "admin.enc"
    return Path.home() / ".config" / "gum" / "admin.enc"


def _openssl_path() -> str:
    openssl = shutil.which("openssl")
    if openssl is None:
        raise AdminAuthError("openssl is required for admin credential storage")
    return openssl

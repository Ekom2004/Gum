from __future__ import annotations

import argparse
import asyncio
import importlib
import inspect
import json
import sys
import traceback
from typing import Any


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="python -m gum.runtime")
    parser.add_argument("--handler", required=True)
    parser.add_argument("--payload-json", required=True)
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--attempt", required=True, type=int)
    args = parser.parse_args(argv)

    try:
        payload = json.loads(args.payload_json)
        if not isinstance(payload, dict):
            raise RuntimeError("Gum job payload must decode to a JSON object.")

        module_name, fn_name = _parse_handler(args.handler)
        module = importlib.import_module(module_name)
        target = getattr(module, fn_name, None)
        if target is None:
            raise RuntimeError(f"Handler {args.handler!r} was not found.")

        result = target(**payload)
        if inspect.isawaitable(result):
            asyncio.run(_await_result(result))
        return 0
    except Exception as exc:
        traceback.print_exc()
        print(
            "__gum_failure__="
            + json.dumps(
                {
                    "failure_class": _classify_exception(exc),
                    "message": str(exc) or exc.__class__.__name__,
                }
            ),
            file=sys.stderr,
        )
        return 1


async def _await_result(result: Any) -> Any:
    return await result


def _parse_handler(handler: str) -> tuple[str, str]:
    if ":" not in handler:
        raise RuntimeError("Handler ref must look like 'module:function'.")
    module_name, fn_name = handler.split(":", 1)
    if not module_name or not fn_name:
        raise RuntimeError("Handler ref must include both module and function name.")
    return module_name, fn_name


def _classify_exception(exc: Exception) -> str:
    status_code = _extract_status_code(exc)
    if status_code is not None:
        if status_code == 429:
            return "provider_429"
        if status_code in {401, 403}:
            return "provider_auth_error"
        if 500 <= status_code <= 599:
            return "provider_5xx"
        if status_code == 408:
            return "provider_timeout"
        if 400 <= status_code <= 499:
            return "user_code_error"

    if isinstance(exc, TimeoutError):
        return "provider_timeout"
    if isinstance(exc, ConnectionError):
        return "provider_connect_error"

    exception_name = exc.__class__.__name__.lower()
    module_name = exc.__class__.__module__.lower()
    if "timeout" in exception_name or "timeout" in module_name:
        return "provider_timeout"
    if "connect" in exception_name or "connection" in exception_name:
        return "provider_connect_error"
    return "user_code_error"


def _extract_status_code(exc: Exception) -> int | None:
    status_code = getattr(exc, "status_code", None)
    if isinstance(status_code, int):
        return status_code

    response = getattr(exc, "response", None)
    response_status = getattr(response, "status_code", None)
    if isinstance(response_status, int):
        return response_status
    return None


if __name__ == "__main__":
    raise SystemExit(main())

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
    except Exception:
        traceback.print_exc()
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


if __name__ == "__main__":
    raise SystemExit(main())

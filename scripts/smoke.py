#!/usr/bin/env python3
"""Perform a bounded MCP transport and initialize smoke check."""

from __future__ import annotations

import argparse
import json
import os
import sys
import urllib.error
import urllib.request
from typing import Any

PROTOCOL_VERSION = "2025-06-18"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Initialize a running local Ordivon Runtime server.")
    parser.add_argument("--endpoint", default=os.environ.get("ORDIVON_ENDPOINT", "http://127.0.0.1:8897/mcp"))
    parser.add_argument("--token", default=os.environ.get("ORDIVON_BEARER_TOKEN"))
    parser.add_argument("--timeout", type=float, default=5.0)
    return parser.parse_args()


def decode_response(content_type: str, body: bytes) -> dict[str, Any]:
    text = body.decode("utf-8")
    if "text/event-stream" in content_type:
        data_lines = [line[5:].strip() for line in text.splitlines() if line.startswith("data:")]
        if not data_lines:
            raise ValueError("SSE response contained no data event")
        return json.loads(data_lines[-1])
    return json.loads(text)


def initialize(endpoint: str, token: str, timeout: float) -> dict[str, Any]:
    payload = {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {"name": "ordivon-smoke", "version": "1"},
        },
    }
    request = urllib.request.Request(
        endpoint,
        data=json.dumps(payload).encode("utf-8"),
        method="POST",
        headers={
            "Authorization": f"Bearer {token}",
            "Content-Type": "application/json",
            "Accept": "application/json, text/event-stream",
            "MCP-Protocol-Version": PROTOCOL_VERSION,
        },
    )
    with urllib.request.urlopen(request, timeout=timeout) as response:
        return decode_response(response.headers.get("Content-Type", ""), response.read())


def main() -> int:
    args = parse_args()
    if not args.token or len(args.token) < 32:
        raise ValueError("provide --token or ORDIVON_BEARER_TOKEN with at least 32 characters")
    if args.timeout <= 0 or args.timeout > 60:
        raise ValueError("timeout must be in (0, 60]")

    message = initialize(args.endpoint, args.token, args.timeout)
    if message.get("id") != 1 or "error" in message:
        raise ValueError(f"initialize failed: {message}")
    result = message.get("result")
    if not isinstance(result, dict) or not isinstance(result.get("serverInfo"), dict):
        raise ValueError("initialize response omitted serverInfo")
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, json.JSONDecodeError, urllib.error.URLError) as error:
        print(f"smoke failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error

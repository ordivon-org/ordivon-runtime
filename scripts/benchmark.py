#!/usr/bin/env python3
"""Capture a bounded exact-head build and MCP initialization benchmark snapshot."""

from __future__ import annotations

import argparse
import json
import os
import platform
import secrets
import shutil
import signal
import socket
import statistics
import subprocess
import sys
import time
import urllib.error
import urllib.request
import uuid
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


PROTOCOL_VERSION = "2025-06-18"


def run(argv: list[str], cwd: Path) -> tuple[int, str, str]:
    started = time.monotonic()
    process = subprocess.run(argv, cwd=cwd, text=True, capture_output=True)
    if process.returncode != 0:
        raise subprocess.CalledProcessError(
            process.returncode, argv, output=process.stdout, stderr=process.stderr
        )
    return round((time.monotonic() - started) * 1000), process.stdout, process.stderr


def git_value(repo: Path, *args: str) -> str:
    return subprocess.run(["git", *args], cwd=repo, check=True, text=True, capture_output=True).stdout.strip()


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
        listener.bind(("127.0.0.1", 0))
        return int(listener.getsockname()[1])


def initialize(endpoint: str, token: str, request_id: int, timeout: float = 10.0) -> dict[str, Any]:
    payload = {
        "jsonrpc": "2.0",
        "id": request_id,
        "method": "initialize",
        "params": {
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {"name": "ordivon-benchmark", "version": "1"},
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
        result = json.loads(response.read().decode("utf-8"))
    if result.get("id") != request_id or "error" in result:
        raise ValueError(f"initialize failed: {result}")
    return result


def percentile(values: list[float], fraction: float) -> float:
    ordered = sorted(values)
    index = min(len(ordered) - 1, max(0, round((len(ordered) - 1) * fraction)))
    return ordered[index]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Capture a bounded current-version benchmark snapshot.")
    parser.add_argument("--repo", type=Path, default=Path.cwd())
    parser.add_argument("--output", type=Path)
    parser.add_argument("--samples", type=int, default=20)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    repo = args.repo.expanduser().resolve()
    if args.samples < 5 or args.samples > 100:
        raise ValueError("samples must be in 5..=100")
    if git_value(repo, "status", "--porcelain"):
        raise ValueError("benchmark requires a clean exact-head worktree")

    started_at = datetime.now(timezone.utc).isoformat()
    build_runner_ms, runner_stdout, runner_stderr = run(
        ["cargo", "build", "--release", "-p", "ordivon-exec", "--bin", "ordivon-task-runner"], repo
    )
    if runner_stderr and "Finished" not in runner_stderr and "Compiling" not in runner_stderr:
        pass
    runner = repo / "target/release/ordivon-task-runner"
    if not runner.is_file():
        raise RuntimeError(f"release runner build failed: {runner_stdout}\n{runner_stderr}")

    build_server_ms, server_stdout, server_stderr = run(
        ["cargo", "build", "--release", "-p", "ordivon-mcp"], repo
    )
    server_binary = repo / "target/release/ordivon-mcp"
    if not server_binary.is_file():
        raise RuntimeError(f"release server build failed: {server_stdout}\n{server_stderr}")

    root = Path("/root/.local/share/ordivon-acceptance") / f"benchmark-{uuid.uuid4()}"
    root.mkdir(parents=True)
    token = secrets.token_urlsafe(48)
    port = free_port()
    endpoint = f"http://127.0.0.1:{port}/mcp"
    log_path = root / "server.log"
    log_handle = log_path.open("w", encoding="utf-8")
    env = os.environ.copy()
    env.update(
        {
            "RUST_LOG": "warn",
            "ORDIVON_BIND": f"127.0.0.1:{port}",
            "ORDIVON_BEARER_TOKEN": token,
            "ORDIVON_STORE_ROOT": str(root / "store"),
            "ORDIVON_REGISTRY_ROOT": str(root / "registry"),
            "ORDIVON_RUNNER_PATH": str(runner),
            "ORDIVON_ALLOWED_EXECUTABLE_ROOTS": "/usr/bin:/usr/local/bin",
            "ORDIVON_EXECUTION_MODE": "trusted-local",
        }
    )
    process = subprocess.Popen(
        [str(server_binary)], cwd=repo, env=env, text=True, stdout=log_handle, stderr=subprocess.STDOUT
    )
    log_handle.close()
    startup_started = time.monotonic()
    last_error: Exception | None = None
    try:
        request_id = 1
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            if process.poll() is not None:
                raise RuntimeError(f"server exited during startup: {log_path.read_text(errors='replace')[-4000:]}")
            try:
                initialize(endpoint, token, request_id)
                break
            except (OSError, ValueError, urllib.error.URLError) as error:
                last_error = error
                time.sleep(0.05)
        else:
            raise RuntimeError(f"server startup timed out: {last_error}")
        startup_initialize_ms = round((time.monotonic() - startup_started) * 1000, 3)

        latencies: list[float] = []
        for request_id in range(2, args.samples + 2):
            started = time.perf_counter_ns()
            initialize(endpoint, token, request_id)
            latencies.append((time.perf_counter_ns() - started) / 1_000_000)
    finally:
        if process.poll() is None:
            process.send_signal(signal.SIGINT)
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=3)

    report = {
        "schemaVersion": 1,
        "kind": "bounded-current-version-snapshot",
        "startedAt": started_at,
        "finishedAt": datetime.now(timezone.utc).isoformat(),
        "head": git_value(repo, "rev-parse", "HEAD"),
        "branch": git_value(repo, "branch", "--show-current"),
        "environment": {
            "platform": platform.platform(),
            "python": platform.python_version(),
            "rustc": subprocess.run(["rustc", "--version"], check=True, text=True, capture_output=True).stdout.strip(),
            "systemd": subprocess.run(["systemctl", "--version"], check=True, text=True, capture_output=True).stdout.splitlines()[0],
            "note": "Build cache state and local machine load are uncontrolled; do not compare without matching conditions.",
        },
        "build": {
            "runnerMs": build_runner_ms,
            "serverMs": build_server_ms,
            "runnerBytes": runner.stat().st_size,
            "serverBytes": server_binary.stat().st_size,
        },
        "initialize": {
            "startupAndFirstRequestMs": startup_initialize_ms,
            "samples": len(latencies),
            "minimumMs": round(min(latencies), 3),
            "medianMs": round(statistics.median(latencies), 3),
            "p95Ms": round(percentile(latencies, 0.95), 3),
            "maximumMs": round(max(latencies), 3),
        },
        "scope": "Loopback MCP initialize only; no inference to execution throughput or production workload.",
    }
    output = (
        args.output.expanduser().resolve()
        if args.output
        else repo / ".ordivon/benchmarks" / f"{report['head']}.json"
    )
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    shutil.rmtree(root, ignore_errors=True)
    print(output)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError, subprocess.CalledProcessError) as error:
        print(f"benchmark failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error

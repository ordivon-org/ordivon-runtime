#!/usr/bin/env python3
"""Reproducible local lifecycle controller for Ordivon M7 hardened transactional dogfood."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import secrets
import shutil
import signal
import socket
import sqlite3
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[2]
LOCK_PATH = Path(__file__).with_name("sdk-lock.json")
DEFAULT_STATE_ROOT = Path("/run/ordivon/m7-stack")
MANIFEST_NAME = "run.json"


def run(
    args: list[str],
    *,
    cwd: Path = REPO_ROOT,
    env: dict[str, str] | None = None,
    check: bool = True,
) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(
        args,
        cwd=cwd,
        env=env,
        text=True,
        capture_output=True,
        check=False,
    )
    if check and result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip()
        raise RuntimeError(f"{' '.join(args)} failed ({result.returncode}): {detail}")
    return result


def sha256_file(path: Path) -> str:
    return "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()


def atomic_json(path: Path, value: dict[str, Any], mode: int = 0o600) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, indent=2) + "\n")
    temporary.chmod(mode)
    temporary.replace(path)


def discover_sdk_root() -> Path:
    lock = json.loads(LOCK_PATH.read_text())
    expected_version = lock["version"]
    expected_digest = lock["packageJsonSha256"]
    override = os.environ.get("ORDIVON_MCP_SDK_ROOT")
    candidates: list[Path] = []
    if override:
        candidates.append(Path(override))
    candidates.extend(
        path.parent
        for path in Path("/root/.npm/_npx").glob(
            "*/node_modules/@modelcontextprotocol/sdk/package.json"
        )
    )
    accepted: list[Path] = []
    for root in candidates:
        package_json = root / "package.json"
        if not package_json.is_file():
            continue
        try:
            package = json.loads(package_json.read_text())
        except json.JSONDecodeError:
            continue
        if package.get("version") != expected_version:
            continue
        if sha256_file(package_json) != expected_digest:
            continue
        accepted.append(root.resolve())
    if not accepted:
        raise RuntimeError(
            f"locked MCP SDK {expected_version} with digest {expected_digest} not found"
        )
    return sorted(set(accepted))[0]


def git_head() -> str:
    return run(["git", "rev-parse", "HEAD"]).stdout.strip()


def allocate_port() -> int:
    with socket.socket() as candidate:
        candidate.bind(("127.0.0.1", 0))
        return int(candidate.getsockname()[1])


def build_binaries() -> tuple[Path, Path]:
    run(
        [
            "cargo",
            "build",
            "-p",
            "ordivon-mcp",
            "--features",
            "experimental-http-m7",
            "--bin",
            "ordivon-m7-http",
        ]
    )
    run(
        [
            "cargo",
            "build",
            "-p",
            "ordivon-exec",
            "--features",
            "runtime-hardening-m7",
            "--bin",
            "ordivon-task-runner",
        ]
    )
    server = (REPO_ROOT / "target/debug/ordivon-m7-http").resolve()
    runner = (REPO_ROOT / "target/debug/ordivon-task-runner").resolve()
    if not server.is_file() or not runner.is_file():
        raise RuntimeError("M7 binaries were not produced")
    return server, runner


def initialize_request() -> bytes:
    body = {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": {"name": "ordivon-m6-stack", "version": "1"},
        },
    }
    return json.dumps(body).encode()


def wait_healthy(endpoint: str, token: str, timeout_seconds: float = 10.0) -> None:
    deadline = time.monotonic() + timeout_seconds
    last_error = "not attempted"
    while time.monotonic() < deadline:
        request = urllib.request.Request(
            endpoint,
            data=initialize_request(),
            method="POST",
            headers={
                "Authorization": f"Bearer {token}",
                "Content-Type": "application/json",
                "Accept": "application/json, text/event-stream",
            },
        )
        try:
            with urllib.request.urlopen(request, timeout=1.0) as response:
                if response.status == 200:
                    return
                last_error = f"HTTP {response.status}"
        except (urllib.error.URLError, TimeoutError) as error:
            last_error = str(error)
        time.sleep(0.1)
    raise RuntimeError(f"experimental MCP did not become healthy: {last_error}")


def load_manifest(state_root: Path) -> dict[str, Any]:
    path = state_root / MANIFEST_NAME
    if not path.is_file():
        raise RuntimeError(f"no active M7 stack manifest at {path}")
    return json.loads(path.read_text())


def remove_worktrees(workspace_root: Path) -> list[str]:
    removed: list[str] = []
    listing = run(["git", "worktree", "list", "--porcelain"]).stdout.splitlines()
    prefix = str(workspace_root.resolve()) + os.sep
    for line in listing:
        if not line.startswith("worktree "):
            continue
        path = Path(line.removeprefix("worktree ")).resolve()
        if str(path).startswith(prefix):
            run(["git", "worktree", "remove", "--force", str(path)], check=False)
            removed.append(str(path))
    run(["git", "worktree", "prune"], check=False)
    return removed


def registered_attempt_units(registry_root: Path) -> list[str]:
    database = registry_root / "registry.sqlite3"
    if not database.is_file():
        return []
    try:
        connection = sqlite3.connect(f"file:{database}?mode=ro", uri=True, timeout=1.0)
        try:
            rows = connection.execute("SELECT unit_name FROM attempts").fetchall()
        finally:
            connection.close()
    except sqlite3.Error:
        return []
    return sorted({str(row[0]) for row in rows if row and row[0]})


def stop_stack(state_root: Path, *, tolerate_missing: bool = False) -> dict[str, Any]:
    manifest_path = state_root / MANIFEST_NAME
    if not manifest_path.is_file():
        if tolerate_missing:
            shutil.rmtree(state_root, ignore_errors=True)
            return {"stopped": False, "reason": "missing"}
        raise RuntimeError(f"no active M7 stack manifest at {manifest_path}")
    manifest = json.loads(manifest_path.read_text())
    server_unit = manifest["unit"]
    registry_root = Path(manifest["registryRoot"])
    attempt_units = registered_attempt_units(registry_root)
    for unit in attempt_units:
        run(["systemctl", "stop", unit], check=False)
        run(["systemctl", "reset-failed", unit], check=False)
    run(["systemctl", "stop", server_unit], check=False)
    run(["systemctl", "reset-failed", server_unit], check=False)
    removed = remove_worktrees(Path(manifest["workspaceRoot"]))
    for item in manifest.get("temporaryFiles", []):
        Path(item).unlink(missing_ok=True)
    for key in ["runtimeViewRoot", "cacheRoot", "workerRoot", "controlRoot"]:
        value = manifest.get(key)
        if value:
            shutil.rmtree(value, ignore_errors=True)
    shutil.rmtree(state_root, ignore_errors=True)
    return {
        "stopped": True,
        "unit": server_unit,
        "attemptUnits": attempt_units,
        "removedWorktrees": removed,
    }

def start_stack(state_root: Path, port: int | None) -> dict[str, Any]:
    stop_stack(state_root, tolerate_missing=True)
    sdk_root = discover_sdk_root()
    server, built_runner = build_binaries()
    installed_runner = Path("/usr/lib/ordivon/ordivon-task-runner")
    run(["install", "-d", "-o", "root", "-g", "root", "-m", "0755", "/usr/lib/ordivon"])
    run(["install", "-o", "root", "-g", "root", "-m", "0555", str(built_runner), str(installed_runner)])
    runner = installed_runner.resolve()
    cargo_path = Path(run(["rustup", "which", "cargo"]).stdout.strip()).resolve()
    rustc_path = Path(run(["rustup", "which", "rustc"]).stdout.strip()).resolve()
    selected_port = port or allocate_port()
    run_id = f"{os.getpid()}-{int(time.time() * 1000)}"
    unit = f"ordivon-m7-http-{run_id}.service"
    control_root = Path("/var/lib/ordivon/control/m7-runs") / run_id
    worker_root = Path("/var/lib/ordivon/worker/m7-runs") / run_id
    cache_root = Path("/var/cache/ordivon-worker/m7-runs") / run_id
    runtime_view_root = Path("/run/ordivon/m7-runs") / run_id
    store_root = control_root / "executor"
    registry_root = control_root / "registry"
    worker_entry = run(["getent", "passwd", "ordivon-worker"]).stdout.strip().split(":")
    if len(worker_entry) < 4:
        raise RuntimeError("ordivon-worker account is unavailable")
    worker_uid, worker_gid = worker_entry[2], worker_entry[3]
    token = secrets.token_hex(32)
    environment_file = (state_root / "server.env").resolve()
    trace_path = (registry_root / "m7-core-trace.jsonl").resolve()
    state_root.mkdir(parents=True, exist_ok=True)
    state_root.chmod(0o700)
    environment_file.write_text(
        "\n".join(
            [
                f"ORDIVON_M7_BIND=127.0.0.1:{selected_port}",
                f"ORDIVON_M7_STORE_ROOT={store_root}",
                f"ORDIVON_M7_REGISTRY_ROOT={registry_root}",
                f"ORDIVON_M7_CONTROL_ROOT={control_root}",
                f"ORDIVON_M7_WORKER_ROOT={worker_root}",
                f"ORDIVON_M7_CACHE_ROOT={cache_root}",
                f"ORDIVON_M7_RUNTIME_VIEW_ROOT={runtime_view_root}",
                f"ORDIVON_M7_WORKER_UID={worker_uid}",
                f"ORDIVON_M7_WORKER_GID={worker_gid}",
                f"ORDIVON_M7_BEARER_TOKEN={token}",
                f"ORDIVON_M7_RUNNER_PATH={runner}",
                "ORDIVON_M7_ALLOWED_EXECUTABLE_ROOTS=/usr/bin",
                f"ORDIVON_M7_TRACE_PATH={trace_path}",
                f"ORDIVON_M7_ALLOWED_SOURCE_REPOS={REPO_ROOT.resolve()}",
                f"ORDIVON_M7_ALLOWED_SOURCE_REVISIONS={git_head()}",
                "ORDIVON_M7_BUSY_TIMEOUT_MS=5000",
                "ORDIVON_M7_STARTUP_GRACE_MS=2000",
            ]
        )
        + "\n"
    )
    environment_file.chmod(0o600)
    endpoint = f"http://127.0.0.1:{selected_port}/mcp"
    manifest = {
        "schemaVersion": 1,
        "runId": run_id,
        "unit": unit,
        "endpoint": endpoint,
        "token": token,
        "storeRoot": str(store_root),
        "registryRoot": str(registry_root),
        "controlRoot": str(control_root),
        "workerRoot": str(worker_root),
        "cacheRoot": str(cache_root),
        "runtimeViewRoot": str(runtime_view_root),
        "workerUid": int(worker_uid),
        "workerGid": int(worker_gid),
        "workspaceRoot": str(worker_root / "workspaces"),
        "registryDb": str(registry_root / "registry.sqlite3"),
        "repoRoot": str(REPO_ROOT.resolve()),
        "sourceRevision": git_head(),
        "sdkRoot": str(sdk_root),
        "sdkPackageDigest": sha256_file(sdk_root / "package.json"),
        "serverBinary": str(server),
        "serverBinaryDigest": sha256_file(server),
        "runnerBinary": str(runner),
        "runnerBinaryDigest": sha256_file(runner),
        "cargoBinary": str(cargo_path),
        "cargoBinaryDigest": sha256_file(cargo_path),
        "rustcBinary": str(rustc_path),
        "rustcBinaryDigest": sha256_file(rustc_path),
        "tracePath": str(trace_path),
        "httpTracePath": str(Path(str(trace_path) + ".http.jsonl")),
        "temporaryFiles": [str(environment_file)],
        "startedUnixMs": int(time.time() * 1000),
    }
    atomic_json(state_root / MANIFEST_NAME, manifest)
    try:
        run(
            [
                "systemd-run",
                "--unit",
                unit.removesuffix(".service"),
                "--property",
                f"EnvironmentFile={environment_file}",
                "--property",
                f"WorkingDirectory={REPO_ROOT}",
                "--collect",
                str(server),
            ]
        )
        wait_healthy(endpoint, token)
    except Exception:
        stop_stack(state_root, tolerate_missing=True)
        raise
    return manifest

def child_environment(manifest: dict[str, Any]) -> dict[str, str]:
    environment = os.environ.copy()
    environment.update(
        {
            "ORDIVON_M7_MCP_URL": manifest["endpoint"],
            "ORDIVON_M7_BEARER_TOKEN": manifest["token"],
            "ORDIVON_MCP_SDK_ROOT": manifest["sdkRoot"],
            "ORDIVON_M7_STORE_ROOT": manifest["storeRoot"],
            "ORDIVON_M7_REGISTRY_ROOT": manifest["registryRoot"],
            "ORDIVON_M7_REGISTRY_DB": manifest["registryDb"],
            "ORDIVON_M7_CONTROL_ROOT": manifest["controlRoot"],
            "ORDIVON_M7_WORKER_ROOT": manifest["workerRoot"],
            "ORDIVON_M7_CACHE_ROOT": manifest["cacheRoot"],
            "ORDIVON_M7_RUNTIME_VIEW_ROOT": manifest["runtimeViewRoot"],
            "ORDIVON_M7_WORKSPACE_ROOT": manifest["workspaceRoot"],
            "ORDIVON_M7_REPO_ROOT": manifest["repoRoot"],
            "ORDIVON_M7_SOURCE_REVISION": manifest["sourceRevision"],
            "ORDIVON_M7_TRACE_PATH": manifest["tracePath"],
            "ORDIVON_M7_HTTP_TRACE_PATH": manifest["httpTracePath"],
            "ORDIVON_M7_CARGO_BINARY": manifest["cargoBinary"],
            "ORDIVON_M7_RUSTC_BINARY": manifest["rustcBinary"],
        }
    )
    return environment

def run_with_stack(state_root: Path, command: list[str]) -> int:
    manifest = start_stack(state_root, None)
    child: subprocess.Popen[str] | None = None

    def forward_signal(signum: int, _frame: Any) -> None:
        if child is not None and child.poll() is None:
            child.send_signal(signum)

    previous_int = signal.signal(signal.SIGINT, forward_signal)
    previous_term = signal.signal(signal.SIGTERM, forward_signal)
    try:
        child = subprocess.Popen(
            command,
            cwd=REPO_ROOT,
            env=child_environment(manifest),
            text=True,
        )
        return child.wait()
    finally:
        signal.signal(signal.SIGINT, previous_int)
        signal.signal(signal.SIGTERM, previous_term)
        stop_stack(state_root, tolerate_missing=True)


def status(state_root: Path) -> dict[str, Any]:
    manifest = load_manifest(state_root)
    unit = manifest["unit"]
    active = run(["systemctl", "is-active", unit], check=False).stdout.strip()
    return {
        "manifest": manifest,
        "unitState": active,
        "storeExists": Path(manifest["storeRoot"]).exists(),
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--state-root", type=Path, default=DEFAULT_STATE_ROOT)
    subparsers = parser.add_subparsers(dest="command", required=True)
    start_parser = subparsers.add_parser("start")
    start_parser.add_argument("--port", type=int)
    subparsers.add_parser("stop")
    subparsers.add_parser("status")
    subparsers.add_parser("doctor")
    exec_parser = subparsers.add_parser("exec")
    exec_parser.add_argument("child", nargs=argparse.REMAINDER)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    state_root = args.state_root.resolve()
    if args.command == "start":
        print(json.dumps(start_stack(state_root, args.port), indent=2))
        return 0
    if args.command == "stop":
        print(json.dumps(stop_stack(state_root, tolerate_missing=True), indent=2))
        return 0
    if args.command == "status":
        print(json.dumps(status(state_root), indent=2))
        return 0
    if args.command == "doctor":
        sdk_root = discover_sdk_root()
        print(
            json.dumps(
                {
                    "repoRoot": str(REPO_ROOT),
                    "gitHead": git_head(),
                    "sdkRoot": str(sdk_root),
                    "sdkDigest": sha256_file(sdk_root / "package.json"),
                },
                indent=2,
            )
        )
        return 0
    if args.command == "exec":
        if not args.child:
            raise RuntimeError("exec requires a child command after --")
        child = args.child[1:] if args.child[0] == "--" else args.child
        return run_with_stack(state_root, child)
    raise AssertionError(args.command)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(f"M6_STACK_ERROR: {error}", file=sys.stderr)
        raise SystemExit(1)

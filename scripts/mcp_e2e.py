#!/usr/bin/env python3
"""Run a real public-protocol Ordivon MCP end-to-end journey."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import secrets
import shutil
import signal
import socket
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.request
import uuid
from dataclasses import dataclass
from pathlib import Path
from typing import Any


PROTOCOL_VERSION = "2025-06-18"
SCHEMA_VERSION = 1
EXPECTED_TOOLS = {
    "artifact.read",
    "task.cancel",
    "task.list",
    "task.observe",
    "workspace.close",
    "workspace.diff",
    "workspace.exec",
    "workspace.mutate",
    "workspace.open",
    "workspace.read",
}
TERMINAL = {"succeeded", "failed", "timed_out", "cancelled", "lost", "orphaned"}


def digest_bytes(value: bytes) -> str:
    return f"sha256:{hashlib.sha256(value).hexdigest()}"


def command(*argv: str, cwd: Path) -> str:
    result = subprocess.run(argv, cwd=cwd, check=True, text=True, capture_output=True)
    return result.stdout.strip()


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
        listener.bind(("127.0.0.1", 0))
        return int(listener.getsockname()[1])


def host_network_probe() -> tuple[int, threading.Thread]:
    listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    listener.bind(("127.0.0.1", 0))
    listener.listen(1)
    port = int(listener.getsockname()[1])

    def serve() -> None:
        try:
            connection, _ = listener.accept()
            with connection:
                connection.sendall(b"ORDIVON_HOST_NETWORK_OK")
        finally:
            listener.close()

    thread = threading.Thread(target=serve, daemon=True)
    thread.start()
    return port, thread


def parse_http_response(content_type: str, body: bytes) -> dict[str, Any]:
    if not body:
        return {}
    text = body.decode("utf-8")
    if "text/event-stream" in content_type:
        lines = [line[5:].strip() for line in text.splitlines() if line.startswith("data:")]
        if not lines:
            raise ValueError("SSE response contained no data event")
        return json.loads(lines[-1])
    return json.loads(text)


class McpClient:
    def __init__(self, endpoint: str, token: str, timeout: float = 15.0) -> None:
        self.endpoint = endpoint
        self.token = token
        self.timeout = timeout
        self.request_id = 0

    def request(self, method: str, params: dict[str, Any] | None = None) -> dict[str, Any]:
        self.request_id += 1
        payload: dict[str, Any] = {"jsonrpc": "2.0", "id": self.request_id, "method": method}
        if params is not None:
            payload["params"] = params
        request = urllib.request.Request(
            self.endpoint,
            data=json.dumps(payload).encode("utf-8"),
            method="POST",
            headers={
                "Authorization": f"Bearer {self.token}",
                "Content-Type": "application/json",
                "Accept": "application/json, text/event-stream",
                "MCP-Protocol-Version": PROTOCOL_VERSION,
            },
        )
        with urllib.request.urlopen(request, timeout=self.timeout) as response:
            message = parse_http_response(response.headers.get("Content-Type", ""), response.read())
        if message.get("id") != self.request_id:
            raise ValueError(f"unexpected response id for {method}: {message}")
        if "error" in message:
            raise ValueError(f"MCP {method} failed: {message['error']}")
        result = message.get("result")
        if not isinstance(result, dict):
            raise ValueError(f"MCP {method} returned no object result: {message}")
        return result

    def initialize(self) -> dict[str, Any]:
        return self.request(
            "initialize",
            {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {"name": "ordivon-mcp-e2e", "version": "1"},
            },
        )

    def tool(self, name: str, arguments: dict[str, Any]) -> dict[str, Any]:
        result = self.request("tools/call", {"name": name, "arguments": arguments})
        if result.get("isError") is True:
            raise ValueError(f"tool {name} returned an error: {result}")
        structured = result.get("structuredContent")
        if not isinstance(structured, dict):
            raise ValueError(f"tool {name} omitted structuredContent: {result}")
        return structured


@dataclass
class ServerProcess:
    process: subprocess.Popen[str]
    log_path: Path

    def stop(self) -> None:
        if self.process.poll() is not None:
            return
        self.process.send_signal(signal.SIGINT)
        try:
            self.process.wait(timeout=8)
        except subprocess.TimeoutExpired:
            self.process.terminate()
            try:
                self.process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=3)


def wait_for_server(process: subprocess.Popen[str], endpoint: str, token: str, log_path: Path) -> McpClient:
    deadline = time.monotonic() + 15
    last_error: Exception | None = None
    while time.monotonic() < deadline:
        if process.poll() is not None:
            log = log_path.read_text(encoding="utf-8", errors="replace") if log_path.exists() else ""
            raise RuntimeError(f"ordivon-mcp exited early with {process.returncode}:\n{log[-8000:]}")
        client = McpClient(endpoint, token)
        try:
            client.initialize()
            return client
        except (OSError, ValueError, urllib.error.URLError) as error:
            last_error = error
            time.sleep(0.1)
    raise RuntimeError(f"ordivon-mcp did not become ready: {last_error}")


def start_server(repo: Path, root: Path, port: int, token: str) -> ServerProcess:
    log_path = root / "server.log"
    log_handle = log_path.open("a", encoding="utf-8")
    env = os.environ.copy()
    env.update(
        {
            "RUST_LOG": "ordivon_mcp=info,rmcp=warn",
            "ORDIVON_BIND": f"127.0.0.1:{port}",
            "ORDIVON_BEARER_TOKEN": token,
            "ORDIVON_STORE_ROOT": str(root / "store"),
            "ORDIVON_REGISTRY_ROOT": str(root / "registry"),
            "ORDIVON_RUNNER_PATH": str(repo / "target/debug/ordivon-task-runner"),
            "ORDIVON_TRACE_PATH": str(root / "registry/runtime-trace.jsonl"),
        }
    )
    process = subprocess.Popen(
        [str(repo / "target/debug/ordivon-mcp")],
        cwd=repo,
        env=env,
        text=True,
        stdout=log_handle,
        stderr=subprocess.STDOUT,
        start_new_session=True,
    )
    log_handle.close()
    return ServerProcess(process=process, log_path=log_path)


def create_source_repo(root: Path) -> tuple[Path, str]:
    source = root / "source-repo"
    source.mkdir(parents=True)
    command("git", "init", "-q", cwd=source)
    command("git", "config", "user.name", "Ordivon Acceptance", cwd=source)
    command("git", "config", "user.email", "acceptance@ordivon.invalid", cwd=source)
    (source / "README.md").write_text("hello\n", encoding="utf-8")
    (source / "append.txt").write_text("first\n", encoding="utf-8")
    command("git", "add", ".", cwd=source)
    command("git", "commit", "-q", "-m", "acceptance source", cwd=source)
    return source, command("git", "rev-parse", "HEAD", cwd=source)


def wait_terminal(client: McpClient, job_id: str, timeout: float = 30.0) -> dict[str, Any]:
    deadline = time.monotonic() + timeout
    last: dict[str, Any] | None = None
    while time.monotonic() < deadline:
        last = client.tool(
            "task.observe",
            {
                "schemaVersion": SCHEMA_VERSION,
                "jobId": job_id,
                "waitMs": 500,
                "stdoutTailBytes": 8192,
                "stderrTailBytes": 8192,
            },
        )
        if last.get("status") in TERMINAL:
            return last
    raise TimeoutError(f"Job did not become terminal: {last}")


def run_journey(repo: Path, keep: bool, output: Path | None) -> dict[str, Any]:
    if os.geteuid() != 0:
        raise ValueError("MCP E2E requires root for systemd-run")
    if Path("/proc/1/comm").read_text(encoding="utf-8").strip() != "systemd":
        raise ValueError("MCP E2E requires systemd as PID 1")
    repo = repo.resolve()
    command("cargo", "build", "-p", "ordivon-exec", "--bin", "ordivon-task-runner", cwd=repo)
    command("cargo", "build", "-p", "ordivon-mcp", cwd=repo)

    root = Path("/root/.local/share/ordivon-acceptance") / f"mcp-e2e-{uuid.uuid4()}"
    root.mkdir(parents=True)
    source, revision = create_source_repo(root)
    token = secrets.token_urlsafe(48)
    port = free_port()
    endpoint = f"http://127.0.0.1:{port}/mcp"
    server = start_server(repo, root, port, token)
    attempt_ids: list[str] = []
    result: dict[str, Any] = {
        "schemaVersion": 1,
        "head": command("git", "rev-parse", "HEAD", cwd=repo),
        "root": str(root),
        "endpoint": endpoint,
        "checks": [],
    }

    def check(name: str, condition: bool, detail: object = None) -> None:
        if not condition:
            raise AssertionError(f"check failed: {name}: {detail}")
        result["checks"].append({"name": name, "detail": detail})

    try:
        client = wait_for_server(server.process, endpoint, token, server.log_path)
        initialized = client.initialize()
        check("initialize", initialized.get("serverInfo", {}).get("name") == "ordivon-mcp", initialized)

        listed = client.request("tools/list")
        names = {tool.get("name") for tool in listed.get("tools", []) if isinstance(tool, dict)}
        check("tool-catalog", names == EXPECTED_TOOLS, sorted(names))

        workspace_id = f"acceptance-{uuid.uuid4()}"
        opened = client.tool(
            "workspace.open",
            {
                "schemaVersion": SCHEMA_VERSION,
                "workspaceId": workspace_id,
                "sourceRepo": str(source),
                "sourceRevision": revision,
            },
        )
        check("workspace-open", opened.get("sourceRevision") == revision, opened)

        full = client.tool(
            "workspace.read",
            {
                "schemaVersion": SCHEMA_VERSION,
                "workspaceId": workspace_id,
                "relativePath": "README.md",
                "mode": "FULL",
                "offset": 0,
                "maxBytes": 4096,
            },
        )
        check("workspace-read-full", full.get("content") == "hello\n", full)
        sliced = client.tool(
            "workspace.read",
            {
                "schemaVersion": SCHEMA_VERSION,
                "workspaceId": workspace_id,
                "relativePath": "README.md",
                "mode": "SLICE",
                "offset": 1,
                "maxBytes": 3,
            },
        )
        check("workspace-read-slice", sliced.get("content") == "ell", sliced)

        mutated = client.tool(
            "workspace.mutate",
            {
                "schemaVersion": SCHEMA_VERSION,
                "workspaceId": workspace_id,
                "mutations": [
                    {
                        "relativePath": "generated.txt",
                        "mode": "WRITE",
                        "content": "generated\n",
                    },
                    {
                        "relativePath": "append.txt",
                        "mode": "APPEND",
                        "content": "second\n",
                        "expectedDigest": digest_bytes(b"first\n"),
                    },
                    {
                        "relativePath": "README.md",
                        "mode": "REPLACE_EXACT",
                        "content": "hello accepted\n",
                        "expectedDigest": full["digest"],
                        "expectedText": "hello\n",
                    },
                ],
            },
        )
        check("workspace-mutate", len(mutated.get("mutations", [])) == 3, mutated)

        diff = client.tool(
            "workspace.diff",
            {"schemaVersion": SCHEMA_VERSION, "workspaceId": workspace_id, "maxBytes": 65_536},
        )
        check("workspace-diff", "hello accepted" in diff.get("diff", ""), diff)
        check("workspace-untracked", "generated.txt" in diff.get("untrackedPaths", []), diff)

        execution = {
            "workspaceId": workspace_id,
            "executable": "/usr/bin/python3",
            "args": [
                "-c",
                "import sys; print('MCP_E2E_OK', flush=True); print('stderr-e2e', file=sys.stderr, flush=True)",
            ],
            "cwdRelative": ".",
            "env": {},
            "timeoutMs": 10_000,
            "stdoutLimitBytes": 65_536,
            "stderrLimitBytes": 65_536,
        }
        submitted = client.tool(
            "workspace.exec",
            {
                "schemaVersion": SCHEMA_VERSION,
                "clientRequestId": f"request:{uuid.uuid4()}",
                "execution": execution,
                "waitMs": 30_000,
                "stdoutTailBytes": 8192,
                "stderrTailBytes": 8192,
            },
        )
        check("workspace-exec", submitted.get("status") == "succeeded", submitted)
        job_id = str(submitted["jobId"])
        attempt_id = str(submitted["attemptId"])
        attempt_ids.append(attempt_id)
        check("exec-stdout", "MCP_E2E_OK" in submitted.get("stdoutTail", ""), submitted)

        probe_port, probe_thread = host_network_probe()
        docker_socket = next(
            (path for path in (Path("/run/docker.sock"), Path("/var/run/docker.sock")) if path.exists()),
            None,
        )
        probe_script = "\n".join(
            [
                "import json, os, pathlib, socket, subprocess",
                f"connection = socket.create_connection(('127.0.0.1', {probe_port}), timeout=3)",
                "network = connection.recv(128).decode('utf-8')",
                "connection.close()",
                f"docker_path = {str(docker_socket)!r}",
                "docker_ping = None",
                "if docker_path:",
                "    docker = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)",
                "    docker.settimeout(3)",
                "    docker.connect(docker_path)",
                r"    docker.sendall(b'GET /_ping HTTP/1.0\r\n\r\n')",
                "    docker_ping = b'OK' in docker.recv(4096)",
                "    docker.close()",
                "systemd = subprocess.run(['/usr/bin/systemctl', 'show', '--property=Version', '--value'], text=True, capture_output=True)",
                r"pathlib.Path('job-created.txt').write_text('created-by-trusted-job\n')",
                "print(json.dumps({",
                "    'network': network,",
                "    'home': os.environ.get('HOME'),",
                "    'pathPresent': bool(os.environ.get('PATH')),",
                "    'sshVisible': pathlib.Path('/root/.ssh').exists(),",
                "    'configVisible': pathlib.Path('/root/.config').exists(),",
                "    'dockerSocket': docker_path,",
                "    'dockerPing': docker_ping,",
                "    'systemdExit': systemd.returncode,",
                "    'systemdVersion': systemd.stdout.strip(),",
                "}, sort_keys=True))",
            ]
        )
        host_probe = client.tool(
            "workspace.exec",
            {
                "schemaVersion": SCHEMA_VERSION,
                "clientRequestId": f"request:{uuid.uuid4()}",
                "execution": {
                    **execution,
                    "args": ["-c", probe_script],
                },
                "waitMs": 30_000,
                "stdoutTailBytes": 16_384,
                "stderrTailBytes": 16_384,
            },
        )
        probe_thread.join(timeout=5)
        check("trusted-host-job", host_probe.get("status") == "succeeded", host_probe)
        probe = json.loads(host_probe.get("stdoutTail", "").strip())
        check("trusted-host-network", probe.get("network") == "ORDIVON_HOST_NETWORK_OK", probe)
        check(
            "trusted-host-environment",
            probe.get("home") == os.environ.get("HOME") and probe.get("pathPresent") is True,
            probe,
        )
        check("trusted-host-config", probe.get("sshVisible") is True and probe.get("configVisible") is True, probe)
        if docker_socket is not None:
            check("trusted-host-docker", probe.get("dockerPing") is True, probe)
        check(
            "trusted-host-systemd",
            probe.get("systemdExit") == 0 and bool(probe.get("systemdVersion")),
            probe,
        )
        created = client.tool(
            "workspace.read",
            {
                "schemaVersion": SCHEMA_VERSION,
                "workspaceId": workspace_id,
                "relativePath": "job-created.txt",
                "mode": "FULL",
                "offset": 0,
                "maxBytes": 4096,
            },
        )
        check("trusted-host-workspace-write", created.get("content") == "created-by-trusted-job\n", created)

        observed = client.tool(
            "task.observe",
            {
                "schemaVersion": SCHEMA_VERSION,
                "jobId": job_id,
                "waitMs": 0,
                "stdoutTailBytes": 8192,
                "stderrTailBytes": 8192,
            },
        )
        check("task-observe", observed.get("status") == "succeeded", observed)

        task_list = client.tool("task.list", {"limit": 100})
        check("task-list", any(job.get("jobId") == job_id for job in task_list.get("jobs", [])), task_list)
        native_list = client.request("tasks/list", {})
        check(
            "native-task-list", any(task.get("taskId") == job_id for task in native_list.get("tasks", [])), native_list
        )

        artifact_id = f"{attempt_id}.stdout"
        artifact = client.tool(
            "artifact.read",
            {
                "schemaVersion": SCHEMA_VERSION,
                "jobId": job_id,
                "artifactId": artifact_id,
                "offset": 0,
                "maxBytes": 65_536,
            },
        )
        check("artifact-content", artifact.get("content") == "MCP_E2E_OK\n", artifact)
        check("artifact-digest", artifact.get("digest") == digest_bytes(artifact["content"].encode("utf-8")), artifact)

        long_task = client.tool(
            "workspace.exec",
            {
                "schemaVersion": SCHEMA_VERSION,
                "clientRequestId": f"request:{uuid.uuid4()}",
                "execution": {
                    **execution,
                    "args": ["-c", "import time; print('CANCEL_READY', flush=True); time.sleep(30)"],
                    "timeoutMs": 60_000,
                },
                "waitMs": 0,
                "stdoutTailBytes": 4096,
                "stderrTailBytes": 4096,
            },
        )
        cancel_job_id = str(long_task["jobId"])
        cancel_attempt_id = str(long_task["attemptId"])
        attempt_ids.append(cancel_attempt_id)
        cancelled = client.tool("task.cancel", {"schemaVersion": SCHEMA_VERSION, "jobId": cancel_job_id})
        if cancelled.get("status") not in TERMINAL:
            cancelled = wait_terminal(client, cancel_job_id)
        check("task-cancel", cancelled.get("status") == "cancelled", cancelled)

        server.stop()
        server = start_server(repo, root, port, token)
        client = wait_for_server(server.process, endpoint, token, server.log_path)
        after_restart = client.tool(
            "task.observe",
            {
                "schemaVersion": SCHEMA_VERSION,
                "jobId": job_id,
                "waitMs": 0,
                "stdoutTailBytes": 8192,
                "stderrTailBytes": 8192,
            },
        )
        check("restart-observe", after_restart.get("status") == "succeeded", after_restart)
        closed = client.tool(
            "workspace.close",
            {"schemaVersion": SCHEMA_VERSION, "workspaceId": workspace_id},
        )
        check(
            "workspace-close",
            closed.get("workspaceId") == workspace_id and closed.get("removed") is True,
            closed,
        )
        result["status"] = "passed"
    finally:
        server.stop()
        for attempt_id in attempt_ids:
            unit = f"ordivon-{attempt_id}.service"
            subprocess.run(["systemctl", "stop", unit], text=True, capture_output=True)
            subprocess.run(["systemctl", "reset-failed", unit], text=True, capture_output=True)
        if output:
            output.parent.mkdir(parents=True, exist_ok=True)
            output.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        if not keep:
            shutil.rmtree(root, ignore_errors=True)
    return result


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run the real Ordivon MCP and systemd execution journey.")
    parser.add_argument("--repo", type=Path, default=Path.cwd())
    parser.add_argument("--output", type=Path)
    parser.add_argument("--keep", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    result = run_journey(args.repo.expanduser().resolve(), args.keep, args.output)
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (AssertionError, OSError, RuntimeError, TimeoutError, ValueError, urllib.error.URLError) as error:
        print(f"MCP E2E failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error

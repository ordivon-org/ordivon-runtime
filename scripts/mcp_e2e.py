#!/usr/bin/env python3
"""Run a real public-protocol Ordivon Runtime end-to-end journey."""

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


MODERN_PROTOCOL_VERSION = "2026-07-28"
LEGACY_PROTOCOL_VERSIONS = ("2025-11-25", "2025-06-18")
SCHEMA_VERSION = 1
EXPECTED_TOOLS = {
    "artifact.read",
    "task.cancel",
    "task.list",
    "task.observe",
    "workspace.close",
    "workspace.diff",
    "workspace.exec",
    "workspace.execPlan",
    "workspace.get",
    "workspace.list",
    "workspace.mutate",
    "workspace.open",
    "workspace.patch",
    "workspace.patch.get",
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


class McpRequestError(ValueError):
    def __init__(self, method: str, status: int, error: dict[str, Any]) -> None:
        super().__init__(f"MCP {method} failed with HTTP {status}: {error}")
        self.method = method
        self.status = status
        self.error = error


class McpClient:
    def __init__(
        self,
        endpoint: str,
        token: str,
        *,
        protocol_version: str = MODERN_PROTOCOL_VERSION,
        lifecycle: str = "modern",
        timeout: float = 15.0,
    ) -> None:
        if lifecycle not in {"modern", "legacy"}:
            raise ValueError("lifecycle must be modern or legacy")
        self.endpoint = endpoint
        self.token = token
        self.protocol_version = protocol_version
        self.lifecycle = lifecycle
        self.timeout = timeout
        self.request_id = 0
        self.session_id: str | None = None
        self.discovery: dict[str, Any] | None = None
        self.last_request_headers: list[tuple[str, str]] = []

    def _metadata(self) -> dict[str, Any]:
        return {
            "io.modelcontextprotocol/protocolVersion": self.protocol_version,
            "io.modelcontextprotocol/clientInfo": {
                "name": "ordivon-runtime-mcp-e2e",
                "version": "1",
            },
            "io.modelcontextprotocol/clientCapabilities": {},
        }

    def _headers(self, method: str, params: dict[str, Any] | None) -> dict[str, str]:
        headers = {
            "Authorization": f"Bearer {self.token}",
            "Content-Type": "application/json",
            "Accept": "application/json, text/event-stream",
            "MCP-Protocol-Version": self.protocol_version,
        }
        if self.lifecycle == "modern":
            headers["Mcp-Method"] = method
            if method == "tools/call" and isinstance(params, dict):
                name = params.get("name")
                if isinstance(name, str):
                    headers["Mcp-Name"] = name
        elif self.session_id is not None:
            headers["Mcp-Session-Id"] = self.session_id
        return headers

    def _payload(
        self,
        method: str,
        params: dict[str, Any] | None,
        request_id: int | None,
        *,
        include_modern_metadata: bool = True,
    ) -> dict[str, Any]:
        payload: dict[str, Any] = {"jsonrpc": "2.0", "method": method}
        if request_id is not None:
            payload["id"] = request_id
        request_params = dict(params or {})
        if self.lifecycle == "modern" and include_modern_metadata:
            request_params["_meta"] = self._metadata()
        if request_params:
            payload["params"] = request_params
        return payload

    def exchange(
        self,
        method: str,
        params: dict[str, Any] | None = None,
        *,
        notification: bool = False,
        include_modern_metadata: bool = True,
        header_method: str | None = None,
        header_version: str | None = None,
    ) -> tuple[int, dict[str, Any], dict[str, str]]:
        request_id: int | None = None
        if not notification:
            self.request_id += 1
            request_id = self.request_id
        payload = self._payload(
            method,
            params,
            request_id,
            include_modern_metadata=include_modern_metadata,
        )
        headers = self._headers(method, params)
        if header_method is not None:
            headers["Mcp-Method"] = header_method
        if header_version is not None:
            headers["MCP-Protocol-Version"] = header_version
        request = urllib.request.Request(
            self.endpoint,
            data=json.dumps(payload).encode("utf-8"),
            method="POST",
            headers=headers,
        )
        self.last_request_headers = request.header_items()
        try:
            with urllib.request.urlopen(request, timeout=self.timeout) as response:
                status = int(response.status)
                message = parse_http_response(response.headers.get("Content-Type", ""), response.read())
                response_headers = {key.lower(): value for key, value in response.headers.items()}
        except urllib.error.HTTPError as error:
            status = int(error.code)
            message = parse_http_response(error.headers.get("Content-Type", ""), error.read())
            response_headers = {key.lower(): value for key, value in error.headers.items()}
        if request_id is not None and message and message.get("id") != request_id:
            raise ValueError(f"unexpected response id for {method}: {message}")
        return status, message, response_headers

    def request(self, method: str, params: dict[str, Any] | None = None) -> dict[str, Any]:
        status, message, _ = self.exchange(method, params)
        error = message.get("error")
        if status >= 400 or isinstance(error, dict):
            raise McpRequestError(method, status, error if isinstance(error, dict) else message)
        result = message.get("result")
        if not isinstance(result, dict):
            raise ValueError(f"MCP {method} returned no object result: {message}")
        return result

    def request_error(
        self, method: str, params: dict[str, Any] | None = None, **exchange_options: Any
    ) -> tuple[int, dict[str, Any]]:
        status, message, _ = self.exchange(method, params, **exchange_options)
        error = message.get("error")
        if status < 400 and not isinstance(error, dict):
            raise AssertionError(
                f"MCP {method} unexpectedly succeeded with headers {self.last_request_headers}: {message}"
            )
        return status, error if isinstance(error, dict) else message

    def discover(self) -> dict[str, Any]:
        if self.lifecycle != "modern":
            raise ValueError("server/discover belongs to the modern lifecycle")
        self.discovery = self.request("server/discover", {})
        if self.session_id is not None:
            raise ValueError("modern discovery unexpectedly created a Session")
        return self.discovery

    def initialize(self) -> dict[str, Any]:
        if self.lifecycle != "legacy":
            raise ValueError("initialize belongs to the legacy lifecycle")
        status, message, headers = self.exchange(
            "initialize",
            {
                "protocolVersion": self.protocol_version,
                "capabilities": {},
                "clientInfo": {"name": "ordivon-runtime-mcp-e2e", "version": "1"},
            },
        )
        error = message.get("error")
        if status >= 400 or isinstance(error, dict):
            raise McpRequestError("initialize", status, error if isinstance(error, dict) else message)
        session_id = headers.get("mcp-session-id")
        if not session_id:
            raise ValueError("legacy initialize omitted Mcp-Session-Id")
        self.session_id = session_id
        result = message.get("result")
        if not isinstance(result, dict):
            raise ValueError(f"initialize returned no object result: {message}")
        notify_status, _, _ = self.exchange("notifications/initialized", notification=True)
        if notify_status not in {200, 202, 204}:
            raise ValueError(f"initialized notification failed: HTTP {notify_status}")
        return result

    def tool_result(self, name: str, arguments: dict[str, Any]) -> dict[str, Any]:
        return self.request("tools/call", {"name": name, "arguments": arguments})

    def tool(self, name: str, arguments: dict[str, Any]) -> dict[str, Any]:
        result = self.tool_result(name, arguments)
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
            raise RuntimeError(f"ordivon-runtime exited early with {process.returncode}:\n{log[-8000:]}")
        client = McpClient(endpoint, token)
        try:
            client.discover()
            return client
        except (OSError, ValueError, urllib.error.URLError) as error:
            last_error = error
            time.sleep(0.1)
    raise RuntimeError(f"ordivon-runtime did not become ready: {last_error}")


def start_server(repo: Path, target_dir: Path, root: Path, port: int, token: str) -> ServerProcess:
    log_path = root / "server.log"
    log_handle = log_path.open("a", encoding="utf-8")
    env = os.environ.copy()
    env.update(
        {
            "RUST_LOG": "ordivon_runtime_mcp=info,rmcp=warn",
            "ORDIVON_BIND": f"127.0.0.1:{port}",
            "ORDIVON_BEARER_TOKEN": token,
            "ORDIVON_STORE_ROOT": str(root / "store"),
            "ORDIVON_REGISTRY_ROOT": str(root / "registry"),
            "ORDIVON_RUNNER_PATH": str(target_dir / "debug/ordivon-runtime-runner"),
            "ORDIVON_TRACE_PATH": str(root / "registry/runtime-trace.jsonl"),
        }
    )
    process = subprocess.Popen(
        [str(target_dir / "debug/ordivon-runtime")],
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
    command("cargo", "build", "-p", "ordivon-runtime-core", "--bin", "ordivon-runtime-runner", cwd=repo)
    command("cargo", "build", "-p", "ordivon-runtime-mcp", cwd=repo)
    metadata = json.loads(command("cargo", "metadata", "--no-deps", "--format-version", "1", cwd=repo))
    target_dir = Path(metadata["target_directory"])
    binary_digests: dict[str, str] = {}
    build_started_ns = time.time_ns()
    for binary in ("ordivon-runtime", "ordivon-runtime-runner"):
        path = target_dir / "debug" / binary
        if not path.is_file():
            raise RuntimeError(f"Cargo did not produce required binary: {binary}")
        binary_digests[binary] = digest_bytes(path.read_bytes())
        if path.stat().st_mtime_ns <= 0 or path.stat().st_size == 0:
            raise RuntimeError(f"Cargo produced an invalid binary: {path}")

    root = Path("/root/.local/share/ordivon-acceptance") / f"mcp-e2e-{uuid.uuid4()}"
    root.mkdir(parents=True)
    source, revision = create_source_repo(root)
    token = secrets.token_urlsafe(48)
    port = free_port()
    endpoint = f"http://127.0.0.1:{port}/mcp"
    server = start_server(repo, target_dir, root, port, token)
    running_executable = Path(f"/proc/{server.process.pid}/exe").resolve()
    running_digest = digest_bytes(running_executable.read_bytes())
    if running_digest != binary_digests["ordivon-runtime"]:
        server.stop()
        raise RuntimeError(
            "running server binary does not match the built candidate: "
            f"{running_executable} {running_digest} != "
            f"{binary_digests['ordivon-runtime']}"
        )
    attempt_ids: list[str] = []
    result: dict[str, Any] = {
        "schemaVersion": 1,
        "head": command("git", "rev-parse", "HEAD", cwd=repo),
        "root": str(root),
        "endpoint": endpoint,
        "candidateBinaries": binary_digests,
        "runningServerExecutable": str(running_executable),
        "runningServerDigest": running_digest,
        "buildObservedAtNs": build_started_ns,
        "checks": [],
    }

    def check(name: str, condition: bool, detail: object = None) -> None:
        if not condition:
            raise AssertionError(f"check failed: {name}: {detail}")
        result["checks"].append({"name": name, "detail": detail})

    try:
        client = wait_for_server(server.process, endpoint, token, server.log_path)
        discovered = client.discovery or client.discover()
        discovery_meta = discovered.get("_meta", {})
        server_info = discovery_meta.get("io.modelcontextprotocol/serverInfo", {})
        check("discover-name", server_info.get("name") == "ordivon-runtime-mcp", server_info)
        check("discover-title", server_info.get("title") == "Ordivon Runtime", server_info)
        check(
            "discover-supported-versions",
            discovered.get("supportedVersions") == [MODERN_PROTOCOL_VERSION, *LEGACY_PROTOCOL_VERSIONS],
            discovered,
        )
        check(
            "discover-private-no-cache",
            discovered.get("ttlMs") == 0 and discovered.get("cacheScope") == "private",
            discovered,
        )
        catalog_digest = discovery_meta.get("com.ordivon/runtime/toolCatalogDigest")
        check(
            "discover-catalog-digest",
            isinstance(catalog_digest, str) and catalog_digest.startswith("sha256:") and len(catalog_digest) == 71,
            catalog_digest,
        )

        listed = client.request("tools/list")
        tool_entries = {
            tool.get("name"): tool
            for tool in listed.get("tools", [])
            if isinstance(tool, dict) and isinstance(tool.get("name"), str)
        }
        names = set(tool_entries)
        check("tool-catalog", names == EXPECTED_TOOLS, sorted(names))
        for legacy_version in LEGACY_PROTOCOL_VERSIONS:
            legacy = McpClient(
                endpoint,
                token,
                protocol_version=legacy_version,
                lifecycle="legacy",
            )
            initialized = legacy.initialize()
            check(
                f"legacy-initialize-{legacy_version}",
                initialized.get("protocolVersion") == legacy_version and legacy.session_id is not None,
                initialized,
            )
            legacy_tools = legacy.request("tools/list").get("tools", [])
            legacy_names = {
                tool.get("name")
                for tool in legacy_tools
                if isinstance(tool, dict) and isinstance(tool.get("name"), str)
            }
            check(
                f"legacy-catalog-{legacy_version}",
                legacy_names == names,
                sorted(legacy_names),
            )

        mismatch_status, mismatch_error = client.request_error("tools/list", header_method="tools/call")
        check(
            "modern-header-method-mismatch",
            mismatch_status == 400 and isinstance(mismatch_error.get("code"), int),
            mismatch_error,
        )
        missing_meta_status, missing_meta_error = client.request_error(
            "tools/list",
            {"_meta": {"io.modelcontextprotocol/protocolVersion": MODERN_PROTOCOL_VERSION}},
            include_modern_metadata=False,
        )
        check(
            "modern-inline-metadata-required",
            missing_meta_status == 400
            and missing_meta_error.get("code") == -32602
            and "required fields" in str(missing_meta_error.get("message"))
            and "io.modelcontextprotocol/" in str(missing_meta_error.get("message")),
            missing_meta_error,
        )
        for tool_name in sorted(EXPECTED_TOOLS - {"task.list"}):
            version_schema = (
                tool_entries.get(tool_name, {}).get("inputSchema", {}).get("properties", {}).get("schemaVersion", {})
            )
            check(
                f"schema-version-{tool_name}",
                version_schema.get("const") == SCHEMA_VERSION
                and version_schema.get("minimum") == SCHEMA_VERSION
                and version_schema.get("maximum") == SCHEMA_VERSION,
                version_schema,
            )
        exec_schema_text = json.dumps(tool_entries["workspace.exec"].get("inputSchema", {}), sort_keys=True)
        check(
            "exec-path-contract",
            "Absolute host path" in exec_schema_text and "Workspace root" in exec_schema_text,
            exec_schema_text,
        )
        mutate_schema_text = json.dumps(tool_entries["workspace.mutate"].get("inputSchema", {}), sort_keys=True)
        check(
            "mutation-digest-contract",
            "Required when the target already exists" in mutate_schema_text,
            mutate_schema_text,
        )

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
        workspace_get = client.tool(
            "workspace.get",
            {"schemaVersion": SCHEMA_VERSION, "workspaceId": workspace_id},
        )
        check(
            "workspace-get",
            workspace_get.get("workspaceId") == workspace_id and workspace_get.get("headMode") == "detached",
            workspace_get,
        )
        workspace_list = client.tool("workspace.list", {"schemaVersion": SCHEMA_VERSION})
        check(
            "workspace-list",
            any(item.get("workspaceId") == workspace_id for item in workspace_list.get("workspaces", [])),
            workspace_list,
        )

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
        missing_digest = client.tool_result(
            "workspace.mutate",
            {
                "schemaVersion": SCHEMA_VERSION,
                "workspaceId": workspace_id,
                "mutations": [
                    {
                        "relativePath": "README.md",
                        "mode": "REPLACE_EXACT",
                        "content": "replacement",
                        "expectedText": "hello\n",
                    }
                ],
            },
        )
        missing_digest_error = missing_digest.get("structuredContent", {}).get("error", {})
        check(
            "mutation-existing-requires-digest",
            missing_digest.get("isError") is True
            and missing_digest_error.get("field") == "mutations[0].expectedDigest",
            missing_digest,
        )

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

        patch_before = client.tool(
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
        patch_request_id = f"patch:{uuid.uuid4()}"
        patch_request = {
            "schemaVersion": SCHEMA_VERSION,
            "clientRequestId": patch_request_id,
            "workspaceId": workspace_id,
            "files": [
                {
                    "relativePath": "README.md",
                    "expectedDigest": patch_before["digest"],
                    "edits": [
                        {
                            "range": {
                                "start": {"line": 1, "column": 0},
                                "end": {"line": 1, "column": 14},
                            },
                            "expectedText": "hello accepted",
                            "replacement": "hello durable",
                        }
                    ],
                }
            ],
            "maxDiffBytes": 65_536,
        }
        patched = client.tool("workspace.patch", patch_request)
        check(
            "workspace-patch",
            patched.get("clientRequestId") == patch_request_id
            and patched.get("replayed") is False
            and "hello durable" in patched.get("patch", {}).get("diff", ""),
            patched,
        )
        patch_replay = client.tool("workspace.patch", patch_request)
        check(
            "workspace-patch-exact-replay",
            patch_replay.get("operationId") == patched.get("operationId")
            and patch_replay.get("requestDigest") == patched.get("requestDigest")
            and patch_replay.get("replayed") is True,
            patch_replay,
        )
        patch_status = client.tool(
            "workspace.patch.get",
            {
                "schemaVersion": SCHEMA_VERSION,
                "clientRequestId": patch_request_id,
            },
        )
        check(
            "workspace-patch-receipt",
            patch_status.get("operationId") == patched.get("operationId")
            and patch_status.get("state") == "committed"
            and patch_status.get("patch") == patched.get("patch"),
            patch_status,
        )
        conflicting_patch = json.loads(json.dumps(patch_request))
        conflicting_patch["files"][0]["edits"][0]["replacement"] = "conflict"
        conflict_result = client.tool_result("workspace.patch", conflicting_patch)
        conflict_error = conflict_result.get("structuredContent", {}).get("error", {})
        check(
            "workspace-patch-idempotency-conflict",
            conflict_result.get("isError") is True
            and conflict_error.get("code") == "IDEMPOTENCY_CONFLICT",
            conflict_result,
        )

        diff = client.tool(
            "workspace.diff",
            {"schemaVersion": SCHEMA_VERSION, "workspaceId": workspace_id, "maxBytes": 65_536},
        )
        check("workspace-diff", "hello durable" in diff.get("diff", ""), diff)
        check("workspace-untracked", "generated.txt" in diff.get("untrackedPaths", []), diff)

        expected_stdout = "MCP_E2E_🙂\n"
        execution_request_id = f"request:{uuid.uuid4()}"
        execution = {
            "workspaceId": workspace_id,
            "executable": "/usr/bin/python3",
            "args": [
                "-c",
                "import sys; print('MCP_E2E_🙂', flush=True); print('stderr-e2e', file=sys.stderr, flush=True)",
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
                "clientRequestId": execution_request_id,
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
        check("exec-stdout", submitted.get("stdoutTail") == expected_stdout, submitted)

        exec_plan = client.tool(
            "workspace.execPlan",
            {
                "schemaVersion": SCHEMA_VERSION,
                "clientRequestId": f"request:{uuid.uuid4()}",
                "execution": {
                    "workspaceId": workspace_id,
                    "steps": [
                        {
                            "id": "prepare",
                            "executable": "/usr/bin/python3",
                            "args": ["-c", "print('PLAN_PREPARE', flush=True)"],
                            "cwdRelative": ".",
                            "env": {},
                            "timeoutMs": 5_000,
                        },
                        {
                            "id": "fail",
                            "executable": "/usr/bin/python3",
                            "args": ["-c", "print('PLAN_FAIL', flush=True); raise SystemExit(7)"],
                            "cwdRelative": ".",
                            "env": {},
                            "timeoutMs": 5_000,
                        },
                        {
                            "id": "must-not-run",
                            "executable": "/usr/bin/python3",
                            "args": ["-c", "print('PLAN_MUST_NOT_RUN', flush=True)"],
                            "cwdRelative": ".",
                            "env": {},
                            "timeoutMs": 5_000,
                        },
                    ],
                    "stdoutLimitBytes": 65_536,
                    "stderrLimitBytes": 65_536,
                },
                "waitMs": 30_000,
                "stdoutTailBytes": 8192,
                "stderrTailBytes": 8192,
            },
        )
        attempt_ids.append(str(exec_plan["attemptId"]))
        check(
            "workspace-exec-plan-fail-fast",
            exec_plan.get("status") == "failed"
            and exec_plan.get("failedStepId") == "fail"
            and exec_plan.get("failedStepIndex") == 1
            and exec_plan.get("completedSteps") == 1
            and exec_plan.get("totalSteps") == 3
            and "PLAN_PREPARE" in exec_plan.get("stdoutTail", "")
            and "PLAN_FAIL" in exec_plan.get("stdoutTail", "")
            and "PLAN_MUST_NOT_RUN" not in exec_plan.get("stdoutTail", ""),
            exec_plan,
        )

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

        too_small = client.tool_result(
            "task.observe",
            {
                "schemaVersion": SCHEMA_VERSION,
                "jobId": job_id,
                "waitMs": 0,
                "stdoutTailBytes": 1,
                "stderrTailBytes": 1,
                "stdoutOffset": len("MCP_E2E_".encode("utf-8")),
                "stderrOffset": 0,
            },
        )
        check("observe-utf8-hard-bound", too_small.get("isError") is True, too_small)
        too_small_error = too_small.get("structuredContent", {}).get("error", {})
        check(
            "observe-utf8-hard-bound-field",
            too_small_error.get("code") == "INVALID_REQUEST" and too_small_error.get("field") == "stdoutTailBytes",
            too_small,
        )

        offset = 0
        incremental_parts: list[str] = []
        while True:
            incremental = client.tool(
                "task.observe",
                {
                    "schemaVersion": SCHEMA_VERSION,
                    "jobId": job_id,
                    "waitMs": 0,
                    "stdoutTailBytes": 4,
                    "stderrTailBytes": 4,
                    "stdoutOffset": offset,
                    "stderrOffset": 0,
                },
            )
            incremental_parts.append(incremental.get("stdoutTail", ""))
            next_offset = incremental.get("stdoutNextOffset")
            check("observe-offset-progress", isinstance(next_offset, int) and next_offset >= offset, incremental)
            offset = int(next_offset)
            if incremental.get("stdoutEof") is True:
                break
        check("observe-incremental", "".join(incremental_parts) == expected_stdout, incremental_parts)

        task_list = client.tool("task.list", {"limit": 100})
        jobs = task_list.get("jobs", [])
        listed_job = next((job for job in jobs if job.get("jobId") == job_id), None)
        check("task-list", listed_job is not None, task_list)
        check(
            "task-list-semantic-summary",
            listed_job.get("clientRequestId") == execution_request_id
            and listed_job.get("workspaceId") == workspace_id
            and str(listed_job.get("executableName", "")).startswith("python3")
            and isinstance(listed_job.get("durationMs"), int),
            listed_job,
        )
        created = [job.get("createdAtMs", 0) for job in jobs]
        check("task-list-newest-first", created == sorted(created, reverse=True), created)
        tasks_status, tasks_error = client.request_error("tasks/list", {})
        check(
            "legacy-core-tasks-retired",
            tasks_error.get("code") == -32601 and tasks_status == 404,
            {"status": tasks_status, "error": tasks_error},
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
        check("artifact-content", artifact.get("content") == expected_stdout, artifact)
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
        active_close = client.tool_result(
            "workspace.close",
            {"schemaVersion": SCHEMA_VERSION, "workspaceId": workspace_id, "force": True},
        )
        check("workspace-close-active-blocked", active_close.get("isError") is True, active_close)
        active_structured = active_close.get("structuredContent", {})
        active_code = active_structured.get("code") or active_structured.get("error", {}).get("code")
        check("workspace-close-active-code", active_code == "WORKSPACE_BUSY", active_close)
        cancelled = client.tool("task.cancel", {"schemaVersion": SCHEMA_VERSION, "jobId": cancel_job_id})
        if cancelled.get("status") not in TERMINAL:
            cancelled = wait_terminal(client, cancel_job_id)
        check("task-cancel", cancelled.get("status") == "cancelled", cancelled)

        server.stop()
        server = start_server(repo, target_dir, root, port, token)
        restarted_executable = Path(f"/proc/{server.process.pid}/exe").resolve()
        restarted_digest = digest_bytes(restarted_executable.read_bytes())
        check(
            "restart-binary-identity",
            restarted_digest == binary_digests["ordivon-runtime"],
            {
                "path": str(restarted_executable),
                "digest": restarted_digest,
            },
        )
        client = wait_for_server(server.process, endpoint, token, server.log_path)
        restarted_discovery = client.discovery or client.discover()
        restarted_meta = restarted_discovery.get("_meta", {})
        check(
            "restart-catalog-identity",
            restarted_meta.get("com.ordivon/runtime/toolCatalogDigest") == catalog_digest,
            restarted_meta,
        )
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
        dirty_close = client.tool_result(
            "workspace.close",
            {"schemaVersion": SCHEMA_VERSION, "workspaceId": workspace_id},
        )
        check("workspace-close-dirty-blocked", dirty_close.get("isError") is True, dirty_close)
        dirty_structured = dirty_close.get("structuredContent", {})
        dirty_code = dirty_structured.get("code") or dirty_structured.get("error", {}).get("code")
        check("workspace-close-dirty-code", dirty_code == "WORKSPACE_DIRTY", dirty_close)
        closed = client.tool(
            "workspace.close",
            {"schemaVersion": SCHEMA_VERSION, "workspaceId": workspace_id, "force": True},
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
    parser = argparse.ArgumentParser(description="Run the real Ordivon Runtime and systemd execution journey.")
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

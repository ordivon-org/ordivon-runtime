"""Minimal MCP lifecycle probe shared by Runtime operations scripts."""

from __future__ import annotations

import json
import stat
import time
import urllib.error
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Any

MODERN_PROTOCOL_VERSION = "2026-07-28"
LEGACY_PROTOCOL_VERSION = "2025-06-18"


class McpProbeError(RuntimeError):
    pass


def load_bearer_token(environment: dict[str, str]) -> str:
    """Resolve one local Runtime Bearer credential without coupling callers to service layout."""
    inline = environment.get("ORDIVON_BEARER_TOKEN")
    token_file = environment.get("ORDIVON_BEARER_TOKEN_FILE")
    if inline is not None and token_file is not None:
        raise McpProbeError(
            "environment must define exactly one of ORDIVON_BEARER_TOKEN or ORDIVON_BEARER_TOKEN_FILE"
        )
    if inline is not None:
        token = inline.strip()
    elif token_file is not None:
        path = Path(token_file)
        if not path.is_absolute():
            raise McpProbeError("ORDIVON_BEARER_TOKEN_FILE must be absolute")
        metadata = path.lstat()
        if path.is_symlink() or not stat.S_ISREG(metadata.st_mode):
            raise McpProbeError("Runtime Bearer token path must be a regular file")
        if stat.S_IMODE(metadata.st_mode) & 0o077:
            raise McpProbeError(
                "Runtime Bearer token file must not be accessible by group or others"
            )
        if metadata.st_size > 16_384:
            raise McpProbeError("Runtime Bearer token file exceeds the configured bound")
        token = path.read_text(encoding="utf-8").strip()
    else:
        raise McpProbeError(
            "environment must define one of ORDIVON_BEARER_TOKEN or ORDIVON_BEARER_TOKEN_FILE"
        )
    if not token or any(character.isspace() for character in token):
        raise McpProbeError("Runtime Bearer token must contain one non-whitespace token")
    return token


def parse_mcp_response(content_type: str, body: bytes) -> dict[str, Any]:
    text = body.decode("utf-8")
    if "text/event-stream" in content_type:
        events = [line[5:].strip() for line in text.splitlines() if line.startswith("data:")]
        if not events:
            raise McpProbeError("MCP returned an empty event stream")
        text = events[-1]
    value = json.loads(text)
    if not isinstance(value, dict):
        raise McpProbeError("MCP response is not an object")
    return value


@dataclass
class McpClient:
    endpoint: str
    token: str
    client_name: str
    lifecycle: str = "modern"
    protocol_version: str = MODERN_PROTOCOL_VERSION
    timeout: float = 5.0
    request_id: int = 0
    session_id: str | None = None

    def _metadata(self) -> dict[str, Any]:
        return {
            "io.modelcontextprotocol/protocolVersion": self.protocol_version,
            "io.modelcontextprotocol/clientInfo": {"name": self.client_name, "version": "1"},
            "io.modelcontextprotocol/clientCapabilities": {},
        }

    def exchange(
        self, method: str, params: dict[str, Any] | None = None, *, notification: bool = False
    ) -> tuple[int, dict[str, Any], dict[str, str]]:
        request_id: int | None = None
        if not notification:
            self.request_id += 1
            request_id = self.request_id
        payload: dict[str, Any] = {"jsonrpc": "2.0", "method": method}
        if request_id is not None:
            payload["id"] = request_id
        request_params = dict(params or {})
        if self.lifecycle == "modern":
            request_params["_meta"] = self._metadata()
        if request_params:
            payload["params"] = request_params
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
        elif self.session_id:
            headers["Mcp-Session-Id"] = self.session_id
        request = urllib.request.Request(
            self.endpoint, data=json.dumps(payload).encode("utf-8"), method="POST", headers=headers
        )
        try:
            with urllib.request.urlopen(request, timeout=self.timeout) as response:
                status = int(response.status)
                body = response.read()
                message = (
                    {}
                    if notification and not body
                    else parse_mcp_response(response.headers.get("Content-Type", ""), body)
                )
                response_headers = {key.lower(): value for key, value in response.headers.items()}
        except urllib.error.HTTPError as error:
            status = int(error.code)
            message = parse_mcp_response(error.headers.get("Content-Type", ""), error.read())
            response_headers = {key.lower(): value for key, value in error.headers.items()}
        if request_id is not None and message.get("id") != request_id:
            raise McpProbeError(f"unexpected response id for {method}: {message}")
        return status, message, response_headers

    def request(self, method: str, params: dict[str, Any] | None = None) -> dict[str, Any]:
        status, message, _ = self.exchange(method, params)
        error = message.get("error")
        if status >= 400 or isinstance(error, dict):
            raise McpProbeError(f"MCP {method} failed with HTTP {status}: {error or message}")
        result = message.get("result")
        if not isinstance(result, dict):
            raise McpProbeError(f"MCP {method} response lacks an object result")
        return result

    def discover(self) -> dict[str, Any]:
        if self.lifecycle != "modern":
            raise McpProbeError("server/discover requires modern lifecycle")
        return self.request("server/discover", {})

    def initialize(self) -> dict[str, Any]:
        if self.lifecycle != "legacy":
            raise McpProbeError("initialize requires legacy lifecycle")
        status, message, headers = self.exchange(
            "initialize",
            {
                "protocolVersion": self.protocol_version,
                "capabilities": {},
                "clientInfo": {"name": self.client_name, "version": "1"},
            },
        )
        error = message.get("error")
        if status >= 400 or isinstance(error, dict):
            raise McpProbeError(f"MCP initialize failed with HTTP {status}: {error or message}")
        session_id = headers.get("mcp-session-id")
        if not session_id:
            raise McpProbeError("legacy initialize omitted Mcp-Session-Id")
        self.session_id = session_id
        notify_status, _, _ = self.exchange("notifications/initialized", notification=True)
        if notify_status not in {200, 202, 204}:
            raise McpProbeError(f"initialized notification failed with HTTP {notify_status}")
        result = message.get("result")
        if not isinstance(result, dict):
            raise McpProbeError("initialize response lacks an object result")
        return result

    def call_tool(self, name: str, arguments: dict[str, Any]) -> dict[str, Any]:
        result = self.request("tools/call", {"name": name, "arguments": arguments})
        if result.get("isError") is True:
            raise McpProbeError(f"tool {name} returned an error: {result}")
        structured = result.get("structuredContent")
        if not isinstance(structured, dict):
            raise McpProbeError(f"tool {name} omitted structuredContent")
        return structured


def _catalog_result(
    client: McpClient,
    lifecycle_result: dict[str, Any],
    *,
    expected_tool_count: int | None,
    required_tools: tuple[str, ...],
) -> dict[str, object]:
    tools = client.request("tools/list").get("tools")
    if not isinstance(tools, list):
        raise McpProbeError("MCP tools/list lacks a tools array")
    names = sorted(
        {str(tool.get("name")) for tool in tools if isinstance(tool, dict) and isinstance(tool.get("name"), str)}
    )
    if expected_tool_count is not None and len(names) != expected_tool_count:
        raise McpProbeError(f"MCP tool count {len(names)} does not equal expected {expected_tool_count}")
    missing = sorted(set(required_tools) - set(names))
    if missing:
        raise McpProbeError("MCP probe is missing required tools: " + ", ".join(missing))
    if client.lifecycle == "modern":
        metadata = lifecycle_result.get("_meta")
        metadata = metadata if isinstance(metadata, dict) else {}
        digest = metadata.get("com.ordivon/runtime/toolCatalogDigest")
        supported = lifecycle_result.get("supportedVersions")
        if not isinstance(supported, list) or MODERN_PROTOCOL_VERSION not in supported:
            raise McpProbeError("modern discovery omitted the stable protocol version")
        if not isinstance(digest, str) or not digest.startswith("sha256:") or len(digest) != 71:
            raise McpProbeError("modern discovery omitted a valid toolCatalogDigest")
        server_info = metadata.get("io.modelcontextprotocol/serverInfo")
        if not isinstance(server_info, dict):
            raise McpProbeError("modern discovery omitted serverInfo")
        protocol_version = MODERN_PROTOCOL_VERSION
    else:
        digest = None
        supported = [str(lifecycle_result.get("protocolVersion", client.protocol_version))]
        server_info = lifecycle_result.get("serverInfo")
        protocol_version = str(lifecycle_result.get("protocolVersion", client.protocol_version))
    return {
        "lifecycle": client.lifecycle,
        "protocolVersion": protocol_version,
        "supportedVersions": supported,
        "serverInfo": server_info,
        "toolCatalogDigest": digest,
        "toolCount": len(names),
        "toolNames": names,
        "requiredTools": sorted(required_tools),
    }


def probe_catalog(
    endpoint: str,
    token: str,
    *,
    client_name: str,
    expected_tool_count: int | None,
    required_tools: tuple[str, ...],
    wait_seconds: float,
    require_modern: bool,
    allow_legacy_fallback: bool,
) -> dict[str, object]:
    deadline = time.monotonic() + wait_seconds
    last_error: Exception | None = None
    while time.monotonic() < deadline:
        try:
            modern = McpClient(endpoint, token, client_name, timeout=min(5.0, wait_seconds))
            return _catalog_result(
                modern, modern.discover(), expected_tool_count=expected_tool_count, required_tools=required_tools
            )
        except Exception as modern_error:
            last_error = modern_error
            if not require_modern and allow_legacy_fallback:
                try:
                    legacy = McpClient(
                        endpoint,
                        token,
                        client_name,
                        lifecycle="legacy",
                        protocol_version=LEGACY_PROTOCOL_VERSION,
                        timeout=min(5.0, wait_seconds),
                    )
                    return _catalog_result(
                        legacy,
                        legacy.initialize(),
                        expected_tool_count=expected_tool_count,
                        required_tools=required_tools,
                    )
                except Exception as legacy_error:
                    last_error = McpProbeError(
                        f"modern probe failed: {modern_error}; legacy probe failed: {legacy_error}"
                    )
            time.sleep(0.2)
    raise McpProbeError(f"MCP readiness probe failed: {last_error}")


def connect_compatible(endpoint: str, token: str, *, client_name: str, timeout: float = 30.0) -> McpClient:
    modern = McpClient(endpoint, token, client_name, timeout=timeout)
    try:
        modern.discover()
        return modern
    except Exception as modern_error:
        legacy = McpClient(
            endpoint, token, client_name, lifecycle="legacy", protocol_version=LEGACY_PROTOCOL_VERSION, timeout=timeout
        )
        try:
            legacy.initialize()
            return legacy
        except Exception as legacy_error:
            raise McpProbeError(
                f"modern connection failed: {modern_error}; legacy connection failed: {legacy_error}"
            ) from legacy_error

from __future__ import annotations

import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock
import json

SCRIPTS = Path(__file__).resolve().parents[1]
if str(SCRIPTS) not in sys.path:
    sys.path.insert(0, str(SCRIPTS))

from mcp_probe import (
    McpClient,
    McpProbeError,
    PROBE_USER_AGENT,
    load_bearer_token,
    load_environment_file,
)


class _Headers(dict):
    def items(self):
        return super().items()


class _Response:
    status = 200
    headers = _Headers({"Content-Type": "application/json"})

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc, tb):
        return False

    def read(self):
        return json.dumps({"jsonrpc": "2.0", "id": 1, "result": {}}).encode()


class RuntimeProbeTransportTests(unittest.TestCase):
    def test_probe_uses_stable_machine_user_agent(self) -> None:
        client = McpClient(
            "https://mcp.example.invalid/mcp",
            "test-token",
            client_name="probe-test",
        )
        with mock.patch("mcp_probe.urllib.request.urlopen", return_value=_Response()) as opened:
            self.assertEqual(client.request("server/discover", {}), {})
        request = opened.call_args.args[0]
        self.assertEqual(request.get_header("User-agent"), PROBE_USER_AGENT)
        self.assertNotIn("Python-urllib", request.get_header("User-agent"))


class RuntimeCredentialTests(unittest.TestCase):
    def test_inline_credential_remains_migration_compatible(self) -> None:
        self.assertEqual(load_bearer_token({"ORDIVON_BEARER_TOKEN": "test"}), "test")

    def test_environment_file_parser_is_shared_and_bounded_to_key_value_lines(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "runtime.env"
            path.write_text(
                "# comment\nORDIVON_BIND=127.0.0.1:8897\n"
                "ORDIVON_BEARER_TOKEN_FILE='/etc/ordivon/runtime-mcp.token'\n",
                encoding="utf-8",
            )
            self.assertEqual(
                load_environment_file(path),
                {
                    "ORDIVON_BIND": "127.0.0.1:8897",
                    "ORDIVON_BEARER_TOKEN_FILE": "/etc/ordivon/runtime-mcp.token",
                },
            )
            path.write_text("BROKEN\n", encoding="utf-8")
            with self.assertRaisesRegex(McpProbeError, "invalid environment line"):
                load_environment_file(path)

    def test_private_token_file_is_the_canonical_local_source(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "runtime-mcp.token"
            path.write_text("token-value\n", encoding="utf-8")
            os.chmod(path, 0o600)
            self.assertEqual(
                load_bearer_token({"ORDIVON_BEARER_TOKEN_FILE": str(path)}),
                "token-value",
            )

    def test_ambiguous_or_permissive_sources_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "runtime-mcp.token"
            path.write_text("token-value\n", encoding="utf-8")
            os.chmod(path, 0o640)
            with self.assertRaisesRegex(McpProbeError, "group or others"):
                load_bearer_token({"ORDIVON_BEARER_TOKEN_FILE": str(path)})
            os.chmod(path, 0o600)
            with self.assertRaisesRegex(McpProbeError, "exactly one"):
                load_bearer_token({
                    "ORDIVON_BEARER_TOKEN": "inline",
                    "ORDIVON_BEARER_TOKEN_FILE": str(path),
                })


if __name__ == "__main__":
    unittest.main()

from __future__ import annotations

import os
from pathlib import Path
import sys
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parents[1]
if str(SCRIPTS) not in sys.path:
    sys.path.insert(0, str(SCRIPTS))

from mcp_probe import McpProbeError, load_bearer_token, load_environment_file


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

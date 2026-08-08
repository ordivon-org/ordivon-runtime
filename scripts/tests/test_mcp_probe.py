from __future__ import annotations

import os
from pathlib import Path
import sys
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parents[1]
if str(SCRIPTS) not in sys.path:
    sys.path.insert(0, str(SCRIPTS))

from mcp_probe import McpProbeError, load_bearer_token


class RuntimeCredentialTests(unittest.TestCase):
    def test_inline_credential_remains_migration_compatible(self) -> None:
        self.assertEqual(load_bearer_token({"ORDIVON_BEARER_TOKEN": "test"}), "test")

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

from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path


REPO = Path(__file__).resolve().parents[2]
SCRIPTS = REPO / "scripts"
if str(SCRIPTS) not in sys.path:
    sys.path.insert(0, str(SCRIPTS))
spec = importlib.util.spec_from_file_location("ordivon_demo_runtime_flow", SCRIPTS / "demo_runtime_flow.py")
assert spec and spec.loader
demo = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = demo
spec.loader.exec_module(demo)


class DemoRuntimeFlowTests(unittest.TestCase):
    def test_environment_file_is_parsed_without_execution(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "runtime.env"
            path.write_text(
                "# comment\nORDIVON_BIND=127.0.0.1:8811\nexport ORDIVON_BEARER_TOKEN='token value' # hidden\n",
                encoding="utf-8",
            )
            self.assertEqual(
                demo.parse_environment_file(path),
                {"ORDIVON_BIND": "127.0.0.1:8811", "ORDIVON_BEARER_TOKEN": "token value"},
            )

    def test_endpoint_normalizes_wildcard_bind(self) -> None:
        self.assertEqual(demo.endpoint_from_bind("0.0.0.0:8811"), "http://127.0.0.1:8811/mcp")
        self.assertEqual(demo.endpoint_from_bind("[::]:8811"), "http://127.0.0.1:8811/mcp")
        self.assertEqual(demo.endpoint_from_bind("https://runtime.invalid/mcp"), "https://runtime.invalid/mcp")

    def test_text_edit_range_uses_one_based_lines(self) -> None:
        content = 'header\nPOLICY = "blind-redispatch"\n'
        self.assertEqual(
            demo.text_edit_range(content, 'POLICY = "blind-redispatch"'),
            {"start": {"line": 2, "column": 0}, "end": {"line": 2, "column": 27}},
        )

    def test_selected_evidence_omits_paths_and_supervisor_identity(self) -> None:
        selected = demo.selected_terminal_evidence(
            {
                "schemaVersion": 1,
                "jobId": "job-1",
                "workspaceId": "ws-1",
                "cwd": "/secret/path",
                "supervisor": {"mainPid": 7},
                "terminalArtifactIds": ["artifact-1"],
            },
            "sha256:" + "a" * 64,
            "artifact-1",
        )
        self.assertNotIn("cwd", selected)
        self.assertNotIn("supervisor", selected)
        self.assertEqual(selected["workspaceId"], "ws-1")

    def test_secret_scan_rejects_token_material(self) -> None:
        with self.assertRaises(demo.DemoError):
            demo.assert_no_secrets({"value": "private-token"}, ("private-token",))

    def test_fixture_fails_before_patch(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / "source"
            revision = demo.create_source_repo(REPO / "examples/runtime-demo", destination)
            self.assertRegex(revision, r"^[a-f0-9]{40}$")
            self.assertEqual(
                (destination / "policy.py").read_text(encoding="utf-8").splitlines()[2],
                'POLICY = "blind-redispatch"',
            )


if __name__ == "__main__":
    unittest.main()

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


REPO = Path(__file__).resolve().parents[2]


class HygieneTests(unittest.TestCase):
    def test_inspect_ignores_current_and_previous_but_finds_obsolete_files(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            install = root / "install"
            runtime = root / "runtime"
            install.mkdir()
            runtime.mkdir()
            for name in (
                "ordivon-runtime",
                "ordivon-runtime.previous",
                "ordivon-runtime.pre-effect-kernel-20260725",
                "ordivon-mcp.previous",
            ):
                (install / name).write_text(name, encoding="utf-8")
            (runtime / "runtime.sqlite3").write_bytes(b"")
            result = subprocess.run(
                [
                    sys.executable,
                    "scripts/ordivon-runtime-hygiene",
                    "inspect",
                    "--install-dir",
                    str(install),
                    "--runtime-store-root",
                    str(runtime),
                ],
                cwd=REPO,
                check=True,
                text=True,
                capture_output=True,
            )
            report = json.loads(result.stdout)
            names = {Path(item["path"]).name for item in report["candidates"]}
            self.assertEqual(
                names,
                {
                    "ordivon-runtime.pre-effect-kernel-20260725",
                    "ordivon-mcp.previous",
                    "runtime.sqlite3",
                },
            )
            self.assertNotIn("ordivon-runtime.previous", names)

    def test_apply_moves_exact_candidates_into_receipt_quarantine(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            install = root / "install"
            runtime = root / "runtime"
            install.mkdir()
            runtime.mkdir()
            obsolete = install / "ordivon-task-runner.previous"
            obsolete.write_text("legacy", encoding="utf-8")
            legacy_database = runtime / "runtime.sqlite3"
            legacy_database.write_bytes(b"")
            result = subprocess.run(
                [
                    sys.executable,
                    "scripts/ordivon-runtime-hygiene",
                    "apply",
                    "--install-dir",
                    str(install),
                    "--runtime-store-root",
                    str(runtime),
                    "--receipt-root",
                    str(root / "receipts"),
                    "--lock-file",
                    str(root / "hygiene.lock"),
                    "--confirm-policy",
                    "QUARANTINE_OBSOLETE_RUNTIME_FILES",
                ],
                cwd=REPO,
                check=True,
                text=True,
                capture_output=True,
            )
            report = json.loads(result.stdout)
            self.assertEqual(report["status"], "completed")
            self.assertFalse(obsolete.exists())
            self.assertFalse(legacy_database.exists())
            for action in report["actions"]:
                self.assertTrue(Path(action["quarantine"]).is_file())


if __name__ == "__main__":
    unittest.main()

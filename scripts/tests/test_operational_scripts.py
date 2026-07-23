from __future__ import annotations

import importlib.util
import json
import sqlite3
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


REPO = Path(__file__).resolve().parents[2]


def load_script(name: str):
    path = REPO / "scripts" / f"{name}.py"
    spec = importlib.util.spec_from_file_location(f"ordivon_{name}", path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


backup = load_script("backup")
restore = load_script("restore")


class OperationalScriptTests(unittest.TestCase):
    def test_backup_restore_round_trip(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            control = root / "control"
            control.mkdir()
            database = control / "registry.sqlite3"
            with sqlite3.connect(database) as connection:
                connection.execute("CREATE TABLE jobs (id TEXT PRIMARY KEY, status TEXT NOT NULL)")
                connection.execute("INSERT INTO jobs VALUES ('job-1', 'succeeded')")
                connection.commit()
            (control / "note.txt").write_text("control-data\n", encoding="utf-8")
            snapshot = root / "snapshot"
            restored = root / "restored"
            subprocess.run(
                [
                    sys.executable,
                    "scripts/backup.py",
                    "--database",
                    str(database),
                    "--control-root",
                    str(control),
                    "--destination",
                    str(snapshot),
                ],
                cwd=REPO,
                check=True,
                text=True,
                capture_output=True,
            )
            subprocess.run(
                [
                    sys.executable,
                    "scripts/restore.py",
                    "--snapshot",
                    str(snapshot),
                    "--destination",
                    str(restored),
                ],
                cwd=REPO,
                check=True,
                text=True,
                capture_output=True,
            )
            with sqlite3.connect(restored / "registry.sqlite3") as connection:
                self.assertEqual(connection.execute("SELECT * FROM jobs").fetchall(), [("job-1", "succeeded")])
            self.assertEqual((restored / "control/note.txt").read_text(encoding="utf-8"), "control-data\n")

    def test_restore_rejects_digest_mismatch(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            snapshot = root / "snapshot"
            snapshot.mkdir()
            database = snapshot / "registry.sqlite3"
            with sqlite3.connect(database) as connection:
                connection.execute("CREATE TABLE t (v INTEGER)")
            (snapshot / "manifest.json").write_text(
                json.dumps(
                    {
                        "schemaVersion": 1,
                        "files": [{"path": "registry.sqlite3", "bytes": database.stat().st_size, "digest": "sha256:bad"}],
                    }
                ),
                encoding="utf-8",
            )
            with self.assertRaisesRegex(ValueError, "digest mismatch"):
                restore.restore_snapshot(snapshot, root / "restored")




if __name__ == "__main__":
    unittest.main()

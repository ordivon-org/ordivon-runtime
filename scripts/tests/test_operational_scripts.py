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

    def test_reclaim_accepts_future_compatible_registry(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            database = root / "registry.sqlite3"
            runtime = root / "runtime"
            (runtime / "workspace-records").mkdir(parents=True)
            (runtime / "workspaces").mkdir(parents=True)
            with sqlite3.connect(database) as connection:
                connection.executescript(
                    """
                    CREATE TABLE schema_migrations(version INTEGER);
                    INSERT INTO schema_migrations VALUES (999);
                    CREATE TABLE jobs(job_id TEXT,workspace_id TEXT,resolution TEXT,created_at_ms INTEGER);
                    CREATE TABLE attempts(attempt_id TEXT,job_id TEXT,state TEXT);
                    CREATE TABLE concurrency_reservations(attempt_id TEXT,state TEXT);
                    """
                )
            result = subprocess.run(
                [
                    sys.executable,
                    "scripts/ordivon-runtime-reclaim",
                    "inspect",
                    "--database",
                    str(database),
                    "--runtime-store-root",
                    str(runtime),
                ],
                cwd=REPO,
                check=True,
                text=True,
                capture_output=True,
            )
            report = json.loads(result.stdout)
            self.assertEqual(report["registryVersion"], 999)
            self.assertEqual(report["releaseTool"], "workspace.close")
            self.assertEqual(report["summary"]["counts"], {})

    def test_reclaim_rejects_missing_registry_capability(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            database = root / "registry.sqlite3"
            runtime = root / "runtime"
            runtime.mkdir()
            with sqlite3.connect(database) as connection:
                connection.executescript(
                    """
                    CREATE TABLE schema_migrations(version INTEGER);
                    INSERT INTO schema_migrations VALUES (4);
                    CREATE TABLE jobs(job_id TEXT,workspace_id TEXT,created_at_ms INTEGER);
                    CREATE TABLE attempts(attempt_id TEXT,job_id TEXT,state TEXT);
                    CREATE TABLE concurrency_reservations(attempt_id TEXT,state TEXT);
                    """
                )
            result = subprocess.run(
                [
                    sys.executable,
                    "scripts/ordivon-runtime-reclaim",
                    "inspect",
                    "--database",
                    str(database),
                    "--runtime-store-root",
                    str(runtime),
                ],
                cwd=REPO,
                check=False,
                text=True,
                capture_output=True,
            )
            self.assertEqual(result.returncode, 1)
            self.assertIn("column:jobs.resolution", result.stderr)

    def test_local_acceptance_contract_is_executable(self) -> None:
        result = subprocess.run(
            ["scripts/local-acceptance", "check"],
            cwd=REPO,
            check=True,
            text=True,
            capture_output=True,
        )
        self.assertIn("local acceptance contract: present", result.stdout)
        acceptance = (REPO / "scripts/local-acceptance").read_text(encoding="utf-8")
        self.assertIn("--features systemd-supervisor", acceptance)
        self.assertIn("ORDIVON_RUN_SYSTEMD_SPIKE=1", acceptance)

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

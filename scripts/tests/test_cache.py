from __future__ import annotations

import hashlib
import json
import runpy
import sqlite3
import subprocess
import sys
import tempfile
import unittest
from contextlib import closing
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]


def initialize_registry(database: Path, active_workspace: str | None = None) -> None:
    with closing(sqlite3.connect(database)) as connection:
        connection.executescript(
            """
            CREATE TABLE jobs(job_id TEXT,workspace_id TEXT,resolution TEXT);
            CREATE TABLE attempts(attempt_id TEXT,job_id TEXT);
            CREATE TABLE concurrency_reservations(attempt_id TEXT,state TEXT);
            """
        )
        if active_workspace:
            connection.execute("INSERT INTO jobs VALUES ('job-1',?,NULL)", (active_workspace,))
            connection.execute("INSERT INTO attempts VALUES ('attempt-1','job-1')")
            connection.execute(
                "INSERT INTO concurrency_reservations VALUES ('attempt-1','active')"
            )
        connection.commit()


def record(runtime: Path, workspace_id: str, source_repo: Path) -> None:
    records = runtime / "workspace-records"
    records.mkdir(parents=True, exist_ok=True)
    (records / f"{workspace_id}.json").write_text(
        json.dumps(
            {
                "schemaVersion": 1,
                "workspaceId": workspace_id,
                "sourceRepo": str(source_repo),
                "sourceRevision": "a" * 40,
                "workspacePath": str(runtime / "workspaces" / workspace_id),
                "createdUnixMs": 1,
            }
        ),
        encoding="utf-8",
    )


class CacheRetentionTests(unittest.TestCase):
    def test_cache_watermark_has_no_legacy_sixteen_tib_ceiling(self) -> None:
        module = runpy.run_path(str(REPO / "scripts/ordivon-runtime-cache"))
        beyond = 16 * 1024 * 1024 * 1024 * 1024 + 1
        self.assertEqual(module["positive_bytes"](str(beyond)), beyond)
        with self.assertRaises(Exception):
            module["positive_bytes"]("0")
        with self.assertRaises(Exception):
            module["positive_bytes"]("-1")

    def test_legacy_source_build_is_reclaimable_even_with_open_active_workspace(self) -> None:
        module = runpy.run_path(str(REPO / "scripts/ordivon-runtime-cache"))
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            runtime = root / "runtime"
            database = root / "registry.sqlite3"
            initialize_registry(database, "workspace-active")
            source = root / "source"
            source.mkdir()
            record(runtime, "workspace-active", source)
            source_key = hashlib.sha256(str(source).encode()).hexdigest()
            legacy = runtime / "cache" / "build" / "sources" / source_key
            legacy.mkdir(parents=True)
            (legacy / "artifact").write_bytes(b"legacy" * 32)

            _before, candidates = module["cache_prune_candidates"](database, runtime)
            matching = [candidate for candidate in candidates if candidate["path"] == str(legacy)]
            self.assertEqual(len(matching), 1)
            self.assertEqual(matching[0]["kind"], "source_build")
            self.assertGreater(matching[0]["bytes"], 0)

            report = module["inspect"](database, runtime)
            self.assertEqual(
                report["kind"], "ordivon.runtime-cache-retention-inspection"
            )
            self.assertEqual(
                report["summary"]["runtimeReclaimableCandidates"], 1
            )
            self.assertGreater(
                report["summary"]["runtimeReclaimableBytes"], 0
            )
            self.assertIn(
                "source_build",
                report["authority"]["runtimeReclaimable"]["candidateBytesByKind"],
            )
            self.assertIn(
                "workspace-active",
                report["authority"]["workspaceProtected"]["openWorkspaceIds"],
            )

    def test_cache_cli_no_longer_exposes_obsolete_migrate_command(self) -> None:
        completed = subprocess.run(
            [sys.executable, "scripts/ordivon-runtime-cache", "--help"],
            cwd=REPO,
            check=True,
            text=True,
            capture_output=True,
        )
        self.assertIn("{inspect,prune}", completed.stdout)
        self.assertNotIn("migrate", completed.stdout)

    def test_prune_uses_watermarks_and_preserves_open_workspace_cache(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            runtime = root / "runtime"
            database = root / "registry.sqlite3"
            initialize_registry(database)
            source = root / "source"
            source.mkdir()
            record(runtime, "workspace-protected", source)
            protected = runtime / "cache" / "build" / "workspace-protected"
            protected.mkdir(parents=True)
            (protected / "artifact").write_bytes(b"p" * 5)
            stale = runtime / "cache" / "build" / "workspace-stale"
            stale.mkdir(parents=True)
            (stale / "artifact").write_bytes(b"s" * 120)

            completed = subprocess.run(
                [
                    sys.executable,
                    "scripts/ordivon-runtime-cache",
                    "prune",
                    "--database",
                    str(database),
                    "--runtime-store-root",
                    str(runtime),
                    "--receipt-root",
                    str(root / "receipts"),
                    "--lock-file",
                    str(root / "cache.lock"),
                    "--high-watermark-bytes",
                    "50",
                    "--low-watermark-bytes",
                    "10",
                    "--confirm-policy",
                    "PRUNE_EXECUTION_CACHES",
                ],
                cwd=REPO,
                check=True,
                text=True,
                capture_output=True,
            )
            report = json.loads(completed.stdout)
            self.assertEqual(report["status"], "completed")
            self.assertTrue(protected.is_dir())
            self.assertFalse(stale.exists())
            self.assertLessEqual(report["afterBytes"], 50)
            self.assertEqual(report["actions"][0]["action"], "cache_removed")
            self.assertTrue(Path(report["receipt"], "plan.json").is_file())

    def test_prune_reports_protected_residual_without_execution_failure(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            runtime = root / "runtime"
            database = root / "registry.sqlite3"
            initialize_registry(database)
            source = root / "source"
            source.mkdir()
            record(runtime, "workspace-protected", source)
            protected = runtime / "cache" / "build" / "workspace-protected"
            protected.mkdir(parents=True)
            (protected / "artifact").write_bytes(b"p" * 120)

            completed = subprocess.run(
                [
                    sys.executable,
                    "scripts/ordivon-runtime-cache",
                    "prune",
                    "--database",
                    str(database),
                    "--runtime-store-root",
                    str(runtime),
                    "--receipt-root",
                    str(root / "receipts"),
                    "--lock-file",
                    str(root / "cache.lock"),
                    "--high-watermark-bytes",
                    "50",
                    "--low-watermark-bytes",
                    "10",
                    "--confirm-policy",
                    "PRUNE_EXECUTION_CACHES",
                ],
                cwd=REPO,
                check=True,
                text=True,
                capture_output=True,
            )
            report = json.loads(completed.stdout)
            self.assertEqual(report["status"], "completed")
            self.assertEqual(report["capacityDisposition"], "protected_residual")
            self.assertGreater(report["afterBytes"], report["highWatermarkBytes"])
            self.assertEqual(report["remainingCandidateBytes"], 0)
            self.assertGreater(report["residualOverHighBytes"], 0)
            self.assertEqual(report["actions"], [])
            self.assertEqual(report["failures"], [])
            self.assertTrue(protected.is_dir())

    def test_prune_keeps_reclaimable_residual_partial_after_delete_failure(self) -> None:
        module = runpy.run_path(str(REPO / "scripts/ordivon-runtime-cache"))
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            runtime = root / "runtime"
            database = root / "registry.sqlite3"
            initialize_registry(database)
            stale = runtime / "cache" / "build" / "workspace-stale"
            stale.mkdir(parents=True)
            (stale / "artifact").write_bytes(b"s" * 120)
            args = type(
                "Args",
                (),
                {
                    "database": database,
                    "runtime_store_root": runtime,
                    "receipt_root": root / "receipts",
                    "lock_file": root / "cache.lock",
                    "env_file": None,
                    "confirm_policy": "PRUNE_EXECUTION_CACHES",
                    "high_watermark_bytes": 50,
                    "low_watermark_bytes": 10,
                },
            )()
            shutil_module = module["shutil"]
            original_rmtree = shutil_module.rmtree
            try:
                def fail_delete(_path: Path) -> None:
                    raise OSError("synthetic delete failure")

                shutil_module.rmtree = fail_delete
                report = module["prune"](args)
            finally:
                shutil_module.rmtree = original_rmtree
            self.assertEqual(report["status"], "partial")
            self.assertEqual(report["capacityDisposition"], "reclaimable_residual")
            self.assertGreaterEqual(report["remainingCandidateBytes"], 120)
            self.assertEqual(len(report["failures"]), 1)
            self.assertTrue(stale.is_dir())

    def test_prune_uses_operator_env_watermarks(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            runtime = root / "runtime"
            database = root / "registry.sqlite3"
            initialize_registry(database)
            stale = runtime / "cache" / "build" / "workspace-stale"
            stale.mkdir(parents=True)
            (stale / "artifact").write_bytes(b"s" * 120)
            env_file = root / "runtime.env"
            env_file.write_text(
                "ORDIVON_CACHE_HIGH_WATERMARK_BYTES=50\n"
                "ORDIVON_CACHE_LOW_WATERMARK_BYTES=10\n",
                encoding="utf-8",
            )

            completed = subprocess.run(
                [
                    sys.executable,
                    "scripts/ordivon-runtime-cache",
                    "prune",
                    "--database",
                    str(database),
                    "--runtime-store-root",
                    str(runtime),
                    "--receipt-root",
                    str(root / "receipts"),
                    "--lock-file",
                    str(root / "cache.lock"),
                    "--env-file",
                    str(env_file),
                    "--confirm-policy",
                    "PRUNE_EXECUTION_CACHES",
                ],
                cwd=REPO,
                check=True,
                text=True,
                capture_output=True,
            )
            report = json.loads(completed.stdout)
            self.assertEqual(report["highWatermarkBytes"], 50)
            self.assertEqual(report["lowWatermarkBytes"], 10)
            self.assertFalse(stale.exists())

    def test_explicit_prune_watermarks_override_operator_env(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            runtime = root / "runtime"
            database = root / "registry.sqlite3"
            initialize_registry(database)
            stale = runtime / "cache" / "build" / "workspace-stale"
            stale.mkdir(parents=True)
            (stale / "artifact").write_bytes(b"s" * 120)
            env_file = root / "runtime.env"
            env_file.write_text(
                "ORDIVON_CACHE_HIGH_WATERMARK_BYTES=500\n"
                "ORDIVON_CACHE_LOW_WATERMARK_BYTES=400\n",
                encoding="utf-8",
            )

            completed = subprocess.run(
                [
                    sys.executable,
                    "scripts/ordivon-runtime-cache",
                    "prune",
                    "--database",
                    str(database),
                    "--runtime-store-root",
                    str(runtime),
                    "--receipt-root",
                    str(root / "receipts"),
                    "--lock-file",
                    str(root / "cache.lock"),
                    "--env-file",
                    str(env_file),
                    "--high-watermark-bytes",
                    "50",
                    "--low-watermark-bytes",
                    "10",
                    "--confirm-policy",
                    "PRUNE_EXECUTION_CACHES",
                ],
                cwd=REPO,
                check=True,
                text=True,
                capture_output=True,
            )
            report = json.loads(completed.stdout)
            self.assertEqual(report["highWatermarkBytes"], 50)
            self.assertEqual(report["lowWatermarkBytes"], 10)
            self.assertFalse(stale.exists())

    def test_prune_env_policy_fails_closed_when_incomplete(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            runtime = root / "runtime"
            database = root / "registry.sqlite3"
            initialize_registry(database)
            stale = runtime / "cache" / "build" / "workspace-stale"
            stale.mkdir(parents=True)
            (stale / "artifact").write_bytes(b"s" * 120)
            env_file = root / "runtime.env"
            env_file.write_text("ORDIVON_CACHE_HIGH_WATERMARK_BYTES=50\n", encoding="utf-8")

            failed = subprocess.run(
                [
                    sys.executable,
                    "scripts/ordivon-runtime-cache",
                    "prune",
                    "--database",
                    str(database),
                    "--runtime-store-root",
                    str(runtime),
                    "--receipt-root",
                    str(root / "receipts"),
                    "--lock-file",
                    str(root / "cache.lock"),
                    "--env-file",
                    str(env_file),
                    "--confirm-policy",
                    "PRUNE_EXECUTION_CACHES",
                ],
                cwd=REPO,
                check=False,
                text=True,
                capture_output=True,
            )
            self.assertEqual(failed.returncode, 2)
            self.assertIn("ORDIVON_CACHE_LOW_WATERMARK_BYTES", failed.stderr)
            self.assertTrue(stale.is_dir())

    def test_prune_without_policy_inputs_keeps_standalone_defaults(self) -> None:
        module = runpy.run_path(str(REPO / "scripts/ordivon-runtime-cache"))
        args = type(
            "Args",
            (),
            {"high_watermark_bytes": None, "low_watermark_bytes": None, "env_file": None},
        )()
        self.assertEqual(
            module["resolve_prune_watermarks"](args),
            (64 * 1024**3, 48 * 1024**3),
        )

    def test_explicit_prune_policy_does_not_depend_on_env_file_contents(self) -> None:
        module = runpy.run_path(str(REPO / "scripts/ordivon-runtime-cache"))
        with tempfile.TemporaryDirectory() as temporary:
            env_file = Path(temporary) / "runtime.env"
            env_file.write_text("this is deliberately malformed\n", encoding="utf-8")
            args = type(
                "Args",
                (),
                {"high_watermark_bytes": 50, "low_watermark_bytes": 10, "env_file": env_file},
            )()
            self.assertEqual(module["resolve_prune_watermarks"](args), (50, 10))

    def test_packaged_lifecycle_uses_operator_cache_policy(self) -> None:
        unit = (REPO / "packaging/systemd/ordivon-runtime-lifecycle.service").read_text(
            encoding="utf-8"
        )
        cache_line = next(line for line in unit.splitlines() if "ordivon-runtime-cache prune" in line)
        self.assertIn("--env-file /etc/ordivon/ordivon-runtime.env", cache_line)
        self.assertNotIn("--high-watermark-bytes", cache_line)
        self.assertNotIn("--low-watermark-bytes", cache_line)


if __name__ == "__main__":
    unittest.main()

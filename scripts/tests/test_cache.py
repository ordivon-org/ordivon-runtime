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


class CacheMigrationTests(unittest.TestCase):
    def test_cache_watermark_has_no_legacy_sixteen_tib_ceiling(self) -> None:
        module = runpy.run_path(str(REPO / "scripts/ordivon-runtime-cache"))
        beyond = 16 * 1024 * 1024 * 1024 * 1024 + 1
        self.assertEqual(module["positive_bytes"](str(beyond)), beyond)
        with self.assertRaises(Exception):
            module["positive_bytes"]("0")
        with self.assertRaises(Exception):
            module["positive_bytes"]("-1")

    def test_migrate_promotes_largest_inactive_cache_and_keeps_fallbacks(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            runtime = root / "runtime"
            database = root / "registry.sqlite3"
            initialize_registry(database, "workspace-active")
            source = root / "source"
            active_source = root / "active-source"
            source.mkdir()
            active_source.mkdir()
            record(runtime, "workspace-active", active_source)
            active_cache = runtime / "cache" / "build" / "workspace-active" / "cargo"
            active_cache.mkdir(parents=True)
            (active_cache / "artifact").write_bytes(b"x" * 1000)
            sizes = {"workspace-small": 10, "workspace-large": 100}
            for workspace_id, size in sizes.items():
                record(runtime, workspace_id, source)
                cache = runtime / "cache" / "build" / workspace_id / "cargo"
                cache.mkdir(parents=True)
                (cache / "artifact").write_bytes(b"x" * size)
            result = subprocess.run(
                [
                    sys.executable,
                    "scripts/ordivon-runtime-cache",
                    "migrate",
                    "--database",
                    str(database),
                    "--runtime-store-root",
                    str(runtime),
                    "--receipt-root",
                    str(root / "receipts"),
                    "--lock-file",
                    str(root / "cache.lock"),
                    "--confirm-policy",
                    "MIGRATE_SHARED_EXECUTION_CACHES",
                ],
                cwd=REPO,
                check=True,
                text=True,
                capture_output=True,
            )
            report = json.loads(result.stdout)
            self.assertEqual(report["status"], "completed")
            self.assertEqual(report["actions"][0]["workspaceId"], "workspace-large")
            key = hashlib.sha256(str(source).encode()).hexdigest()
            target = runtime / "cache" / "build" / "sources" / key
            self.assertTrue((target / "cargo" / "artifact").is_file())
            self.assertTrue((runtime / "cache" / "build" / "workspace-small").is_dir())
            self.assertTrue((runtime / "cache" / "build" / "workspace-active").is_dir())
            self.assertTrue((runtime / "cache" / "shared" / "pnpm" / "store").is_dir())
            self.assertTrue(Path(report["receipt"], "plan.json").is_file())

    def test_empty_target_is_replaced_but_active_source_is_not(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            runtime = root / "runtime"
            database = root / "registry.sqlite3"
            initialize_registry(database)
            source = root / "source"
            source.mkdir()
            record(runtime, "workspace-donor", source)
            legacy = runtime / "cache" / "build" / "workspace-donor" / "cargo"
            legacy.mkdir(parents=True)
            (legacy / "artifact").write_text("cache", encoding="utf-8")
            key = hashlib.sha256(str(source).encode()).hexdigest()
            empty_target = runtime / "cache" / "build" / "sources" / key / "cargo"
            empty_target.mkdir(parents=True)
            result = subprocess.run(
                [
                    sys.executable,
                    "scripts/ordivon-runtime-cache",
                    "migrate",
                    "--database",
                    str(database),
                    "--runtime-store-root",
                    str(runtime),
                    "--receipt-root",
                    str(root / "receipts"),
                    "--lock-file",
                    str(root / "cache.lock"),
                    "--confirm-policy",
                    "MIGRATE_SHARED_EXECUTION_CACHES",
                ],
                cwd=REPO,
                check=True,
                text=True,
                capture_output=True,
            )
            report = json.loads(result.stdout)
            self.assertEqual(len(report["actions"]), 1)
            self.assertTrue((empty_target / "artifact").is_file())

            active_database = root / "active.sqlite3"
            initialize_registry(active_database, "workspace-active")
            active_runtime = root / "active-runtime"
            active_source = root / "active-source"
            active_source.mkdir()
            record(active_runtime, "workspace-active", active_source)
            record(active_runtime, "workspace-donor", active_source)
            active_legacy = active_runtime / "cache" / "build" / "workspace-donor"
            active_legacy.mkdir(parents=True)
            (active_legacy / "artifact").write_text("cache", encoding="utf-8")
            inspected = subprocess.run(
                [
                    sys.executable,
                    "scripts/ordivon-runtime-cache",
                    "inspect",
                    "--database",
                    str(active_database),
                    "--runtime-store-root",
                    str(active_runtime),
                ],
                cwd=REPO,
                check=True,
                text=True,
                capture_output=True,
            )
            active_report = json.loads(inspected.stdout)
            self.assertEqual(active_report["groups"][0]["activeWorkspaceIds"], ["workspace-active"])
            self.assertFalse(active_report["groups"][0]["migrationEligible"])

    def test_zero_byte_legacy_cache_is_not_promoted(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            runtime = root / "runtime"
            database = root / "registry.sqlite3"
            initialize_registry(database)
            source = root / "source"
            source.mkdir()
            record(runtime, "workspace-empty", source)
            (runtime / "cache" / "build" / "workspace-empty").mkdir(parents=True)
            result = subprocess.run(
                [
                    sys.executable,
                    "scripts/ordivon-runtime-cache",
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
            self.assertEqual(report["groups"][0]["selectedDonor"]["bytes"], 0)
            self.assertFalse(report["groups"][0]["migrationEligible"])

    def test_missing_source_repository_is_not_promoted(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            runtime = root / "runtime"
            database = root / "registry.sqlite3"
            initialize_registry(database)
            missing_source = root / "retired-source"
            record(runtime, "workspace-retired", missing_source)
            cache = runtime / "cache" / "build" / "workspace-retired"
            cache.mkdir(parents=True)
            (cache / "artifact").write_text("cache", encoding="utf-8")
            result = subprocess.run(
                [
                    sys.executable,
                    "scripts/ordivon-runtime-cache",
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
            self.assertFalse(report["groups"][0]["sourceRepoAvailable"])
            self.assertFalse(report["groups"][0]["migrationEligible"])

    def test_inspect_does_not_select_active_only_group(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            runtime = root / "runtime"
            database = root / "registry.sqlite3"
            initialize_registry(database, "workspace-active")
            source = root / "source"
            source.mkdir()
            record(runtime, "workspace-active", source)
            cache = runtime / "cache" / "build" / "workspace-active"
            cache.mkdir(parents=True)
            (cache / "artifact").write_text("cache", encoding="utf-8")
            result = subprocess.run(
                [
                    sys.executable,
                    "scripts/ordivon-runtime-cache",
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
            self.assertEqual(report["summary"]["migrationEligibleGroups"], 0)
            self.assertIsNone(report["groups"][0]["selectedDonor"])


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

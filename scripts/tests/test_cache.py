from __future__ import annotations

import hashlib
import json
import sqlite3
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]


def initialize_registry(database: Path, active_workspace: str | None = None) -> None:
    with sqlite3.connect(database) as connection:
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


if __name__ == "__main__":
    unittest.main()

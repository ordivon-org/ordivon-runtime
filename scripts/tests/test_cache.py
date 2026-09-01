from __future__ import annotations

import fcntl
import hashlib
import json
import runpy
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import threading
import time
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

    def test_zero_byte_cache_directories_are_not_retention_candidates(self) -> None:
        module = runpy.run_path(str(REPO / "scripts/ordivon-runtime-cache"))
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            runtime = root / "runtime"
            database = root / "registry.sqlite3"
            initialize_registry(database)
            source = root / "source"
            source.mkdir()
            workspace_id = "workspace-empty-build"
            record(runtime, workspace_id, source)
            empty_build = runtime / "cache" / "build" / workspace_id
            (empty_build / "cargo").mkdir(parents=True)

            _total, candidates = module["cache_prune_candidates"](
                database, runtime, include_idle_open_build=True
            )
            self.assertNotIn(str(empty_build), [str(row["path"]) for row in candidates])

    def test_prune_evicts_idle_open_build_but_keeps_open_generic_cache(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            runtime = root / "runtime"
            database = root / "registry.sqlite3"
            initialize_registry(database)
            source = root / "source"
            source.mkdir()
            workspace_id = "workspace-idle"
            record(runtime, workspace_id, source)
            build = runtime / "cache" / "build" / workspace_id
            build.mkdir(parents=True)
            (build / "artifact").write_bytes(b"b" * 120)
            generic = runtime / "cache" / "workspaces" / workspace_id
            generic.mkdir(parents=True)
            (generic / "state").write_bytes(b"g" * 5)

            inspected = subprocess.run(
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
            inspection = json.loads(inspected.stdout)
            self.assertGreater(inspection["summary"]["fencedIdleWorkspaceBuildBytes"], 0)
            fenced = inspection["authority"]["fencedIdleWorkspaceBuild"]["candidates"]
            self.assertEqual(len(fenced), 1)
            self.assertEqual(fenced[0]["workspaceId"], workspace_id)
            self.assertEqual(fenced[0]["kind"], "idle_open_workspace_build")
            self.assertTrue(fenced[0]["requiresAdmissionFence"])

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
            self.assertFalse(build.exists())
            self.assertTrue(generic.is_dir())
            self.assertLessEqual(report["afterBytes"], 50)
            self.assertEqual(report["actions"][0]["action"], "idle_workspace_build_evicted")
            self.assertTrue(Path(report["receipt"], "plan.json").is_file())

    def test_unresolved_and_held_jobs_protect_workspace_build_targets(self) -> None:
        module = runpy.run_path(str(REPO / "scripts/ordivon-runtime-cache"))
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            runtime = root / "runtime"
            database = root / "registry.sqlite3"
            initialize_registry(database)
            source = root / "source"
            source.mkdir()
            unresolved_id = "workspace-unresolved"
            held_id = "workspace-held"
            record(runtime, unresolved_id, source)
            record(runtime, held_id, source)
            for workspace_id in (unresolved_id, held_id):
                build = runtime / "cache" / "build" / workspace_id
                build.mkdir(parents=True)
                (build / "artifact").write_bytes(b"b" * 120)
            with closing(sqlite3.connect(database)) as connection:
                connection.execute(
                    "INSERT INTO jobs VALUES ('job-unresolved',?,NULL)", (unresolved_id,)
                )
                connection.execute(
                    "INSERT INTO jobs VALUES ('job-held',?,'failed')", (held_id,)
                )
                connection.execute(
                    "INSERT INTO attempts VALUES ('attempt-held','job-held')"
                )
                connection.execute(
                    "INSERT INTO concurrency_reservations VALUES ('attempt-held','held_orphaned')"
                )
                connection.commit()

            _total, candidates = module["cache_prune_candidates"](
                database, runtime, include_idle_open_build=True
            )
            candidate_ids = {item.get("workspaceId") for item in candidates}
            self.assertNotIn(unresolved_id, candidate_ids)
            self.assertNotIn(held_id, candidate_ids)
            active = module["active_workspace_ids"](database)
            self.assertIn(unresolved_id, active)
            self.assertIn(held_id, active)

    def test_prune_reports_active_workspace_build_as_protected_residual(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            runtime = root / "runtime"
            database = root / "registry.sqlite3"
            initialize_registry(database, "workspace-protected")
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

    def test_prune_rechecks_idle_workspace_under_admission_fence(self) -> None:
        module = runpy.run_path(str(REPO / "scripts/ordivon-runtime-cache"))
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            runtime = root / "runtime"
            database = root / "registry.sqlite3"
            initialize_registry(database)
            source = root / "source"
            source.mkdir()
            workspace_id = "workspace-raced-active"
            record(runtime, workspace_id, source)
            build = runtime / "cache" / "build" / workspace_id
            build.mkdir(parents=True)
            (build / "artifact").write_bytes(b"b" * 120)
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
            fence = module["admission_fence_path"](database)
            fence.parent.mkdir(parents=True, exist_ok=True)
            result: dict[str, object] = {}
            errors: list[BaseException] = []

            def run_prune() -> None:
                try:
                    result["report"] = module["prune"](args)
                except BaseException as error:  # pragma: no cover - surfaced below
                    errors.append(error)

            with fence.open("a+", encoding="utf-8") as handle:
                fcntl.flock(handle.fileno(), fcntl.LOCK_SH)
                thread = threading.Thread(target=run_prune)
                thread.start()
                deadline = time.monotonic() + 2
                plan_paths: list[Path] = []
                while time.monotonic() < deadline:
                    plan_paths = list((root / "receipts").glob("*/plan.json"))
                    if plan_paths:
                        break
                    time.sleep(0.01)
                self.assertEqual(len(plan_paths), 1)
                self.assertTrue(thread.is_alive())
                with closing(sqlite3.connect(database)) as connection:
                    connection.execute(
                        "INSERT INTO jobs VALUES ('job-race',?,NULL)", (workspace_id,)
                    )
                    connection.execute(
                        "INSERT INTO attempts VALUES ('attempt-race','job-race')"
                    )
                    connection.execute(
                        "INSERT INTO concurrency_reservations VALUES ('attempt-race','active')"
                    )
                    connection.commit()
                fcntl.flock(handle.fileno(), fcntl.LOCK_UN)
            thread.join(timeout=2)
            self.assertFalse(thread.is_alive())
            self.assertEqual(errors, [])
            report = result["report"]
            self.assertIsInstance(report, dict)
            assert isinstance(report, dict)
            self.assertEqual(report["status"], "completed")
            self.assertEqual(report["capacityDisposition"], "protected_residual")
            self.assertEqual(report["failures"], [])
            self.assertEqual(len(report["actions"]), 1)
            self.assertEqual(report["actions"][0]["action"], "cache_retained")
            self.assertEqual(
                report["actions"][0]["reason"], "workspace_active_after_plan"
            )
            self.assertTrue(build.is_dir())
            self.assertTrue(Path(report["receipt"], "plan.json").is_file())

    def test_plan_then_terminal_reuse_invalidates_idle_build_candidate(self) -> None:
        module = runpy.run_path(str(REPO / "scripts/ordivon-runtime-cache"))
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            runtime = root / "runtime"
            database = root / "registry.sqlite3"
            initialize_registry(database)
            source = root / "source"
            source.mkdir()
            workspace_id = "workspace-raced-terminal"
            record(runtime, workspace_id, source)
            build = runtime / "cache" / "build" / workspace_id
            build.mkdir(parents=True)
            (build / "artifact").write_bytes(b"b" * 120)
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
            fence = module["admission_fence_path"](database)
            fence.parent.mkdir(parents=True, exist_ok=True)
            result: dict[str, object] = {}
            errors: list[BaseException] = []

            def run_prune() -> None:
                try:
                    result["report"] = module["prune"](args)
                except BaseException as error:  # pragma: no cover - surfaced below
                    errors.append(error)

            with fence.open("a+", encoding="utf-8") as handle:
                fcntl.flock(handle.fileno(), fcntl.LOCK_SH)
                thread = threading.Thread(target=run_prune)
                thread.start()
                deadline = time.monotonic() + 2
                plan_paths: list[Path] = []
                while time.monotonic() < deadline:
                    plan_paths = list((root / "receipts").glob("*/plan.json"))
                    if plan_paths:
                        break
                    time.sleep(0.01)
                self.assertEqual(len(plan_paths), 1)
                self.assertTrue(thread.is_alive())
                with closing(sqlite3.connect(database)) as connection:
                    connection.execute(
                        "INSERT INTO jobs VALUES ('job-terminal-race',?,'succeeded')",
                        (workspace_id,),
                    )
                    connection.execute(
                        "INSERT INTO attempts VALUES ('attempt-terminal-race','job-terminal-race')"
                    )
                    connection.commit()
                fcntl.flock(handle.fileno(), fcntl.LOCK_UN)
            thread.join(timeout=2)
            self.assertFalse(thread.is_alive())
            self.assertEqual(errors, [])
            report = result["report"]
            self.assertIsInstance(report, dict)
            assert isinstance(report, dict)
            self.assertEqual(report["status"], "partial")
            self.assertEqual(report["capacityDisposition"], "reclaimable_residual")
            self.assertEqual(report["failures"], [])
            self.assertEqual(len(report["actions"]), 1)
            self.assertEqual(report["actions"][0]["action"], "cache_retained")
            self.assertEqual(
                report["actions"][0]["reason"],
                "workspace_activity_changed_after_plan",
            )
            self.assertTrue(build.is_dir())

    def test_plan_then_workspace_record_unknown_retains_idle_build(self) -> None:
        module = runpy.run_path(str(REPO / "scripts/ordivon-runtime-cache"))
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            runtime = root / "runtime"
            database = root / "registry.sqlite3"
            initialize_registry(database)
            source = root / "source"
            source.mkdir()
            workspace_id = "workspace-record-race"
            record(runtime, workspace_id, source)
            build = runtime / "cache" / "build" / workspace_id
            build.mkdir(parents=True)
            (build / "artifact").write_bytes(b"b" * 120)
            _total, candidates = module["cache_prune_candidates"](
                database, runtime, include_idle_open_build=True
            )
            candidate = next(
                item for item in candidates if item.get("workspaceId") == workspace_id
            )
            record_path = runtime / "workspace-records" / f"{workspace_id}.json"
            record_path.write_text("{", encoding="utf-8")

            detached, retained_reason = module["detach_idle_open_build_candidate"](
                database, runtime, candidate, "receipt-test"
            )
            self.assertIsNone(detached)
            self.assertEqual(retained_reason, "workspace_record_unknown_after_plan")
            self.assertTrue(build.is_dir())

    def test_idle_build_detach_waits_for_runtime_admission_fence(self) -> None:
        module = runpy.run_path(str(REPO / "scripts/ordivon-runtime-cache"))
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            runtime = root / "runtime"
            database = root / "registry.sqlite3"
            initialize_registry(database)
            source = root / "source"
            source.mkdir()
            workspace_id = "workspace-fenced"
            record(runtime, workspace_id, source)
            build = runtime / "cache" / "build" / workspace_id
            build.mkdir(parents=True)
            (build / "artifact").write_bytes(b"b" * 120)
            _total, candidates = module["cache_prune_candidates"](
                database, runtime, include_idle_open_build=True
            )
            candidate = next(
                item for item in candidates if item.get("workspaceId") == workspace_id
            )
            fence = module["admission_fence_path"](database)
            fence.parent.mkdir(parents=True, exist_ok=True)
            result: dict[str, object] = {}
            errors: list[BaseException] = []

            def detach() -> None:
                try:
                    result["path"] = module["detach_idle_open_build_candidate"](
                        database, runtime, candidate, "receipt-test"
                    )
                except BaseException as error:  # pragma: no cover - surfaced below
                    errors.append(error)

            with fence.open("a+", encoding="utf-8") as handle:
                fcntl.flock(handle.fileno(), fcntl.LOCK_SH)
                thread = threading.Thread(target=detach)
                thread.start()
                time.sleep(0.05)
                self.assertTrue(thread.is_alive())
                self.assertTrue(build.is_dir())
                fcntl.flock(handle.fileno(), fcntl.LOCK_UN)
            thread.join(timeout=2)
            self.assertFalse(thread.is_alive())
            self.assertEqual(errors, [])
            detached_result = result["path"]
            self.assertIsInstance(detached_result, tuple)
            assert isinstance(detached_result, tuple)
            detached, retained_reason = detached_result
            self.assertIsInstance(detached, Path)
            assert isinstance(detached, Path)
            self.assertIsNone(retained_reason)
            self.assertFalse(build.exists())
            self.assertTrue(detached.is_dir())
            shutil.rmtree(detached)

    def test_idle_build_delete_failure_leaves_reclaimable_detached_residual(self) -> None:
        module = runpy.run_path(str(REPO / "scripts/ordivon-runtime-cache"))
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            runtime = root / "runtime"
            database = root / "registry.sqlite3"
            initialize_registry(database)
            source = root / "source"
            source.mkdir()
            workspace_id = "workspace-detach-failure"
            record(runtime, workspace_id, source)
            build = runtime / "cache" / "build" / workspace_id
            build.mkdir(parents=True)
            (build / "artifact").write_bytes(b"b" * 120)
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
            observed_plan_before_delete = False

            def fail_detached_delete(path: Path) -> None:
                nonlocal observed_plan_before_delete
                if ".prune-detached" in path.parts:
                    observed_plan_before_delete = bool(
                        list((root / "receipts").glob("*/plan.json"))
                    )
                    raise OSError("synthetic detached delete failure")
                original_rmtree(path)

            shutil_module.rmtree = fail_detached_delete
            try:
                report = module["prune"](args)
            finally:
                shutil_module.rmtree = original_rmtree
            self.assertTrue(observed_plan_before_delete)
            self.assertEqual(report["status"], "partial")
            self.assertEqual(report["capacityDisposition"], "reclaimable_residual")
            self.assertFalse(build.exists())
            detached = list((runtime / "cache" / ".prune-detached").iterdir())
            self.assertEqual(len(detached), 1)
            self.assertTrue((detached[0] / "artifact").is_file())
            self.assertGreaterEqual(report["remainingCandidateBytes"], 120)
            _total, candidates = module["cache_prune_candidates"](
                database, runtime, include_idle_open_build=True
            )
            stranded = [item for item in candidates if item["kind"] == "detached_eviction"]
            self.assertEqual(len(stranded), 1)
            self.assertEqual(Path(stranded[0]["path"]), detached[0])

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

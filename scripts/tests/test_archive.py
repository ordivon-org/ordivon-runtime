from __future__ import annotations

import hashlib
import json
from pathlib import Path
import sqlite3
import subprocess
import sys
import tempfile
import unittest


REPO = Path(__file__).resolve().parents[2]
SCRIPT = REPO / "scripts" / "ordivon-runtime-archive"
STABLE = {"succeeded", "failed", "timed_out", "cancelled"}


def initialize_registry(database: Path) -> None:
    with sqlite3.connect(database) as connection:
        connection.executescript(
            """
            CREATE TABLE schema_migrations(version INTEGER);
            INSERT INTO schema_migrations VALUES (4);
            CREATE TABLE jobs(
              job_id TEXT PRIMARY KEY, principal TEXT, client_request_id TEXT,
              request_digest TEXT, operation_digest TEXT, workspace_id TEXT,
              workspace_snapshot_json TEXT, execution_plan_json TEXT,
              execution_plan_digest TEXT, created_at_ms INTEGER,
              resolution TEXT, current_attempt_id TEXT
            );
            CREATE TABLE attempts(
              attempt_id TEXT PRIMARY KEY, job_id TEXT, attempt_number INTEGER, state TEXT,
              bundle_path TEXT, created_at_ms INTEGER
            );
            CREATE TABLE idempotency_keys(
              principal TEXT,client_request_id TEXT,operation_digest TEXT,
              job_id TEXT,created_at_ms INTEGER
            );
            CREATE TABLE concurrency_reservations(
              reservation_id TEXT,attempt_id TEXT,state TEXT
            );
            CREATE TABLE job_events(
              event_id TEXT,job_id TEXT,attempt_id TEXT,detail_json TEXT,detail_digest TEXT
            );
            CREATE TABLE artifacts(
              artifact_id TEXT,job_id TEXT,attempt_id TEXT,kind TEXT,relative_path TEXT,
              digest TEXT,media_type TEXT,byte_length INTEGER
            );
            CREATE TABLE attempt_conditions(
              attempt_id TEXT,condition_type TEXT,status TEXT,reason_code TEXT,evidence_digest TEXT
            );
            CREATE TABLE job_execution_providers(
              job_id TEXT,snapshot_json TEXT,snapshot_digest TEXT
            );
            CREATE TABLE job_runtime_release_effects(
              job_id TEXT,effect_id TEXT,binding_digest TEXT
            );
            CREATE TABLE job_host_dependencies(
              job_id TEXT,bindings_json TEXT,bindings_digest TEXT
            );
            CREATE TABLE attempt_supervisor_owners(
              attempt_id TEXT,owner_json TEXT,owner_digest TEXT
            );
            """
        )


def add_job(
    connection: sqlite3.Connection,
    job_id: str,
    *,
    created_at_ms: int = 1,
    resolution: str | None = "succeeded",
    attempt_state: str = "succeeded",
    release_effect: bool = False,
    reservation_state: str = "released",
    supervisor_owner: bool = False,
    current_pointer: bool = True,
) -> None:
    attempt_id = f"attempt:{job_id}"
    connection.execute(
        "INSERT INTO jobs VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
        (
            job_id,
            "principal:test",
            f"request:{job_id}",
            "sha256:" + "a" * 64,
            "sha256:" + "b" * 64,
            "ws:test",
            '{"workspace":"snapshot"}',
            '{"args":["fixture"],"env":{"PATH":"/bin"}}',
            "sha256:" + "c" * 64,
            created_at_ms,
            resolution,
            attempt_id if current_pointer else None,
        ),
    )
    connection.execute(
        "INSERT INTO attempts VALUES (?,?,?,?,?,?)",
        (attempt_id, job_id, 1, attempt_state, f"/attempts/{attempt_id}", created_at_ms),
    )
    connection.execute(
        "INSERT INTO idempotency_keys VALUES (?,?,?,?,?)",
        ("principal:test", f"request:{job_id}", "sha256:" + "b" * 64, job_id, created_at_ms),
    )
    connection.execute(
        "INSERT INTO concurrency_reservations VALUES (?,?,?)",
        (f"reservation:{job_id}", attempt_id, reservation_state),
    )
    connection.execute(
        "INSERT INTO job_events VALUES (?,?,?,?,?)",
        (f"event:{job_id}", job_id, attempt_id, '{"ok":true}', "sha256:" + "d" * 64),
    )
    connection.execute(
        "INSERT INTO artifacts VALUES (?,?,?,?,?,?,?,?)",
        (
            f"artifact:{job_id}",
            job_id,
            attempt_id,
            "stdout",
            "stdout.txt",
            "sha256:" + "e" * 64,
            "text/plain",
            123,
        ),
    )
    connection.execute(
        "INSERT INTO attempt_conditions VALUES (?,?,?,?,?)",
        (attempt_id, "result_available", "true", "fixture", "sha256:" + "f" * 64),
    )
    connection.execute(
        "INSERT INTO job_execution_providers VALUES (?,?,?)",
        (job_id, '{"provider":"fixture"}', "sha256:" + "1" * 64),
    )
    connection.execute(
        "INSERT INTO job_host_dependencies VALUES (?,?,?)",
        (job_id, '[]', "sha256:" + "2" * 64),
    )
    if release_effect:
        connection.execute(
            "INSERT INTO job_runtime_release_effects VALUES (?,?,?)",
            (job_id, f"effect:{job_id}", "sha256:" + "3" * 64),
        )
    if supervisor_owner:
        connection.execute(
            "INSERT INTO attempt_supervisor_owners VALUES (?,?,?)",
            (attempt_id, '{"owner":"fixture"}', "sha256:" + "4" * 64),
        )


class RuntimeArchiveInspectTests(unittest.TestCase):
    def run_archive(self, database: Path, *extra: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [
                sys.executable,
                str(SCRIPT),
                "--database",
                str(database),
                "--now-ms",
                "1000000000",
                "--minimum-age-hours",
                "1",
                *extra,
            ],
            cwd=REPO,
            text=True,
            capture_output=True,
            check=False,
        )

    @staticmethod
    def classifications(report: dict[str, object]) -> dict[str, int]:
        summary = report["summary"]
        assert isinstance(summary, dict)
        rows = summary["byClassification"]
        assert isinstance(rows, list)
        return {str(row["classification"]): int(row["jobs"]) for row in rows}

    def test_stable_old_terminal_job_is_eligible_with_complete_closure_counts(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            database = Path(directory) / "registry.sqlite3"
            initialize_registry(database)
            with sqlite3.connect(database) as connection:
                add_job(connection, "job:eligible")
                connection.commit()

            completed = self.run_archive(database)
            self.assertEqual(completed.returncode, 0, completed.stderr)
            report = json.loads(completed.stdout)
            self.assertEqual(report["truthRole"], "archive-eligibility-observation-not-export-or-deletion-authority")
            self.assertEqual(self.classifications(report), {"eligible": 1})
            closure = report["summary"]["eligibleClosure"]
            self.assertEqual(closure["jobs"], 1)
            self.assertEqual(closure["attempts"], 1)
            self.assertEqual(closure["idempotencyKeys"], 1)
            self.assertEqual(closure["reservations"], 1)
            self.assertEqual(closure["events"], 1)
            self.assertEqual(closure["artifacts"], 1)
            self.assertEqual(closure["conditions"], 1)
            self.assertEqual(closure["executionProviders"], 1)
            self.assertEqual(closure["hostDependencies"], 1)
            self.assertEqual(closure["supervisorOwners"], 0)
            self.assertEqual(closure["artifactLogicalPayloadBytes"], 123)
            self.assertFalse(report["boundaries"]["mutatesRegistry"])
            self.assertFalse(report["boundaries"]["deletesHotHistory"])

    def test_legacy_null_current_attempt_uses_runtime_latest_attempt_fallback(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            database = Path(directory) / "registry.sqlite3"
            initialize_registry(database)
            with sqlite3.connect(database) as connection:
                add_job(connection, "job:legacy", current_pointer=False)
                connection.commit()

            completed = self.run_archive(database)
            self.assertEqual(completed.returncode, 0, completed.stderr)
            report = json.loads(completed.stdout)
            self.assertEqual(self.classifications(report), {"eligible": 1})
            self.assertIsNone(report["eligibleSample"][0]["jobCurrentAttemptId"])
            self.assertEqual(
                report["eligibleSample"][0]["effectiveAttemptId"],
                "attempt:job:legacy",
            )

    def test_repairable_and_unsettled_states_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            database = Path(directory) / "registry.sqlite3"
            initialize_registry(database)
            with sqlite3.connect(database) as connection:
                add_job(connection, "job:lost", resolution="lost", attempt_state="lost")
                add_job(connection, "job:orphaned", resolution="orphaned", attempt_state="orphaned")
                add_job(connection, "job:running-attempt", attempt_state="running")
                add_job(connection, "job:held", reservation_state="held_orphaned")
                add_job(connection, "job:owner", supervisor_owner=True)
                connection.commit()

            completed = self.run_archive(database)
            self.assertEqual(completed.returncode, 0, completed.stderr)
            self.assertEqual(
                self.classifications(json.loads(completed.stdout)),
                {
                    "active_or_held_reservation": 1,
                    "attempt_not_stable_terminal": 1,
                    "repairable_terminal": 2,
                    "supervisor_owner_present": 1,
                },
            )

    def test_release_effect_and_young_terminal_are_excluded(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            database = Path(directory) / "registry.sqlite3"
            initialize_registry(database)
            with sqlite3.connect(database) as connection:
                add_job(connection, "job:release", release_effect=True)
                add_job(connection, "job:young", created_at_ms=999_999_999)
                connection.commit()

            completed = self.run_archive(database)
            self.assertEqual(completed.returncode, 0, completed.stderr)
            self.assertEqual(
                self.classifications(json.loads(completed.stdout)),
                {"runtime_release_effect": 1, "younger_than_minimum_age": 1},
            )

    def test_missing_required_registry_capability_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            database = Path(directory) / "registry.sqlite3"
            initialize_registry(database)
            with sqlite3.connect(database) as connection:
                connection.execute("DROP TABLE job_host_dependencies")
                connection.commit()
            completed = self.run_archive(database)
            self.assertEqual(completed.returncode, 1)
            self.assertIn("table:job_host_dependencies", completed.stderr)

    def test_inspection_does_not_mutate_registry_bytes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            database = Path(directory) / "registry.sqlite3"
            initialize_registry(database)
            with sqlite3.connect(database) as connection:
                add_job(connection, "job:immutable")
                connection.commit()
            before = hashlib.sha256(database.read_bytes()).hexdigest()
            completed = self.run_archive(database, "--sample-limit", "0")
            after = hashlib.sha256(database.read_bytes()).hexdigest()
            self.assertEqual(completed.returncode, 0, completed.stderr)
            self.assertEqual(after, before)


if __name__ == "__main__":
    unittest.main()

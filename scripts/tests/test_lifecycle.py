from __future__ import annotations

import json
import os
import runpy
import sqlite3
import subprocess
import sys
import tempfile
import unittest
from contextlib import closing
from pathlib import Path


REPO = Path(__file__).resolve().parents[2]


def initialize_registry(database: Path, workspace_id: str | None = None, activity_ms: int = 0) -> None:
    with closing(sqlite3.connect(database)) as connection:
        connection.executescript(
            """
            CREATE TABLE schema_migrations(version INTEGER);
            INSERT INTO schema_migrations VALUES (4);
            CREATE TABLE jobs(
              job_id TEXT,
              workspace_id TEXT,
              resolution TEXT,
              created_at_ms INTEGER
            );
            CREATE TABLE attempts(
              attempt_id TEXT,
              job_id TEXT,
              state TEXT,
              created_at_ms INTEGER,
              started_at_ms INTEGER,
              finished_at_ms INTEGER
            );
            CREATE TABLE concurrency_reservations(attempt_id TEXT,state TEXT);
            """
        )
        if workspace_id is not None:
            connection.execute(
                "INSERT INTO jobs VALUES ('job-1', ?, 'succeeded', ?)",
                (workspace_id, activity_ms),
            )
            connection.execute(
                "INSERT INTO attempts VALUES ('attempt-1','job-1','succeeded',?,?,?)",
                (activity_ms, activity_ms, activity_ms),
            )
        connection.commit()


def init_repository(path: Path) -> str:
    subprocess.run(["git", "init", "-q", str(path)], check=True)
    subprocess.run(["git", "-C", str(path), "config", "user.name", "Test"], check=True)
    subprocess.run(
        ["git", "-C", str(path), "config", "user.email", "test@example.invalid"],
        check=True,
    )
    (path / "README.md").write_text("baseline\n", encoding="utf-8")
    subprocess.run(["git", "-C", str(path), "add", "README.md"], check=True)
    subprocess.run(["git", "-C", str(path), "commit", "-qm", "baseline"], check=True)
    return subprocess.run(
        ["git", "-C", str(path), "rev-parse", "HEAD"],
        check=True,
        text=True,
        capture_output=True,
    ).stdout.strip()


class LifecycleTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.module = runpy.run_path(str(REPO / "scripts/ordivon-runtime-lifecycle"))

    def test_trusted_tmp_presentation_matches_runtime_hash_vector(self) -> None:
        self.assertEqual(
            self.module["trusted_tmp_presentation_path"](Path("/tmp"), "workspace-env"),
            Path("/tmp/ordivon-t/3536fc95f765287c720c"),
        )

    def test_trusted_tmp_presentation_separates_runtime_store_namespaces(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            first_store = root / "store-a"
            second_store = root / "store-b"
            first_store.mkdir()
            second_store.mkdir()
            first = self.module["trusted_tmp_presentation_path"](first_store, "same-workspace-id")
            second = self.module["trusted_tmp_presentation_path"](second_store, "same-workspace-id")
            self.assertNotEqual(first, second)
            self.assertEqual(len(str(first)), 35)
            self.assertEqual(len(str(second)), 35)

    def test_trusted_tmp_presentation_converges_equivalent_store_spelling(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            store = root / "store"
            store.mkdir()
            alias = root / "store-link"
            alias.symlink_to(store, target_is_directory=True)
            direct = self.module["trusted_tmp_presentation_path"](store, "same-workspace-id")
            through_alias = self.module["trusted_tmp_presentation_path"](alias, "same-workspace-id")
            self.assertEqual(direct, through_alias)

    def test_policy_numbers_follow_real_representation_not_legacy_round_limits(self) -> None:
        self.assertEqual(cls_timeout := self.module["positive_timeout"]("60001"), 60_001)
        self.assertEqual(cls_timeout, 60_001)
        self.assertEqual(self.module["positive_timeout"]("2147483647"), 2_147_483_647)
        with self.assertRaises(Exception):
            self.module["positive_timeout"]("2147483648")
        self.assertEqual(self.module["nonnegative_float"]("87601"), 87_601.0)
        with self.assertRaises(Exception):
            self.module["nonnegative_float"]("inf")
        with self.assertRaises(Exception):
            self.module["nonnegative_float"]("-1")

    def test_carrier_projection_preserves_physical_and_semantic_boundaries(self) -> None:
        cases = (
            (
                {
                    "classification": "closable",
                    "policyEligible": True,
                    "retentionHours": 24.0,
                },
                "CLEAN_IDLE",
                "DECLARE_CLOSE_CLEAN_OR_RETAIN",
                "ELIGIBLE",
            ),
            (
                {"classification": "blocked_dirty", "policyEligible": False},
                "DIRTY_IDLE",
                "DECLARE_DIRTY_HANDOFF_OR_CHECKPOINT",
                "NOT_APPLICABLE",
            ),
            (
                {"classification": "blocked_active", "policyEligible": False},
                "ACTIVE_DIRTY_STATE_UNINSPECTED",
                "OBSERVE_OR_RECONCILE_ACTIVE_JOB",
                "NOT_APPLICABLE",
            ),
            (
                {
                    "classification": "stale_record",
                    "policyEligible": False,
                    "retentionHours": 24.0,
                },
                "MISSING_WITH_OPEN_RECORD",
                "RECONCILE_STALE_RECORD",
                "WAITING",
            ),
            (
                {"classification": "unknown", "policyEligible": False},
                "UNPROVEN",
                "INSPECT_BEFORE_DISPOSITION",
                "NOT_APPLICABLE",
            ),
        )
        for item, physical_state, next_step, retention_state in cases:
            with self.subTest(classification=item["classification"]):
                projection = self.module["carrier_projection"](item)
                self.assertEqual(projection["physicalState"], physical_state)
                self.assertEqual(projection["nextProtocolStep"], next_step)
                self.assertEqual(
                    projection["retentionPolicyState"], retention_state
                )
                self.assertFalse(projection["semanticCompletionEvaluated"])
                self.assertIn("Host/owner", projection["interpretation"])

    def test_packaged_lifecycle_unit_only_invokes_supported_lifecycle_commands(self) -> None:
        unit = (REPO / "packaging/systemd/ordivon-runtime-lifecycle.service").read_text(
            encoding="utf-8"
        )
        marker = "/ordivon-runtime-lifecycle "
        commands = [
            line.split(marker, 1)[1].split()[0]
            for line in unit.splitlines()
            if marker in line
        ]
        self.assertTrue(commands)
        help_result = subprocess.run(
            [sys.executable, "scripts/ordivon-runtime-lifecycle", "--help"],
            cwd=REPO,
            check=True,
            text=True,
            capture_output=True,
        )
        for command in commands:
            self.assertIn(command, help_result.stdout)
        self.assertNotIn("inputs-sweep", commands)

    def test_inspect_applies_retention_class_and_last_activity(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            runtime = root / "runtime"
            records = runtime / "workspace-records"
            workspaces = runtime / "workspaces"
            records.mkdir(parents=True)
            workspaces.mkdir()
            database = root / "registry.sqlite3"
            initialize_registry(database, "named-audit-workspace", 2_000)
            workspace = workspaces / "named-audit-workspace"
            revision = init_repository(workspace)
            (records / "named-audit-workspace.json").write_text(
                json.dumps(
                    {
                        "schemaVersion": 1,
                        "workspaceId": "named-audit-workspace",
                        "sourceRepo": str(workspace),
                        "sourceRevision": revision,
                        "workspacePath": str(workspace),
                        "createdUnixMs": 1_000,
                    }
                ),
                encoding="utf-8",
            )
            result = subprocess.run(
                [
                    sys.executable,
                    "scripts/ordivon-runtime-lifecycle",
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
            item = report["candidates"][0]
            self.assertEqual(item["retentionClass"], "ephemeral")
            self.assertEqual(item["retentionHours"], 24.0)
            self.assertEqual(item["lastActivityUnixMs"], 2_000)
            self.assertEqual(item["retentionBasisUnixMs"], 2_000)
            self.assertTrue(item["policyEligible"])
            self.assertEqual(
                item["carrierProjection"]["physicalState"], "CLEAN_IDLE"
            )
            self.assertEqual(
                item["carrierProjection"]["nextProtocolStep"],
                "DECLARE_CLOSE_CLEAN_OR_RETAIN",
            )
            self.assertFalse(
                item["carrierProjection"]["semanticCompletionEvaluated"]
            )


    def test_sweep_selects_only_policy_expired_reclaimable_workspaces(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            runtime = root / "runtime"
            records = runtime / "workspace-records"
            workspaces = runtime / "workspaces"
            records.mkdir(parents=True)
            workspaces.mkdir()
            database = root / "registry.sqlite3"
            initialize_registry(database, "ws-expired", 1)
            workspace = workspaces / "ws-expired"
            revision = init_repository(workspace)
            (records / "ws-expired.json").write_text(
                json.dumps(
                    {
                        "schemaVersion": 1,
                        "workspaceId": "ws-expired",
                        "sourceRepo": str(workspace),
                        "sourceRevision": revision,
                        "workspacePath": str(workspace),
                        "createdUnixMs": 1,
                    }
                ),
                encoding="utf-8",
            )
            policy = root / "policy.json"
            policy.write_text(
                json.dumps(
                    {
                        "schemaVersion": 1,
                        "classes": {"ephemeral": {"retentionHours": 0}},
                    }
                ),
                encoding="utf-8",
            )
            fake_reclaim = root / "fake-reclaim"
            fake_reclaim.write_text(
                "#!/usr/bin/env python3\n"
                "import json, sys\n"
                "command = sys.argv[1]\n"
                "if command == 'inspect':\n"
                " print(json.dumps({'schemaVersion':1,'summary':{'counts':{'closable':1},'estimatedBytes':{'closable':10}},'candidates':[{'workspaceId':'ws-expired','classification':'closable','createdUnixMs':1,'estimatedBytes':10}]}))\n"
                "elif command == 'apply':\n"
                " ids=[sys.argv[i+1] for i,v in enumerate(sys.argv) if v == '--workspace-id']\n"
                " print(json.dumps({'schemaVersion':1,'status':'completed','actions':[{'workspaceId':x,'action':'workspace_closed'} for x in ids],'failures':[],'receipt':'/fake/reclaim'}))\n"
                "else: raise SystemExit(1)\n",
                encoding="utf-8",
            )
            fake_reclaim.chmod(0o755)
            env_file = root / "runtime.env"
            env_file.write_text(
                "ORDIVON_BIND=127.0.0.1:1\nORDIVON_BEARER_TOKEN=test\n",
                encoding="utf-8",
            )
            environment = os.environ.copy()
            environment["ORDIVON_RUNTIME_RECLAIM"] = str(fake_reclaim)
            result = subprocess.run(
                [
                    sys.executable,
                    "scripts/ordivon-runtime-lifecycle",
                    "sweep",
                    "--database",
                    str(database),
                    "--runtime-store-root",
                    str(runtime),
                    "--policy-file",
                    str(policy),
                    "--env-file",
                    str(env_file),
                    "--receipt-root",
                    str(root / "receipts"),
                    "--lock-file",
                    str(root / "lifecycle.lock"),
                    "--confirm-policy",
                    "APPLY_WORKSPACE_RETENTION_POLICY",
                ],
                cwd=REPO,
                env=environment,
                check=True,
                text=True,
                capture_output=True,
            )
            report = json.loads(result.stdout)
            self.assertEqual(report["status"], "completed")
            self.assertEqual(report["selectedWorkspaceIds"], ["ws-expired"])
            self.assertEqual(report["actions"][0]["workspaceId"], "ws-expired")

    def test_repair_rebinds_worktree_after_source_repository_rename(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            old_repo = root / "old-repo"
            revision = init_repository(old_repo)
            runtime = root / "runtime"
            records = runtime / "workspace-records"
            workspaces = runtime / "workspaces"
            records.mkdir(parents=True)
            workspaces.mkdir()
            workspace_id = "renamed-worktree"
            workspace = workspaces / workspace_id
            subprocess.run(
                ["git", "-C", str(old_repo), "worktree", "add", "--detach", str(workspace), revision],
                check=True,
                capture_output=True,
            )
            record = records / f"{workspace_id}.json"
            record.write_text(
                json.dumps(
                    {
                        "schemaVersion": 1,
                        "workspaceId": workspace_id,
                        "sourceRepo": str(old_repo),
                        "sourceRevision": revision,
                        "workspacePath": str(workspace),
                        "createdUnixMs": 1,
                    }
                ),
                encoding="utf-8",
            )
            new_repo = root / "new-repo"
            old_repo.rename(new_repo)
            database = root / "registry.sqlite3"
            initialize_registry(database)
            policy = root / "policy.json"
            policy.write_text(
                json.dumps(
                    {
                        "schemaVersion": 1,
                        "sourceRepoAliases": {str(old_repo): str(new_repo)},
                    }
                ),
                encoding="utf-8",
            )
            result = subprocess.run(
                [
                    sys.executable,
                    "scripts/ordivon-runtime-lifecycle",
                    "repair",
                    "--database",
                    str(database),
                    "--runtime-store-root",
                    str(runtime),
                    "--policy-file",
                    str(policy),
                    "--receipt-root",
                    str(root / "receipts"),
                    "--lock-file",
                    str(root / "lifecycle.lock"),
                    "--workspace-id",
                    workspace_id,
                    "--confirm-policy",
                    "REPAIR_WORKSPACE_IDENTITIES",
                ],
                cwd=REPO,
                check=True,
                text=True,
                capture_output=True,
            )
            report = json.loads(result.stdout)
            self.assertEqual(report["status"], "completed")
            self.assertEqual(report["actions"][0]["action"], "worktree_repaired")
            updated = json.loads(record.read_text(encoding="utf-8"))
            self.assertEqual(updated["sourceRepo"], str(new_repo.resolve()))
            observed = subprocess.run(
                ["git", "-C", str(workspace), "rev-parse", "HEAD"],
                check=True,
                text=True,
                capture_output=True,
            ).stdout.strip()
            self.assertEqual(observed, revision)

    def test_repair_can_quarantine_unrepairable_workspace_with_tombstone(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            runtime = root / "runtime"
            records = runtime / "workspace-records"
            workspaces = runtime / "workspaces"
            records.mkdir(parents=True)
            workspaces.mkdir()
            workspace_id = "broken-quarantine-tmp-presentation"
            workspace = workspaces / workspace_id
            workspace.mkdir()
            (workspace / "important.txt").write_text("preserve me\n", encoding="utf-8")
            tmp_backing = (
                self.module["canonical_runtime_store_root"](runtime)
                / "cache"
                / "tmp"
                / workspace_id
            )
            tmp_backing.mkdir(parents=True)
            tmp_presentation = self.module["trusted_tmp_presentation_path"](runtime, workspace_id)
            tmp_presentation.parent.mkdir(mode=0o700, exist_ok=True)
            try:
                tmp_presentation.unlink()
            except FileNotFoundError:
                pass
            tmp_presentation.symlink_to(tmp_backing, target_is_directory=True)
            record = records / f"{workspace_id}.json"
            record.write_text(
                json.dumps(
                    {
                        "schemaVersion": 1,
                        "workspaceId": workspace_id,
                        "sourceRepo": str(root / "missing-source"),
                        "sourceRevision": "a" * 40,
                        "workspacePath": str(workspace),
                        "createdUnixMs": 1,
                    }
                ),
                encoding="utf-8",
            )
            database = root / "registry.sqlite3"
            initialize_registry(database)
            quarantine = root / "quarantine"
            result = subprocess.run(
                [
                    sys.executable,
                    "scripts/ordivon-runtime-lifecycle",
                    "repair",
                    "--database",
                    str(database),
                    "--runtime-store-root",
                    str(runtime),
                    "--receipt-root",
                    str(root / "receipts"),
                    "--lock-file",
                    str(root / "lifecycle.lock"),
                    "--workspace-id",
                    workspace_id,
                    "--confirm-policy",
                    "REPAIR_WORKSPACE_IDENTITIES",
                    "--quarantine-unrepairable",
                    "--quarantine-root",
                    str(quarantine),
                    "--confirm-quarantine",
                    "QUARANTINE_BROKEN_WORKSPACES",
                ],
                cwd=REPO,
                check=True,
                text=True,
                capture_output=True,
            )
            report = json.loads(result.stdout)
            self.assertEqual(report["actions"][0]["action"], "workspace_quarantined")
            tombstone = json.loads(record.read_text(encoding="utf-8"))
            self.assertEqual(tombstone["state"], "closed")
            self.assertEqual(tombstone["removalResult"], "quarantined_broken")
            quarantined = Path(report["actions"][0]["quarantinePath"])
            self.assertEqual((quarantined / "important.txt").read_text(), "preserve me\n")
            self.assertEqual(report["actions"][0]["removedTmpPresentation"], str(tmp_presentation))
            self.assertFalse(tmp_presentation.is_symlink())

    def test_repair_explicitly_quarantines_true_orphan_without_inventing_source_identity(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "source"
            revision = init_repository(source)
            runtime = root / "runtime"
            records = runtime / "workspace-records"
            workspaces = runtime / "workspaces"
            records.mkdir(parents=True)
            workspaces.mkdir()
            workspace_id = "true-orphan-worktree"
            workspace = workspaces / workspace_id
            subprocess.run(
                ["git", "-C", str(source), "worktree", "add", "--detach", str(workspace), revision],
                check=True, capture_output=True,
            )
            (workspace / "important-untracked.txt").write_text("preserve uncertain bytes\n", encoding="utf-8")
            database = root / "registry.sqlite3"
            initialize_registry(database)
            quarantine = root / "quarantine"
            result = subprocess.run(
                [
                    sys.executable, "scripts/ordivon-runtime-lifecycle", "repair",
                    "--database", str(database),
                    "--runtime-store-root", str(runtime),
                    "--receipt-root", str(root / "receipts"),
                    "--lock-file", str(root / "lifecycle.lock"),
                    "--workspace-id", workspace_id,
                    "--confirm-policy", "REPAIR_WORKSPACE_IDENTITIES",
                    "--quarantine-unrepairable",
                    "--quarantine-root", str(quarantine),
                    "--confirm-quarantine", "QUARANTINE_BROKEN_WORKSPACES",
                ],
                cwd=REPO, check=True, text=True, capture_output=True,
            )
            report = json.loads(result.stdout)
            action = report["actions"][0]
            self.assertEqual(action["action"], "orphan_workspace_quarantined")
            self.assertEqual(action["sourceIdentityStanding"], "UNKNOWN_NO_RUNTIME_RECORD")
            self.assertFalse(workspace.exists())
            target = Path(action["quarantinePath"])
            self.assertEqual((target / "important-untracked.txt").read_text(), "preserve uncertain bytes\n")
            self.assertEqual(
                subprocess.run(["git", "-C", str(target), "rev-parse", "HEAD"], check=True, text=True, capture_output=True).stdout.strip(),
                revision,
            )
            self.assertEqual(action["gitRegistrationRepair"]["standing"], "REPAIRED")
            worktrees = subprocess.run(["git", "-C", str(source), "worktree", "list", "--porcelain"], check=True, text=True, capture_output=True).stdout
            self.assertIn(str(target), worktrees)
            self.assertNotIn(str(workspace) + "\n", worktrees)
            tombstone = json.loads((records / f"{workspace_id}.json").read_text(encoding="utf-8"))
            self.assertEqual(tombstone["state"], "closed")
            self.assertEqual(tombstone["removalResult"], "quarantined_orphan")
            self.assertEqual(tombstone["finalHead"], revision)
            self.assertNotIn("sourceRepo", tombstone)
            self.assertNotIn("sourceRevision", tombstone)
            inspect = subprocess.run(
                [sys.executable, "scripts/ordivon-runtime-reclaim", "inspect", "--database", str(database), "--runtime-store-root", str(runtime)],
                cwd=REPO, check=True, text=True, capture_output=True,
            )
            candidates = json.loads(inspect.stdout)["candidates"]
            self.assertFalse(any(row["workspaceId"] == workspace_id for row in candidates))

    def test_true_orphan_quarantine_is_never_bulk_or_implicit(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            runtime = root / "runtime"
            (runtime / "workspace-records").mkdir(parents=True)
            workspaces = runtime / "workspaces"
            workspaces.mkdir()
            workspace_id = "true-orphan-explicit-only"
            workspace = workspaces / workspace_id
            workspace.mkdir()
            (workspace / "important.txt").write_text("keep\n", encoding="utf-8")
            database = root / "registry.sqlite3"
            initialize_registry(database)
            base = [
                sys.executable, "scripts/ordivon-runtime-lifecycle", "repair",
                "--database", str(database), "--runtime-store-root", str(runtime),
                "--receipt-root", str(root / "receipts"), "--lock-file", str(root / "lifecycle.lock"),
                "--confirm-policy", "REPAIR_WORKSPACE_IDENTITIES",
            ]
            bulk = subprocess.run(base + ["--quarantine-unrepairable", "--quarantine-root", str(root / "q"), "--confirm-quarantine", "QUARANTINE_BROKEN_WORKSPACES"], cwd=REPO, check=True, text=True, capture_output=True)
            self.assertEqual(json.loads(bulk.stdout)["actions"], [])
            self.assertTrue(workspace.exists())
            implicit = subprocess.run(base + ["--workspace-id", workspace_id], cwd=REPO, check=False, text=True, capture_output=True)
            self.assertEqual(implicit.returncode, 2)
            self.assertIn("explicit --quarantine-unrepairable", implicit.stdout)
            self.assertTrue(workspace.exists())
            self.assertFalse((runtime / "workspace-records" / f"{workspace_id}.json").exists())

    def test_true_orphan_quarantine_refuses_active_job(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            runtime = root / "runtime"
            (runtime / "workspace-records").mkdir(parents=True)
            workspaces = runtime / "workspaces"
            workspaces.mkdir()
            workspace_id = "true-orphan-active"
            workspace = workspaces / workspace_id
            workspace.mkdir()
            database = root / "registry.sqlite3"
            initialize_registry(database, workspace_id, 1)
            with closing(sqlite3.connect(database)) as connection:
                connection.execute("UPDATE jobs SET resolution=NULL WHERE workspace_id=?", (workspace_id,))
                connection.execute("UPDATE attempts SET state='running',finished_at_ms=NULL WHERE job_id='job-1'")
                connection.execute("INSERT INTO concurrency_reservations VALUES ('attempt-1','active')")
                connection.commit()
            result = subprocess.run(
                [
                    sys.executable, "scripts/ordivon-runtime-lifecycle", "repair",
                    "--database", str(database), "--runtime-store-root", str(runtime),
                    "--receipt-root", str(root / "receipts"), "--lock-file", str(root / "lifecycle.lock"),
                    "--workspace-id", workspace_id, "--confirm-policy", "REPAIR_WORKSPACE_IDENTITIES",
                    "--quarantine-unrepairable", "--quarantine-root", str(root / "q"),
                    "--confirm-quarantine", "QUARANTINE_BROKEN_WORKSPACES",
                ],
                cwd=REPO, check=False, text=True, capture_output=True,
            )
            self.assertEqual(result.returncode, 2)
            self.assertIn("active or held Job", result.stdout)
            self.assertTrue(workspace.exists())
            self.assertFalse((runtime / "workspace-records" / f"{workspace_id}.json").exists())

    def test_quarantine_tmp_presentation_wrong_target_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            runtime = Path(temporary) / "runtime"
            workspace_id = "wrong-target-quarantine"
            expected = runtime / "cache" / "tmp" / workspace_id
            wrong = runtime / "cache" / "tmp" / "another-workspace"
            expected.mkdir(parents=True)
            wrong.mkdir(parents=True)
            presentation = self.module["trusted_tmp_presentation_path"](runtime, workspace_id)
            presentation.parent.mkdir(mode=0o700, exist_ok=True)
            try:
                presentation.unlink()
            except FileNotFoundError:
                pass
            presentation.symlink_to(wrong, target_is_directory=True)
            try:
                with self.assertRaisesRegex(RuntimeError, "points at"):
                    self.module["remove_trusted_tmp_presentation"](runtime, workspace_id)
                self.assertTrue(presentation.is_symlink())
                self.assertEqual(Path(os.readlink(presentation)), wrong)
            finally:
                try:
                    presentation.unlink()
                except FileNotFoundError:
                    pass

    def test_dirty_review_records_exact_evidence_without_mutating_workspace(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            runtime = root / "runtime"
            records = runtime / "workspace-records"
            workspaces = runtime / "workspaces"
            records.mkdir(parents=True)
            workspaces.mkdir()
            workspace_id = "dirty-review"
            workspace = workspaces / workspace_id
            revision = init_repository(workspace)
            (workspace / "README.md").write_text("changed\n", encoding="utf-8")
            (workspace / "new.txt").write_text("untracked\n", encoding="utf-8")
            record = records / f"{workspace_id}.json"
            record.write_text(
                json.dumps({
                    "schemaVersion": 1,
                    "workspaceId": workspace_id,
                    "sourceRepo": str(workspace),
                    "sourceRevision": revision,
                    "workspacePath": str(workspace),
                    "createdUnixMs": 1,
                }),
                encoding="utf-8",
            )
            database = root / "registry.sqlite3"
            initialize_registry(database)
            fake_reclaim = root / "fake-reclaim"
            fake_reclaim.write_text(
                "#!/usr/bin/env python3\n"
                "import json\n"
                f"print(json.dumps({{'schemaVersion': 1, 'summary': {{'counts': {{'blocked_dirty': 1}}}}, 'candidates': [{{'workspaceId': '{workspace_id}', 'classification': 'blocked_dirty', 'createdUnixMs': 1, 'lastActivityUnixMs': 1}}]}}))\n",
                encoding="utf-8",
            )
            fake_reclaim.chmod(0o755)
            environment = os.environ.copy()
            environment["ORDIVON_RUNTIME_RECLAIM"] = str(fake_reclaim)
            before = subprocess.run(
                ["git", "-C", str(workspace), "status", "--porcelain=v1", "--untracked-files=all"],
                check=True, text=True, capture_output=True,
            ).stdout
            result = subprocess.run(
                [
                    sys.executable, "scripts/ordivon-runtime-lifecycle", "dirty-review",
                    "--database", str(database),
                    "--runtime-store-root", str(runtime),
                    "--receipt-root", str(root / "receipts"),
                    "--lock-file", str(root / "lifecycle.lock"),
                    "--workspace-id", workspace_id,
                ],
                cwd=REPO, env=environment, check=False, text=True, capture_output=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            report = json.loads(result.stdout)
            self.assertEqual(report["status"], "completed")
            action = report["actions"][0]
            self.assertEqual(action["workspaceId"], workspace_id)
            self.assertEqual(action["currentHeadRevision"], revision)
            self.assertEqual(action["recommendedOwnerAction"], "quarantine_review")
            self.assertFalse(action["automaticDeletionAllowed"])
            self.assertGreater(action["trackedDiffBytes"], 0)
            self.assertEqual([item["path"] for item in action["untracked"]], ["new.txt"])
            after = subprocess.run(
                ["git", "-C", str(workspace), "status", "--porcelain=v1", "--untracked-files=all"],
                check=True, text=True, capture_output=True,
            ).stdout
            self.assertEqual(after, before)
            receipt = Path(report["receipt"]) / "result.json"
            self.assertTrue(receipt.is_file())
            self.assertEqual(json.loads(receipt.read_text())["actions"][0]["statusDigest"], action["statusDigest"])


    def test_sweep_rejects_incompatible_reclaim_contract_before_apply(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            marker = root / "apply-called"
            fake_reclaim = root / "fake-reclaim"
            fake_reclaim.write_text(
                "#!/usr/bin/env python3\n"
                "import json, pathlib, sys\n"
                "if sys.argv[1] == 'inspect':\n"
                " print(json.dumps({'schemaVersion': 2, 'summary': {}, 'candidates': []}))\n"
                "elif sys.argv[1] == 'apply':\n"
                f" pathlib.Path({str(marker)!r}).write_text('called')\n"
                " print(json.dumps({'schemaVersion': 2, 'status': 'completed', 'actions': [], 'failures': [], 'receipt': '/fake'}))\n"
                "else: raise SystemExit(1)\n",
                encoding="utf-8",
            )
            fake_reclaim.chmod(0o755)
            environment = os.environ.copy()
            environment["ORDIVON_RUNTIME_RECLAIM"] = str(fake_reclaim)
            result = subprocess.run(
                [
                    sys.executable,
                    "scripts/ordivon-runtime-lifecycle",
                    "sweep",
                    "--database",
                    str(root / "unused.sqlite3"),
                    "--runtime-store-root",
                    str(root / "runtime"),
                    "--env-file",
                    str(root / "unused.env"),
                    "--receipt-root",
                    str(root / "receipts"),
                    "--lock-file",
                    str(root / "lifecycle.lock"),
                    "--confirm-policy",
                    "APPLY_WORKSPACE_RETENTION_POLICY",
                ],
                cwd=REPO,
                env=environment,
                check=False,
                text=True,
                capture_output=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("unsupported schemaVersion", result.stderr)
            self.assertFalse(marker.exists(), "incompatible inspect contract reached apply")

    def test_reclaim_result_validators_reject_malformed_structures(self) -> None:
        with self.assertRaisesRegex(RuntimeError, "candidates must be an array"):
            self.module["validate_reclaim_inspect"](
                {"schemaVersion": 1, "summary": {}, "candidates": {}}
            )
        with self.assertRaisesRegex(RuntimeError, "duplicate workspaceId"):
            self.module["validate_reclaim_inspect"](
                {
                    "schemaVersion": 1,
                    "summary": {},
                    "candidates": [
                        {"workspaceId": "same", "classification": "closable"},
                        {"workspaceId": "same", "classification": "closable"},
                    ],
                }
            )
        with self.assertRaisesRegex(RuntimeError, "actions must be an array of objects"):
            self.module["validate_reclaim_apply"](
                {
                    "schemaVersion": 1,
                    "status": "completed",
                    "actions": ["not-an-object"],
                    "failures": [],
                    "receipt": "/fake",
                }
            )


if __name__ == "__main__":
    unittest.main()

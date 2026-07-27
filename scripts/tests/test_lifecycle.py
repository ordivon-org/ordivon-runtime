from __future__ import annotations

import json
import os
import sqlite3
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


REPO = Path(__file__).resolve().parents[2]


def initialize_registry(database: Path, workspace_id: str | None = None, activity_ms: int = 0) -> None:
    with sqlite3.connect(database) as connection:
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
    def test_inspect_applies_retention_class_and_last_activity(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            runtime = root / "runtime"
            records = runtime / "workspace-records"
            workspaces = runtime / "workspaces"
            records.mkdir(parents=True)
            workspaces.mkdir()
            database = root / "registry.sqlite3"
            initialize_registry(database, "ws-example", 2_000)
            workspace = workspaces / "ws-example"
            revision = init_repository(workspace)
            (records / "ws-example.json").write_text(
                json.dumps(
                    {
                        "schemaVersion": 1,
                        "workspaceId": "ws-example",
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
            workspace_id = "broken"
            workspace = workspaces / workspace_id
            workspace.mkdir()
            (workspace / "important.txt").write_text("preserve me\n", encoding="utf-8")
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


if __name__ == "__main__":
    unittest.main()

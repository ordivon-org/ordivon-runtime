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
from types import SimpleNamespace
from unittest import mock


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


    def test_windows_input_bindings_digest_matches_provider_contract_v2(self) -> None:
        digest = self.module["windows_input_bindings_digest"](
            [
                {
                    "access": "read_only",
                    "authority": "finance-research-materializations",
                    "byteLength": 6086,
                    "digest": "sha256:23c39b521f68959018a12411a63af9ec9dab1434a3d7558efbd19110c9597386",
                    "presentationRelativePath": "probe/manifest.json",
                    "relativeObject": "ignored-by-provider-tree-digest",
                }
            ]
        )
        self.assertEqual(
            digest,
            "sha256:50000df99583f188fc5704f68331274e5f9c05f0e649fb8bacdaf5d38ce24c88",
        )

    def test_inputs_inspect_protects_nonterminal_and_selects_terminal_physical_state(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            database = root / "registry.sqlite3"
            now_ms = 100_000_000
            input_digest = "sha256:23c39b521f68959018a12411a63af9ec9dab1434a3d7558efbd19110c9597386"
            def plan(input_set_id: str) -> str:
                return json.dumps(
                    {
                        "executionTarget": "windows_native",
                        "inputSetId": input_set_id,
                        "effectiveInputs": [
                            {
                                "access": "read_only",
                                "authority": "authority",
                                "byteLength": 6086,
                                "digest": input_digest,
                                "presentationRelativePath": "probe/manifest.json",
                                "relativeObject": "object",
                            }
                        ],
                        "env": {
                            "ProgramData": r"C:\ProgramData",
                            "ORDIVON_INPUT_ROOT": rf"C:\ProgramData\OrdivonImmutableInputs\{input_set_id}",
                        },
                    }
                )
            with closing(sqlite3.connect(database)) as connection:
                connection.executescript(
                    """
                    CREATE TABLE jobs(
                      job_id TEXT PRIMARY KEY,
                      resolution TEXT,
                      execution_plan_json TEXT,
                      current_attempt_id TEXT,
                      created_at_ms INTEGER
                    );
                    CREATE TABLE attempts(
                      attempt_id TEXT PRIMARY KEY,
                      job_id TEXT,
                      attempt_number INTEGER,
                      state TEXT,
                      finished_at_ms INTEGER,
                      bundle_path TEXT
                    );
                    CREATE TABLE concurrency_reservations(attempt_id TEXT,state TEXT);
                    CREATE TABLE artifacts(
                      artifact_id TEXT,
                      attempt_id TEXT,
                      kind TEXT,
                      relative_path TEXT,
                      digest TEXT
                    );
                    """
                )
                terminal_id = "a" * 64
                running_id = "b" * 64
                connection.execute(
                    "INSERT INTO jobs VALUES (?,?,?,?,?)",
                    ("job-terminal", "succeeded", plan(terminal_id), None, 1),
                )
                connection.execute(
                    "INSERT INTO attempts VALUES (?,?,?,?,?,?)",
                    ("attempt-terminal", "job-terminal", 1, "succeeded", 1, str(root / "terminal-bundle")),
                )
                connection.execute(
                    "INSERT INTO jobs VALUES (?,?,?,?,?)",
                    ("job-running", None, plan(running_id), "attempt-running", 1),
                )
                connection.execute(
                    "INSERT INTO attempts VALUES (?,?,?,?,?,?)",
                    ("attempt-running", "job-running", 1, "running", None, str(root / "running-bundle")),
                )
                connection.commit()
            args = SimpleNamespace(
                database=database,
                env_file=root / "runtime.env",
                busy_timeout_ms=5_000,
                minimum_terminal_age_hours=0.0,
                job_ids=None,
            )
            function_globals = self.module["inspect_input_presentations"].__globals__
            with mock.patch.dict(
                function_globals,
                {
                    "discover_windows_input_presentations": lambda _env: (
                        root / "launcher",
                        r"C:\ProgramData",
                        root / "native",
                        {"a" * 64, "b" * 64},
                        set(),
                    ),
                    "time": SimpleNamespace(time=lambda: now_ms / 1000),
                },
            ):
                report = self.module["inspect_input_presentations"](args)
            by_job = {item["jobId"]: item for item in report["candidates"]}
            self.assertEqual(by_job["job-terminal"]["classification"], "terminal_reclaimable")
            self.assertTrue(by_job["job-terminal"]["policyEligible"])
            self.assertEqual(by_job["job-running"]["classification"], "protected_nonterminal")
            self.assertFalse(by_job["job-running"]["policyEligible"])

    def test_inputs_sweep_blocks_provider_transition_before_apply(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            input_set_id = "c" * 64
            bindings_digest = "sha256:" + "a" * 64
            presentation = "C:\\ProgramData\\OrdivonImmutableInputs\\" + input_set_id
            item = {
                "jobId": "job-terminal",
                "attemptId": "attempt-terminal",
                "inputSetId": input_set_id,
                "inputBindingsDigest": bindings_digest,
                "inputPresentationRoot": presentation,
                "policyEligible": True,
            }
            before = {
                "schemaVersion": 1,
                "status": "completed",
                "windowsLauncher": str(root / "launcher-old.exe"),
                "programData": r"C:\ProgramData",
                "candidates": [dict(item)],
            }
            transitioned = {
                "schemaVersion": 1,
                "status": "completed",
                "windowsLauncher": str(root / "launcher-new.exe"),
                "programData": r"C:\ProgramData",
                "candidates": [dict(item)],
            }
            reports = iter([before, transitioned, transitioned])
            reclaim_calls: list[tuple[str, str]] = []
            args = SimpleNamespace(
                confirm_policy="RECLAIM_TERMINAL_WINDOWS_INPUT_PRESENTATIONS",
                lock_file=root / "lifecycle.lock",
                receipt_root=root / "receipts",
                job_ids=None,
            )
            function_globals = self.module["sweep_input_presentations"].__globals__
            with mock.patch.dict(
                function_globals,
                {
                    "inspect_input_presentations": lambda _args: next(reports),
                    "invoke_input_presentation_reclaim": lambda _launcher, sid, digest: reclaim_calls.append(
                        (sid, digest)
                    ),
                },
            ):
                result = self.module["sweep_input_presentations"](args)
            self.assertEqual(result["status"], "partial")
            self.assertEqual(reclaim_calls, [])
            self.assertEqual(result["failures"][0]["jobId"], "job-terminal")
            self.assertIn("provider eligibility", result["failures"][0]["error"])

    def test_inputs_sweep_rechecks_eligibility_and_receipts_launcher_action(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            input_set_id = "d" * 64
            bindings_digest = "sha256:" + "e" * 64
            item = {
                "jobId": "job-terminal",
                "attemptId": "attempt-terminal",
                "inputSetId": input_set_id,
                "inputBindingsDigest": bindings_digest,
                "policyEligible": True,
            }
            before = {
                "schemaVersion": 1,
                "status": "completed",
                "windowsLauncher": str(root / "launcher"),
                "candidates": [dict(item)],
            }
            after = {
                "schemaVersion": 1,
                "status": "completed",
                "windowsLauncher": str(root / "launcher"),
                "candidates": [{**item, "policyEligible": False}],
            }
            reports = iter([before, before, after])
            calls: list[tuple[str, str]] = []
            def fake_inspect(_args):
                return next(reports)
            def fake_reclaim(_launcher, observed_input_set_id, observed_digest):
                calls.append((observed_input_set_id, observed_digest))
                return {
                    "schemaVersion": 1,
                    "disposition": "deleted",
                    "inputSetId": observed_input_set_id,
                    "inputBindingsDigest": observed_digest,
                }
            args = SimpleNamespace(
                confirm_policy="RECLAIM_TERMINAL_WINDOWS_INPUT_PRESENTATIONS",
                lock_file=root / "lifecycle.lock",
                receipt_root=root / "receipts",
                job_ids=None,
            )
            function_globals = self.module["sweep_input_presentations"].__globals__
            with mock.patch.dict(
                function_globals,
                {
                    "inspect_input_presentations": fake_inspect,
                    "invoke_input_presentation_reclaim": fake_reclaim,
                },
            ):
                result = self.module["sweep_input_presentations"](args)
            self.assertEqual(result["status"], "completed")
            self.assertEqual(calls, [(input_set_id, bindings_digest)])
            self.assertEqual(result["actions"][0]["disposition"], "deleted")
            receipt = Path(result["receipt"])
            self.assertTrue((receipt / "before.json").is_file())
            self.assertTrue((receipt / "after.json").is_file())
            self.assertEqual(json.loads((receipt / "result.json").read_text())["actions"], result["actions"])


if __name__ == "__main__":
    unittest.main()

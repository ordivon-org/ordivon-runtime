from __future__ import annotations

import json
import os
import sqlite3
import subprocess
import sys
import tempfile
import unittest
from contextlib import closing
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]


def initialize_registry(database: Path) -> None:
    with closing(sqlite3.connect(database)) as connection:
        connection.executescript(
            """
            CREATE TABLE schema_migrations(version INTEGER);
            INSERT INTO schema_migrations VALUES (4);
            CREATE TABLE jobs(job_id TEXT,workspace_id TEXT,resolution TEXT,created_at_ms INTEGER);
            CREATE TABLE attempts(
              attempt_id TEXT,job_id TEXT,state TEXT,created_at_ms INTEGER,
              started_at_ms INTEGER,finished_at_ms INTEGER
            );
            CREATE TABLE concurrency_reservations(attempt_id TEXT,state TEXT);
            """
        )
        connection.commit()


def init_repository(path: Path) -> str:
    subprocess.run(["git", "init", "-q", str(path)], check=True)
    subprocess.run(["git", "-C", str(path), "config", "user.name", "Test"], check=True)
    subprocess.run(["git", "-C", str(path), "config", "user.email", "test@example.invalid"], check=True)
    (path / "README.md").write_text("base\n", encoding="utf-8")
    subprocess.run(["git", "-C", str(path), "add", "README.md"], check=True)
    subprocess.run(["git", "-C", str(path), "commit", "-q", "-m", "base"], check=True)
    return subprocess.run(
        ["git", "-C", str(path), "rev-parse", "HEAD"],
        check=True,
        text=True,
        capture_output=True,
    ).stdout.strip()


def write_record(runtime: Path, workspace_id: str, workspace: Path, revision: str) -> None:
    record = runtime / "workspace-records" / f"{workspace_id}.json"
    record.write_text(
        json.dumps(
            {
                "schemaVersion": 1,
                "workspaceId": workspace_id,
                "sourceRepo": str(workspace),
                "sourceRevision": revision,
                "workspacePath": str(workspace),
                "createdUnixMs": 1,
            }
        ),
        encoding="utf-8",
    )


def checkpoint_environment(root: Path, workspace_id: str) -> tuple[dict[str, str], list[str]]:
    fake_reclaim = root / "fake-reclaim"
    fake_reclaim.write_text(
        "#!/usr/bin/env python3\n"
        "import json\n"
        f"print(json.dumps({{'schemaVersion':1,'summary':{{'counts':{{'blocked_dirty':1}}}},"
        f"'candidates':[{{'workspaceId':'{workspace_id}','classification':'blocked_dirty','createdUnixMs':1}}]}}))\n",
        encoding="utf-8",
    )
    fake_reclaim.chmod(0o755)
    environment = os.environ.copy()
    environment["ORDIVON_RUNTIME_RECLAIM"] = str(fake_reclaim)
    command = [
        sys.executable,
        "scripts/ordivon-runtime-lifecycle",
        "dirty-checkpoint",
        "--database",
        str(root / "registry.sqlite3"),
        "--runtime-store-root",
        str(root / "runtime"),
        "--receipt-root",
        str(root / "receipts"),
        "--lock-file",
        str(root / "lifecycle.lock"),
        "--workspace-id",
        workspace_id,
        "--confirm-policy",
        "CHECKPOINT_DIRTY_WORKSPACE_STATE",
    ]
    return environment, command


class DirtyCheckpointTests(unittest.TestCase):
    def test_preserves_staged_unstaged_and_untracked_layers_and_replays(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            runtime = root / "runtime"
            workspace_id = "dirty-checkpoint-layers"
            workspace = runtime / "workspaces" / workspace_id
            (runtime / "workspace-records").mkdir(parents=True)
            workspace.parent.mkdir(parents=True)
            init_repository(workspace)
            (workspace / "tracked.txt").write_text("base tracked\n", encoding="utf-8")
            subprocess.run(["git", "-C", str(workspace), "add", "tracked.txt"], check=True)
            subprocess.run(["git", "-C", str(workspace), "commit", "-q", "-m", "tracked"], check=True)
            revision = subprocess.run(
                ["git", "-C", str(workspace), "rev-parse", "HEAD"], check=True, text=True, capture_output=True
            ).stdout.strip()
            (workspace / "README.md").write_text("staged change\n", encoding="utf-8")
            subprocess.run(["git", "-C", str(workspace), "add", "README.md"], check=True)
            (workspace / "tracked.txt").write_text("unstaged change\n", encoding="utf-8")
            (workspace / "new.txt").write_text("untracked content\n", encoding="utf-8")
            write_record(runtime, workspace_id, workspace, revision)
            initialize_registry(root / "registry.sqlite3")
            environment, command = checkpoint_environment(root, workspace_id)
            before = subprocess.run(
                ["git", "-C", str(workspace), "status", "--porcelain=v1", "--untracked-files=all"],
                check=True,
                text=True,
                capture_output=True,
            ).stdout

            first = subprocess.run(command, cwd=REPO, env=environment, text=True, capture_output=True)
            self.assertEqual(first.returncode, 0, first.stderr)
            action = json.loads(first.stdout)["actions"][0]
            self.assertEqual(action["checkpointDisposition"], "created")
            self.assertFalse(action["automaticDeletionAllowed"])
            self.assertFalse(action["hasUnmergedIndex"])
            self.assertIsNotNone(action["indexTree"])
            ref = action["checkpointRef"]
            self.assertEqual(
                subprocess.run(
                    ["git", "-C", str(workspace), "show", f"{ref}:index/README.md"],
                    check=True,
                    text=True,
                    capture_output=True,
                ).stdout,
                "staged change\n",
            )
            self.assertEqual(
                subprocess.run(
                    ["git", "-C", str(workspace), "show", f"{ref}:index/tracked.txt"],
                    check=True,
                    text=True,
                    capture_output=True,
                ).stdout,
                "base tracked\n",
            )
            self.assertEqual(
                subprocess.run(
                    ["git", "-C", str(workspace), "show", f"{ref}:worktree/tracked.txt"],
                    check=True,
                    text=True,
                    capture_output=True,
                ).stdout,
                "unstaged change\n",
            )
            self.assertEqual(
                subprocess.run(
                    ["git", "-C", str(workspace), "show", f"{ref}:worktree/new.txt"],
                    check=True,
                    text=True,
                    capture_output=True,
                ).stdout,
                "untracked content\n",
            )
            self.assertNotEqual(
                subprocess.run(
                    ["git", "-C", str(workspace), "cat-file", "-e", f"{ref}:index/new.txt"],
                    capture_output=True,
                ).returncode,
                0,
            )
            after = subprocess.run(
                ["git", "-C", str(workspace), "status", "--porcelain=v1", "--untracked-files=all"],
                check=True,
                text=True,
                capture_output=True,
            ).stdout
            self.assertEqual(after, before)

            reconstructed = root / "reconstructed"
            subprocess.run(
                ["git", "-C", str(workspace), "worktree", "add", "--detach", str(reconstructed), revision],
                check=True, capture_output=True, text=True,
            )
            try:
                subprocess.run(
                    ["git", "-C", str(reconstructed), "read-tree", "--reset", "-u", action["worktreeTree"]],
                    check=True, capture_output=True, text=True,
                )
                index_path_raw = subprocess.run(
                    ["git", "-C", str(reconstructed), "rev-parse", "--git-path", "index"],
                    check=True, text=True, capture_output=True,
                ).stdout.strip()
                index_path = Path(index_path_raw)
                if not index_path.is_absolute():
                    index_path = (reconstructed / index_path).resolve()
                raw_index = subprocess.run(
                    ["git", "-C", str(workspace), "show", f"{ref}:index.raw"],
                    check=True, capture_output=True,
                ).stdout
                index_path.write_bytes(raw_index)
                reconstructed_status = subprocess.run(
                    ["git", "-C", str(reconstructed), "status", "--porcelain=v1", "--untracked-files=all"],
                    check=True, text=True, capture_output=True,
                ).stdout
                self.assertEqual(reconstructed_status, before)
            finally:
                subprocess.run(
                    ["git", "-C", str(workspace), "worktree", "remove", "--force", str(reconstructed)],
                    check=True, capture_output=True, text=True,
                )

            replay = subprocess.run(command, cwd=REPO, env=environment, text=True, capture_output=True)
            self.assertEqual(replay.returncode, 0, replay.stderr)
            replay_action = json.loads(replay.stdout)["actions"][0]
            self.assertEqual(replay_action["checkpointDisposition"], "existing")
            self.assertEqual(replay_action["stateDigest"], action["stateDigest"])
            self.assertEqual(replay_action["checkpointRef"], ref)
            self.assertEqual(replay_action["checkpointCommit"], action["checkpointCommit"])

    def test_rejects_split_index_until_shared_index_recovery_is_supported(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            runtime = root / "runtime"
            workspace_id = "dirty-checkpoint-split-index"
            workspace = runtime / "workspaces" / workspace_id
            (runtime / "workspace-records").mkdir(parents=True)
            workspace.parent.mkdir(parents=True)
            revision = init_repository(workspace)
            subprocess.run(["git", "-C", str(workspace), "update-index", "--split-index"], check=True)
            shared = subprocess.run(
                ["git", "-C", str(workspace), "rev-parse", "--shared-index-path"],
                check=True, text=True, capture_output=True,
            ).stdout.strip()
            self.assertTrue(shared)
            (workspace / "README.md").write_text("dirty split index\n", encoding="utf-8")
            write_record(runtime, workspace_id, workspace, revision)
            initialize_registry(root / "registry.sqlite3")
            environment, command = checkpoint_environment(root, workspace_id)
            result = subprocess.run(command, cwd=REPO, env=environment, text=True, capture_output=True)
            self.assertEqual(result.returncode, 2, result.stderr)
            report = json.loads(result.stdout)
            self.assertEqual(report["actions"], [])
            self.assertIn("split-index", report["failures"][0]["error"])


    def test_rejects_in_progress_merge_instead_of_overclaiming_recoverability(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            runtime = root / "runtime"
            workspace_id = "dirty-checkpoint-conflict"
            workspace = runtime / "workspaces" / workspace_id
            (runtime / "workspace-records").mkdir(parents=True)
            workspace.parent.mkdir(parents=True)
            init_repository(workspace)
            (workspace / "conflict.txt").write_text("base\n", encoding="utf-8")
            subprocess.run(["git", "-C", str(workspace), "add", "conflict.txt"], check=True)
            subprocess.run(["git", "-C", str(workspace), "commit", "-q", "-m", "conflict base"], check=True)
            base_branch = subprocess.run(
                ["git", "-C", str(workspace), "branch", "--show-current"], check=True, text=True, capture_output=True
            ).stdout.strip()
            subprocess.run(["git", "-C", str(workspace), "checkout", "-q", "-b", "other"], check=True)
            (workspace / "conflict.txt").write_text("other\n", encoding="utf-8")
            subprocess.run(["git", "-C", str(workspace), "commit", "-qam", "other"], check=True)
            subprocess.run(["git", "-C", str(workspace), "checkout", "-q", base_branch], check=True)
            (workspace / "conflict.txt").write_text("main\n", encoding="utf-8")
            subprocess.run(["git", "-C", str(workspace), "commit", "-qam", "main"], check=True)
            merge = subprocess.run(["git", "-C", str(workspace), "merge", "other"], text=True, capture_output=True)
            self.assertNotEqual(merge.returncode, 0)
            revision = subprocess.run(
                ["git", "-C", str(workspace), "rev-parse", "HEAD"], check=True, text=True, capture_output=True
            ).stdout.strip()
            write_record(runtime, workspace_id, workspace, revision)
            initialize_registry(root / "registry.sqlite3")
            environment, command = checkpoint_environment(root, workspace_id)
            before = subprocess.run(
                ["git", "-C", str(workspace), "status", "--porcelain=v1"], check=True, text=True, capture_output=True
            ).stdout

            result = subprocess.run(command, cwd=REPO, env=environment, text=True, capture_output=True)
            self.assertEqual(result.returncode, 2, result.stderr)
            report = json.loads(result.stdout)
            self.assertEqual(report["status"], "partial")
            self.assertEqual(report["actions"], [])
            self.assertIn("in-progress Git operations", report["failures"][0]["error"])
            self.assertEqual(
                subprocess.run(
                    ["git", "-C", str(workspace), "status", "--porcelain=v1"], check=True, text=True, capture_output=True
                ).stdout,
                before,
            )
            refs = subprocess.run(
                ["git", "-C", str(workspace), "for-each-ref", "--format=%(refname)", f"refs/ordivon/checkpoints/{workspace_id}/"],
                check=True,
                text=True,
                capture_output=True,
            ).stdout
            self.assertEqual(refs, "")


if __name__ == "__main__":
    unittest.main()

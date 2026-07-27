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
SCRIPT = REPO / "scripts/ordivon-runtime-status"
COMMIT = "a" * 40


def digest(path: Path) -> str:
    return "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()


def executable(path: Path, text: str) -> None:
    path.write_text(text, encoding="utf-8")
    path.chmod(0o755)


def fixture(root: Path) -> dict[str, Path]:
    database = root / "registry.sqlite3"
    with sqlite3.connect(database) as connection:
        connection.executescript(
            """
            CREATE TABLE schema_migrations(version INTEGER);
            INSERT INTO schema_migrations VALUES (4);
            CREATE TABLE jobs(job_id TEXT PRIMARY KEY, workspace_id TEXT, resolution TEXT);
            CREATE TABLE attempts(attempt_id TEXT PRIMARY KEY, job_id TEXT, state TEXT);
            CREATE TABLE concurrency_reservations(attempt_id TEXT, state TEXT);
            CREATE TABLE attempt_conditions(attempt_id TEXT, condition_type TEXT, status TEXT);
            INSERT INTO jobs VALUES ('job-1','workspace-1','succeeded');
            INSERT INTO attempts VALUES ('attempt-1','job-1','succeeded');
            INSERT INTO concurrency_reservations VALUES ('attempt-1','released');
            INSERT INTO attempt_conditions VALUES ('attempt-1','recovery_required','false');
            """
        )
    store = root / "runtime"
    records = store / "workspace-records"
    workspaces = store / "workspaces"
    records.mkdir(parents=True)
    workspaces.mkdir()
    workspace = workspaces / "workspace-1"
    workspace.mkdir()
    subprocess.run(["git", "init", "-q", str(workspace)], check=True)
    subprocess.run(["git", "-C", str(workspace), "config", "user.name", "Test"], check=True)
    subprocess.run(
        ["git", "-C", str(workspace), "config", "user.email", "test@example.invalid"],
        check=True,
    )
    (workspace / "README.md").write_text("ok\n", encoding="utf-8")
    subprocess.run(["git", "-C", str(workspace), "add", "README.md"], check=True)
    subprocess.run(["git", "-C", str(workspace), "commit", "-qm", "baseline"], check=True)
    (records / "workspace-1.json").write_text(
        json.dumps(
            {
                "schemaVersion": 1,
                "workspaceId": "workspace-1",
                "sourceRepo": "/private/source",
                "sourceRevision": COMMIT,
                "workspacePath": str(workspace),
                "createdUnixMs": 1,
            }
        ),
        encoding="utf-8",
    )
    lifecycle = store / "lifecycle-receipts" / "receipt-1"
    lifecycle.mkdir(parents=True)
    (lifecycle / "result.json").write_text(
        json.dumps({"status": "completed", "actions": [], "failures": []}),
        encoding="utf-8",
    )
    install = root / "install"
    install.mkdir()
    executable(install / "runtime", "runtime\n")
    deployments = root / "deployments" / "deploy-1"
    deployments.mkdir(parents=True)
    (deployments / "result.json").write_text(
        json.dumps(
            {
                "schemaVersion": 1,
                "status": "deployed",
                "commit": COMMIT,
                "finishedAtMs": 2,
                "installed": [
                    {
                        "name": "runtime",
                        "digest": digest(install / "runtime"),
                        "bytes": 8,
                        "path": "/private/install/runtime",
                    }
                ],
            }
        ),
        encoding="utf-8",
    )
    (deployments / "plan.json").write_text(
        json.dumps(
            {"candidateManifest": {"builtAtMs": 1, "path": "/private/candidate"}}
        ),
        encoding="utf-8",
    )
    env_file = root / "runtime.env"
    env_file.write_text(
        "ORDIVON_GLOBAL_MAX_CONCURRENCY=4\n"
        "ORDIVON_RECONCILE_INTERVAL_MS=15000\n"
        "ORDIVON_BEARER_TOKEN=super-secret-token\n"
        "PRIVATE_PATH=/private/value\n",
        encoding="utf-8",
    )
    systemctl = root / "systemctl"
    executable(
        systemctl,
        "#!/usr/bin/env python3\n"
        "print('ActiveState=active')\n"
        "print('SubState=running')\n"
        "print('MainPID=123')\n"
        "print('ExecMainStartTimestamp=now')\n"
        "print('NRestarts=0')\n",
    )
    return {
        "database": database,
        "store": store,
        "deployments": deployments.parent,
        "install": install,
        "env": env_file,
        "systemctl": systemctl,
    }


def command(paths: dict[str, Path], *extra: str) -> list[str]:
    return [
        sys.executable,
        str(SCRIPT),
        "--database",
        str(paths["database"]),
        "--runtime-store-root",
        str(paths["store"]),
        "--deployment-root",
        str(paths["deployments"]),
        "--install-dir",
        str(paths["install"]),
        "--env-file",
        str(paths["env"]),
        "--systemctl",
        str(paths["systemctl"]),
        *extra,
    ]


class RuntimeStatusTests(unittest.TestCase):
    def test_healthy_json_is_secret_free_and_path_free(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            paths = fixture(Path(temporary))
            result = subprocess.run(
                command(paths, "--json", "--expected-commit", COMMIT),
                cwd=REPO,
                check=True,
                text=True,
                capture_output=True,
            )
            report = json.loads(result.stdout)
            self.assertEqual(report["status"], "healthy")
            self.assertEqual(report["deployment"]["commit"], COMMIT)
            self.assertEqual(report["config"]["globalMaxConcurrency"], 4)
            self.assertEqual(report["workspaces"]["dirty"], 0)
            self.assertEqual(report["operatorAction"]["count"], 0)
            self.assertNotIn("super-secret-token", result.stdout)
            self.assertNotIn("/private/", result.stdout)
            self.assertNotIn(str(Path(temporary)), result.stdout)

    def test_mismatched_binary_and_expected_commit_require_action(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            paths = fixture(Path(temporary))
            executable(paths["install"] / "runtime", "changed\n")
            result = subprocess.run(
                command(paths, "--json", "--expected-commit", "b" * 40),
                cwd=REPO,
                check=False,
                text=True,
                capture_output=True,
            )
            self.assertEqual(result.returncode, 1)
            report = json.loads(result.stdout)
            self.assertEqual(report["status"], "attention")
            self.assertIn(
                "EXPECTED_COMMIT_MISMATCH", report["operatorAction"]["reasons"]
            )
            self.assertIn(
                "BINARY_DIGEST_MISMATCH:runtime",
                report["operatorAction"]["reasons"],
            )

    def test_human_output_is_one_compact_line(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            paths = fixture(Path(temporary))
            result = subprocess.run(
                command(paths), cwd=REPO, check=True, text=True, capture_output=True
            )
            self.assertEqual(len(result.stdout.splitlines()), 1)
            self.assertIn("HEALTHY commit=aaaaaaaaaaaa", result.stdout)
            self.assertIn("capacity=0+0/4", result.stdout)


if __name__ == "__main__":
    unittest.main()

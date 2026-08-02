from __future__ import annotations

import hashlib
import json
import sqlite3
import subprocess
import sys
import tempfile
import time
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
    connection = sqlite3.connect(database)
    try:
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
    finally:
        connection.close()
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
    candidates = root / "candidates"
    candidates.mkdir()
    install = root / "install"
    install.mkdir()
    executable(install / "runtime", "runtime\n")
    deployment_root = root / "deployments"
    previous_deployment = deployment_root / "deploy-0"
    previous_deployment.mkdir(parents=True)
    (previous_deployment / "result.json").write_text(
        json.dumps(
            {
                "schemaVersion": 1,
                "status": "deployed",
                "commit": "0" * 40,
                "finishedAtMs": 1,
                "installed": [],
                "probe": {"protocolVersion": "2025-06-18"},
            }
        ),
        encoding="utf-8",
    )
    deployments = deployment_root / "deploy-1"
    deployments.mkdir(parents=True)
    previous_binary = deployments / "previous" / "ordivon-runtime"
    previous_binary.parent.mkdir()
    executable(previous_binary, "previous-runtime\n")
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
                "probe": {
                    "lifecycle": "modern",
                    "protocolVersion": "2026-07-28",
                    "supportedVersions": [
                        "2026-07-28",
                        "2025-11-25",
                        "2025-06-18",
                    ],
                    "serverInfo": {
                        "name": "ordivon-runtime-mcp",
                        "version": "0.1.0",
                    },
                    "toolCatalogDigest": "sha256:" + "b" * 64,
                    "toolCount": 15,
                },
            }
        ),
        encoding="utf-8",
    )
    (deployments / "plan.json").write_text(
        json.dumps({"candidateManifest": {"builtAtMs": 1, "path": "/private/candidate"}}),
        encoding="utf-8",
    )
    trace_path = root / "runtime-trace.jsonl"
    trace_path.write_text(
        "\n".join(
            [
                json.dumps(
                    {
                        "kind": "mcp_protocol_observation",
                        "observedUnixMs": 3,
                        "protocolVersion": "2025-11-25",
                        "method": "initialize",
                        "client": {"name": "openai-mcp", "version": "1.0.0"},
                    }
                ),
                json.dumps(
                    {
                        "kind": "mcp_protocol_observation",
                        "observedUnixMs": 4,
                        "protocolVersion": "2025-11-25",
                        "method": "tools/call",
                        "tool": "workspace.exec",
                        "client": {"name": "openai-mcp", "version": "1.0.0"},
                    }
                ),
            ]
        )
        + "\n",
        encoding="utf-8",
    )
    env_file = root / "runtime.env"
    env_file.write_text(
        "ORDIVON_GLOBAL_MAX_CONCURRENCY=8\n"
        "ORDIVON_MAX_RUNTIME_MS=3600000\n"
        "ORDIVON_BODY_LIMIT_BYTES=1048576\n"
        "ORDIVON_CACHE_HIGH_WATERMARK_BYTES=68719476736\n"
        "ORDIVON_CACHE_LOW_WATERMARK_BYTES=51539607552\n"
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
        "candidates": candidates,
        "install": install,
        "env": env_file,
        "systemctl": systemctl,
    }


def raw_command(paths: dict[str, Path], *extra: str) -> list[str]:
    return [
        sys.executable,
        str(SCRIPT),
        "--database",
        str(paths["database"]),
        "--runtime-store-root",
        str(paths["store"]),
        "--deployment-root",
        str(paths["deployments"]),
        "--candidate-root",
        str(paths["candidates"]),
        "--install-dir",
        str(paths["install"]),
        "--env-file",
        str(paths["env"]),
        "--systemctl",
        str(paths["systemctl"]),
        *extra,
    ]


def command(paths: dict[str, Path], *extra: str) -> list[str]:
    mode = [] if any(value in {"--health", "--diagnose"} for value in extra) else ["--diagnose"]
    return raw_command(paths, *mode, *extra)


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
            self.assertEqual(report["schemaVersion"], 3)
            self.assertEqual(report["status"], "healthy")
            self.assertEqual(report["mode"], "diagnose")
            self.assertEqual(report["health"]["status"], "healthy")
            self.assertEqual(report["maintenance"]["status"], "healthy")
            self.assertEqual(report["deployment"]["commit"], COMMIT)
            self.assertEqual(report["deployment"]["protocolLifecycle"], "modern")
            self.assertEqual(report["deployment"]["protocolVersion"], "2026-07-28")
            self.assertEqual(
                report["deployment"]["supportedProtocolVersions"],
                ["2026-07-28", "2025-11-25", "2025-06-18"],
            )
            self.assertEqual(
                report["deployment"]["toolCatalogDigest"],
                "sha256:" + "b" * 64,
            )
            self.assertEqual(report["deployment"]["toolCount"], 15)
            compatibility = report["compatibility"]
            self.assertEqual(compatibility["canonicalProtocolVersion"], "2026-07-28")
            self.assertEqual(compatibility["deletionCandidates"], [])
            decisions = {item["protocolVersion"]: item for item in compatibility["decisions"]}
            self.assertIn(
                "production-deploy-and-acceptance",
                decisions["2026-07-28"]["consumers"],
            )
            self.assertIn(
                "live-client:openai-mcp@1.0.0",
                decisions["2025-11-25"]["consumers"],
            )
            self.assertIn(
                "receipt-bound-rollback:deploy-0",
                decisions["2025-06-18"]["consumers"],
            )
            self.assertEqual(report["config"]["globalMaxConcurrency"], 8)
            self.assertEqual(report["config"]["maxRuntimeMs"], 3_600_000)
            self.assertEqual(report["config"]["bodyLimitBytes"], 1_048_576)
            self.assertEqual(report["workspaces"]["dirty"], 0)
            self.assertEqual(report["workspaces"]["staleDirty"], 0)
            self.assertIsNotNone(report["storage"]["cacheBytes"])
            self.assertIsNotNone(report["storage"]["registryBytes"])
            self.assertEqual(report["operatorAction"]["count"], 0)
            self.assertEqual(report["maintenanceAction"]["count"], 0)
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
            self.assertEqual(report["status"], "unhealthy")
            self.assertIn("EXPECTED_COMMIT_MISMATCH", report["operatorAction"]["reasons"])
            self.assertIn(
                "BINARY_DIGEST_MISMATCH:runtime",
                report["operatorAction"]["reasons"],
            )

    def test_missing_protocol_trace_blocks_compatibility_deletion_without_health_incident(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            paths = fixture(Path(temporary))
            (paths["database"].parent / "runtime-trace.jsonl").unlink()
            result = subprocess.run(
                command(paths, "--json"),
                cwd=REPO,
                check=True,
                text=True,
                capture_output=True,
            )
            report = json.loads(result.stdout)
            self.assertEqual(report["status"], "healthy")
            self.assertEqual(report["operatorAction"]["count"], 0)
            self.assertEqual(report["compatibility"]["deletionCandidates"], [])
            decisions = {item["protocolVersion"]: item for item in report["compatibility"]["decisions"]}
            self.assertIn(
                "PROTOCOL_TRACE_UNAVAILABLE",
                decisions["2025-11-25"]["blockers"],
            )
            self.assertFalse(decisions["2025-11-25"]["deletionEligible"])

    def test_recent_deployment_blocks_protocol_deletion_until_retention_window_completes(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            paths = fixture(Path(temporary))
            current = paths["deployments"] / "deploy-1" / "result.json"
            value = json.loads(current.read_text(encoding="utf-8"))
            value["finishedAtMs"] = int(time.time() * 1000)
            value["probe"]["supportedVersions"].append("2024-11-05")
            current.write_text(json.dumps(value), encoding="utf-8")
            result = subprocess.run(
                command(paths, "--json"),
                cwd=REPO,
                check=True,
                text=True,
                capture_output=True,
            )
            report = json.loads(result.stdout)
            compatibility = report["compatibility"]
            self.assertEqual(compatibility["retentionHours"], 168)
            self.assertFalse(compatibility["retentionSatisfied"])
            decisions = {item["protocolVersion"]: item for item in compatibility["decisions"]}
            self.assertIn(
                "RETENTION_WINDOW_INCOMPLETE",
                decisions["2024-11-05"]["blockers"],
            )
            self.assertFalse(decisions["2024-11-05"]["deletionEligible"])
            self.assertNotIn("2024-11-05", compatibility["deletionCandidates"])

    def test_rotated_protocol_trace_is_part_of_bounded_compatibility_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            paths = fixture(Path(temporary))
            trace = paths["database"].parent / "runtime-trace.jsonl"
            rotated = Path(str(trace) + ".1")
            trace.replace(rotated)
            trace.write_text(
                json.dumps(
                    {
                        "kind": "mcp_protocol_observation",
                        "observedUnixMs": 5,
                        "protocolVersion": "2025-11-25",
                        "method": "tools/call",
                        "tool": "workspace.list",
                        "client": {"name": "openai-mcp", "version": "1.0.0"},
                    }
                )
                + "\n",
                encoding="utf-8",
            )
            completed = subprocess.run(
                command(paths, "--json"),
                cwd=REPO,
                check=True,
                text=True,
                capture_output=True,
            )
            report = json.loads(completed.stdout)
            observations = report["compatibility"]["observations"]
            self.assertEqual(observations["segments"], 2)
            self.assertGreaterEqual(observations["events"], 3)

    def test_stale_dirty_workspace_requires_operator_attention_without_deletion(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            paths = fixture(Path(temporary))
            workspace = paths["store"] / "workspaces" / "workspace-1"
            (workspace / "README.md").write_text("dirty\n", encoding="utf-8")
            completed = subprocess.run(
                command(paths, "--json", "--stale-dirty-hours", "1"),
                cwd=REPO,
                check=False,
                text=True,
                capture_output=True,
            )
            self.assertEqual(completed.returncode, 0)
            report = json.loads(completed.stdout)
            self.assertEqual(report["status"], "attention")
            self.assertEqual(report["health"]["status"], "healthy")
            self.assertEqual(report["maintenance"]["status"], "attention")
            self.assertEqual(report["workspaces"]["staleDirty"], 1)
            self.assertIn(
                "STALE_DIRTY_WORKSPACES",
                report["maintenanceAction"]["reasons"],
            )
            self.assertTrue(workspace.is_dir())

    def test_default_mode_is_fast_health(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            paths = fixture(Path(temporary))
            completed = subprocess.run(
                raw_command(paths, "--json", "--expected-commit", COMMIT),
                cwd=REPO,
                check=True,
                text=True,
                capture_output=True,
            )
            report = json.loads(completed.stdout)
            self.assertEqual(report["mode"], "health")
            self.assertNotIn("workspaces", report)
            self.assertNotIn("storage", report)

    def test_health_mode_skips_expensive_maintenance_probes(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            paths = fixture(Path(temporary))
            workspace = paths["store"] / "workspaces" / "workspace-1"
            (workspace / "README.md").write_text("dirty\n", encoding="utf-8")
            completed = subprocess.run(
                command(paths, "--health", "--json", "--expected-commit", COMMIT),
                cwd=REPO,
                check=True,
                text=True,
                capture_output=True,
            )
            report = json.loads(completed.stdout)
            self.assertEqual(report["mode"], "health")
            self.assertEqual(report["status"], "healthy")
            self.assertEqual(report["health"]["status"], "healthy")
            self.assertEqual(report["maintenance"]["status"], "not_checked")
            self.assertNotIn("workspaces", report)
            self.assertNotIn("storage", report)
            self.assertNotIn("compatibility", report)
            self.assertNotIn("lifecycle", report)

    def test_human_output_is_one_compact_line(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            paths = fixture(Path(temporary))
            result = subprocess.run(command(paths), cwd=REPO, check=True, text=True, capture_output=True)
            self.assertEqual(len(result.stdout.splitlines()), 1)
            self.assertIn("HEALTHY mode=diagnose commit=aaaaaaaaaaaa", result.stdout)
            self.assertIn("capacity=0+0/8", result.stdout)


if __name__ == "__main__":
    unittest.main()

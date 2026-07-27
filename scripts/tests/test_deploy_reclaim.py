from __future__ import annotations

import hashlib
import json
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import threading
import unittest
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path


REPO = Path(__file__).resolve().parents[2]


@contextmanager
def mcp_server(tool_names: list[str], close_callback=None):
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, _format: str, *args) -> None:
            return

        def do_POST(self) -> None:
            length = int(self.headers.get("Content-Length", "0"))
            request = json.loads(self.rfile.read(length))
            method = request.get("method")
            if method == "initialize":
                result = {
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "serverInfo": {"name": "test-runtime", "version": "1"},
                }
            elif method == "tools/list":
                result = {"tools": [{"name": name} for name in tool_names]}
            elif method == "tools/call" and request.get("params", {}).get("name") == "workspace.close":
                arguments = request["params"]["arguments"]
                if close_callback is not None:
                    close_callback(str(arguments["workspaceId"]))
                result = {
                    "isError": False,
                    "structuredContent": {
                        "workspaceId": arguments["workspaceId"],
                        "removed": True,
                    },
                }
            else:
                self.send_response(400)
                self.end_headers()
                return
            response = json.dumps(
                {"jsonrpc": "2.0", "id": request.get("id"), "result": result}
            ).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(response)))
            self.end_headers()
            self.wfile.write(response)

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield server.server_address[1]
    finally:
        server.shutdown()
        thread.join(timeout=5)
        server.server_close()


def initialize_registry(database: Path, *, active_workspace: str | None = None) -> None:
    with sqlite3.connect(database) as connection:
        connection.executescript(
            """
            CREATE TABLE schema_migrations(version INTEGER);
            INSERT INTO schema_migrations VALUES (1);
            CREATE TABLE jobs(
              job_id TEXT,
              workspace_id TEXT,
              resolution TEXT,
              created_at_ms INTEGER
            );
            CREATE TABLE attempts(attempt_id TEXT,job_id TEXT,state TEXT);
            CREATE TABLE concurrency_reservations(attempt_id TEXT,state TEXT);
            """
        )
        if active_workspace is not None:
            connection.execute(
                "INSERT INTO jobs VALUES ('job-active', ?, NULL, 1)",
                (active_workspace,),
            )
            connection.execute(
                "INSERT INTO attempts VALUES ('attempt-active','job-active','running')"
            )
            connection.execute(
                "INSERT INTO concurrency_reservations VALUES ('attempt-active','active')"
            )
        connection.commit()


def initialize_git_repository(path: Path, *, remote: bool) -> str:
    subprocess.run(["git", "init", "-q", str(path)], check=True)
    subprocess.run(["git", "-C", str(path), "config", "user.name", "Test"], check=True)
    subprocess.run(
        ["git", "-C", str(path), "config", "user.email", "test@example.invalid"],
        check=True,
    )
    (path / "README.md").write_text("baseline\n", encoding="utf-8")
    (path / ".gitignore").write_text("target/\n", encoding="utf-8")
    subprocess.run(["git", "-C", str(path), "add", "README.md", ".gitignore"], check=True)
    subprocess.run(["git", "-C", str(path), "commit", "-qm", "baseline"], check=True)
    subprocess.run(["git", "-C", str(path), "branch", "-M", "main"], check=True)
    if remote:
        remote_path = path.parent / "remote.git"
        subprocess.run(["git", "init", "--bare", "-q", str(remote_path)], check=True)
        subprocess.run(
            ["git", "-C", str(path), "remote", "add", "origin", str(remote_path)],
            check=True,
        )
        subprocess.run(["git", "-C", str(path), "push", "-qu", "origin", "main"], check=True)
    return subprocess.run(
        ["git", "-C", str(path), "rev-parse", "HEAD"],
        check=True,
        text=True,
        capture_output=True,
    ).stdout.strip()


def write_executable(path: Path, content: str) -> None:
    path.write_text(content, encoding="utf-8")
    path.chmod(0o755)


def write_candidate_manifest(
    path: Path,
    candidate: Path,
    commit: str,
    names: tuple[str, ...],
    source_repo: Path,
) -> None:
    binaries = []
    for name in names:
        binary = candidate / name
        binaries.append(
            {
                "name": name,
                "path": str(binary),
                "bytes": binary.stat().st_size,
                "digest": "sha256:" + hashlib.sha256(binary.read_bytes()).hexdigest(),
            }
        )
    path.write_text(
        json.dumps(
            {
                "schemaVersion": 1,
                "commit": commit,
                "sourceRepo": str(source_repo.resolve()),
                "candidateDir": str(candidate.resolve()),
                "builtAtMs": 1,
                "cargo": "/test/cargo",
                "binaries": binaries,
            }
        ),
        encoding="utf-8",
    )


def fake_systemctl(root: Path) -> Path:
    state = root / "service-state"
    state.write_text("active\n", encoding="utf-8")
    systemctl = root / "systemctl"
    write_executable(
        systemctl,
        "#!/bin/sh\n"
        f"state='{state}'\n"
        "case \"$1\" in\n"
        "  is-active) cat \"$state\" ;;\n"
        "  stop) printf 'inactive\\n' > \"$state\" ;;\n"
        "  start) printf 'active\\n' > \"$state\" ;;\n"
        "  *) exit 1 ;;\n"
        "esac\n",
    )
    return systemctl


class DeployReclaimTests(unittest.TestCase):
    def test_deploy_prepare_binds_binaries_to_exact_clean_commit(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repo = root / "repo"
            commit = initialize_git_repository(repo, remote=False)
            candidate = root / "target/release"
            manifest = root / "candidate-manifest.json"
            cargo = root / "cargo"
            write_executable(
                cargo,
                "#!/bin/sh\n"
                "target=''\n"
                "while [ $# -gt 0 ]; do\n"
                "  if [ \"$1\" = --target-dir ]; then target=$2; shift 2; else shift; fi\n"
                "done\n"
                "mkdir -p \"$target/release\"\n"
                "printf 'built-from-commit\\n' > \"$target/release/runtime\"\n"
                "chmod 755 \"$target/release/runtime\"\n",
            )
            result = subprocess.run(
                [
                    sys.executable,
                    "scripts/ordivon-runtime-deploy",
                    "prepare",
                    "--source-repo",
                    str(repo),
                    "--commit",
                    commit,
                    "--candidate-dir",
                    str(candidate),
                    "--candidate-manifest",
                    str(manifest),
                    "--cargo",
                    str(cargo),
                    "--binary",
                    "runtime",
                ],
                cwd=REPO,
                check=True,
                text=True,
                capture_output=True,
            )
            report = json.loads(result.stdout)
            self.assertEqual(report["commit"], commit)
            self.assertEqual(report["binaries"][0]["name"], "runtime")
            self.assertEqual(json.loads(manifest.read_text())["commit"], commit)

    def test_deploy_prepare_rejects_build_that_changes_source(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repo = root / "repo"
            commit = initialize_git_repository(repo, remote=False)
            candidate = root / "target/release"
            manifest = root / "candidate-manifest.json"
            cargo = root / "cargo"
            write_executable(
                cargo,
                "#!/bin/sh\n"
                "target=''\n"
                "while [ $# -gt 0 ]; do\n"
                "  if [ \"$1\" = --target-dir ]; then target=$2; shift 2; else shift; fi\n"
                "done\n"
                "mkdir -p \"$target/release\"\n"
                "printf 'built\n' > \"$target/release/runtime\"\n"
                "chmod 755 \"$target/release/runtime\"\n"
                "printf 'changed\n' > README.md\n",
            )
            result = subprocess.run(
                [
                    sys.executable,
                    "scripts/ordivon-runtime-deploy",
                    "prepare",
                    "--source-repo",
                    str(repo),
                    "--commit",
                    commit,
                    "--candidate-dir",
                    str(candidate),
                    "--candidate-manifest",
                    str(manifest),
                    "--cargo",
                    str(cargo),
                    "--binary",
                    "runtime",
                ],
                cwd=REPO,
                check=False,
                text=True,
                capture_output=True,
            )
            self.assertEqual(result.returncode, 1)
            self.assertIn("build changed the source repository", result.stderr)
            self.assertFalse(manifest.exists())

    def test_reclaim_missing_workspace_with_active_job_is_blocked(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            runtime = root / "runtime"
            records = runtime / "workspace-records"
            records.mkdir(parents=True)
            database = root / "registry.sqlite3"
            initialize_registry(database, active_workspace="stale-active")
            (records / "stale-active.json").write_text(
                json.dumps(
                    {
                        "schemaVersion": 1,
                        "workspaceId": "stale-active",
                        "sourceRepo": "/source",
                        "sourceRevision": "a" * 40,
                        "workspacePath": str(runtime / "workspaces/stale-active"),
                        "createdUnixMs": 1,
                    }
                ),
                encoding="utf-8",
            )
            result = subprocess.run(
                [
                    sys.executable,
                    "scripts/ordivon-runtime-reclaim",
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
            self.assertEqual(report["candidates"][0]["classification"], "blocked_active")
            self.assertEqual(report["candidates"][0]["activeJobIds"], ["job-active"])

    def test_reclaim_symlink_record_is_unknown(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            runtime = root / "runtime"
            records = runtime / "workspace-records"
            records.mkdir(parents=True)
            (runtime / "workspaces").mkdir()
            database = root / "registry.sqlite3"
            initialize_registry(database)
            target = root / "outside.json"
            target.write_text("{}\n", encoding="utf-8")
            (records / "linked.json").symlink_to(target)
            result = subprocess.run(
                [
                    sys.executable,
                    "scripts/ordivon-runtime-reclaim",
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
            self.assertEqual(report["candidates"][0]["classification"], "unknown")
            self.assertIn("regular file", report["candidates"][0]["detail"])

    def test_reclaim_apply_receipts_stale_record_deletion(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            runtime = root / "runtime"
            records = runtime / "workspace-records"
            records.mkdir(parents=True)
            (runtime / "workspaces").mkdir()
            database = root / "registry.sqlite3"
            initialize_registry(database)
            record = records / "stale.json"
            record.write_text(
                json.dumps(
                    {
                        "schemaVersion": 1,
                        "workspaceId": "stale",
                        "sourceRepo": "/source",
                        "sourceRevision": "b" * 40,
                        "workspacePath": str(runtime / "workspaces/stale"),
                        "createdUnixMs": 1,
                    }
                ),
                encoding="utf-8",
            )
            env_file = root / "runtime.env"
            env_file.write_text(
                "ORDIVON_BIND=127.0.0.1:1\nORDIVON_BEARER_TOKEN=test\n",
                encoding="utf-8",
            )
            result = subprocess.run(
                [
                    sys.executable,
                    "scripts/ordivon-runtime-reclaim",
                    "apply",
                    "--database",
                    str(database),
                    "--runtime-store-root",
                    str(runtime),
                    "--env-file",
                    str(env_file),
                    "--receipt-root",
                    str(root / "receipts"),
                    "--lock-file",
                    str(root / "reclaim.lock"),
                    "--minimum-age-hours",
                    "0",
                    "--classification",
                    "stale_record",
                    "--workspace-id",
                    "stale",
                    "--confirm-policy",
                    "RECLAIM_ELIGIBLE_WORKSPACES",
                ],
                cwd=REPO,
                check=True,
                text=True,
                capture_output=True,
            )
            report = json.loads(result.stdout)
            self.assertEqual(report["status"], "completed")
            self.assertFalse(record.exists())
            receipt = Path(report["receipt"])
            self.assertTrue((receipt / "records/stale.json").is_file())
            self.assertEqual(report["actions"][0]["action"], "record_deleted")

    def test_reclaim_apply_uses_workspace_close_for_clean_workspace(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            runtime = root / "runtime"
            records = runtime / "workspace-records"
            workspaces = runtime / "workspaces"
            records.mkdir(parents=True)
            workspaces.mkdir()
            database = root / "registry.sqlite3"
            initialize_registry(database)
            workspace = workspaces / "closable"
            revision = initialize_git_repository(workspace, remote=False)
            record = records / "closable.json"
            record.write_text(
                json.dumps(
                    {
                        "schemaVersion": 1,
                        "workspaceId": "closable",
                        "sourceRepo": str(workspace),
                        "sourceRevision": revision,
                        "workspacePath": str(workspace),
                        "createdUnixMs": 1,
                    }
                ),
                encoding="utf-8",
            )

            def close_callback(workspace_id: str) -> None:
                self.assertEqual(workspace_id, "closable")
                shutil.rmtree(workspace)
                record.write_text(
                    json.dumps(
                        {
                            "schemaVersion": 1,
                            "workspaceId": workspace_id,
                            "state": "closed",
                        }
                    ),
                    encoding="utf-8",
                )

            with mcp_server(["workspace.close"], close_callback) as port:
                env_file = root / "runtime.env"
                env_file.write_text(
                    f"ORDIVON_BIND=127.0.0.1:{port}\nORDIVON_BEARER_TOKEN=test\n",
                    encoding="utf-8",
                )
                result = subprocess.run(
                    [
                        sys.executable,
                        "scripts/ordivon-runtime-reclaim",
                        "apply",
                        "--database",
                        str(database),
                        "--runtime-store-root",
                        str(runtime),
                        "--env-file",
                        str(env_file),
                        "--receipt-root",
                        str(root / "receipts"),
                        "--lock-file",
                        str(root / "reclaim.lock"),
                        "--minimum-age-hours",
                        "0",
                        "--classification",
                        "closable",
                        "--workspace-id",
                        "closable",
                        "--confirm-policy",
                        "RECLAIM_ELIGIBLE_WORKSPACES",
                    ],
                    cwd=REPO,
                    check=True,
                    text=True,
                    capture_output=True,
                )
            report = json.loads(result.stdout)
            self.assertEqual(report["actions"][0]["action"], "workspace_closed")
            self.assertFalse(workspace.exists())

    def test_deploy_plan_rejects_candidate_manifest_source_mismatch(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repo = root / "repo"
            other_repo = root / "other-repo"
            commit = initialize_git_repository(repo, remote=True)
            initialize_git_repository(other_repo, remote=False)
            candidate = root / "candidate"
            install = root / "install"
            candidate.mkdir()
            install.mkdir()
            write_executable(candidate / "runtime", "candidate\n")
            write_executable(install / "runtime", "old\n")
            manifest = root / "candidate-manifest.json"
            write_candidate_manifest(manifest, candidate, commit, ("runtime",), other_repo)
            database = root / "registry.sqlite3"
            initialize_registry(database)
            env_file = root / "runtime.env"
            env_file.write_text(
                "ORDIVON_BIND=127.0.0.1:1\nORDIVON_BEARER_TOKEN=test\n",
                encoding="utf-8",
            )
            result = subprocess.run(
                [
                    sys.executable,
                    "scripts/ordivon-runtime-deploy",
                    "plan",
                    "--source-repo",
                    str(repo),
                    "--commit",
                    commit,
                    "--candidate-dir",
                    str(candidate),
                    "--candidate-manifest",
                    str(manifest),
                    "--install-dir",
                    str(install),
                    "--database",
                    str(database),
                    "--env-file",
                    str(env_file),
                    "--receipt-root",
                    str(root / "receipts"),
                    "--git",
                    shutil.which("git") or "/usr/bin/git",
                    "--binary",
                    "runtime",
                ],
                cwd=REPO,
                check=False,
                text=True,
                capture_output=True,
            )
            self.assertEqual(result.returncode, 2)
            plan = json.loads(result.stdout)
            self.assertIn(
                "candidate manifest source repository does not match source-repo",
                plan["blockers"],
            )

    def test_deploy_plan_rejects_candidate_manifest_digest_mismatch(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repo = root / "repo"
            commit = initialize_git_repository(repo, remote=True)
            candidate = root / "external-candidate"
            install = root / "install"
            candidate.mkdir()
            install.mkdir()
            write_executable(candidate / "runtime", "manifest-version\n")
            write_executable(install / "runtime", "old\n")
            manifest = root / "candidate-manifest.json"
            write_candidate_manifest(manifest, candidate, commit, ("runtime",), repo)
            write_executable(candidate / "runtime", "changed-after-manifest\n")
            database = root / "registry.sqlite3"
            initialize_registry(database)
            env_file = root / "runtime.env"
            env_file.write_text(
                "ORDIVON_BIND=127.0.0.1:1\nORDIVON_BEARER_TOKEN=test\n",
                encoding="utf-8",
            )
            result = subprocess.run(
                [
                    sys.executable,
                    "scripts/ordivon-runtime-deploy",
                    "plan",
                    "--source-repo",
                    str(repo),
                    "--commit",
                    commit,
                    "--candidate-dir",
                    str(candidate),
                    "--candidate-manifest",
                    str(manifest),
                    "--install-dir",
                    str(install),
                    "--database",
                    str(database),
                    "--env-file",
                    str(env_file),
                    "--receipt-root",
                    str(root / "receipts"),
                    "--git",
                    shutil.which("git") or "/usr/bin/git",
                    "--binary",
                    "runtime",
                ],
                cwd=REPO,
                check=False,
                text=True,
                capture_output=True,
            )
            self.assertEqual(result.returncode, 2)
            plan = json.loads(result.stdout)
            self.assertFalse(plan["eligible"])
            self.assertIn(
                "candidate binary does not match manifest: runtime",
                plan["blockers"],
            )

    def test_deploy_apply_and_explicit_rollback_are_receipted(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repo = root / "repo"
            commit = initialize_git_repository(repo, remote=True)
            candidate = repo / "target" / "release"
            install = root / "install"
            candidate.mkdir(parents=True)
            install.mkdir()
            for name in ("runtime", "runner"):
                write_executable(candidate / name, f"new-{name}\n")
                write_executable(install / name, f"old-{name}\n")
            manifest = root / "candidate-manifest.json"
            write_candidate_manifest(manifest, candidate, commit, ("runtime", "runner"), repo)
            database = root / "registry.sqlite3"
            initialize_registry(database)
            systemctl = fake_systemctl(root)
            with mcp_server(["workspace.get", "workspace.execPlan"]) as port:
                env_file = root / "runtime.env"
                env_file.write_text(
                    f"ORDIVON_BIND=127.0.0.1:{port}\nORDIVON_BEARER_TOKEN=test\n",
                    encoding="utf-8",
                )
                command = [
                    sys.executable,
                    "scripts/ordivon-runtime-deploy",
                    "apply",
                    "--source-repo",
                    str(repo),
                    "--commit",
                    commit,
                    "--confirm-commit",
                    commit,
                    "--candidate-dir",
                    str(candidate),
                    "--candidate-manifest",
                    str(manifest),
                    "--install-dir",
                    str(install),
                    "--database",
                    str(database),
                    "--env-file",
                    str(env_file),
                    "--receipt-root",
                    str(root / "receipts"),
                    "--systemctl",
                    str(systemctl),
                    "--git",
                    shutil.which("git") or "/usr/bin/git",
                    "--lock-file",
                    str(root / "deploy.lock"),
                    "--binary",
                    "runtime",
                    "--binary",
                    "runner",
                    "--required-tool",
                    "workspace.get",
                    "--required-tool",
                    "workspace.execPlan",
                    "--expected-tool-count",
                    "2",
                    "--wait-seconds",
                    "2",
                ]
                deployed = subprocess.run(
                    command,
                    cwd=REPO,
                    check=True,
                    text=True,
                    capture_output=True,
                )
                deployment = json.loads(deployed.stdout)
                self.assertEqual(deployment["probe"]["protocolVersion"], "2025-06-18")
                self.assertEqual(deployment["probe"]["toolCount"], 2)
                self.assertEqual((install / "runtime").read_text(), "new-runtime\n")
                self.assertEqual((install / "runtime.previous").read_text(), "old-runtime\n")
                receipt = Path(deployment["receipt"])
                rolled_back = subprocess.run(
                    [
                        sys.executable,
                        "scripts/ordivon-runtime-deploy",
                        "rollback",
                        "--receipt",
                        str(receipt),
                        "--confirm-receipt",
                        str(receipt),
                        "--install-dir",
                        str(install),
                        "--env-file",
                        str(env_file),
                        "--systemctl",
                        str(systemctl),
                        "--lock-file",
                        str(root / "deploy.lock"),
                        "--wait-seconds",
                        "2",
                    ],
                    cwd=REPO,
                    check=True,
                    text=True,
                    capture_output=True,
                )
            rollback = json.loads(rolled_back.stdout)
            self.assertEqual(rollback["status"], "restored_previous")
            self.assertEqual((install / "runtime").read_text(), "old-runtime\n")
            self.assertTrue(Path(rollback["currentBackup"]).is_dir())

    def test_explicit_rollback_rejects_tampered_previous_binary(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repo = root / "repo"
            commit = initialize_git_repository(repo, remote=True)
            candidate = repo / "target" / "release"
            install = root / "install"
            candidate.mkdir(parents=True)
            install.mkdir()
            write_executable(candidate / "runtime", "new\n")
            write_executable(install / "runtime", "old\n")
            manifest = root / "candidate-manifest.json"
            write_candidate_manifest(manifest, candidate, commit, ("runtime",), repo)
            database = root / "registry.sqlite3"
            initialize_registry(database)
            systemctl = fake_systemctl(root)
            with mcp_server(["workspace.get"]) as port:
                env_file = root / "runtime.env"
                env_file.write_text(
                    f"ORDIVON_BIND=127.0.0.1:{port}\nORDIVON_BEARER_TOKEN=test\n",
                    encoding="utf-8",
                )
                deployed = subprocess.run(
                    [
                        sys.executable,
                        "scripts/ordivon-runtime-deploy",
                        "apply",
                        "--source-repo",
                        str(repo),
                        "--commit",
                        commit,
                        "--confirm-commit",
                        commit,
                        "--candidate-dir",
                        str(candidate),
                        "--candidate-manifest",
                        str(manifest),
                        "--install-dir",
                        str(install),
                        "--database",
                        str(database),
                        "--env-file",
                        str(env_file),
                        "--receipt-root",
                        str(root / "receipts"),
                        "--systemctl",
                        str(systemctl),
                        "--git",
                        shutil.which("git") or "/usr/bin/git",
                        "--lock-file",
                        str(root / "deploy.lock"),
                        "--binary",
                        "runtime",
                        "--required-tool",
                        "workspace.get",
                        "--expected-tool-count",
                        "1",
                        "--wait-seconds",
                        "2",
                    ],
                    cwd=REPO,
                    check=True,
                    text=True,
                    capture_output=True,
                )
                receipt = Path(json.loads(deployed.stdout)["receipt"])
                write_executable(receipt / "previous" / "runtime", "tampered\n")
                rollback = subprocess.run(
                    [
                        sys.executable,
                        "scripts/ordivon-runtime-deploy",
                        "rollback",
                        "--receipt",
                        str(receipt),
                        "--confirm-receipt",
                        str(receipt),
                        "--install-dir",
                        str(install),
                        "--env-file",
                        str(env_file),
                        "--systemctl",
                        str(systemctl),
                        "--lock-file",
                        str(root / "deploy.lock"),
                        "--wait-seconds",
                        "2",
                    ],
                    cwd=REPO,
                    check=False,
                    text=True,
                    capture_output=True,
                )
            self.assertEqual(rollback.returncode, 1)
            self.assertIn("previous binary digest does not match receipt", rollback.stderr)
            self.assertEqual((install / "runtime").read_text(), "new\n")

    def test_deploy_post_stop_job_race_restarts_original_without_commit(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repo = root / "repo"
            commit = initialize_git_repository(repo, remote=True)
            candidate = repo / "target" / "release"
            install = root / "install"
            candidate.mkdir(parents=True)
            install.mkdir()
            write_executable(candidate / "runtime", "new\n")
            write_executable(install / "runtime", "old\n")
            manifest = root / "candidate-manifest.json"
            write_candidate_manifest(manifest, candidate, commit, ("runtime",), repo)
            database = root / "registry.sqlite3"
            initialize_registry(database)
            state = root / "service-state"
            state.write_text("active\n", encoding="utf-8")
            systemctl = root / "systemctl"
            write_executable(
                systemctl,
                "#!/usr/bin/env python3\n"
                "import sqlite3, sys\n"
                "from pathlib import Path\n"
                f"state = Path({str(state)!r})\n"
                f"database = {str(database)!r}\n"
                "command = sys.argv[1]\n"
                "if command == 'is-active':\n"
                "    print(state.read_text().strip())\n"
                "elif command == 'stop':\n"
                "    state.write_text('inactive\\n')\n"
                "    with sqlite3.connect(database) as connection:\n"
                "        connection.execute(\"INSERT INTO jobs VALUES ('job-race','other',NULL,2)\")\n"
                "        connection.execute(\"INSERT INTO attempts VALUES ('attempt-race','job-race','running')\")\n"
                "        connection.execute(\"INSERT INTO concurrency_reservations VALUES ('attempt-race','active')\")\n"
                "        connection.commit()\n"
                "elif command == 'start':\n"
                "    state.write_text('active\\n')\n"
                "else:\n"
                "    raise SystemExit(1)\n",
            )
            with mcp_server(["workspace.get"]) as port:
                env_file = root / "runtime.env"
                env_file.write_text(
                    f"ORDIVON_BIND=127.0.0.1:{port}\nORDIVON_BEARER_TOKEN=test\n",
                    encoding="utf-8",
                )
                result = subprocess.run(
                    [
                        sys.executable,
                        "scripts/ordivon-runtime-deploy",
                        "apply",
                        "--source-repo",
                        str(repo),
                        "--commit",
                        commit,
                        "--confirm-commit",
                        commit,
                        "--candidate-dir",
                        str(candidate),
                        "--candidate-manifest",
                        str(manifest),
                        "--install-dir",
                        str(install),
                        "--database",
                        str(database),
                        "--env-file",
                        str(env_file),
                        "--receipt-root",
                        str(root / "receipts"),
                        "--systemctl",
                        str(systemctl),
                        "--git",
                        shutil.which("git") or "/usr/bin/git",
                        "--lock-file",
                        str(root / "deploy.lock"),
                        "--binary",
                        "runtime",
                        "--required-tool",
                        "workspace.get",
                        "--expected-tool-count",
                        "1",
                        "--wait-seconds",
                        "2",
                    ],
                    cwd=REPO,
                    check=False,
                    text=True,
                    capture_output=True,
                )
            self.assertEqual(result.returncode, 1)
            self.assertIn("not_committed", result.stderr)
            self.assertEqual((install / "runtime").read_text(), "old\n")
            self.assertEqual(state.read_text(), "active\n")
            self.assertFalse((install / "runtime.next").exists())
            receipts = list((root / "receipts").iterdir())
            self.assertEqual(len(receipts), 1)
            receipt_result = json.loads((receipts[0] / "result.json").read_text())
            self.assertEqual(receipt_result["status"], "not_committed")
            self.assertTrue(receipt_result["rollback"]["serviceActive"])

    def test_deploy_probe_failure_automatically_restores_previous_binary(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repo = root / "repo"
            commit = initialize_git_repository(repo, remote=True)
            candidate = repo / "target" / "release"
            install = root / "install"
            candidate.mkdir(parents=True)
            install.mkdir()
            write_executable(candidate / "runtime", "new\n")
            write_executable(install / "runtime", "old\n")
            manifest = root / "candidate-manifest.json"
            write_candidate_manifest(manifest, candidate, commit, ("runtime",), repo)
            database = root / "registry.sqlite3"
            initialize_registry(database)
            systemctl = fake_systemctl(root)
            with mcp_server(["workspace.get"]) as port:
                env_file = root / "runtime.env"
                env_file.write_text(
                    f"ORDIVON_BIND=127.0.0.1:{port}\nORDIVON_BEARER_TOKEN=test\n",
                    encoding="utf-8",
                )
                result = subprocess.run(
                    [
                        sys.executable,
                        "scripts/ordivon-runtime-deploy",
                        "apply",
                        "--source-repo",
                        str(repo),
                        "--commit",
                        commit,
                        "--confirm-commit",
                        commit,
                        "--candidate-dir",
                        str(candidate),
                        "--candidate-manifest",
                        str(manifest),
                        "--install-dir",
                        str(install),
                        "--database",
                        str(database),
                        "--env-file",
                        str(env_file),
                        "--receipt-root",
                        str(root / "receipts"),
                        "--systemctl",
                        str(systemctl),
                        "--git",
                        shutil.which("git") or "/usr/bin/git",
                        "--lock-file",
                        str(root / "deploy.lock"),
                        "--binary",
                        "runtime",
                        "--expected-tool-count",
                        "2",
                        "--required-tool",
                        "workspace.get",
                        "--wait-seconds",
                        "0.5",
                    ],
                    cwd=REPO,
                    check=False,
                    text=True,
                    capture_output=True,
                )
            self.assertEqual(result.returncode, 1)
            self.assertIn("rolled_back", result.stderr)
            self.assertEqual((install / "runtime").read_text(), "old\n")
            receipts = list((root / "receipts").iterdir())
            self.assertEqual(len(receipts), 1)
            status = json.loads((receipts[0] / "result.json").read_text())["status"]
            self.assertEqual(status, "rolled_back")


if __name__ == "__main__":
    unittest.main()

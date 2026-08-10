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
import time
import threading
import unittest
from contextlib import closing, contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path


REPO = Path(__file__).resolve().parents[2]


@contextmanager
def mcp_server(tool_names: list[str], close_callback=None, *, modern: bool = True):
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, _format: str, *args) -> None:
            return

        def send_json(
            self,
            request: dict[str, object],
            *,
            result: dict[str, object] | None = None,
            error: dict[str, object] | None = None,
            status: int = 200,
            session_id: str | None = None,
        ) -> None:
            response: dict[str, object] = {
                "jsonrpc": "2.0",
                "id": request.get("id"),
            }
            if error is not None:
                response["error"] = error
            else:
                response["result"] = result or {}
            body = json.dumps(response).encode()
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            if session_id is not None:
                self.send_header("Mcp-Session-Id", session_id)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def do_POST(self) -> None:
            length = int(self.headers.get("Content-Length", "0"))
            request = json.loads(self.rfile.read(length))
            method = request.get("method")
            params = request.get("params")
            params = params if isinstance(params, dict) else {}
            if method == "server/discover":
                if not modern:
                    self.send_json(
                        request,
                        error={"code": -32601, "message": "Method not found"},
                        status=404,
                    )
                    return
                self.send_json(
                    request,
                    result={
                        "supportedVersions": [
                            "2026-07-28",
                            "2025-11-25",
                            "2025-06-18",
                        ],
                        "capabilities": {"tools": {}},
                        "instructions": "test Runtime",
                        "ttlMs": 0,
                        "cacheScope": "private",
                        "_meta": {
                            "io.modelcontextprotocol/serverInfo": {
                                "name": "test-runtime",
                                "version": "1",
                                "title": "Test Runtime",
                            },
                            "com.ordivon/runtime/toolCatalogDigest": ("sha256:" + "a" * 64),
                        },
                    },
                )
                return
            if method == "initialize":
                requested_version = params.get("protocolVersion")
                protocol_version = requested_version if isinstance(requested_version, str) else "2025-06-18"
                self.send_json(
                    request,
                    result={
                        "protocolVersion": protocol_version,
                        "capabilities": {"tools": {}},
                        "serverInfo": {
                            "name": "test-runtime",
                            "version": "1",
                        },
                    },
                    session_id="test-session",
                )
                return
            if method == "notifications/initialized":
                self.send_response(202)
                self.send_header("Content-Length", "0")
                self.end_headers()
                return
            if not modern and self.headers.get("Mcp-Session-Id") != "test-session":
                self.send_json(
                    request,
                    error={"code": -32001, "message": "Missing session"},
                    status=400,
                )
                return
            if method == "tools/list":
                self.send_json(
                    request,
                    result={"tools": [{"name": name} for name in tool_names]},
                )
                return
            if method == "tools/call" and params.get("name") == "workspace.close":
                arguments = params.get("arguments")
                arguments = arguments if isinstance(arguments, dict) else {}
                workspace_id = str(arguments.get("workspaceId"))
                if close_callback is not None:
                    close_callback(workspace_id)
                self.send_json(
                    request,
                    result={
                        "isError": False,
                        "structuredContent": {
                            "workspaceId": workspace_id,
                            "removed": True,
                        },
                    },
                )
                return
            self.send_json(
                request,
                error={"code": -32601, "message": "Method not found"},
                status=404,
            )

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
    with closing(sqlite3.connect(database)) as connection:
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
            connection.execute("INSERT INTO attempts VALUES ('attempt-active','job-active','running')")
            connection.execute("INSERT INTO concurrency_reservations VALUES ('attempt-active','active')")
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


def add_release_operator_sources(path: Path, *, push: bool) -> str:
    scripts = path / "scripts"
    scripts.mkdir(exist_ok=True)
    names = (
        "ordivon-runtime-deploy",
        "ordivon-runtime-lifecycle",
        "ordivon-runtime-reclaim",
        "ordivon-runtime-cache",
        "ordivon-runtime-status",
        "ordivon-runtime-capacity-acceptance",
    )
    for name in names:
        write_executable(scripts / name, f"#!/bin/sh\nprintf '{name}-candidate\\n'\n")
    (scripts / "mcp_probe.py").write_text("PROBE_REVISION = 'candidate'\n", encoding="utf-8")
    (scripts / "mcp_probe.py").chmod(0o644)
    subprocess.run(["git", "-C", str(path), "add", "scripts"], check=True)
    subprocess.run(["git", "-C", str(path), "commit", "-qm", "release operator sources"], check=True)
    if push:
        subprocess.run(["git", "-C", str(path), "push", "-qu", "origin", "main"], check=True)
    return subprocess.run(
        ["git", "-C", str(path), "rev-parse", "HEAD"],
        check=True,
        text=True,
        capture_output=True,
    ).stdout.strip()


def fake_cargo_for_default_release(root: Path) -> Path:
    write_executable(
        root / "rustc",
        "#!/bin/sh\nprintf 'rustc 1.95.0 (test)\\nbinary: rustc\\nhost: x86_64-unknown-linux-gnu\\n'\n",
    )
    cargo = root / "cargo"
    binary_commands = "".join(
        f"printf '{name}-candidate\\n' > \"$target/release/{name}\"\nchmod 755 \"$target/release/{name}\"\n"
        for name in (
            "ordivon-runtime",
            "ordivon-runtime-runner",
            "ordivon-runtime-doctor",
            "ordivon-runtime-inspect",
            "ordivon-runtime-repair",
        )
    )
    write_executable(
        cargo,
        "#!/bin/sh\n"
        'if [ "${1:-}" = --version ]; then printf "cargo 1.95.0 (test)\\nrelease: 1.95.0\\n"; exit 0; fi\n'
        "target=''\n"
        "while [ $# -gt 0 ]; do\n"
        '  if [ "$1" = --target-dir ]; then target=$2; shift 2; else shift; fi\n'
        "done\n"
        'mkdir -p "$target/release"\n'
        + binary_commands,
    )
    return cargo


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
                "kind": "binary",
                "mode": 0o755,
                "path": str(binary),
                "bytes": binary.stat().st_size,
                "digest": "sha256:" + hashlib.sha256(binary.read_bytes()).hexdigest(),
            }
        )
    path.write_text(
        json.dumps(
            {
                "schemaVersion": 2,
                "commit": commit,
                "sourceRepo": str(source_repo.resolve()),
                "sourceMaterialization": "detached_git_checkout",
                "candidateDir": str(candidate.resolve()),
                "builtAtMs": 1,
                "cargo": "/test/cargo",
                "toolchain": {
                    "cargo": {
                        "path": "/test/cargo",
                        "resolvedPath": "/test/cargo",
                        "digest": "sha256:" + "c" * 64,
                        "version": "cargo 1.95.0",
                        "details": [],
                    },
                    "rustc": {
                        "path": "/test/rustc",
                        "resolvedPath": "/test/rustc",
                        "digest": "sha256:" + "d" * 64,
                        "version": "rustc 1.95.0",
                        "details": ["host: x86_64-unknown-linux-gnu"],
                    },
                    "host": "x86_64-unknown-linux-gnu",
                },
                "artifacts": binaries,
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
        'case "$1" in\n'
        '  is-active) cat "$state" ;;\n'
        "  stop) printf 'inactive\\n' > \"$state\" ;;\n"
        "  start) printf 'active\\n' > \"$state\" ;;\n"
        "  *) exit 1 ;;\n"
        "esac\n",
    )
    return systemctl


class DeployReclaimTests(unittest.TestCase):
    def test_default_prepare_binds_complete_release_artifact_set(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repo = root / "repo"
            initialize_git_repository(repo, remote=False)
            commit = add_release_operator_sources(repo, push=False)
            candidate = repo / "target" / "release"
            manifest = root / "candidate-manifest.json"
            cargo = fake_cargo_for_default_release(root)
            completed = subprocess.run(
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
                ],
                cwd=REPO,
                check=True,
                text=True,
                capture_output=True,
            )
            report = json.loads(completed.stdout)
            self.assertEqual(report["schemaVersion"], 2)
            artifacts = {item["name"]: item for item in report["artifacts"]}
            self.assertEqual(len(artifacts), 12)
            self.assertEqual(artifacts["mcp_probe.py"]["kind"], "support")
            self.assertEqual(artifacts["mcp_probe.py"]["mode"], 0o644)
            self.assertEqual(artifacts["ordivon-runtime-status"]["kind"], "operator")
            self.assertEqual(artifacts["ordivon-runtime-status"]["mode"], 0o755)
            self.assertEqual(
                (candidate / "ordivon-runtime-status").read_bytes(),
                (repo / "scripts/ordivon-runtime-status").read_bytes(),
            )
            self.assertEqual(
                hashlib.sha256((candidate / "mcp_probe.py").read_bytes()).hexdigest(),
                hashlib.sha256((repo / "scripts/mcp_probe.py").read_bytes()).hexdigest(),
            )

    def test_default_apply_and_rollback_cover_operator_and_support_artifacts(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repo = root / "repo"
            initialize_git_repository(repo, remote=True)
            commit = add_release_operator_sources(repo, push=True)
            candidate = repo / "target" / "release"
            manifest = root / "candidate-manifest.json"
            cargo = fake_cargo_for_default_release(root)
            subprocess.run(
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
                ],
                cwd=REPO,
                check=True,
                text=True,
                capture_output=True,
            )
            install = root / "install"
            install.mkdir()
            release_names = [
                "ordivon-runtime",
                "ordivon-runtime-runner",
                "ordivon-runtime-doctor",
                "ordivon-runtime-inspect",
                "ordivon-runtime-repair",
                "ordivon-runtime-deploy",
                "ordivon-runtime-lifecycle",
                "ordivon-runtime-reclaim",
                "ordivon-runtime-cache",
                "ordivon-runtime-status",
                "ordivon-runtime-capacity-acceptance",
            ]
            for name in release_names:
                write_executable(install / name, f"old-{name}\n")
            (install / "mcp_probe.py").write_text("OLD_PROBE = True\n", encoding="utf-8")
            (install / "mcp_probe.py").chmod(0o644)
            prior_commit = "a" * 40
            prior_receipt = root / "receipts" / "prior"
            prior_receipt.mkdir(parents=True)
            prior_installed = []
            for name in [*release_names, "mcp_probe.py"]:
                path = install / name
                prior_installed.append(
                    {
                        "name": name,
                        "kind": "support" if name == "mcp_probe.py" else (
                            "binary" if name in {
                                "ordivon-runtime",
                                "ordivon-runtime-runner",
                                "ordivon-runtime-doctor",
                                "ordivon-runtime-inspect",
                                "ordivon-runtime-repair",
                            } else "operator"
                        ),
                        "mode": path.stat().st_mode & 0o777,
                        "digest": "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest(),
                        "bytes": path.stat().st_size,
                        "path": str(path),
                    }
                )
            (prior_receipt / "result.json").write_text(
                json.dumps(
                    {
                        "schemaVersion": 2,
                        "status": "deployed",
                        "commit": prior_commit,
                        "finishedAtMs": 1,
                        "installed": prior_installed,
                    }
                ),
                encoding="utf-8",
            )
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
                result = json.loads(deployed.stdout)
                installed = {item["name"]: item for item in result["installed"]}
                self.assertEqual(len(installed), 12)
                self.assertEqual(installed["mcp_probe.py"]["mode"], 0o644)
                self.assertEqual((install / "mcp_probe.py").stat().st_mode & 0o777, 0o644)
                self.assertEqual(
                    (install / "ordivon-runtime-status").read_bytes(),
                    (repo / "scripts/ordivon-runtime-status").read_bytes(),
                )
                receipt = Path(result["receipt"])
                receipt_manifest = json.loads((receipt / "manifest.json").read_text())
                self.assertEqual(receipt_manifest["schemaVersion"], 2)
                self.assertEqual(len(receipt_manifest["artifacts"]), 12)
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
                        "--database",
                        str(database),
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
            self.assertEqual(rollback["commit"], prior_commit)
            self.assertTrue(rollback["commitKnown"])
            self.assertEqual(len(rollback["installed"]), 12)
            self.assertEqual((install / "ordivon-runtime-status").read_text(), "old-ordivon-runtime-status\n")
            self.assertEqual((install / "mcp_probe.py").read_text(), "OLD_PROBE = True\n")
            self.assertEqual((install / "mcp_probe.py").stat().st_mode & 0o777, 0o644)

    def test_new_deployer_rolls_back_legacy_v1_binary_receipt(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            install = root / "install"
            install.mkdir()
            write_executable(install / "runtime", "current\n")
            receipt = root / "receipt"
            previous = receipt / "previous"
            previous.mkdir(parents=True)
            write_executable(previous / "runtime", "legacy-previous\n")
            previous_digest = "sha256:" + hashlib.sha256((previous / "runtime").read_bytes()).hexdigest()
            env_file = root / "runtime.env"
            database = root / "registry.sqlite3"
            initialize_registry(database)
            systemctl = fake_systemctl(root)
            with mcp_server(["workspace.get"]) as port:
                env_file.write_text(
                    f"ORDIVON_BIND=127.0.0.1:{port}\nORDIVON_BEARER_TOKEN=test\n",
                    encoding="utf-8",
                )
                (receipt / "manifest.json").write_text(
                    json.dumps(
                        {
                            "schemaVersion": 1,
                            "service": "ordivon-runtime.service",
                            "binaries": ["runtime"],
                            "installDir": str(install),
                            "envFile": str(env_file),
                            "previous": [
                                {
                                    "name": "runtime",
                                    "digest": previous_digest,
                                    "bytes": (previous / "runtime").stat().st_size,
                                }
                            ],
                        }
                    ),
                    encoding="utf-8",
                )
                completed = subprocess.run(
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
                        "--database",
                        str(database),
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
            self.assertEqual(json.loads(completed.stdout)["status"], "restored_previous")
            self.assertEqual((install / "runtime").read_text(), "legacy-previous\n")
            self.assertEqual((install / "runtime").stat().st_mode & 0o777, 0o755)

    def test_deploy_admission_fence_is_exclusive_and_drain_waits_for_natural_release(self) -> None:
        scripts_path = str(REPO / "scripts")
        sys.path.insert(0, scripts_path)
        try:
            module = runpy.run_path(str(REPO / "scripts/ordivon-runtime-deploy"))
        finally:
            sys.path.remove(scripts_path)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            database = root / "registry.sqlite3"
            initialize_registry(database, active_workspace="busy")
            fence_path = module["admission_fence_path"](database)
            self.assertEqual(fence_path, root / "admission.lock")

            def release_job() -> None:
                time.sleep(0.15)
                with closing(sqlite3.connect(database)) as connection:
                    connection.execute("UPDATE jobs SET resolution='succeeded' WHERE job_id='job-active'")
                    connection.execute("UPDATE concurrency_reservations SET state='released' WHERE attempt_id='attempt-active'")
                    connection.commit()

            release = threading.Thread(target=release_job)
            release.start()
            started = time.monotonic()
            fence = module["acquire_exclusive_admission_fence"](database)
            try:
                self.assertEqual(module["wait_for_execution_drain"](database, 2.0), [])
                competing = fence_path.open("a+")
                try:
                    with self.assertRaises(BlockingIOError):
                        fcntl.flock(competing.fileno(), fcntl.LOCK_SH | fcntl.LOCK_NB)
                finally:
                    competing.close()
            finally:
                fence.close()
            release.join(timeout=2)
            self.assertGreaterEqual(time.monotonic() - started, 0.1)

    def test_deploy_drain_timeout_reports_remaining_jobs_without_mutation(self) -> None:
        scripts_path = str(REPO / "scripts")
        sys.path.insert(0, scripts_path)
        try:
            module = runpy.run_path(str(REPO / "scripts/ordivon-runtime-deploy"))
        finally:
            sys.path.remove(scripts_path)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            database = root / "registry.sqlite3"
            initialize_registry(database, active_workspace="busy")
            fence = module["acquire_exclusive_admission_fence"](database)
            try:
                remaining = module["wait_for_execution_drain"](database, 0.15)
            finally:
                fence.close()
            self.assertEqual(remaining, ["job-active"])

    def test_deploy_wait_policy_has_no_legacy_five_minute_ceiling(self) -> None:
        module = runpy.run_path(str(REPO / "scripts/ordivon-runtime-deploy"))
        self.assertEqual(module["positive_float"]("301"), 301.0)
        self.assertEqual(module["positive_float"]("3600"), 3600.0)
        for invalid in ("0", "-1", "inf", "nan"):
            with self.assertRaises(Exception):
                module["positive_float"](invalid)

    def test_reclaim_numeric_bounds_follow_sqlite_and_finite_policy_values(self) -> None:
        module = runpy.run_path(str(REPO / "scripts/ordivon-runtime-reclaim"))
        self.assertEqual(module["positive_timeout"]("60001"), 60_001)
        self.assertEqual(module["positive_timeout"]("2147483647"), 2_147_483_647)
        with self.assertRaises(Exception):
            module["positive_timeout"]("2147483648")
        self.assertEqual(module["nonnegative_float"]("87601"), 87_601.0)
        with self.assertRaises(Exception):
            module["nonnegative_float"]("nan")

    def test_deploy_prepare_binds_binaries_to_exact_clean_commit(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repo = root / "repo"
            commit = initialize_git_repository(repo, remote=False)
            candidate = root / "target/release"
            manifest = root / "candidate-manifest.json"
            cargo = root / "cargo"
            write_executable(
                root / "rustc",
                "#!/bin/sh\n"
                "printf 'rustc 1.95.0 (test)\nbinary: rustc\nhost: x86_64-unknown-linux-gnu\n'\n",
            )
            write_executable(
                cargo,
                "#!/bin/sh\n"
                'if [ "${1:-}" = --version ]; then printf "cargo 1.95.0 (test)\nrelease: 1.95.0\n"; exit 0; fi\n'
                "target=''\n"
                "while [ $# -gt 0 ]; do\n"
                '  if [ "$1" = --target-dir ]; then target=$2; shift 2; else shift; fi\n'
                "done\n"
                'mkdir -p "$target/release"\n'
                "printf '%s\\n' \"$RUSTC\" > \"$target/release/RUSTC_USED\"\n"
                "printf '%s\\n' \"${PATH%%:*}\" > \"$target/release/PATH_HEAD\"\n"
                "printf 'built-from-commit\\n' > \"$target/release/runtime\"\n"
                'chmod 755 "$target/release/runtime"\n',
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
            self.assertEqual(report["toolchain"]["host"], "x86_64-unknown-linux-gnu")
            self.assertEqual((candidate / "RUSTC_USED").read_text().strip(), str(root / "rustc"))
            self.assertEqual((candidate / "PATH_HEAD").read_text().strip(), str(root))
            stored = json.loads(manifest.read_text())
            self.assertEqual(stored["commit"], commit)
            self.assertEqual(stored["toolchain"]["cargo"]["version"], "cargo 1.95.0 (test)")

    def test_deploy_prepare_preserves_rustup_proxy_invocation_paths(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repo = root / "repo"
            commit = initialize_git_repository(repo, remote=False)
            candidate = root / "target/release"
            manifest = root / "candidate-manifest.json"
            proxy = root / "rustup"
            write_executable(
                proxy,
                "#!/bin/sh\n"
                "tool=${0##*/}\n"
                'if [ "$tool" = rustc ]; then printf "rustc 1.95.0 (test)\\nbinary: rustc\\nhost: x86_64-unknown-linux-gnu\\n"; exit 0; fi\n'
                'if [ "$tool" = cargo ] && [ "${1:-}" = --version ]; then printf "cargo 1.95.0 (test)\\nrelease: 1.95.0\\n"; exit 0; fi\n'
                'if [ "$tool" != cargo ]; then exit 2; fi\n'
                "target=''\n"
                "while [ $# -gt 0 ]; do\n"
                '  if [ "$1" = --target-dir ]; then target=$2; shift 2; else shift; fi\n'
                "done\n"
                'mkdir -p "$target/release"\n'
                'printf "%s\\n" "$RUSTC" > "$target/release/RUSTC_USED"\n'
                'printf "%s\\n" "$RUSTDOC" > "$target/release/RUSTDOC_USED"\n'
                'printf "built-from-proxy\\n" > "$target/release/runtime"\n'
                'chmod 755 "$target/release/runtime"\n',
            )
            cargo = root / "cargo"
            rustc = root / "rustc"
            rustdoc = root / "rustdoc"
            cargo.symlink_to(proxy)
            rustc.symlink_to(proxy)
            rustdoc.symlink_to(proxy)
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
            self.assertEqual(report["toolchain"]["cargo"]["resolvedPath"], str(proxy))
            self.assertEqual(report["toolchain"]["rustc"]["resolvedPath"], str(proxy))
            self.assertEqual((candidate / "RUSTC_USED").read_text().strip(), str(rustc))
            self.assertEqual((candidate / "RUSTDOC_USED").read_text().strip(), str(rustdoc))

    def test_prepare_ignores_hidden_mutable_checkout_drift_and_builds_exact_commit(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repo = root / "repo"
            subprocess.run(["git", "init", "-q", str(repo)], check=True)
            subprocess.run(["git", "-C", str(repo), "config", "user.name", "Test"], check=True)
            subprocess.run(
                ["git", "-C", str(repo), "config", "user.email", "test@example.invalid"],
                check=True,
            )
            (repo / ".gitignore").write_text("target/\n", encoding="utf-8")
            (repo / "SOURCE.txt").write_text("committed\n", encoding="utf-8")
            subprocess.run(["git", "-C", str(repo), "add", "."], check=True)
            subprocess.run(["git", "-C", str(repo), "commit", "-qm", "baseline"], check=True)
            commit = subprocess.run(
                ["git", "-C", str(repo), "rev-parse", "HEAD"],
                check=True,
                text=True,
                capture_output=True,
            ).stdout.strip()
            subprocess.run(
                ["git", "-C", str(repo), "update-index", "--assume-unchanged", "SOURCE.txt"],
                check=True,
            )
            (repo / "SOURCE.txt").write_text("hidden-drift\n", encoding="utf-8")
            self.assertEqual(
                subprocess.run(
                    ["git", "-C", str(repo), "status", "--porcelain"],
                    check=True,
                    text=True,
                    capture_output=True,
                ).stdout,
                "",
            )
            candidate = repo / "target" / "release"
            manifest = root / "candidate-manifest.json"
            cargo = root / "cargo"
            write_executable(
                root / "rustc",
                "#!/bin/sh\nprintf 'rustc 1.95.0 (test)\\nbinary: rustc\\nhost: x86_64-unknown-linux-gnu\\n'\n",
            )
            write_executable(
                cargo,
                "#!/bin/sh\n"
                'if [ "${1:-}" = --version ]; then printf "cargo 1.95.0 (test)\\nrelease: 1.95.0\\n"; exit 0; fi\n'
                "target=''\n"
                "while [ $# -gt 0 ]; do\n"
                '  if [ "$1" = --target-dir ]; then target=$2; shift 2; else shift; fi\n'
                "done\n"
                'mkdir -p "$target/release"\n'
                'cat SOURCE.txt > "$target/release/runtime"\n'
                'chmod 755 "$target/release/runtime"\n',
            )
            completed = subprocess.run(
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
            report = json.loads(completed.stdout)
            self.assertEqual(report["sourceMaterialization"], "detached_git_checkout")
            self.assertEqual((candidate / "runtime").read_text(), "committed\n")
            self.assertEqual(report["commit"], commit)

    def test_deploy_prepare_rejects_build_that_changes_source(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repo = root / "repo"
            commit = initialize_git_repository(repo, remote=False)
            candidate = root / "target/release"
            manifest = root / "candidate-manifest.json"
            cargo = root / "cargo"
            write_executable(
                root / "rustc",
                "#!/bin/sh\n"
                "printf 'rustc 1.95.0 (test)\nbinary: rustc\nhost: x86_64-unknown-linux-gnu\n'\n",
            )
            write_executable(
                cargo,
                "#!/bin/sh\n"
                'if [ "${1:-}" = --version ]; then printf "cargo 1.95.0 (test)\nrelease: 1.95.0\n"; exit 0; fi\n'
                "target=''\n"
                "while [ $# -gt 0 ]; do\n"
                '  if [ "$1" = --target-dir ]; then target=$2; shift 2; else shift; fi\n'
                "done\n"
                'mkdir -p "$target/release"\n'
                "printf 'built\n' > \"$target/release/runtime\"\n"
                'chmod 755 "$target/release/runtime"\n'
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
            self.assertIn("build changed the detached release source", result.stderr)
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

            with mcp_server(["workspace.close"], close_callback, modern=False) as port:
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

    def test_prepare_and_plan_treat_local_head_as_diagnostic_not_release_authority(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repo = root / "repo"
            commit = initialize_git_repository(repo, remote=True)
            (repo / "LOCAL.md").write_text("later local commit\n", encoding="utf-8")
            subprocess.run(["git", "-C", str(repo), "add", "LOCAL.md"], check=True)
            subprocess.run(["git", "-C", str(repo), "commit", "-qm", "local later commit"], check=True)
            local_head = subprocess.run(
                ["git", "-C", str(repo), "rev-parse", "HEAD"],
                check=True,
                text=True,
                capture_output=True,
            ).stdout.strip()
            self.assertNotEqual(local_head, commit)
            candidate = repo / "target" / "release"
            manifest = root / "candidate-manifest.json"
            cargo = root / "cargo"
            write_executable(
                root / "rustc",
                "#!/bin/sh\nprintf 'rustc 1.95.0 (test)\\nbinary: rustc\\nhost: x86_64-unknown-linux-gnu\\n'\n",
            )
            write_executable(
                cargo,
                "#!/bin/sh\n"
                'if [ "${1:-}" = --version ]; then printf "cargo 1.95.0 (test)\\nrelease: 1.95.0\\n"; exit 0; fi\n'
                "target=''\n"
                "while [ $# -gt 0 ]; do\n"
                '  if [ "$1" = --target-dir ]; then target=$2; shift 2; else shift; fi\n'
                "done\n"
                'mkdir -p "$target/release"\n'
                "printf 'candidate\n' > \"$target/release/runtime\"\n"
                'chmod 755 "$target/release/runtime"\n',
            )
            prepared = subprocess.run(
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
            self.assertEqual(json.loads(prepared.stdout)["commit"], commit)
            install = root / "install"
            install.mkdir()
            write_executable(install / "runtime", "installed\n")
            database = root / "registry.sqlite3"
            initialize_registry(database)
            env_file = root / "runtime.env"
            env_file.write_text(
                "ORDIVON_BIND=127.0.0.1:1\nORDIVON_BEARER_TOKEN=test\n",
                encoding="utf-8",
            )
            planned = subprocess.run(
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
            self.assertEqual(planned.returncode, 0, planned.stderr)
            plan = json.loads(planned.stdout)
            self.assertTrue(plan["eligible"])
            self.assertEqual(plan["sourceHead"], local_head)
            self.assertFalse(plan["sourceHeadMatchesCommit"])
            self.assertEqual(plan["requiredRefCommit"], commit)

    def test_deploy_plan_treats_mutable_checkout_dirt_as_diagnostic_only(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repo = root / "repo"
            commit = initialize_git_repository(repo, remote=True)
            candidate = root / "candidate"
            install = root / "install"
            candidate.mkdir()
            install.mkdir()
            write_executable(candidate / "runtime", "candidate\n")
            write_executable(install / "runtime", "installed\n")
            manifest = root / "candidate-manifest.json"
            write_candidate_manifest(manifest, candidate, commit, ("runtime",), repo)
            (repo / "README.md").write_text("local-uncommitted-work\n", encoding="utf-8")
            database = root / "registry.sqlite3"
            initialize_registry(database)
            env_file = root / "runtime.env"
            env_file.write_text(
                "ORDIVON_BIND=127.0.0.1:1\nORDIVON_BEARER_TOKEN=test\n",
                encoding="utf-8",
            )
            completed = subprocess.run(
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
            self.assertEqual(completed.returncode, 0, completed.stderr)
            plan = json.loads(completed.stdout)
            self.assertTrue(plan["eligible"])
            self.assertFalse(plan["sourceClean"])
            self.assertNotIn("source repository is dirty", plan["blockers"])

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
                "candidate release artifact does not match manifest: runtime",
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
            initialize_registry(database, active_workspace="draining")
            systemctl = fake_systemctl(root)

            def release_drain_job() -> None:
                time.sleep(0.2)
                with closing(sqlite3.connect(database)) as connection:
                    connection.execute("UPDATE jobs SET resolution='succeeded' WHERE job_id='job-active'")
                    connection.execute("UPDATE concurrency_reservations SET state='released' WHERE attempt_id='attempt-active'")
                    connection.commit()

            release = threading.Thread(target=release_drain_job)
            release.start()
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
                    "--drain-seconds",
                    "2",
                ]
                deployed = subprocess.run(
                    command,
                    cwd=REPO,
                    check=True,
                    text=True,
                    capture_output=True,
                )
                release.join(timeout=2)
                self.assertFalse(release.is_alive())
                deployment = json.loads(deployed.stdout)
                self.assertEqual(deployment["status"], "deployed")
                self.assertEqual(deployment["probe"]["lifecycle"], "modern")
                self.assertEqual(deployment["probe"]["protocolVersion"], "2026-07-28")
                self.assertIn("2026-07-28", deployment["probe"]["supportedVersions"])
                self.assertEqual(
                    deployment["probe"]["toolCatalogDigest"],
                    "sha256:" + "a" * 64,
                )
                self.assertEqual(deployment["probe"]["toolCount"], 2)
                self.assertEqual(deployment["candidatePrune"]["status"], "skipped")
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
                        "--database",
                        str(database),
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

    def test_candidate_prune_keeps_current_and_previous_deployments(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            candidate_root = root / "candidates"
            current = "c" * 40
            previous = "b" * 40
            obsolete = "a" * 40
            current_release = candidate_root / current / "release"
            current_release.mkdir(parents=True)
            (current_release / "runtime").write_text("current", encoding="utf-8")
            for commit in (previous, obsolete):
                release = candidate_root / commit / "release"
                release.mkdir(parents=True)
                (release / "runtime").write_text(commit, encoding="utf-8")
            (candidate_root / "notes").mkdir()
            deployments = root / "deployments"
            prior_receipt = deployments / "prior"
            prior_receipt.mkdir(parents=True)
            (prior_receipt / "result.json").write_text(
                json.dumps(
                    {
                        "status": "deployed",
                        "commit": previous,
                        "finishedAtMs": 10,
                    }
                ),
                encoding="utf-8",
            )
            scripts_path = str(REPO / "scripts")
            sys.path.insert(0, scripts_path)
            try:
                namespace = runpy.run_path(str(REPO / "scripts/ordivon-runtime-deploy"))
            finally:
                sys.path.remove(scripts_path)
            result = namespace["prune_candidate_artifacts"](
                current_release,
                deployments,
                current,
                keep=2,
            )
            self.assertEqual(result["status"], "completed")
            self.assertTrue((candidate_root / current).is_dir())
            self.assertTrue((candidate_root / previous).is_dir())
            self.assertFalse((candidate_root / obsolete).exists())
            self.assertTrue((candidate_root / "notes").is_dir())
            self.assertEqual(result["actions"][0]["commit"], obsolete)

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
                        "--database",
                        str(database),
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
            self.assertIn("previous artifact digest does not match receipt", rollback.stderr)
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
                "    with closing(sqlite3.connect(database)) as connection:\n"
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

    def test_deploy_requires_modern_and_restores_legacy_previous_binary(self) -> None:
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
            with mcp_server(["workspace.get"], modern=False) as port:
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
                        "1",
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


    def test_rollback_fence_refuses_to_cross_active_or_held_job(self) -> None:
        scripts_path = str(REPO / "scripts")
        sys.path.insert(0, scripts_path)
        try:
            module = runpy.run_path(str(REPO / "scripts/ordivon-runtime-deploy"))
        finally:
            sys.path.remove(scripts_path)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            database = root / "registry.sqlite3"
            initialize_registry(database, active_workspace="rollback-busy")
            with self.assertRaisesRegex(RuntimeError, "rollback drain timed out"):
                with module["fenced_execution_drain"](database, 0.05):
                    self.fail("rollback crossed an active Runtime Job")
            handle = module["acquire_exclusive_admission_fence"](database)
            handle.close()


if __name__ == "__main__":
    unittest.main()

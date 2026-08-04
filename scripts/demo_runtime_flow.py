#!/usr/bin/env python3
"""Run a bounded, presentation-ready proof against the installed Runtime service."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import time
import uuid
from pathlib import Path
from typing import Any

from mcp_probe import MODERN_PROTOCOL_VERSION, McpClient, McpProbeError


SCHEMA_VERSION = 1
TERMINAL = {"succeeded", "failed", "timed_out", "cancelled", "lost", "orphaned"}
REQUIRED_TOOLS = {
    "artifact.read",
    "task.cancel",
    "task.list",
    "task.observe",
    "workspace.close",
    "workspace.diff",
    "workspace.execPlan",
    "workspace.get",
    "workspace.open",
    "workspace.patch",
    "workspace.read",
}
SECRET_KEYS = {"ORDIVON_BEARER_TOKEN"}


class DemoError(RuntimeError):
    pass


def sha256_bytes(value: bytes) -> str:
    return f"sha256:{hashlib.sha256(value).hexdigest()}"


def run(
    *args: str,
    cwd: Path,
    check: bool = True,
    env: dict[str, str] | None = None,
) -> subprocess.CompletedProcess[str]:
    return subprocess.run(args, cwd=cwd, check=check, text=True, capture_output=True, env=env)


def parse_environment_file(path: Path) -> dict[str, str]:
    values: dict[str, str] = {}
    for line_number, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("export "):
            line = line[7:].lstrip()
        if "=" not in line:
            raise DemoError(f"{path}:{line_number}: expected KEY=VALUE")
        key, raw_value = line.split("=", 1)
        key = key.strip()
        if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", key):
            raise DemoError(f"{path}:{line_number}: invalid environment key")
        lexer = shlex.shlex(raw_value, posix=True)
        lexer.whitespace_split = True
        lexer.commenters = "#"
        parts = list(lexer)
        if len(parts) > 1:
            raise DemoError(f"{path}:{line_number}: value must be one systemd EnvironmentFile token")
        values[key] = parts[0] if parts else ""
    return values


def endpoint_from_bind(bind: str) -> str:
    value = bind.strip()
    if value.startswith(("http://", "https://")):
        return value if value.rstrip("/").endswith("/mcp") else f"{value.rstrip('/')}/mcp"
    host: str
    port: str
    if value.startswith("["):
        closing = value.find("]")
        if closing < 0 or closing + 2 > len(value) or value[closing + 1] != ":":
            raise DemoError(f"invalid ORDIVON_BIND: {bind!r}")
        host, port = value[1:closing], value[closing + 2 :]
    else:
        host, separator, port = value.rpartition(":")
        if not separator:
            raise DemoError(f"invalid ORDIVON_BIND: {bind!r}")
    if not port.isdigit():
        raise DemoError(f"invalid ORDIVON_BIND port: {bind!r}")
    if host in {"", "0.0.0.0", "::", "[::]"}:
        host = "127.0.0.1"
    formatted_host = f"[{host}]" if ":" in host else host
    return f"http://{formatted_host}:{port}/mcp"


def load_connection(env_file: Path, endpoint_override: str | None) -> tuple[str, str]:
    values = parse_environment_file(env_file) if env_file.is_file() else {}
    values.update({key: value for key, value in os.environ.items() if key.startswith("ORDIVON_")})
    token = values.get("ORDIVON_BEARER_TOKEN", "")
    if not token:
        raise DemoError("ORDIVON_BEARER_TOKEN is missing from the environment or EnvironmentFile")
    endpoint = endpoint_override or endpoint_from_bind(values.get("ORDIVON_BIND", "127.0.0.1:8811"))
    return endpoint, token


def text_edit_range(content: str, expected: str) -> dict[str, dict[str, int]]:
    start = content.find(expected)
    if start < 0 or content.find(expected, start + 1) >= 0:
        raise DemoError("expected patch text must occur exactly once")
    prefix = content[:start]
    start_line = prefix.count("\n") + 1
    start_column = len(prefix.rsplit("\n", 1)[-1])
    replaced = content[start : start + len(expected)]
    end_line = start_line + replaced.count("\n")
    end_column = start_column + len(replaced) if "\n" not in replaced else len(replaced.rsplit("\n", 1)[-1])
    return {
        "start": {"line": start_line, "column": start_column},
        "end": {"line": end_line, "column": end_column},
    }


def short(value: object, length: int = 12) -> str:
    text = str(value or "")
    if text.startswith("sha256:"):
        return f"sha256:{text[7:7 + length]}…"
    return f"{text[:length]}…" if len(text) > length else text


def event(kind: str, detail: str) -> dict[str, str]:
    print(f"{kind:<10} {detail}", flush=True)
    return {"kind": kind.lower(), "detail": detail}


def selected_terminal_evidence(document: dict[str, Any], digest: str, artifact_id: str) -> dict[str, Any]:
    allowed = (
        "schemaVersion",
        "jobId",
        "attemptId",
        "workspaceId",
        "sourceRevision",
        "executionProfile",
        "startDisposition",
        "cancellationDisposition",
        "executionDisposition",
        "deliveryDisposition",
        "processTreeDisposition",
        "reasonCode",
        "terminalArtifactIds",
        "observedAtMs",
    )
    selected = {key: document[key] for key in allowed if key in document}
    selected["artifactId"] = artifact_id
    selected["digest"] = digest
    return selected


def assert_no_secrets(value: object, secrets: tuple[str, ...]) -> None:
    encoded = json.dumps(value, ensure_ascii=False, sort_keys=True)
    for secret in secrets:
        if secret and secret in encoded:
            raise DemoError("receipt contains secret material")
    for key in SECRET_KEYS:
        if key in encoded:
            raise DemoError(f"receipt exposes secret key name: {key}")


def create_source_repo(fixture: Path, destination: Path) -> str:
    if not fixture.is_dir():
        raise DemoError(f"fixture does not exist: {fixture}")
    shutil.copytree(fixture, destination)
    deterministic_env = {
        **os.environ,
        "PYTHONDONTWRITEBYTECODE": "1",
        "PYTHONHASHSEED": "0",
    }
    pre_patch = run(
        sys.executable,
        "-m",
        "unittest",
        "-v",
        cwd=destination,
        check=False,
        env=deterministic_env,
    )
    if pre_patch.returncode == 0:
        raise DemoError("fixture must fail verification before the guarded Patch")
    run("git", "init", "-q", cwd=destination)
    run("git", "config", "user.name", "Ordivon Runtime Demo", cwd=destination)
    run("git", "config", "user.email", "demo@ordivon.invalid", cwd=destination)
    run("git", "add", ".", cwd=destination)
    run("git", "commit", "-q", "-m", "runtime demo source", cwd=destination)
    return run("git", "rev-parse", "HEAD", cwd=destination).stdout.strip()


def tool_names(client: McpClient) -> set[str]:
    tools = client.request("tools/list").get("tools", [])
    return {
        str(item["name"])
        for item in tools
        if isinstance(item, dict) and isinstance(item.get("name"), str)
    }


def wait_for_terminal(client: McpClient, job_id: str, events: list[dict[str, str]]) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    observations: list[dict[str, Any]] = []
    last_step: str | None = None
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        observed = client.call_tool(
            "task.observe",
            {
                "schemaVersion": SCHEMA_VERSION,
                "jobId": job_id,
                "waitMs": 500,
                "waitUntil": "change_or_terminal",
                "stdoutTailBytes": 8192,
                "stderrTailBytes": 8192,
            },
        )
        observations.append(
            {
                key: observed[key]
                for key in (
                    "status",
                    "attemptId",
                    "elapsedMs",
                    "progressRevision",
                    "completedSteps",
                    "totalSteps",
                    "currentStepId",
                    "currentStepIndex",
                    "failedStepId",
                    "exitCode",
                )
                if key in observed
            }
        )
        current_step = observed.get("currentStepId")
        if isinstance(current_step, str) and current_step != last_step:
            completed = int(observed.get("completedSteps", 0))
            total = int(observed.get("totalSteps", 0))
            events.append(event("STEP", f"{current_step}  {completed + 1}/{total}"))
            last_step = current_step
        if observed.get("status") in TERMINAL:
            return observations, observed
    raise DemoError("Job did not become terminal within 30 seconds")


def cleanup(client: McpClient | None, workspace_id: str | None, client_request_id: str | None) -> None:
    if client is None:
        return
    if client_request_id:
        try:
            listed = client.call_tool("task.list", {"limit": 20, "clientRequestId": client_request_id})
            for job in listed.get("jobs", []):
                if isinstance(job, dict) and job.get("status") not in TERMINAL and isinstance(job.get("jobId"), str):
                    client.call_tool("task.cancel", {"schemaVersion": SCHEMA_VERSION, "jobId": job["jobId"]})
        except Exception:
            pass
    if workspace_id:
        try:
            client.call_tool(
                "workspace.close",
                {"schemaVersion": SCHEMA_VERSION, "workspaceId": workspace_id, "force": True},
            )
        except Exception:
            pass


def run_demo(
    *,
    repo: Path,
    fixture: Path,
    env_file: Path,
    endpoint_override: str | None,
    receipt_path: Path,
) -> dict[str, Any]:
    endpoint, token = load_connection(env_file, endpoint_override)
    run_id = uuid.uuid4().hex
    workspace_id = f"runtime-demo-{run_id}"
    patch_request_id = f"demo:runtime-flow:patch:{run_id}"
    execution_request_id = f"demo:runtime-flow:exec:{run_id}"
    client: McpClient | None = None
    events: list[dict[str, str]] = []
    closed = False

    with tempfile.TemporaryDirectory(prefix="ordivon-runtime-demo-") as temporary:
        source = Path(temporary) / "source"
        revision = create_source_repo(fixture, source)
        events.append(event("SOURCE", short(revision)))
        try:
            client = McpClient(endpoint, token, "ordivon-runtime-demo", timeout=10.0)
            discovered = client.discover()
            missing = REQUIRED_TOOLS - tool_names(client)
            if missing:
                raise DemoError("installed Runtime is missing tools: " + ", ".join(sorted(missing)))
            metadata = discovered.get("_meta") if isinstance(discovered.get("_meta"), dict) else {}
            catalog_digest = metadata.get("com.ordivon/runtime/toolCatalogDigest")

            opened = client.call_tool(
                "workspace.open",
                {
                    "schemaVersion": SCHEMA_VERSION,
                    "workspaceId": workspace_id,
                    "sourceRepo": str(source),
                    "sourceRevision": revision,
                },
            )
            if opened.get("sourceRevision") != revision:
                raise DemoError("workspace.open returned a different source revision")
            events.append(event("WORKSPACE", f"{short(workspace_id)}  detached  {short(revision)}"))

            read = client.call_tool(
                "workspace.read",
                {
                    "schemaVersion": SCHEMA_VERSION,
                    "workspaceId": workspace_id,
                    "relativePath": "policy.py",
                    "mode": "FULL",
                    "offset": 0,
                    "maxBytes": 16_384,
                },
            )
            before = 'POLICY = "blind-redispatch"'
            after = 'POLICY = "recover-recorded-job"'
            patch_request = {
                "schemaVersion": SCHEMA_VERSION,
                "clientRequestId": patch_request_id,
                "workspaceId": workspace_id,
                "files": [
                    {
                        "relativePath": "policy.py",
                        "expectedDigest": read["digest"],
                        "edits": [
                            {
                                "range": text_edit_range(str(read["content"]), before),
                                "expectedText": before,
                                "replacement": after,
                            }
                        ],
                    }
                ],
                "maxDiffBytes": 65_536,
            }
            patched = client.call_tool("workspace.patch", patch_request)
            if patched.get("replayed") is not False:
                raise DemoError("first guarded Patch was unexpectedly replayed")
            events.append(event("PATCH", f"committed  {short(patched.get('requestDigest'))}"))

            execution_request = {
                "schemaVersion": SCHEMA_VERSION,
                "clientRequestId": execution_request_id,
                "execution": {
                    "workspaceId": workspace_id,
                    "steps": [
                        {
                            "id": "inspect",
                            "executable": "/usr/bin/python3",
                            "args": ["inspect_policy.py"],
                            "cwdRelative": ".",
                            "env": {"PYTHONDONTWRITEBYTECODE": "1", "PYTHONHASHSEED": "0"},
                            "timeoutMs": 5_000,
                        },
                        {
                            "id": "verify",
                            "executable": "/usr/bin/python3",
                            "args": ["-m", "unittest", "-v"],
                            "cwdRelative": ".",
                            "env": {"PYTHONDONTWRITEBYTECODE": "1", "PYTHONHASHSEED": "0"},
                            "timeoutMs": 5_000,
                        },
                        {
                            "id": "report",
                            "executable": "/usr/bin/python3",
                            "args": ["report.py"],
                            "cwdRelative": ".",
                            "env": {"PYTHONDONTWRITEBYTECODE": "1", "PYTHONHASHSEED": "0"},
                            "timeoutMs": 5_000,
                        },
                    ],
                    "stdoutLimitBytes": 65_536,
                    "stderrLimitBytes": 65_536,
                },
                "waitMs": 0,
                "stdoutTailBytes": 8192,
                "stderrTailBytes": 8192,
            }
            submitted = client.call_tool("workspace.execPlan", execution_request)
            job_id = str(submitted.get("jobId", ""))
            attempt_id = str(submitted.get("attemptId", ""))
            if not job_id or not attempt_id:
                raise DemoError("workspace.execPlan omitted Job or Attempt identity")
            events.append(event("JOB", f"{short(job_id)}  {short(attempt_id)}"))

            recovered_client = McpClient(endpoint, token, "ordivon-runtime-demo-reconnect", timeout=10.0)
            recovered_client.discover()
            replay = recovered_client.call_tool("workspace.execPlan", execution_request)
            if replay.get("jobId") != job_id or replay.get("attemptId") != attempt_id:
                raise DemoError("exact replay did not return the same Job and Attempt")
            events.append(event("RECOVER", f"same {short(job_id)}"))
            client = recovered_client

            listed = client.call_tool("task.list", {"limit": 20, "clientRequestId": execution_request_id})
            matching = [job for job in listed.get("jobs", []) if isinstance(job, dict) and job.get("jobId") == job_id]
            if len(matching) != 1:
                raise DemoError("task.list did not recover exactly one matching Job")

            observations, terminal = wait_for_terminal(client, job_id, events)
            if not any(observation.get("status") not in TERMINAL for observation in observations):
                raise DemoError("fixture completed before a non-terminal observation was captured")
            if terminal.get("status") != "succeeded" or terminal.get("attemptId") != attempt_id:
                raise DemoError(f"demo Job did not succeed: {terminal.get('status')}")

            artifacts = terminal.get("artifacts", [])
            terminal_descriptor = next(
                (
                    artifact
                    for artifact in artifacts
                    if isinstance(artifact, dict) and artifact.get("kind") == "terminal_evidence"
                ),
                None,
            )
            if not isinstance(terminal_descriptor, dict):
                raise DemoError("terminal observation omitted terminal-evidence Artifact")
            artifact_id = str(terminal_descriptor["artifactId"])
            artifact = client.call_tool(
                "artifact.read",
                {
                    "schemaVersion": SCHEMA_VERSION,
                    "jobId": job_id,
                    "artifactId": artifact_id,
                    "offset": 0,
                    "maxBytes": 262_144,
                },
            )
            evidence_document = json.loads(str(artifact["content"]))
            expected_binding = {
                "jobId": job_id,
                "attemptId": attempt_id,
                "workspaceId": workspace_id,
                "sourceRevision": revision,
            }
            for key, expected in expected_binding.items():
                if evidence_document.get(key) != expected:
                    raise DemoError(f"terminal evidence {key} does not match the execution")
            if artifact.get("digest") != terminal_descriptor.get("digest"):
                raise DemoError("artifact.read digest does not match its descriptor")
            events.append(event("EVIDENCE", f"terminal-evidence  {short(artifact.get('digest'))}"))

            diff = client.call_tool(
                "workspace.diff",
                {"schemaVersion": SCHEMA_VERSION, "workspaceId": workspace_id, "maxBytes": 65_536},
            )
            changed_paths = diff.get("changedPaths", [])
            if changed_paths != ["policy.py"] or diff.get("untrackedPaths"):
                raise DemoError(f"unexpected Workspace changes: {changed_paths!r} {diff.get('untrackedPaths')!r}")
            events.append(event("DIFF", "1 modified path  policy.py"))

            workspace = client.call_tool(
                "workspace.get",
                {"schemaVersion": SCHEMA_VERSION, "workspaceId": workspace_id},
            )
            source_state_digest = workspace.get("sourceStateDigest")
            if not isinstance(source_state_digest, str) or not source_state_digest.startswith("sha256:"):
                raise DemoError("workspace.get omitted sourceStateDigest")
            close = client.call_tool(
                "workspace.close",
                {
                    "schemaVersion": SCHEMA_VERSION,
                    "workspaceId": workspace_id,
                    "force": True,
                    "expectedSourceStateDigest": source_state_digest,
                },
            )
            closed = close.get("removed") is True and close.get("sourceStateDigest") == source_state_digest
            if not closed:
                raise DemoError("compare-and-close did not preserve the reviewed source digest")
            events.append(event("CLOSE", f"exact state matched  {short(source_state_digest)}"))

            receipt: dict[str, Any] = {
                "schemaVersion": 1,
                "kind": "ordivon-runtime-demo-receipt",
                "status": "passed",
                "protocolVersion": MODERN_PROTOCOL_VERSION,
                "toolCatalogDigest": catalog_digest,
                "source": {"revision": revision},
                "workspace": {
                    "workspaceId": workspace_id,
                    "sourceRevision": revision,
                    "sourceStateDigest": source_state_digest,
                    "closed": True,
                },
                "patch": {
                    "clientRequestId": patch_request_id,
                    "operationId": patched.get("operationId"),
                    "requestDigest": patched.get("requestDigest"),
                    "replayed": patched.get("replayed"),
                    "files": ["policy.py"],
                },
                "execution": {
                    "clientRequestId": execution_request_id,
                    "jobId": job_id,
                    "attemptId": attempt_id,
                    "sameJobAfterReplay": True,
                    "recoveredByTaskList": True,
                    "status": terminal.get("status"),
                    "exitCode": terminal.get("exitCode"),
                    "elapsedMs": terminal.get("elapsedMs"),
                    "observations": observations,
                },
                "evidence": selected_terminal_evidence(
                    evidence_document,
                    str(artifact["digest"]),
                    artifact_id,
                ),
                "diff": {
                    "digest": diff.get("digest"),
                    "changedPaths": changed_paths,
                    "modifiedPaths": diff.get("modifiedPaths", []),
                    "truncated": diff.get("truncated", False),
                },
                "close": {
                    "removed": close.get("removed"),
                    "sourceStateDigest": close.get("sourceStateDigest"),
                    "exactStateMatched": True,
                },
                "presentation": events,
            }
            assert_no_secrets(receipt, (token, str(source), str(repo)))
            receipt_path.parent.mkdir(parents=True, exist_ok=True)
            receipt_path.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n", encoding="utf-8")
            return receipt
        except Exception:
            if not closed:
                cleanup(client, workspace_id, execution_request_id)
            raise


def parse_args() -> argparse.Namespace:
    repo = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=repo)
    parser.add_argument("--fixture", type=Path, default=repo / "examples/runtime-demo")
    parser.add_argument("--env-file", type=Path, default=Path("/etc/ordivon/ordivon-runtime.env"))
    parser.add_argument("--endpoint")
    parser.add_argument("--receipt", type=Path, required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        receipt = run_demo(
            repo=args.repo.expanduser().resolve(),
            fixture=args.fixture.expanduser().resolve(),
            env_file=args.env_file.expanduser().resolve(),
            endpoint_override=args.endpoint,
            receipt_path=args.receipt.expanduser().resolve(),
        )
    except (DemoError, McpProbeError, OSError, subprocess.SubprocessError, ValueError, json.JSONDecodeError) as error:
        print(f"demo failed: {error}", file=sys.stderr)
        return 1
    print(f"RECEIPT    {args.receipt}  {sha256_bytes(json.dumps(receipt, sort_keys=True).encode('utf-8'))}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

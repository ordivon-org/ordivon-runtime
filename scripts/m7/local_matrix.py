#!/usr/bin/env python3
"""Run the local M6/M7 compatibility, hardening, and lifecycle matrix."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import pwd
import grp
import re
import shutil
import subprocess
import time
from pathlib import Path
from typing import Any

REPO = Path(__file__).resolve().parents[2]
SUMMARY = re.compile(
    r"test result: ok\. (?P<passed>\d+) passed; (?P<failed>\d+) failed; "
    r"(?P<ignored>\d+) ignored;"
)
TEST_OK = re.compile(r"^test (?P<name>\S+) \.\.\. ok$", re.MULTILINE)


def sha256_bytes(data: bytes) -> str:
    return "sha256:" + hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def run_command(item_id: str, command: list[str], env: dict[str, str] | None = None) -> dict[str, Any]:
    started = time.monotonic_ns()
    result = subprocess.run(
        command,
        cwd=REPO,
        env=env,
        text=True,
        capture_output=True,
        check=False,
    )
    elapsed_ms = (time.monotonic_ns() - started) // 1_000_000
    combined = result.stdout + result.stderr
    summaries = [
        {key: int(value) for key, value in match.groupdict().items()}
        for match in SUMMARY.finditer(combined)
    ]
    record = {
        "id": item_id,
        "command": command,
        "exitCode": result.returncode,
        "elapsedMs": elapsed_ms,
        "stdoutDigest": sha256_bytes(result.stdout.encode()),
        "stderrDigest": sha256_bytes(result.stderr.encode()),
        "summaries": summaries,
        "passedTests": TEST_OK.findall(combined),
    }
    if result.returncode != 0:
        print(result.stdout)
        print(result.stderr)
        raise RuntimeError(f"{item_id} failed with exit {result.returncode}")
    return record


def git_head() -> str:
    return subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=REPO, text=True).strip()


def systemd_version() -> str:
    return subprocess.check_output(["systemd", "--version"], text=True).splitlines()[0]


def active_units() -> list[str]:
    result = subprocess.run(
        ["systemctl", "list-units", "--all", "--plain", "--no-legend", "ordivon-m6-*"],
        text=True,
        capture_output=True,
        check=False,
    )
    units = []
    for line in result.stdout.splitlines():
        fields = line.split()
        if fields and fields[0].endswith(".service"):
            units.append(fields[0])
    return sorted(units)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    worker = pwd.getpwnam("ordivon-worker")
    group = grp.getgrnam("ordivon-worker")
    commands: list[dict[str, Any]] = []

    commands.append(run_command("workspace-default-tests", ["cargo", "test", "--workspace"]))
    commands.append(
        run_command(
            "m6-feature-tests",
            ["cargo", "test", "-p", "ordivon-exec", "--features", "transactional-registry-m6"],
        )
    )
    commands.append(
        run_command(
            "m7-feature-tests",
            ["cargo", "test", "-p", "ordivon-exec", "--features", "runtime-hardening-m7"],
        )
    )
    commands.append(
        run_command(
            "m7-mcp-tests",
            ["cargo", "test", "-p", "ordivon-mcp", "--features", "experimental-http-m7"],
        )
    )

    commands.append(
        run_command(
            "build-m7-runner",
            [
                "cargo", "build", "-p", "ordivon-exec", "--features", "runtime-hardening-m7",
                "--bin", "ordivon-task-runner",
            ],
        )
    )
    runner_source = REPO / "target/debug/ordivon-task-runner"
    runner_installed = Path("/usr/lib/ordivon/ordivon-task-runner")
    runner_installed.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(runner_source, runner_installed)
    runner_installed.chmod(0o555)

    m6_env = os.environ.copy()
    m6_env.update(
        {
            "ORDIVON_RUN_M6_INTEGRATION": "1",
            "ORDIVON_M6_RUNNER_PATH": str(runner_source),
        }
    )
    commands.append(
        run_command(
            "m6-systemd-matrix",
            [
                "cargo", "test", "-p", "ordivon-exec", "--features", "transactional-registry-m6",
                "--test", "m6_transactional_runtime", "--", "--ignored", "--test-threads=1",
            ],
            m6_env,
        )
    )
    m7_env = os.environ.copy()
    m7_env.update(
        {
            "ORDIVON_RUN_M7_INTEGRATION": "1",
            "ORDIVON_M7_RUNNER_PATH": str(runner_installed),
        }
    )
    commands.append(
        run_command(
            "m7-systemd-matrix",
            [
                "cargo", "test", "-p", "ordivon-exec", "--features", "runtime-hardening-m7",
                "--test", "m7_runtime_hardening", "--", "--ignored", "--test-threads=1",
            ],
            m7_env,
        )
    )

    commands.append(
        run_command(
            "default-strict-clippy",
            ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
        )
    )
    commands.append(
        run_command(
            "m7-exec-strict-clippy",
            [
                "cargo", "clippy", "-p", "ordivon-exec", "--features", "runtime-hardening-m7",
                "--all-targets", "--", "-D", "warnings",
            ],
        )
    )
    commands.append(
        run_command(
            "m7-mcp-strict-clippy",
            [
                "cargo", "clippy", "-p", "ordivon-mcp", "--features", "experimental-http-m7",
                "--all-targets", "--", "-D", "warnings",
            ],
        )
    )

    expected = {
        "m6SystemdTests": [
            "m6_ambiguous_dispatch_is_lost_without_automatic_redispatch",
            "m6_cancel_intent_survives_runtime_reconstruction_and_cleans_cgroup",
            "m6_core_restart_recovers_running_attempt_and_terminal_result",
            "m6_corrupt_runner_result_is_orphaned_and_quarantined",
            "m6_fast_failures_never_race_into_lost",
            "m6_live_unit_without_launch_token_is_orphaned_and_holds_capacity",
            "m6_reconciler_rebuilds_bundle_after_admission_commit",
            "m6_transactional_runtime_executes_replays_and_releases_capacity",
        ],
        "m7SystemdTests": [
            "active_unit_has_minimal_capabilities_and_cancel_cleans_worker_descendants",
            "orphan_remediation_holds_live_tree_then_terminates_and_releases",
            "payload_runs_as_worker_and_cannot_cross_trust_boundaries",
            "reused_unit_is_not_terminated_while_original_reservation_is_released",
        ],
        "lifecycleTests": [
            "m7::tests::backup_is_blocked_while_capacity_is_active",
            "m7::tests::investigation_hold_blocks_gc_then_tombstones_artifacts",
            "m7::tests::online_backup_and_empty_root_restore_preserve_registry_and_bundles",
            "m7::tests::quota_rejection_is_atomic_and_auditable",
        ],
    }
    lookup = {item["id"]: item for item in commands}
    m6_names = sorted(lookup["m6-systemd-matrix"]["passedTests"])
    m7_names = sorted(lookup["m7-systemd-matrix"]["passedTests"])
    feature_names = set(lookup["m7-feature-tests"]["passedTests"])
    gates = {
        "allCommandsPassed": all(item["exitCode"] == 0 for item in commands),
        "m6SystemdMatrixComplete": m6_names == sorted(expected["m6SystemdTests"]),
        "m7SystemdMatrixComplete": m7_names == sorted(expected["m7SystemdTests"]),
        "lifecycleMatrixComplete": set(expected["lifecycleTests"]).issubset(feature_names),
        "staticWorkerIdentity": worker.pw_uid == group.gr_gid and worker.pw_shell == "/usr/bin/nologin",
        "runnerRootOwnedReadOnly": runner_installed.stat().st_uid == 0
        and runner_installed.stat().st_gid == 0
        and (runner_installed.stat().st_mode & 0o777) == 0o555,
        "noResidualAttemptUnits": active_units() == [],
    }
    evidence = {
        "schemaVersion": 1,
        "phase": "ORDIVON-MIGRATION-M7-LOCAL-MATRIX-2026-07-22",
        "generatedAt": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "sourceRevision": git_head(),
        "environment": {
            "kernel": subprocess.check_output(["uname", "-r"], text=True).strip(),
            "architecture": subprocess.check_output(["uname", "-m"], text=True).strip(),
            "bootId": Path("/proc/sys/kernel/random/boot_id").read_text().strip(),
            "systemdVersion": systemd_version(),
            "worker": {
                "user": worker.pw_name,
                "uid": worker.pw_uid,
                "gid": worker.pw_gid,
                "groupGid": group.gr_gid,
                "home": worker.pw_dir,
                "shell": worker.pw_shell,
            },
            "runnerPath": str(runner_installed),
            "runnerDigest": sha256_file(runner_installed),
        },
        "commands": commands,
        "expected": expected,
        "gates": gates,
        "passed": all(gates.values()),
        "claimsNotMade": [
            "Local systemd integration tests do not establish production reliability.",
            "The matrix does not authorize remote access, credentials, deployment, push, merge, or external side effects.",
            "A static worker and local mount namespaces do not protect against a compromised kernel or root control plane.",
        ],
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(evidence, indent=2) + "\n")
    if not evidence["passed"]:
        raise RuntimeError(f"local matrix gates failed: {gates}")
    print(f"M7_LOCAL_MATRIX_PASS {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

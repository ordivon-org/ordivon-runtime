#!/usr/bin/env python3
"""Emit a machine-readable final stage state snapshot."""

from __future__ import annotations

import argparse
import json
import subprocess
from pathlib import Path
from typing import Any


def run(args: list[str], cwd: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        args,
        cwd=cwd,
        text=True,
        capture_output=True,
        check=False,
    )


def lines(result: subprocess.CompletedProcess[str]) -> list[str]:
    return [line for line in result.stdout.splitlines() if line.strip()]


def git_value(repo: Path, *args: str) -> str:
    result = run(["git", *args], repo)
    if result.returncode != 0:
        return ""
    return result.stdout.strip()


def collect_units(repo: Path, patterns: list[str]) -> list[str]:
    command = ["systemctl", "list-units", "--all", "--no-legend", "--plain", *patterns]
    return lines(run(command, repo))


def collect_processes(repo: Path, pattern: str) -> list[str]:
    if not pattern:
        return []
    return lines(run(["pgrep", "-af", pattern], repo))


def collect_listeners(repo: Path, pattern: str) -> list[str]:
    output = lines(run(["ss", "-ltnp"], repo))
    if not pattern:
        return output
    return [line for line in output if pattern in line]


def collect_worktrees(repo: Path, pattern: str) -> list[str]:
    output = lines(run(["git", "worktree", "list", "--porcelain"], repo))
    if not pattern:
        return output
    return [line for line in output if pattern in line]


def load_receipt(path: Path | None) -> dict[str, Any] | None:
    if path is None or not path.is_file():
        return None
    data = json.loads(path.read_text())
    return {
        "path": str(path),
        "stageId": data.get("stage_id"),
        "status": data.get("status"),
        "measuredImplementationCommit": data.get("measured_implementation_commit"),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo-root", type=Path, default=Path.cwd())
    parser.add_argument("--unit-pattern", action="append", default=[])
    parser.add_argument("--process-pattern", default="")
    parser.add_argument("--listener-pattern", default="")
    parser.add_argument("--worktree-pattern", default="")
    parser.add_argument("--state-root", action="append", type=Path, default=[])
    parser.add_argument("--receipt", type=Path)
    parser.add_argument("--require-clean", action="store_true")
    args = parser.parse_args()

    repo = args.repo_root.resolve()
    status = git_value(repo, "status", "--porcelain")
    units = collect_units(repo, args.unit_pattern)
    processes = collect_processes(repo, args.process_pattern)
    listeners = collect_listeners(repo, args.listener_pattern)
    worktrees = collect_worktrees(repo, args.worktree_pattern)
    state_roots = [str(path) for path in args.state_root if path.exists()]
    clean = not status and not units and not processes and not listeners and not worktrees and not state_roots

    snapshot = {
        "schemaVersion": 1,
        "branch": git_value(repo, "branch", "--show-current"),
        "head": git_value(repo, "rev-parse", "HEAD"),
        "worktreeStatus": status.splitlines() if status else [],
        "units": units,
        "processes": processes,
        "listeners": listeners,
        "matchingWorktrees": worktrees,
        "existingStateRoots": state_roots,
        "receipt": load_receipt(args.receipt),
        "clean": clean,
    }
    print(json.dumps(snapshot, indent=2))
    return 1 if args.require_clean and not clean else 0


if __name__ == "__main__":
    raise SystemExit(main())

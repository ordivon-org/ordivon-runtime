#!/usr/bin/env python3
"""Structured Git operations kept outside the Ordivon Runtime kernel."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path
from typing import Any


def git(repo: Path, args: list[str], *, check: bool = True) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(["git", "-C", str(repo), *args], text=True, capture_output=True, check=False)
    if check and result.returncode != 0:
        raise ValueError(result.stderr.strip() or f"git {' '.join(args)} failed")
    return result


def status(repo: Path) -> dict[str, Any]:
    commit = git(repo, ["rev-parse", "HEAD"]).stdout.strip()
    branch_result = git(repo, ["symbolic-ref", "--quiet", "--short", "HEAD"], check=False)
    branch = branch_result.stdout.strip() if branch_result.returncode == 0 else None
    porcelain = git(repo, ["status", "--porcelain=v1", "-z", "--untracked-files=all"]).stdout
    files = []
    for record in porcelain.split("\0"):
        if not record:
            continue
        code, path = record[:2], record[3:]
        files.append({"code": code, "path": path})
    return {
        "schemaVersion": 1,
        "repository": str(repo),
        "commit": commit,
        "headMode": "branch" if branch else "detached",
        "branch": branch,
        "dirty": bool(files),
        "files": files,
    }


def validate_publish_ref(value: str) -> None:
    if not value.startswith("refs/heads/") or any(character.isspace() for character in value):
        raise ValueError("publish ref must be a whitespace-free refs/heads/* name")
    result = subprocess.run(
        ["git", "check-ref-format", value],
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        raise ValueError("publish ref is not a valid Git reference")


def publish(repo: Path, remote: str, ref: str) -> dict[str, Any]:
    validate_publish_ref(ref)
    if not remote or any(character.isspace() for character in remote):
        raise ValueError("remote must be a non-empty name without whitespace")
    before = status(repo)
    if before["dirty"]:
        raise ValueError("repository is dirty; commit or discard changes before publishing HEAD")
    pushed = git(repo, ["push", "--porcelain", remote, f"HEAD:{ref}"])
    return {
        "schemaVersion": 1,
        "repository": str(repo),
        "commit": before["commit"],
        "remote": remote,
        "publishRef": ref,
        "output": pushed.stdout.strip(),
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    status_parser = subparsers.add_parser("status")
    status_parser.add_argument("--repo", required=True, type=Path)
    publish_parser = subparsers.add_parser("publish")
    publish_parser.add_argument("--repo", required=True, type=Path)
    publish_parser.add_argument("--remote", required=True)
    publish_parser.add_argument("--ref", required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    repo = args.repo.expanduser().resolve()
    if not repo.is_dir():
        raise ValueError(f"repository is not a directory: {repo}")
    output = status(repo) if args.command == "status" else publish(repo, args.remote, args.ref)
    print(json.dumps(output, indent=2, sort_keys=True, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError) as error:
        print(f"Git skill failed: {error}", file=sys.stderr)
        raise SystemExit(2) from error

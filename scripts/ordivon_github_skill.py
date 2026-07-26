#!/usr/bin/env python3
"""Structured GitHub PR and CI operations backed by the gh CLI."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import time
from typing import Any

PR_FIELDS = "number,url,state,headRefName,baseRefName,mergeStateStatus,statusCheckRollup"
CHECK_FIELDS = "name,state,link,bucket,workflow"
RUN_FIELDS = "databaseId,status,conclusion,url,workflowName,jobs"


def gh(args: list[str]) -> Any:
    result = subprocess.run(["gh", *args], text=True, capture_output=True, check=False)
    if result.returncode != 0:
        raise ValueError(result.stderr.strip() or f"gh {' '.join(args)} failed")
    text = result.stdout.strip()
    return json.loads(text) if text else {}


def pr_view_command(repo: str, pr: str) -> list[str]:
    return ["pr", "view", pr, "--repo", repo, "--json", PR_FIELDS]


def pr_checks_command(repo: str, pr: str) -> list[str]:
    return ["pr", "checks", pr, "--repo", repo, "--json", CHECK_FIELDS]


def summarize_checks(checks: list[dict[str, Any]]) -> dict[str, Any]:
    pending_buckets = {"pending"}
    failing_buckets = {"fail", "cancel"}
    pending = [check for check in checks if str(check.get("bucket", "")).lower() in pending_buckets]
    failing = [check for check in checks if str(check.get("bucket", "")).lower() in failing_buckets]
    return {
        "schemaVersion": 1,
        "status": "failed" if failing else "pending" if pending else "passed",
        "summary": {"total": len(checks), "pending": len(pending), "failed": len(failing)},
        "checks": checks,
    }


def await_checks(repo: str, pr: str, timeout_seconds: int, interval_seconds: float) -> dict[str, Any]:
    deadline = time.monotonic() + timeout_seconds
    while True:
        summary = summarize_checks(gh(pr_checks_command(repo, pr)))
        if summary["status"] != "pending":
            return summary
        if time.monotonic() >= deadline:
            summary["status"] = "timed_out"
            return summary
        time.sleep(interval_seconds)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    for name in ("pr-view", "pr-checks", "await-checks"):
        sub = subparsers.add_parser(name)
        sub.add_argument("--repo", required=True)
        sub.add_argument("--pr", required=True)
        if name == "await-checks":
            sub.add_argument("--timeout-seconds", type=int, default=900)
            sub.add_argument("--interval-seconds", type=float, default=15)
    run = subparsers.add_parser("run-view")
    run.add_argument("--repo", required=True)
    run.add_argument("--run", required=True)
    merge = subparsers.add_parser("pr-merge")
    merge.add_argument("--repo", required=True)
    merge.add_argument("--pr", required=True)
    merge.add_argument("--method", choices=("merge", "squash", "rebase"), default="squash")
    merge.add_argument("--confirm", required=True, help="must be exactly MERGE")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.command == "pr-view":
        output = gh(pr_view_command(args.repo, args.pr))
    elif args.command == "pr-checks":
        output = summarize_checks(gh(pr_checks_command(args.repo, args.pr)))
    elif args.command == "await-checks":
        if args.timeout_seconds < 0 or args.timeout_seconds > 86_400 or args.interval_seconds < 1:
            raise ValueError("invalid check waiting bounds")
        output = await_checks(args.repo, args.pr, args.timeout_seconds, args.interval_seconds)
    elif args.command == "run-view":
        output = gh(["run", "view", args.run, "--repo", args.repo, "--json", RUN_FIELDS])
    else:
        if args.confirm != "MERGE":
            raise ValueError("pr-merge requires --confirm MERGE")
        output = gh(["pr", "merge", args.pr, "--repo", args.repo, f"--{args.method}"])
    print(json.dumps(output, indent=2, sort_keys=True, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"GitHub skill failed: {error}", file=sys.stderr)
        raise SystemExit(2) from error

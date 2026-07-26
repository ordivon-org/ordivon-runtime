#!/usr/bin/env python3
"""Prepare and grade version-bound Ordivon Runtime coding-agent regressions."""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import tempfile
import time
from pathlib import Path
from typing import Any

DEFAULT_SUITE = Path("evals/runtime-agent/suite.json")


def command(
    argv: list[str],
    cwd: Path,
    *,
    check: bool,
    env: dict[str, str] | None = None,
    timeout: int | None = None,
) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        argv,
        cwd=cwd,
        check=check,
        text=True,
        capture_output=True,
        env=env,
        timeout=timeout,
    )


def load_suite(path: Path) -> dict[str, Any]:
    suite = json.loads(path.read_text(encoding="utf-8"))
    if suite.get("schemaVersion") != 1:
        raise ValueError("suite schemaVersion must equal 1")
    tasks = suite.get("tasks")
    if not isinstance(tasks, list) or not tasks:
        raise ValueError("suite must contain tasks")
    if suite.get("taskCount") != len(tasks):
        raise ValueError("taskCount does not match tasks")
    identifiers = [task.get("id") for task in tasks]
    if any(not isinstance(identifier, str) or not identifier for identifier in identifiers):
        raise ValueError("every task requires a non-empty id")
    if len(set(identifiers)) != len(identifiers):
        raise ValueError("task ids must be unique")
    for task in tasks:
        mutation = task.get("mutation", {})
        grader = task.get("grader", {})
        for field in ("path", "before", "after"):
            if not isinstance(mutation.get(field), str) or not mutation[field]:
                raise ValueError(f"{task['id']} mutation.{field} is required")
        if mutation["before"] == mutation["after"]:
            raise ValueError(f"{task['id']} mutation must change the source")
        if not isinstance(task.get("instruction"), str) or not task["instruction"].strip():
            raise ValueError(f"{task['id']} instruction is required")
        if not isinstance(grader.get("argv"), list) or not grader["argv"]:
            raise ValueError(f"{task['id']} grader.argv is required")
        if not all(isinstance(value, str) and value for value in grader["argv"]):
            raise ValueError(f"{task['id']} grader.argv must contain strings")
        if not isinstance(grader.get("timeoutSeconds"), int) or grader["timeoutSeconds"] <= 0:
            raise ValueError(f"{task['id']} grader.timeoutSeconds must be positive")
    return suite


def task_by_id(suite: dict[str, Any], task_id: str) -> dict[str, Any]:
    for task in suite["tasks"]:
        if task["id"] == task_id:
            return task
    raise ValueError(f"unknown task: {task_id}")


def replace_exact(path: Path, before: str, after: str) -> None:
    content = path.read_text(encoding="utf-8")
    count = content.count(before)
    if count != 1:
        raise ValueError(f"expected exactly one mutation anchor in {path}, found {count}")
    path.write_text(content.replace(before, after, 1), encoding="utf-8")


def validate_against_tree(repo: Path, suite: dict[str, Any]) -> None:
    for task in suite["tasks"]:
        mutation = task["mutation"]
        path = repo / mutation["path"]
        if not path.is_file():
            raise ValueError(f"{task['id']} target does not exist: {path}")
        count = path.read_text(encoding="utf-8").count(mutation["before"])
        if count != 1:
            raise ValueError(f"{task['id']} expected one source anchor, found {count}")


def resolved_revision(repo: Path, revision: str) -> str:
    return command(["git", "rev-parse", revision], repo, check=True).stdout.strip()


def add_worktree(repo: Path, revision: str) -> tuple[Path, Path]:
    root = Path(tempfile.mkdtemp(prefix="ordivon-agent-eval-"))
    worktree = root / "worktree"
    command(["git", "worktree", "add", "--detach", str(worktree), revision], repo, check=True)
    return root, worktree


def remove_worktree(repo: Path, root: Path, worktree: Path) -> None:
    command(["git", "worktree", "remove", "--force", str(worktree)], repo, check=False)
    shutil.rmtree(root, ignore_errors=True)


def grader_environment(repo: Path, task: dict[str, Any]) -> dict[str, str]:
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(repo / "target/agent-eval")
    env["ORDIVON_EVAL_TASK_ID"] = task["id"]
    env["ORDIVON_EVAL_INSTRUCTION"] = task["instruction"]
    return env


def run_grader(repo: Path, worktree: Path, task: dict[str, Any]) -> dict[str, Any]:
    grader = task["grader"]
    started = time.monotonic()
    result = command(
        grader["argv"],
        worktree,
        check=False,
        env=grader_environment(repo, task),
        timeout=grader["timeoutSeconds"],
    )
    return {
        "exitCode": result.returncode,
        "durationMs": round((time.monotonic() - started) * 1_000),
        "stdoutTail": result.stdout[-8_000:],
        "stderrTail": result.stderr[-8_000:],
    }


def reference_trial(repo: Path, revision: str, task: dict[str, Any]) -> dict[str, Any]:
    root, worktree = add_worktree(repo, revision)
    mutation = task["mutation"]
    try:
        target = worktree / mutation["path"]
        replace_exact(target, mutation["before"], mutation["after"])
        failing = run_grader(repo, worktree, task)
        if failing["exitCode"] == 0:
            raise AssertionError(f"{task['id']} mutation did not fail its grader")
        replace_exact(target, mutation["after"], mutation["before"])
        passing = run_grader(repo, worktree, task)
        if passing["exitCode"] != 0:
            raise AssertionError(
                f"{task['id']} reference repair did not pass: {passing['stderrTail']}"
            )
        return {
            "taskId": task["id"],
            "failToPass": True,
            "failing": failing,
            "passing": passing,
        }
    finally:
        remove_worktree(repo, root, worktree)


def agent_trial(
    repo: Path,
    revision: str,
    task: dict[str, Any],
    agent_argv: list[str],
    keep: bool,
) -> dict[str, Any]:
    root, worktree = add_worktree(repo, revision)
    mutation = task["mutation"]
    target = worktree / mutation["path"]
    replace_exact(target, mutation["before"], mutation["after"])
    instruction = worktree / ".ordivon-eval-instruction.md"
    instruction.write_text(task["instruction"] + "\n", encoding="utf-8")
    env = grader_environment(repo, task)
    env["ORDIVON_EVAL_INSTRUCTION_FILE"] = str(instruction)
    started = time.monotonic()
    agent = command(
        agent_argv,
        worktree,
        check=False,
        env=env,
        timeout=task["grader"]["timeoutSeconds"],
    )
    grade = run_grader(repo, worktree, task)
    report = {
        "taskId": task["id"],
        "agentExitCode": agent.returncode,
        "agentDurationMs": round((time.monotonic() - started) * 1_000),
        "agentStdoutTail": agent.stdout[-8_000:],
        "agentStderrTail": agent.stderr[-8_000:],
        "passed": grade["exitCode"] == 0,
        "grader": grade,
        "worktree": str(worktree) if keep else None,
    }
    if not keep:
        remove_worktree(repo, root, worktree)
    return report


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("--repo", type=Path, default=Path.cwd())
    result.add_argument("--suite", type=Path, default=DEFAULT_SUITE)
    result.add_argument("--revision", default="HEAD")
    sub = result.add_subparsers(dest="command", required=True)
    sub.add_parser("validate")
    sub.add_parser("list")
    reference = sub.add_parser("reference")
    reference.add_argument("--task", default="all")
    run = sub.add_parser("run")
    run.add_argument("--task", required=True)
    run.add_argument("--keep", action="store_true")
    run.add_argument("agent_command", nargs=argparse.REMAINDER)
    return result


def main() -> int:
    args = parser().parse_args()
    repo = args.repo.resolve()
    suite_path = args.suite if args.suite.is_absolute() else repo / args.suite
    suite = load_suite(suite_path)
    validate_against_tree(repo, suite)
    revision = resolved_revision(repo, args.revision)
    if args.command == "validate":
        print(
            json.dumps(
                {
                    "suiteId": suite["suiteId"],
                    "revision": revision,
                    "taskCount": len(suite["tasks"]),
                }
            )
        )
        return 0
    if args.command == "list":
        print(
            json.dumps(
                [
                    {
                        "id": task["id"],
                        "kind": task["kind"],
                        "instruction": task["instruction"],
                    }
                    for task in suite["tasks"]
                ],
                indent=2,
            )
        )
        return 0
    if args.command == "reference":
        tasks = suite["tasks"] if args.task == "all" else [task_by_id(suite, args.task)]
        reports = [reference_trial(repo, revision, task) for task in tasks]
        print(
            json.dumps(
                {"suiteId": suite["suiteId"], "revision": revision, "trials": reports},
                indent=2,
            )
        )
        return 0
    if not args.agent_command:
        raise ValueError("run requires an agent command after --")
    agent_argv = args.agent_command[1:] if args.agent_command[0] == "--" else args.agent_command
    report = agent_trial(repo, revision, task_by_id(suite, args.task), agent_argv, args.keep)
    print(json.dumps(report, indent=2))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Run layered Ordivon acceptance and emit an exact-head JSON manifest."""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import time
from dataclasses import asdict, dataclass
from datetime import datetime, timezone
from pathlib import Path


@dataclass(frozen=True)
class CommandSpec:
    name: str
    argv: tuple[str, ...]
    environment: tuple[tuple[str, str], ...] = ()


@dataclass
class CommandResult:
    name: str
    argv: list[str]
    exit_code: int
    duration_ms: int
    stdout_tail: str
    stderr_tail: str


def command_profiles(repo: Path, runner: Path) -> dict[str, list[CommandSpec]]:
    python = sys.executable
    fast = [
        CommandSpec("cargo-fmt", ("cargo", "fmt", "--check")),
        CommandSpec("cargo-test-workspace", ("cargo", "test", "--workspace")),
        CommandSpec("ruff", ("uvx", "ruff", "check", "scripts/")),
        CommandSpec("tracked-secret-scan", (python, "scripts/tracked_secret_scan.py", "--repo", str(repo))),
    ]
    merge = fast + [
        CommandSpec("cargo-clippy", ("cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings")),
        CommandSpec(
            "transactional-feature",
            ("cargo", "test", "-p", "ordivon-exec", "--no-default-features", "--features", "transactional-runtime"),
        ),
        CommandSpec("isolated-feature", ("cargo", "test", "-p", "ordivon-exec", "--features", "isolated-execution")),
        CommandSpec("python-tests", (python, "-m", "unittest", "discover", "-s", "scripts/tests", "-v")),
        CommandSpec("legacy-reference-scan", (python, "scripts/legacy_reference_scan.py", "--root", str(repo))),
        CommandSpec("mcp-e2e", (python, "scripts/mcp_e2e.py", "--repo", str(repo))),
    ]
    release = merge + [
        CommandSpec(
            "systemd-supervisor-real",
            ("cargo", "test", "-p", "ordivon-exec", "--features", "systemd-supervisor", "--test", "systemd_supervisor", "--", "--ignored", "--nocapture"),
            (("ORDIVON_RUN_SYSTEMD_SPIKE", "1"),),
        ),
        CommandSpec(
            "transactional-runtime-real",
            ("cargo", "test", "-p", "ordivon-exec", "--features", "isolated-execution", "--test", "transactional_runtime", "--", "--ignored", "--nocapture", "--test-threads=1"),
            (("ORDIVON_RUN_INTEGRATION", "1"), ("ORDIVON_RUNNER_PATH", str(runner))),
        ),
        CommandSpec("bounded-benchmark", (python, "scripts/benchmark.py", "--repo", str(repo))),
    ]
    return {"fast": fast, "merge": merge, "release": release}


def tail(value: str, limit: int = 16_384) -> str:
    return value[-limit:]


def git_value(repo: Path, *args: str) -> str:
    return subprocess.run(["git", *args], cwd=repo, check=True, text=True, capture_output=True).stdout.strip()


def preflight(repo: Path, profile: str, runner: Path, release_opt_in: bool) -> None:
    if not (repo / ".git").exists() and not (repo / ".git").is_file():
        raise ValueError(f"not a Git worktree: {repo}")
    dirty = git_value(repo, "status", "--porcelain")
    if dirty:
        raise ValueError("exact-head acceptance requires a clean Git worktree")
    required = {spec.argv[0] for spec in command_profiles(repo, runner)[profile]}
    missing = sorted(program for program in required if shutil.which(program) is None)
    if missing:
        raise ValueError(f"required programs are missing: {', '.join(missing)}")
    if profile == "release":
        if not release_opt_in:
            raise ValueError("release profile requires --release-opt-in")
        if os.geteuid() != 0:
            raise ValueError("release profile requires root")
        if Path("/proc/1/comm").read_text(encoding="utf-8").strip() != "systemd":
            raise ValueError("release profile requires systemd as PID 1")
        cgroup = subprocess.run(
            ["stat", "-fc", "%T", "/sys/fs/cgroup"], check=True, text=True, capture_output=True
        ).stdout.strip()
        if cgroup != "cgroup2fs":
            raise ValueError("release profile requires cgroup v2")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run layered exact-head Ordivon acceptance.")
    parser.add_argument("profile", choices=("fast", "merge", "release"))
    parser.add_argument("--repo", type=Path, default=Path.cwd())
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--list", action="store_true", help="print the command plan without execution")
    parser.add_argument("--release-opt-in", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    repo = args.repo.expanduser().resolve()
    runner = repo / "target/debug/ordivon-task-runner"
    profiles = command_profiles(repo, runner)
    specs = profiles[args.profile]
    if args.list:
        for spec in specs:
            prefix = " ".join(f"{key}={value}" for key, value in spec.environment)
            print((prefix + " " if prefix else "") + " ".join(spec.argv))
        return 0

    preflight(repo, args.profile, runner, args.release_opt_in)
    output_dir = (
        args.output_dir.expanduser().resolve()
        if args.output_dir
        else repo / ".ordivon/acceptance" / datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    )
    output_dir.mkdir(parents=True, exist_ok=False)
    manifest_path = output_dir / "manifest.json"
    manifest: dict[str, object] = {
        "schemaVersion": 1,
        "profile": args.profile,
        "startedAt": datetime.now(timezone.utc).isoformat(),
        "repository": str(repo),
        "head": git_value(repo, "rev-parse", "HEAD"),
        "branch": git_value(repo, "branch", "--show-current"),
        "dirty": bool(git_value(repo, "status", "--porcelain")),
        "commands": [],
        "status": "running",
    }
    manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    results: list[CommandResult] = []
    overall = 0
    for spec in specs:
        started = time.monotonic()
        env = os.environ.copy()
        env.update(dict(spec.environment))
        process = subprocess.run(spec.argv, cwd=repo, env=env, text=True, capture_output=True)
        result = CommandResult(
            name=spec.name,
            argv=list(spec.argv),
            exit_code=process.returncode,
            duration_ms=round((time.monotonic() - started) * 1000),
            stdout_tail=tail(process.stdout),
            stderr_tail=tail(process.stderr),
        )
        results.append(result)
        manifest["commands"] = [asdict(item) for item in results]
        manifest["status"] = "running" if process.returncode == 0 else "failed"
        manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        print(f"[{result.exit_code}] {result.name} ({result.duration_ms} ms)")
        if process.returncode != 0:
            overall = process.returncode or 1
            break
    manifest["finishedAt"] = datetime.now(timezone.utc).isoformat()
    manifest["status"] = "passed" if overall == 0 and len(results) == len(specs) else "failed"
    manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(manifest_path)
    return overall


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"acceptance failed: {error}", file=sys.stderr)
        raise SystemExit(2) from error

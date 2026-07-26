#!/usr/bin/env python3
"""Measure Ordivon Runtime local control-plane latency on one stable host."""

from __future__ import annotations

import argparse
import json
import os
import platform
import statistics
import subprocess
import sys
import time
import uuid
from pathlib import Path
from typing import Any

SCRIPT_ROOT = Path(__file__).resolve().parent
if str(SCRIPT_ROOT) not in sys.path:
    sys.path.insert(0, str(SCRIPT_ROOT))

from mcp_e2e import McpClient, TERMINAL  # noqa: E402

SCHEMA_VERSION = 1
DEFAULT_ENDPOINT = "http://127.0.0.1:8897/mcp"
DEFAULT_THRESHOLD = 0.20


def percentile(values: list[float], quantile: float) -> float:
    ordered = sorted(values)
    index = min(len(ordered) - 1, max(0, round((len(ordered) - 1) * quantile)))
    return ordered[index]


def summarize(values: list[float]) -> dict[str, float | int]:
    if not values:
        raise ValueError("cannot summarize an empty sample")
    return {
        "n": len(values),
        "minMs": round(min(values), 3),
        "p50Ms": round(statistics.median(values), 3),
        "meanMs": round(statistics.mean(values), 3),
        "p90Ms": round(percentile(values, 0.90), 3),
        "p95Ms": round(percentile(values, 0.95), 3),
        "maxMs": round(max(values), 3),
        "stdevMs": round(statistics.pstdev(values), 3),
    }


def compare_results(
    current: dict[str, dict[str, float | int]],
    baseline: dict[str, dict[str, float | int]],
    threshold: float,
) -> list[dict[str, Any]]:
    regressions: list[dict[str, Any]] = []
    for scenario, current_stats in sorted(current.items()):
        baseline_stats = baseline.get(scenario)
        if not baseline_stats:
            continue
        for metric in ("p50Ms", "p95Ms"):
            before = float(baseline_stats[metric])
            after = float(current_stats[metric])
            if before <= 0:
                continue
            ratio = after / before - 1.0
            if ratio > threshold:
                regressions.append(
                    {
                        "scenario": scenario,
                        "metric": metric,
                        "baselineMs": before,
                        "currentMs": after,
                        "regressionRatio": round(ratio, 4),
                    }
                )
    return regressions


def elapsed_ms(started_ns: int) -> float:
    return (time.perf_counter_ns() - started_ns) / 1_000_000


def run_direct(argv: list[str], samples: int, cwd: Path | None = None) -> list[float]:
    values: list[float] = []
    for _ in range(samples):
        started = time.perf_counter_ns()
        subprocess.run(argv, cwd=cwd, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        values.append(elapsed_ms(started))
    return values


def call(client: McpClient, name: str, arguments: dict[str, Any]) -> dict[str, Any]:
    return client.tool(name, arguments)


def wait_terminal(client: McpClient, job_id: str) -> dict[str, Any]:
    observation: dict[str, Any] = {"status": "working"}
    while observation.get("status") not in TERMINAL:
        observation = call(
            client,
            "task.observe",
            {
                "schemaVersion": SCHEMA_VERSION,
                "jobId": job_id,
                "waitMs": 1_000,
                "stdoutTailBytes": 0,
                "stderrTailBytes": 0,
            },
        )
    return observation


def execution(
    workspace_id: str,
    executable: str,
    args: list[str],
    request_id: str,
    wait_ms: int,
) -> dict[str, Any]:
    return {
        "schemaVersion": SCHEMA_VERSION,
        "clientRequestId": request_id,
        "execution": {
            "workspaceId": workspace_id,
            "cwdRelative": ".",
            "executable": executable,
            "args": args,
            "env": {},
            "timeoutMs": 30_000,
            "stdoutLimitBytes": 4_096,
            "stderrLimitBytes": 4_096,
        },
        "waitMs": wait_ms,
        "stdoutTailBytes": 0,
        "stderrTailBytes": 0,
    }


def benchmark(args: argparse.Namespace) -> dict[str, Any]:
    token = os.environ.get(args.token_env)
    if not token:
        raise ValueError(f"{args.token_env} is required")
    source_repo = args.source_repo.resolve()
    client = McpClient(args.endpoint, token, timeout=45)
    for _ in range(3):
        client.request("tools/list")

    samples = args.samples
    results: dict[str, dict[str, float | int]] = {}
    results["directTrue"] = summarize(run_direct(["/usr/bin/true"], samples * 2))
    results["directBashC"] = summarize(run_direct(["/usr/bin/bash", "-c", "true"], samples))
    results["directBashLC"] = summarize(run_direct(["/usr/bin/bash", "-lc", "true"], samples))
    results["directGitStatus"] = summarize(
        run_direct(["/usr/bin/git", "status", "--short"], samples, source_repo)
    )
    if args.include_systemd:
        systemd: list[float] = []
        for _ in range(samples):
            unit = f"ordivon-benchmark-{uuid.uuid4().hex[:12]}"
            started = time.perf_counter_ns()
            subprocess.run(
                [
                    "/usr/bin/systemd-run",
                    "--quiet",
                    "--wait",
                    "--collect",
                    f"--unit={unit}",
                    "/usr/bin/true",
                ],
                check=True,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            systemd.append(elapsed_ms(started))
        results["directSystemdTrue"] = summarize(systemd)

    tools_list: list[float] = []
    for _ in range(samples):
        started = time.perf_counter_ns()
        client.request("tools/list")
        tools_list.append(elapsed_ms(started))
    results["mcpToolsList"] = summarize(tools_list)

    run_id = uuid.uuid4().hex[:10]
    workspace_id = f"ordivon-benchmark-{run_id}"
    call(
        client,
        "workspace.open",
        {
            "schemaVersion": SCHEMA_VERSION,
            "sourceRepo": str(source_repo),
            "sourceRevision": "HEAD",
            "workspaceId": workspace_id,
        },
    )
    try:
        reads: list[float] = []
        for _ in range(samples):
            started = time.perf_counter_ns()
            call(
                client,
                "workspace.read",
                {
                    "schemaVersion": SCHEMA_VERSION,
                    "workspaceId": workspace_id,
                    "relativePath": "README.md",
                    "mode": "SLICE",
                    "offset": 0,
                    "maxBytes": 64,
                },
            )
            reads.append(elapsed_ms(started))
        results["workspaceRead64B"] = summarize(reads)

        diffs: list[float] = []
        for _ in range(samples):
            started = time.perf_counter_ns()
            call(
                client,
                "workspace.diff",
                {"schemaVersion": SCHEMA_VERSION, "workspaceId": workspace_id, "maxBytes": 4_096},
            )
            diffs.append(elapsed_ms(started))
        results["workspaceDiffClean"] = summarize(diffs)

        replay_request = execution(
            workspace_id,
            "/usr/bin/true",
            [],
            f"benchmark-replay-{run_id}",
            30_000,
        )
        first = call(client, "workspace.exec", replay_request)
        replays: list[float] = []
        for _ in range(samples):
            started = time.perf_counter_ns()
            repeated = call(client, "workspace.exec", replay_request)
            replays.append(elapsed_ms(started))
            if repeated.get("jobId") != first.get("jobId"):
                raise AssertionError("idempotent replay returned a different Job")
        results["workspaceExecReplay"] = summarize(replays)

        true_jobs: list[float] = []
        for index in range(samples):
            request = execution(
                workspace_id,
                "/usr/bin/true",
                [],
                f"benchmark-true-{run_id}-{index}",
                30_000,
            )
            started = time.perf_counter_ns()
            result = call(client, "workspace.exec", request)
            true_jobs.append(elapsed_ms(started))
            if result.get("status") != "succeeded":
                raise AssertionError(result)
        results["workspaceExecTrue"] = summarize(true_jobs)

        sleep_jobs: list[float] = []
        for index in range(min(samples, 3)):
            request = execution(
                workspace_id,
                "/usr/bin/sleep",
                ["1"],
                f"benchmark-sleep-{run_id}-{index}",
                5_000,
            )
            started = time.perf_counter_ns()
            result = call(client, "workspace.exec", request)
            sleep_jobs.append(elapsed_ms(started))
            if result.get("status") != "succeeded":
                raise AssertionError(result)
        results["workspaceExecSleep1s"] = summarize(sleep_jobs)

        batch_command = (
            "git status --short >/dev/null; "
            "git rev-parse HEAD >/dev/null; "
            "git branch --show-current >/dev/null; "
            "git diff --stat >/dev/null; "
            "rg -n Runtime README.md >/dev/null"
        )
        batches: list[float] = []
        for index in range(samples):
            request = execution(
                workspace_id,
                "/usr/bin/bash",
                ["-c", batch_command],
                f"benchmark-batch-{run_id}-{index}",
                30_000,
            )
            started = time.perf_counter_ns()
            result = call(client, "workspace.exec", request)
            batches.append(elapsed_ms(started))
            if result.get("status") != "succeeded":
                raise AssertionError(result)
        results["workspaceExecFiveChecksBatched"] = summarize(batches)
    finally:
        call(
            client,
            "workspace.close",
            {"schemaVersion": SCHEMA_VERSION, "workspaceId": workspace_id, "force": True},
        )

    report: dict[str, Any] = {
        "schemaVersion": 1,
        "observedUnixMs": int(time.time() * 1_000),
        "environment": {
            "hostname": platform.node(),
            "kernel": platform.release(),
            "python": platform.python_version(),
            "endpoint": args.endpoint,
            "sourceRepo": str(source_repo),
            "sourceRevision": subprocess.check_output(
                ["git", "-C", str(source_repo), "rev-parse", "HEAD"], text=True
            ).strip(),
        },
        "samples": samples,
        "results": results,
    }
    if args.compare:
        baseline = json.loads(args.compare.read_text(encoding="utf-8"))
        report["comparison"] = {
            "baseline": str(args.compare),
            "threshold": args.regression_threshold,
            "regressions": compare_results(
                results,
                baseline["results"],
                args.regression_threshold,
            ),
        }
    return report


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("--endpoint", default=DEFAULT_ENDPOINT)
    result.add_argument("--token-env", default="ORDIVON_BEARER_TOKEN")
    result.add_argument("--source-repo", type=Path, default=Path.cwd())
    result.add_argument("--samples", type=int, default=10)
    result.add_argument("--include-systemd", action=argparse.BooleanOptionalAction, default=True)
    result.add_argument("--output", type=Path)
    result.add_argument("--compare", type=Path)
    result.add_argument("--regression-threshold", type=float, default=DEFAULT_THRESHOLD)
    return result


def main() -> int:
    args = parser().parse_args()
    if args.samples < 3:
        raise ValueError("--samples must be at least 3")
    if args.regression_threshold < 0:
        raise ValueError("--regression-threshold must be non-negative")
    report = benchmark(args)
    rendered = json.dumps(report, indent=2, sort_keys=True) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(rendered, encoding="utf-8")
    sys.stdout.write(rendered)
    regressions = report.get("comparison", {}).get("regressions", [])
    return 1 if regressions else 0


if __name__ == "__main__":
    raise SystemExit(main())

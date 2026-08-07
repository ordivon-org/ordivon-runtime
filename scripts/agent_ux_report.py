#!/usr/bin/env python3
from __future__ import annotations

import argparse
from collections import Counter, defaultdict
import json
from pathlib import Path
import sys
import time
from typing import Any, Iterable


def _percentile(values: list[int], quantile: float) -> int | None:
    if not values:
        return None
    ordered = sorted(values)
    index = int((len(ordered) - 1) * quantile)
    return ordered[index]


def _trace_segments(path: Path) -> tuple[Path, ...]:
    rotated = Path(str(path) + ".1")
    return tuple(candidate for candidate in (rotated, path) if candidate.is_file())


def _records(paths: Iterable[Path]) -> tuple[list[dict[str, Any]], int]:
    records: list[dict[str, Any]] = []
    malformed = 0
    for path in paths:
        with path.open("r", encoding="utf-8") as handle:
            for line in handle:
                try:
                    value = json.loads(line)
                except json.JSONDecodeError:
                    malformed += 1
                    continue
                if isinstance(value, dict):
                    records.append(value)
                else:
                    malformed += 1
    return records, malformed


def _client_key(record: dict[str, Any]) -> str | None:
    client = record.get("client")
    if not isinstance(client, dict):
        return None
    name = client.get("name")
    version = client.get("version")
    if not isinstance(name, str) or not name:
        return None
    return f"{name}@{version}" if isinstance(version, str) and version else name


def _counter(values: Iterable[str | None]) -> dict[str, int]:
    return dict(sorted(Counter(value for value in values if value).items()))


def build_report(trace_path: Path, since_ms: int) -> dict[str, Any]:
    if since_ms < 0:
        raise ValueError("since_ms must be non-negative")
    segments = _trace_segments(trace_path)
    if not segments:
        raise FileNotFoundError(f"trace does not exist: {trace_path}")
    records, malformed = _records(segments)
    boundary = [
        record
        for record in records
        if record.get("kind") == "mcp_tool_call_outcome"
        and int(record.get("observedUnixMs", 0)) >= since_ms
    ]
    if boundary:
        source_mode = "mcp_tool_call_outcome"
        effective_since_ms = max(
            since_ms,
            min(
                max(
                    0,
                    int(record.get("observedUnixMs", 0))
                    - int(record.get("totalMs", 0)),
                )
                for record in boundary
            ),
        )
        outcomes = [
            record
            for record in boundary
            if int(record.get("observedUnixMs", 0)) >= effective_since_ms
        ]
        classification_available = True
    else:
        source_mode = "legacy_core"
        effective_since_ms = since_ms
        outcomes = [
            record
            for record in records
            if record.get("kind") is None
            and isinstance(record.get("tool"), str)
            and "ok" in record
            and "coreMs" in record
            and int(record.get("observedUnixMs", 0)) >= effective_since_ms
        ]
        classification_available = False

    protocol_calls = [
        record
        for record in records
        if record.get("kind") == "mcp_protocol_observation"
        and record.get("method") == "tools/call"
        and isinstance(record.get("tool"), str)
        and int(record.get("observedUnixMs", 0)) >= effective_since_ms
    ]
    calls_by_tool = Counter(str(record["tool"]) for record in protocol_calls)
    outcomes_by_tool: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for record in outcomes:
        tool = record.get("tool")
        if isinstance(tool, str):
            outcomes_by_tool[tool].append(record)

    tools: dict[str, Any] = {}
    for tool in sorted(set(calls_by_tool) | set(outcomes_by_tool)):
        tool_outcomes = outcomes_by_tool.get(tool, [])
        durations = [int(record.get("totalMs", 0)) for record in tool_outcomes]
        failed = sum(record.get("ok") is False for record in tool_outcomes)
        calls = calls_by_tool.get(tool, 0)
        outcome_count = len(tool_outcomes)
        tools[tool] = {
            "calls": calls,
            "outcomes": outcome_count,
            "missingOutcomes": max(0, calls - outcome_count),
            "outcomeCoverageBasisPoints": (
                min(10_000, outcome_count * 10_000 // calls) if calls else None
            ),
            "failed": failed,
            "failureRateBasisPoints": (
                failed * 10_000 // outcome_count if outcome_count else None
            ),
            "p50Ms": _percentile(durations, 0.50),
            "p95Ms": _percentile(durations, 0.95),
        }
        if classification_available:
            tools[tool].update(
                {
                    "errorCodes": _counter(record.get("errorCode") for record in tool_outcomes),
                    "errorOrigins": _counter(record.get("errorOrigin") for record in tool_outcomes),
                    "retryClasses": _counter(record.get("retryClass") for record in tool_outcomes),
                    "commitStates": _counter(record.get("commitState") for record in tool_outcomes),
                    "fields": _counter(record.get("field") for record in tool_outcomes),
                }
            )

    outcome_count = len(outcomes)
    protocol_call_count = len(protocol_calls)
    failed_count = sum(record.get("ok") is False for record in outcomes)
    report = {
        "schemaVersion": 1,
        "kind": "ordivon.runtime-agent-ux-report",
        "generatedAtMs": time.time_ns() // 1_000_000,
        "sinceMs": since_ms,
        "effectiveSinceMs": effective_since_ms,
        "sourceMode": source_mode,
        "classificationAvailable": classification_available,
        "traceSegmentCount": len(segments),
        "malformedLineCount": malformed,
        "protocolCallCount": protocol_call_count,
        "outcomeCount": outcome_count,
        "missingOutcomeCount": max(0, protocol_call_count - outcome_count),
        "outcomeCoverageBasisPoints": (
            min(10_000, outcome_count * 10_000 // protocol_call_count)
            if protocol_call_count
            else None
        ),
        "failedOutcomeCount": failed_count,
        "failureRateBasisPoints": (
            failed_count * 10_000 // outcome_count if outcome_count else None
        ),
        "clients": _counter(_client_key(record) for record in protocol_calls),
        "signals": {
            "invalidRequestCount": sum(
                record.get("errorCode") == "INVALID_REQUEST" for record in outcomes
            ),
            "internalErrorCount": sum(
                record.get("errorCode") == "INTERNAL_ERROR" for record in outcomes
            ),
            "protocolErrorCount": sum(
                record.get("outcome") == "protocol_error" for record in outcomes
            ),
            "reconcileFirstCount": sum(
                record.get("retryClass") == "reconcile_first" for record in outcomes
            ),
            "unknownCommitCount": sum(
                record.get("commitState") == "unknown" for record in outcomes
            ),
        },
        "tools": tools,
    }
    return report


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=(
            "Derive Agent-facing Runtime Tool friction from the existing bounded MCP trace. "
            "This report is diagnostic and never becomes Runtime execution authority."
        )
    )
    parser.add_argument("--trace", type=Path, required=True)
    parser.add_argument("--since-ms", type=int, default=0)
    parser.add_argument("--pretty", action="store_true")
    return parser


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    try:
        report = build_report(args.trace, args.since_ms)
    except (OSError, ValueError) as error:
        print(f"runtime Agent UX report: {type(error).__name__}: {error}", file=sys.stderr)
        return 1
    print(
        json.dumps(
            report,
            ensure_ascii=False,
            indent=2 if args.pretty else None,
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

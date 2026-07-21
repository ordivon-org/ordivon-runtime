#!/usr/bin/env python3
"""Validate Ordivon M2 differential benchmark evidence."""

from __future__ import annotations

import argparse
import json
import statistics
import sys
from pathlib import Path
from typing import Any

EXPECTED_PHASE = "ORDIVON-MIGRATION-M2-2026-07-21"
EXPECTED_ITERATIONS = 5
SUMMARY_FIELDS = (
    "elapsedMs",
    "toolCalls",
    "remoteRoundTrips",
    "contextBytes",
    "outputBytes",
)


def fail(message: str) -> None:
    raise ValueError(message)


def median_int(values: list[int]) -> int:
    return round(statistics.median(values))


def require(condition: bool, message: str) -> None:
    if not condition:
        fail(message)


def load(path: Path) -> dict[str, Any]:
    try:
        data = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as error:
        fail(f"cannot read valid JSON evidence: {error}")
    require(isinstance(data, dict), "evidence root must be an object")
    return data


def validate_samples(
    data: dict[str, Any],
) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
    raw = data.get("rawSamples")
    require(isinstance(raw, dict), "rawSamples must be an object")
    legacy = raw.get("legacy")
    ordivon = raw.get("ordivon")
    require(isinstance(legacy, list), "legacy samples must be a list")
    require(isinstance(ordivon, list), "ordivon samples must be a list")
    require(len(legacy) == EXPECTED_ITERATIONS, "expected five legacy samples")
    require(len(ordivon) == EXPECTED_ITERATIONS, "expected five Ordivon samples")

    for index, (left, right) in enumerate(zip(legacy, ordivon, strict=True)):
        require(
            left.get("backend") == "LEGACY_DESKTOP_COMMANDER",
            f"legacy backend {index}",
        )
        require(right.get("backend") == "ORDIVON", f"Ordivon backend {index}")
        require(left.get("pairId") == right.get("pairId"), f"pair ID mismatch {index}")
        require(left.get("succeeded") is True, f"legacy sample failed {index}")
        require(right.get("succeeded") is True, f"Ordivon sample failed {index}")
        require(
            left.get("semanticDigest") == right.get("semanticDigest"),
            f"semantic digest mismatch {index}",
        )
        require(left.get("fallbackCount") == 0, f"legacy fallback present {index}")
        require(right.get("fallbackCount") == 0, f"Ordivon fallback present {index}")
        require(
            right.get("recoveredAfterDisconnect") is True,
            f"Ordivon recovery false {index}",
        )
        transport = left.get("transport", {}).get("task", {})
        require(
            transport.get("httpRequests", 0) >= left.get("toolCalls", 0),
            f"legacy HTTP request count too small {index}",
        )
        require(
            right.get("transport", {}).get("actualHttpRequests") is None,
            f"fabricated Ordivon HTTP metric {index}",
        )
    return legacy, ordivon


def validate_summary(
    data: dict[str, Any],
    legacy: list[dict[str, Any]],
    ordivon: list[dict[str, Any]],
) -> None:
    summaries = data.get("summaries")
    require(isinstance(summaries, dict), "summaries must be an object")
    for name, samples in (("legacy", legacy), ("ordivon", ordivon)):
        summary = summaries.get(name)
        require(isinstance(summary, dict), f"missing {name} summary")
        for field in SUMMARY_FIELDS:
            expected = median_int([int(sample[field]) for sample in samples])
            require(summary.get(field) == expected, f"{name} median mismatch: {field}")
        expected_digests = sorted({sample["semanticDigest"] for sample in samples})
        require(
            summary.get("semanticDigests") == expected_digests,
            f"{name} semantic digest summary mismatch",
        )
        calls = summary.get("callBreakdown")
        require(isinstance(calls, list), f"{name} callBreakdown missing")
        require(
            len(calls) == len(samples[0]["callBreakdown"]),
            f"{name} callBreakdown length mismatch",
        )
        for index, call in enumerate(calls):
            expected = median_int(
                [sample["callBreakdown"][index]["responseBytes"] for sample in samples]
            )
            require(call.get("sequence") == index + 1, f"{name} call sequence mismatch")
            require(call.get("medianResponseBytes") == expected, f"{name} call bytes mismatch")


def reduction_percent(legacy: int, ordivon: int) -> float:
    if legacy == 0:
        return 0.0 if ordivon == 0 else float("-inf")
    return ((legacy - ordivon) / legacy) * 100.0


def validate_cutover(data: dict[str, Any]) -> None:
    summaries = data["summaries"]
    legacy = summaries["legacy"]
    ordivon = summaries["ordivon"]
    gates = {
        "completionNotWorse": (not legacy["succeeded"]) or ordivon["succeeded"],
        "toolCallsReduced30Percent": reduction_percent(
            legacy["toolCalls"], ordivon["toolCalls"]
        )
        >= 30.0,
        "contextReduced50Percent": reduction_percent(
            legacy["contextBytes"], ordivon["contextBytes"]
        )
        >= 50.0,
        "elapsedWithin10Percent": ordivon["elapsedMs"] <= round(legacy["elapsedMs"] * 1.1),
        "disconnectRecovery": ordivon["recoveredAfterDisconnect"],
        "noFallback": ordivon["fallbackCount"] == 0,
        "semanticEquivalence": legacy["semanticDigests"] == ordivon["semanticDigests"],
    }
    cutover = data.get("cutover")
    require(isinstance(cutover, dict), "cutover must be an object")
    require(cutover.get("gates") == gates, "cutover gates do not recompute")
    require(cutover.get("eligible") == all(gates.values()), "cutover eligibility mismatch")
    require(cutover.get("eligible") is False, "M2 must not claim route eligibility")
    require(gates["elapsedWithin10Percent"], "Ordivon elapsed gate unexpectedly failed")
    require(gates["disconnectRecovery"], "Ordivon recovery gate unexpectedly failed")
    require(not gates["toolCallsReduced30Percent"], "tool-call gate unexpectedly passed")
    require(not gates["contextReduced50Percent"], "context gate unexpectedly passed")


def validate(data: dict[str, Any]) -> None:
    require(data.get("schemaVersion") == 1, "unsupported evidence schema")
    require(data.get("phase") == EXPECTED_PHASE, "unexpected M2 phase")
    require(
        data.get("evidenceClass") == "LOCAL_DIFFERENTIAL_BENCHMARK",
        "unexpected evidence class",
    )
    configuration = data.get("configuration")
    require(isinstance(configuration, dict), "configuration missing")
    require(configuration.get("iterations") == EXPECTED_ITERATIONS, "iteration count mismatch")
    require(configuration.get("warmups") == 1, "warm-up count mismatch")
    probe = data.get("disconnectProbe")
    require(isinstance(probe, dict), "disconnect probe missing")
    require(probe.get("recovered") is False, "Legacy recovery unexpectedly succeeded")
    require("No session found" in probe.get("observation", ""), "Legacy loss evidence missing")
    legacy, ordivon = validate_samples(data)
    validate_summary(data, legacy, ordivon)
    validate_cutover(data)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("evidence", type=Path)
    args = parser.parse_args()
    try:
        validate(load(args.evidence))
    except ValueError as error:
        print(f"M2_EVIDENCE_FAIL: {error}", file=sys.stderr)
        return 1
    print("M2_EVIDENCE_PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

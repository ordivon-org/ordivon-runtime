#!/usr/bin/env python3
from __future__ import annotations

import json
import statistics
import sys
from pathlib import Path
from typing import Any

EXPECTED_HEAD = "e1a40df3878881e47a56fd428ec6eb316301799b"
EXPECTED_SAMPLES = 5
METRICS = (
    "elapsedMs",
    "toolCalls",
    "remoteRoundTrips",
    "contextBytes",
    "outputBytes",
    "fallbackCount",
)


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def load(path: Path) -> dict[str, Any]:
    with path.open(encoding="utf-8") as handle:
        value = json.load(handle)
    require(isinstance(value, dict), f"{path}: root must be an object")
    return value


def median(values: list[int]) -> int:
    return round(statistics.median(values))


def validate_samples(data: dict[str, Any]) -> tuple[dict[str, Any], dict[str, Any]]:
    raw = data["rawSamples"]
    legacy = raw["legacy"]
    ordivon = raw["ordivon"]
    require(len(legacy) == EXPECTED_SAMPLES, "legacy sample count mismatch")
    require(len(ordivon) == EXPECTED_SAMPLES, "Ordivon sample count mismatch")
    for index, (left, right) in enumerate(zip(legacy, ordivon, strict=True)):
        require(left["pairId"] == right["pairId"], f"pair {index}: ID mismatch")
        require(left["succeeded"] is True, f"pair {index}: Legacy failed")
        require(right["succeeded"] is True, f"pair {index}: Ordivon failed")
        require(
            left["semanticDigest"] == right["semanticDigest"],
            f"pair {index}: semantic mismatch",
        )
        require(left["fallbackCount"] == 0, f"pair {index}: Legacy fallback")
        require(right["fallbackCount"] == 0, f"pair {index}: Ordivon fallback")
        require(
            left["recoveredAfterDisconnect"] is False,
            f"pair {index}: Legacy recovery claim changed",
        )
        require(
            right["recoveredAfterDisconnect"] is True,
            f"pair {index}: Ordivon recovery missing",
        )
    return {"legacy": legacy, "ordivon": ordivon}, data["summaries"]


def validate_summary(
    samples: dict[str, list[dict[str, Any]]], summaries: dict[str, Any]
) -> None:
    for backend in ("legacy", "ordivon"):
        summary = summaries[backend]
        for metric in METRICS:
            observed = median([sample[metric] for sample in samples[backend]])
            require(summary[metric] == observed, f"{backend} {metric} median mismatch")
        require(summary["succeeded"] is True, f"{backend} summary not successful")
        require(
            len(summary["semanticDigests"]) == EXPECTED_SAMPLES,
            f"{backend} semantic digest count mismatch",
        )


def validate_common(data: dict[str, Any], phase: str, task_journey: str) -> None:
    require(data["schemaVersion"] == 1, f"{phase}: schema mismatch")
    require(data["phase"] == phase, f"{phase}: phase mismatch")
    require(data["sourceRevision"] == EXPECTED_HEAD, f"{phase}: source mismatch")
    require(data["taskJourney"] == task_journey, f"{phase}: journey mismatch")
    runtime = data["runtimeIdentity"]
    require(runtime["gitHeadAtRun"] == EXPECTED_HEAD, f"{phase}: runtime head mismatch")
    require(runtime["branch"] == "agent/m3-compact-agent-interface", f"{phase}: branch mismatch")
    require(runtime["desktopCommander"]["version"] == "0.2.46", f"{phase}: Legacy version")
    require(runtime["m1Cli"]["digest"].startswith("sha256:"), f"{phase}: CLI digest")
    require(runtime["m1Runner"]["digest"].startswith("sha256:"), f"{phase}: runner digest")
    samples, summaries = validate_samples(data)
    validate_summary(samples, summaries)
    require(data["cutover"]["eligible"] is True, f"{phase}: eligibility false")
    require(all(data["cutover"]["gates"].values()), f"{phase}: a gate is false")
    require(data["disconnectProbe"]["recovered"] is False, f"{phase}: Legacy probe changed")
    require(
        "No session found for PID" in data["disconnectProbe"]["observation"],
        f"{phase}: disconnect evidence missing",
    )


def validate_m3a(data: dict[str, Any]) -> None:
    phase = "ORDIVON-MIGRATION-M3A-2026-07-22"
    validate_common(data, phase, "M3A_FULL_READ_COMPACT_TASK_JOURNEY")
    legacy = data["summaries"]["legacy"]
    ordivon = data["summaries"]["ordivon"]
    require(ordivon["toolCalls"] <= 6, "M3A exceeds six calls")
    require(ordivon["contextBytes"] <= legacy["contextBytes"], "M3A context regressed")
    require(ordivon["elapsedMs"] <= round(legacy["elapsedMs"] * 1.10), "M3A too slow")
    require(ordivon["recoveredAfterDisconnect"] is True, "M3A recovery missing")


def validate_m3b(data: dict[str, Any]) -> None:
    phase = "ORDIVON-MIGRATION-M3B-2026-07-22"
    validate_common(data, phase, "M3B_TARGETED_READ_COMPACT_TASK_JOURNEY")
    legacy = data["summaries"]["legacy"]
    ordivon = data["summaries"]["ordivon"]
    require(ordivon["toolCalls"] <= 5, "M3B exceeds five calls")
    require(ordivon["contextBytes"] <= 3220, "M3B context exceeds budget")
    require(ordivon["elapsedMs"] <= round(legacy["elapsedMs"] * 1.10), "M3B too slow")
    require(ordivon["recoveredAfterDisconnect"] is True, "M3B recovery missing")


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: check_m3_evidence.py <m3a.json> <m3b.json>", file=sys.stderr)
        return 64
    try:
        m3a = load(Path(sys.argv[1]))
        m3b = load(Path(sys.argv[2]))
        validate_m3a(m3a)
        validate_m3b(m3b)
    except (KeyError, TypeError, ValueError, json.JSONDecodeError) as error:
        print(f"M3_EVIDENCE_FAIL: {error}", file=sys.stderr)
        return 1
    print("M3_EVIDENCE_PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

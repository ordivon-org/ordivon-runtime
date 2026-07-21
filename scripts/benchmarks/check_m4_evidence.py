#!/usr/bin/env python3
"""Independently verify M4 MCP differential evidence and trace invariants."""

from __future__ import annotations

import json
import math
import sys
from pathlib import Path
from statistics import median
from typing import Any

EXPECTED_HEAD = "4592689dc9183fcb08f4828d3d752a4cf57e318f"
EXPECTED_SAMPLES = 20


def fail(message: str) -> None:
    raise SystemExit(f"M4_EVIDENCE_FAIL: {message}")


def load(path: Path) -> dict[str, Any]:
    try:
        return json.loads(path.read_text())
    except Exception as error:
        fail(f"cannot load {path}: {error}")


def rounded_median(values: list[int]) -> int:
    value = median(values)
    return int(math.floor(value + 0.5))


def percentile(values: list[int], fraction: float) -> int:
    ordered = sorted(values)
    index = max(0, math.ceil(len(ordered) * fraction) - 1)
    return ordered[index]

def pairwise_equivalent(legacy: list[dict[str, Any]], m4: list[dict[str, Any]]) -> bool:
    legacy_by_pair = {sample["pairId"]: sample["semanticDigest"] for sample in legacy}
    m4_by_pair = {sample["pairId"]: sample["semanticDigest"] for sample in m4}
    return legacy_by_pair == m4_by_pair and len(legacy_by_pair) == EXPECTED_SAMPLES


def recompute_summary(samples: list[dict[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for field in [
        "elapsedMs",
        "toolCalls",
        "remoteRoundTrips",
        "contextBytes",
        "outputBytes",
    ]:
        result[field] = rounded_median([int(sample[field]) for sample in samples])
    elapsed = [int(sample["elapsedMs"]) for sample in samples]
    result["elapsedP50Ms"] = percentile(elapsed, 0.50)
    result["elapsedP95Ms"] = percentile(elapsed, 0.95)
    result["taskHttpRequests"] = rounded_median(
        [int(sample["transport"]["task"]["httpRequests"]) for sample in samples]
    )
    result["taskRequestBytes"] = rounded_median(
        [int(sample["transport"]["task"]["requestBytes"]) for sample in samples]
    )
    result["taskResponseBytes"] = rounded_median(
        [int(sample["transport"]["task"]["responseBytes"]) for sample in samples]
    )
    result["succeeded"] = all(bool(sample["succeeded"]) for sample in samples)
    result["recoveredAfterDisconnect"] = all(
        bool(sample["recoveredAfterDisconnect"]) for sample in samples
    )
    result["fallbackCount"] = sum(int(sample["fallbackCount"]) for sample in samples)
    result["semanticDigests"] = sorted({sample["semanticDigest"] for sample in samples})
    return result

def require_equal(actual: Any, expected: Any, label: str) -> None:
    if actual != expected:
        fail(f"{label}: expected {expected!r}, got {actual!r}")


def verify_evidence(path: Path, phase: str) -> None:
    evidence = load(path)
    require_equal(evidence.get("phase"), phase, f"{path.name} phase")
    require_equal(evidence.get("sourceRevision"), EXPECTED_HEAD, f"{path.name} sourceRevision")
    require_equal(
        evidence.get("runtimeIdentity", {}).get("gitHeadAtRun"),
        EXPECTED_HEAD,
        f"{path.name} runtime head",
    )
    configuration = evidence.get("configuration", {})
    require_equal(configuration.get("iterations"), EXPECTED_SAMPLES, f"{path.name} iterations")
    require_equal(configuration.get("warmups"), 3, f"{path.name} warmups")
    if configuration.get("legacyMcpUrl") != "http://127.0.0.1:8811/mcp":
        fail(f"{path.name}: Legacy endpoint drift")
    if configuration.get("m4McpUrl") != "http://127.0.0.1:8895/mcp":
        fail(f"{path.name}: M4 endpoint drift")

    legacy = evidence["rawSamples"]["legacy"]
    m4 = evidence["rawSamples"]["ordivon"]
    if len(legacy) != EXPECTED_SAMPLES or len(m4) != EXPECTED_SAMPLES:
        fail(f"{path.name}: expected {EXPECTED_SAMPLES} paired samples")
    if not pairwise_equivalent(legacy, m4):
        fail(f"{path.name}: pairwise semantic digests differ")
    legacy_summary = recompute_summary(legacy)
    m4_summary = recompute_summary(m4)
    for backend, recomputed in [("legacy", legacy_summary), ("ordivon", m4_summary)]:
        recorded = evidence["summaries"][backend]
        for field, expected in recomputed.items():
            require_equal(recorded.get(field), expected, f"{path.name} {backend}.{field}")

    if evidence.get("disconnectProbe", {}).get("recovered") is not False:
        fail(f"{path.name}: Legacy disconnect probe must fail recovery")
    if evidence.get("m4DisconnectProbe", {}).get("recovered") is not True:
        fail(f"{path.name}: M4 disconnect probe must recover")
    if not m4_summary["recoveredAfterDisconnect"]:
        fail(f"{path.name}: M4 samples lost recovery")
    if m4_summary["fallbackCount"] != 0:
        fail(f"{path.name}: M4 fallback count is nonzero")
    if not m4_summary["succeeded"] or not legacy_summary["succeeded"]:
        fail(f"{path.name}: one or more samples failed")

    gates = {
        "completionNotWorse": m4_summary["succeeded"],
        "elapsedWithin10Percent": m4_summary["elapsedMs"]
        <= math.ceil(legacy_summary["elapsedMs"] * 1.10),
        "p95Within10Percent": m4_summary["elapsedP95Ms"]
        <= math.ceil(legacy_summary["elapsedP95Ms"] * 1.10),
        "httpRequestsNotWorse": m4_summary["taskHttpRequests"]
        <= legacy_summary["taskHttpRequests"],
        "disconnectRecovery": m4_summary["recoveredAfterDisconnect"],
        "noFallback": m4_summary["fallbackCount"] == 0,
        "semanticEquivalence": pairwise_equivalent(legacy, m4),
    }
    if phase.endswith("M4A-2026-07-22"):
        gates["toolCallsAtMostSix"] = m4_summary["toolCalls"] <= 6
        gates["contextNotWorse"] = m4_summary["contextBytes"] <= legacy_summary["contextBytes"]
    else:
        gates["toolCallsAtMostFive"] = m4_summary["toolCalls"] <= 5
        gates["contextAtMost3220"] = m4_summary["contextBytes"] <= 3220

    if not all(gates.values()):
        fail(f"{path.name}: recomputed gate failure {gates}")
    require_equal(evidence.get("cutover", {}).get("eligible"), True, f"{path.name} eligibility")
    for gate, value in gates.items():
        require_equal(
            evidence.get("cutover", {}).get("gates", {}).get(gate),
            value,
            f"{path.name} gate {gate}",
        )

    server = evidence.get("runtimeIdentity", {}).get("m4Server", {})
    require_equal(server.get("name"), "ordivon-m4-experimental", f"{path.name} server name")
    require_equal(
        evidence.get("runtimeIdentity", {}).get("mcpSdkVersion"),
        "1.29.0",
        f"{path.name} SDK version",
    )


def verify_trace(path: Path, kind: str) -> None:
    rows = [json.loads(line) for line in path.read_text().splitlines() if line.strip()]
    if not rows:
        fail(f"{path}: empty trace")
    identifiers = [row["traceId"] for row in rows]
    if len(identifiers) != len(set(identifiers)):
        fail(f"{path}: duplicate trace IDs")
    if any("Bearer" in json.dumps(row) for row in rows):
        fail(f"{path}: credential material leaked into trace")
    if kind == "http" and not all(row.get("kind") == "http" for row in rows):
        fail(f"{path}: non-HTTP row in HTTP trace")

def main() -> None:
    if len(sys.argv) not in {3, 5}:
        fail("usage: check_m4_evidence.py M4A.json M4B.json [CORE_TRACE HTTP_TRACE]")
    verify_evidence(Path(sys.argv[1]), "ORDIVON-MIGRATION-M4A-2026-07-22")
    verify_evidence(Path(sys.argv[2]), "ORDIVON-MIGRATION-M4B-2026-07-22")
    if len(sys.argv) == 5:
        verify_trace(Path(sys.argv[3]), "core")
        verify_trace(Path(sys.argv[4]), "http")
    print("M4_EVIDENCE_PASS")


if __name__ == "__main__":
    main()

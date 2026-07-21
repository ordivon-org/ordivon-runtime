#!/usr/bin/env python3
"""Independently verify M5 wire, dogfood, and shadow evidence."""

from __future__ import annotations

import json
import math
import sys
from pathlib import Path
from statistics import median
from typing import Any

EXPECTED_REVISION = "ff57d258c0697dba1335bf8a86ccb32ca85e9128"
EXPECTED_JOURNEYS = [
    "readonly-audit",
    "single-file-edit",
    "multi-file-test",
    "failure-repair-loop",
    "rust-target-test",
    "bounded-log-artifact",
    "durable-recovery-cancel",
]
EXPECTED_SHADOW_KINDS = EXPECTED_JOURNEYS[:4]


def fail(message: str) -> None:
    raise SystemExit(f"M5_EVIDENCE_FAIL: {message}")


def load(path: Path) -> dict[str, Any]:
    try:
        return json.loads(path.read_text())
    except Exception as error:
        fail(f"cannot load {path}: {error}")


def require(actual: Any, expected: Any, label: str) -> None:
    if actual != expected:
        fail(f"{label}: expected {expected!r}, got {actual!r}")


def rounded_median(values: list[int]) -> int:
    value = median(values)
    return int(math.floor(value + 0.5))


def summarize(samples: list[dict[str, Any]]) -> dict[str, Any]:
    return {
        "samples": len(samples),
        "succeeded": all(bool(sample["succeeded"]) for sample in samples),
        "elapsedMs": rounded_median([int(sample["elapsedMs"]) for sample in samples]),
        "toolCalls": rounded_median([int(sample["toolCalls"]) for sample in samples]),
        "contextBytes": rounded_median([int(sample["contextBytes"]) for sample in samples]),
        "outputBytes": rounded_median([int(sample["outputBytes"]) for sample in samples]),
        "httpRequests": rounded_median([int(sample["httpRequests"]) for sample in samples]),
        "repairRounds": rounded_median([int(sample["repairRounds"]) for sample in samples]),
        "fallbackCount": sum(int(sample["fallbackCount"]) for sample in samples),
    }


def verify_wire(path: Path) -> None:
    evidence = load(path)
    require(evidence.get("phase"), "ORDIVON-M5-WIRE-CONTRACT", "wire phase")
    require(evidence.get("passed"), True, "wire passed")
    require(evidence.get("concurrentReads"), 64, "wire concurrent reads")
    if int(evidence.get("coreTraceRows", 0)) < 71:
        fail("wire core trace count is too small")
    if int(evidence.get("httpTraceRows", 0)) < 72:
        fail("wire HTTP trace count is too small")
    observations = {item["name"]: int(item["bytes"]) for item in evidence["observations"]}
    budgets = {
        "workspace.open": 220,
        "workspace.read": 360,
        "workspace.mutate": 460,
        "workspace.exec": 420,
        "task.observe": 420,
        "artifact.read": 560,
        "workspace.diff": 900,
    }
    require(set(observations), set(budgets), "wire observations")
    for name, budget in budgets.items():
        if observations[name] > budget:
            fail(f"wire response budget exceeded for {name}")


def verify_dogfood(path: Path) -> None:
    evidence = load(path)
    require(
        evidence.get("phase"),
        "ORDIVON-MIGRATION-M5-LIMITED-DOGFOOD-2026-07-22",
        "dogfood phase",
    )
    require(evidence.get("sourceRevision"), EXPECTED_REVISION, "dogfood revision")
    journeys = evidence.get("journeys", [])
    require([journey["name"] for journey in journeys], EXPECTED_JOURNEYS, "dogfood journeys")
    if not all(journey.get("succeeded") for journey in journeys):
        fail("one or more dogfood journeys failed")
    if any(int(journey.get("fallbackCount", 0)) != 0 for journey in journeys):
        fail("dogfood used Legacy fallback")
    summary = evidence["summary"]
    recomputed = {
        "journeyCount": len(journeys),
        "succeeded": all(journey["succeeded"] for journey in journeys),
        "totalToolCalls": sum(int(journey["toolCalls"]) for journey in journeys),
        "totalContextBytes": sum(int(journey["contextBytes"]) for journey in journeys),
        "totalOutputBytes": sum(int(journey["outputBytes"]) for journey in journeys),
        "totalRepairRounds": sum(int(journey["repairRounds"]) for journey in journeys),
        "fallbackCount": sum(int(journey["fallbackCount"]) for journey in journeys),
        "recoveredAfterDisconnect": any(
            journey.get("recoveredAfterDisconnect") is True for journey in journeys
        ),
        "cancellationClean": any(journey.get("cancellationClean") is True for journey in journeys),
    }
    require(summary, recomputed, "dogfood summary")
    require(summary["totalRepairRounds"], 1, "dogfood repair rounds")
    repair = next(journey for journey in journeys if journey["name"] == "failure-repair-loop")
    require(repair.get("failureObserved"), True, "dogfood failure observation")
    rust = next(journey for journey in journeys if journey["name"] == "rust-target-test")
    require(rust.get("realRepositoryTest"), True, "dogfood real Rust test")
    log = next(journey for journey in journeys if journey["name"] == "bounded-log-artifact")
    require(log.get("truncatedOutput"), True, "dogfood log truncation")
    policy = evidence["routePolicy"]
    require(policy.get("automaticLegacyFallback"), False, "dogfood fallback policy")
    forbidden = set(policy.get("forbiddenCapabilities", []))
    for capability in ["git push", "deployment", "credential access"]:
        if capability not in forbidden:
            fail(f"dogfood policy omitted forbidden capability {capability}")


def verify_shadow(path: Path) -> None:
    evidence = load(path)
    require(
        evidence.get("phase"),
        "ORDIVON-MIGRATION-M5-SHADOW-COMPARISON-2026-07-22",
        "shadow phase",
    )
    require(evidence.get("sourceRevision"), EXPECTED_REVISION, "shadow revision")
    require(evidence.get("iterationsPerJourney"), 3, "shadow iterations")
    require(evidence.get("alternatingOrder"), True, "shadow alternating order")
    require(evidence.get("journeyKinds"), EXPECTED_SHADOW_KINDS, "shadow journey kinds")
    legacy = evidence["rawSamples"]["legacy"]
    ordivon = evidence["rawSamples"]["ordivon"]
    require(len(legacy), 12, "legacy sample count")
    require(len(ordivon), 12, "ordivon sample count")
    legacy_by_pair = {sample["pairId"]: sample for sample in legacy}
    ordivon_by_pair = {sample["pairId"]: sample for sample in ordivon}
    require(set(legacy_by_pair), set(ordivon_by_pair), "shadow pair IDs")
    for pair_id, legacy_sample in legacy_by_pair.items():
        ordivon_sample = ordivon_by_pair[pair_id]
        require(
            legacy_sample["semanticDigest"],
            ordivon_sample["semanticDigest"],
            f"shadow semantic digest {pair_id}",
        )
        require(legacy_sample["succeeded"], True, f"legacy completion {pair_id}")
        require(ordivon_sample["succeeded"], True, f"ordivon completion {pair_id}")
        if int(ordivon_sample["repairRounds"]) > int(legacy_sample["repairRounds"]):
            fail(f"ordivon used more repair rounds for {pair_id}")
        require(ordivon_sample["fallbackCount"], 0, f"ordivon fallback {pair_id}")
    summaries = evidence["summaries"]
    require(summaries["overall"]["legacy"], summarize(legacy), "legacy overall summary")
    require(summaries["overall"]["ordivon"], summarize(ordivon), "ordivon overall summary")
    for kind in EXPECTED_SHADOW_KINDS:
        expected_legacy = summarize([sample for sample in legacy if sample["kind"] == kind])
        expected_ordivon = summarize([sample for sample in ordivon if sample["kind"] == kind])
        require(
            summaries["byKind"][kind]["legacy"],
            expected_legacy,
            f"legacy summary {kind}",
        )
        require(
            summaries["byKind"][kind]["ordivon"],
            expected_ordivon,
            f"ordivon summary {kind}",
        )

    overall_legacy = summarize(legacy)
    overall_ordivon = summarize(ordivon)
    gates = {
        "completionNotWorse": overall_ordivon["succeeded"] and overall_legacy["succeeded"],
        "semanticEquivalence": True,
        "repairRoundsNotWorse": overall_ordivon["repairRounds"]
        <= overall_legacy["repairRounds"],
        "toolCallsNotWorse": overall_ordivon["toolCalls"] <= overall_legacy["toolCalls"],
        "contextWithinTenPercent": overall_ordivon["contextBytes"]
        <= math.ceil(overall_legacy["contextBytes"] * 1.10),
        "elapsedWithinTwentyFivePercent": overall_ordivon["elapsedMs"]
        <= math.ceil(overall_legacy["elapsedMs"] * 1.25),
        "noFallback": overall_ordivon["fallbackCount"] == 0,
    }
    require(evidence["decision"]["gates"], gates, "shadow gates")
    require(evidence["decision"]["limitedDogfoodEligible"], all(gates.values()), "shadow decision")


def main() -> None:
    if len(sys.argv) != 4:
        fail("usage: check_m5_evidence.py WIRE.json DOGFOOD.json SHADOW.json")
    verify_wire(Path(sys.argv[1]))
    verify_dogfood(Path(sys.argv[2]))
    verify_shadow(Path(sys.argv[3]))
    print("M5_EVIDENCE_PASS")


if __name__ == "__main__":
    main()

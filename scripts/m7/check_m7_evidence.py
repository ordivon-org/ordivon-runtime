#!/usr/bin/env python3
"""Independently recompute the M7 hardening and lifecycle evidence gates."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import re
from pathlib import Path
from statistics import median
from typing import Any

REPO = Path(__file__).resolve().parents[2]
SHA256 = re.compile(r"^sha256:[0-9a-f]{64}$")
ENVELOPE_FIELDS = {
    "evidenceType",
    "implementationCommit",
    "harnesses",
    "environment",
    "observationsDigest",
}
M7_JOURNEYS = [
    "readonly-audit",
    "single-file-edit",
    "multi-file-test",
    "failure-repair-loop",
    "python-package-validation",
    "bounded-log-artifact",
    "durable-recovery-cancel",
    "idempotency-and-list",
]
SHADOW_KINDS = [
    "readonly-audit",
    "single-file-edit",
    "multi-file-test",
    "failure-repair-loop",
]
TRANSPORT = {
    "missingAuthorization": 401,
    "invalidAuthorization": 401,
    "disallowedOrigin": 403,
    "disallowedHost": 403,
    "oversizedBody": 413,
}


def fail(message: str) -> None:
    raise SystemExit(f"M7_EVIDENCE_FAIL: {message}")


def load(path: Path) -> dict[str, Any]:
    try:
        return json.loads(path.read_text())
    except Exception as error:
        fail(f"cannot load {path}: {error}")


def digest_bytes(data: bytes) -> str:
    return "sha256:" + hashlib.sha256(data).hexdigest()


def digest_file(path: Path) -> str:
    return digest_bytes(path.read_bytes())


def canonical(value: Any) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode()


def require(actual: Any, expected: Any, label: str) -> None:
    if actual != expected:
        fail(f"{label}: expected {expected!r}, got {actual!r}")


def verify_envelope(document: dict[str, Any], commit: str, label: str) -> None:
    require(document.get("sourceRevision"), commit, f"{label} sourceRevision")
    require(document.get("implementationCommit"), commit, f"{label} implementationCommit")
    harnesses = document.get("harnesses")
    if not isinstance(harnesses, list) or not harnesses:
        fail(f"{label} missing harnesses")
    for harness in harnesses:
        path = (REPO / harness["path"]).resolve()
        try:
            path.relative_to(REPO)
        except ValueError:
            fail(f"{label} harness escapes repo: {path}")
        require(harness["sha256"], digest_file(path), f"{label} harness {path}")
    observations = {
        key: value for key, value in document.items() if key not in ENVELOPE_FIELDS
    }
    require(
        document.get("observationsDigest"),
        digest_bytes(canonical(observations)),
        f"{label} observations digest",
    )
    if not SHA256.fullmatch(document.get("observationsDigest", "")):
        fail(f"{label} observations digest format")
    claims = document.get("claimsNotMade")
    if not isinstance(claims, list) or not claims or not all(
        isinstance(item, str) and item.strip() for item in claims
    ):
        fail(f"{label} claimsNotMade")


def rounded_median(values: list[int]) -> int:
    return int(math.floor(median(values) + 0.5))


def summarize(samples: list[dict[str, Any]]) -> dict[str, Any]:
    return {
        "samples": len(samples),
        "succeeded": all(item["succeeded"] for item in samples),
        "elapsedMs": rounded_median([item["elapsedMs"] for item in samples]),
        "toolCalls": rounded_median([item["toolCalls"] for item in samples]),
        "contextBytes": rounded_median([item["contextBytes"] for item in samples]),
        "outputBytes": rounded_median([item["outputBytes"] for item in samples]),
        "httpRequests": rounded_median([item["httpRequests"] for item in samples]),
        "repairRounds": rounded_median([item["repairRounds"] for item in samples]),
        "fallbackCount": sum(item["fallbackCount"] for item in samples),
    }


def verify_wire(document: dict[str, Any], phase: str, label: str) -> None:
    require(document["phase"], phase, f"{label} phase")
    require(document["passed"], True, f"{label} passed")
    require(document["concurrentReads"], 64, f"{label} concurrent reads")
    if document["coreTraceRows"] < 73 or document["httpTraceRows"] < 87:
        fail(f"{label} trace rows")
    budgets = {
        "workspace.open": 220,
        "workspace.read": 360,
        "workspace.mutate": 460,
        "workspace.exec": 420,
        "task.list": 420,
        "artifact.read": 560,
        "workspace.diff": 900,
    }
    for item in document["observations"]:
        if item["name"] in budgets and item["bytes"] > budgets[item["name"]]:
            fail(f"{label} context budget {item['name']}")


def verify_dogfood(document: dict[str, Any], names: list[str], label: str) -> None:
    journeys = document["journeys"]
    require([item["name"] for item in journeys], names, f"{label} journeys")
    summary = {
        "journeyCount": len(journeys),
        "succeeded": all(item["succeeded"] for item in journeys),
        "totalToolCalls": sum(item["toolCalls"] for item in journeys),
        "totalContextBytes": sum(item["contextBytes"] for item in journeys),
        "totalOutputBytes": sum(item["outputBytes"] for item in journeys),
        "totalRepairRounds": sum(item["repairRounds"] for item in journeys),
        "fallbackCount": sum(item["fallbackCount"] for item in journeys),
        "recoveredAfterDisconnect": any(
            item.get("recoveredAfterDisconnect") is True for item in journeys
        ),
        "cancellationClean": any(item.get("cancellationClean") is True for item in journeys),
        "idempotentReplay": any(item.get("idempotentReplay") is True for item in journeys),
    }
    require(document["summary"], summary, f"{label} summary")
    require(summary["succeeded"], True, f"{label} completion")
    require(summary["totalRepairRounds"], 1, f"{label} repair rounds")
    require(summary["fallbackCount"], 0, f"{label} fallback")
    require(summary["recoveredAfterDisconnect"], True, f"{label} recovery")
    require(summary["cancellationClean"], True, f"{label} cancellation")
    require(summary["idempotentReplay"], True, f"{label} replay")


def verify_shadow(document: dict[str, Any]) -> None:
    require(document["phase"], "ORDIVON-MIGRATION-M7-WORKER-SHADOW-2026-07-22", "shadow phase")
    if document["iterationsPerJourney"] < 3:
        fail("shadow requires at least three iterations")
    require(document["journeyKinds"], SHADOW_KINDS, "shadow kinds")
    require(document["alternatingOrder"], True, "shadow alternating order")
    m6 = document["rawSamples"]["m6"]
    m7 = document["rawSamples"]["m7"]
    expected_count = len(SHADOW_KINDS) * document["iterationsPerJourney"]
    require(len(m6), expected_count, "shadow m6 samples")
    require(len(m7), expected_count, "shadow m7 samples")
    left = {item["pairId"]: item for item in m6}
    right = {item["pairId"]: item for item in m7}
    require(set(left), set(right), "shadow pair identities")
    for pair_id in left:
        require(
            left[pair_id]["semanticDigest"],
            right[pair_id]["semanticDigest"],
            f"shadow semantic {pair_id}",
        )
    summary6 = summarize(m6)
    summary7 = summarize(m7)
    require(document["summaries"]["overall"]["m6"], summary6, "shadow m6 summary")
    require(document["summaries"]["overall"]["m7"], summary7, "shadow m7 summary")
    gates = {
        "completionNotWorse": summary6["succeeded"] and summary7["succeeded"],
        "semanticEquivalence": True,
        "repairRoundsNotWorse": summary7["repairRounds"] <= summary6["repairRounds"],
        "toolCallsNotWorse": summary7["toolCalls"] <= summary6["toolCalls"],
        "contextNotIncreased": summary7["contextBytes"] <= summary6["contextBytes"],
        "elapsedWithinTwentyPercent": summary7["elapsedMs"]
        <= math.ceil(summary6["elapsedMs"] * 1.20),
        "noFallback": summary7["fallbackCount"] == 0,
    }
    require(document["decision"]["gates"], gates, "shadow gates")
    require(
        document["decision"]["localHardeningDogfoodEligible"],
        all(gates.values()),
        "shadow decision",
    )


def verify_local_matrix(document: dict[str, Any]) -> None:
    require(document["phase"], "ORDIVON-MIGRATION-M7-LOCAL-MATRIX-2026-07-22", "matrix phase")
    require(document["passed"], True, "matrix passed")
    require(all(document["gates"].values()), True, "matrix gates")
    command_ids = {item["id"] for item in document["commands"]}
    expected_ids = {
        "workspace-default-tests",
        "m6-feature-tests",
        "m7-feature-tests",
        "m7-mcp-tests",
        "build-m7-runner",
        "m6-systemd-matrix",
        "m7-systemd-matrix",
        "default-strict-clippy",
        "m7-exec-strict-clippy",
        "m7-mcp-strict-clippy",
    }
    require(command_ids, expected_ids, "matrix command ids")
    require(all(item["exitCode"] == 0 for item in document["commands"]), True, "matrix exits")
    worker = document["environment"]["worker"]
    require(worker["user"], "ordivon-worker", "worker user")
    require(worker["uid"], worker["gid"], "worker uid/gid")
    require(worker["shell"], "/usr/bin/nologin", "worker shell")


def verify_reboot(document: dict[str, Any]) -> None:
    require(
        document["phase"],
        "ORDIVON-MIGRATION-M7-REBOOT-RECOVERY-2026-07-22",
        "reboot phase",
    )
    require(document["bootIdChanged"], True, "reboot boot id")
    expected = {
        "runningAtReboot": ("lost", "released"),
        "resultPendingCommit": ("succeeded", "released"),
        "dispatchIntentUnbound": ("lost", "released"),
        "heldOrphaned": ("orphaned", "held_orphaned"),
    }
    for name, pair in expected.items():
        item = document["scenarios"][name]
        require((item["state"], item["reservationState"]), pair, f"reboot {name}")
        require(item["passed"], True, f"reboot {name} passed")
    cancel = document["scenarios"]["cancelIntentPending"]
    if cancel["state"] not in {"lost", "cancelled"} or cancel["reservationState"] != "released":
        fail("reboot cancelIntentPending")
    require(document["attemptCountAfter"], document["attemptCountBefore"], "reboot attempt count")
    require(document["noNewAttempts"], True, "reboot no new attempts")
    require(document["nonterminalAfter"], 0, "reboot nonterminal")
    require(document["activeReservationsAfter"], 1, "reboot held capacity")
    require(document["activeUnitsAfter"], [], "reboot units")
    require(document["automaticRedispatchDetected"], False, "reboot redispatch")
    require(document["passed"], True, "reboot passed")


def verify_concurrency(document: dict[str, Any]) -> None:
    levels = document["levels"]
    require([item["level"] for item in levels], [2, 4, 8], "m6 concurrency levels")
    for item in levels:
        require(item["jobs"], item["level"], "m6 concurrency jobs")
        require(item["uniqueJobs"], item["level"], "m6 concurrency unique")
        require(item["activeAfterCompletion"], 0, "m6 concurrency release")
        require(item["overflowCode"], "CONCURRENCY_LIMIT", "m6 concurrency overflow")
    require(all(document["gates"].values()), True, "m6 concurrency gates")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--implementation-commit", required=True)
    parser.add_argument("--local-matrix", type=Path, required=True)
    parser.add_argument("--m7-wire", type=Path, required=True)
    parser.add_argument("--m7-transport", type=Path, required=True)
    parser.add_argument("--m7-dogfood", type=Path, required=True)
    parser.add_argument("--shadow", type=Path, required=True)
    parser.add_argument("--reboot", type=Path, required=True)
    parser.add_argument("--m6-wire", type=Path, required=True)
    parser.add_argument("--m6-transport", type=Path, required=True)
    parser.add_argument("--m6-dogfood", type=Path, required=True)
    parser.add_argument("--m6-concurrency", type=Path, required=True)
    args = parser.parse_args()

    documents = {
        name: load(getattr(args, name))
        for name in [
            "local_matrix",
            "m7_wire",
            "m7_transport",
            "m7_dogfood",
            "shadow",
            "reboot",
            "m6_wire",
            "m6_transport",
            "m6_dogfood",
            "m6_concurrency",
        ]
    }
    for label, document in documents.items():
        verify_envelope(document, args.implementation_commit, label)

    verify_local_matrix(documents["local_matrix"])
    verify_wire(documents["m7_wire"], "ORDIVON-M7-WIRE-CONTRACT", "m7 wire")
    require(documents["m7_transport"]["results"], TRANSPORT, "m7 transport")
    verify_dogfood(documents["m7_dogfood"], M7_JOURNEYS, "m7 dogfood")
    if not any(item.get("workerPackageValidation") is True for item in documents["m7_dogfood"]["journeys"]):
        fail("m7 dogfood worker package validation")
    verify_shadow(documents["shadow"])
    verify_reboot(documents["reboot"])
    verify_wire(documents["m6_wire"], "ORDIVON-M6-WIRE-CONTRACT", "m6 wire")
    require(documents["m6_transport"]["results"], TRANSPORT, "m6 transport")
    verify_dogfood(
        documents["m6_dogfood"],
        [
            "readonly-audit",
            "single-file-edit",
            "multi-file-test",
            "failure-repair-loop",
            "rust-target-test",
            "bounded-log-artifact",
            "durable-recovery-cancel",
            "idempotency-and-list",
        ],
        "m6 dogfood",
    )
    verify_concurrency(documents["m6_concurrency"])
    print("M7_EVIDENCE_PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

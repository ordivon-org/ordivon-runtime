#!/usr/bin/env python3
"""Research-only typed claim/support checker.

This tool intentionally models support roles as sets rather than scalar evidence strength.
It does not decide domain truth; it checks whether a requested claim category has an
explicit supporting evidence role in the bounded C5 grammar.
"""

from __future__ import annotations

import argparse
import json

SUPPORT = {
    "RUNTIME_TERMINAL": {
        "ATTEMPT_TERMINAL_SUCCESS",
        "PROCESS_EXIT_ZERO",
        "RESULT_AVAILABLE",
        "MECHANICAL_CONVERGENCE",
    },
    "REMOTE_STATE_OBSERVATION": {
        "REMOTE_STATE_AT_HORIZON",
    },
    "HOST_CONTINUITY": {
        "TASK_CONTINUITY_AT_REVISION",
    },
    "OWNER_PUBLICATION": {
        "OWNER_CLAIM_WITHIN_FENCE",
        "OWNER_AUTHORITY_VERSION",
    },
    "ATLAS_PROJECTION": {
        "PROJECTED_AUTHORITY_VERSION",
        "PROJECTION_HEALTH",
        "PROJECTED_RECOVERY_BINDING",
    },
    "HOST_GOAL_EVALUATION": {
        "HOST_GOAL_SUCCEEDED",
    },
    "WORLD_AUTHORITY": {
        "WORLD_TRUE",
    },
    "EFFECT_AUTHORITY": {
        "EXTERNAL_EFFECT_PRESENT",
    },
}


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--evidence-role", action="append", choices=sorted(SUPPORT), required=True)
    p.add_argument("--claim", required=True)
    args = p.parse_args()

    proven = set()
    for role in args.evidence_role:
        proven.update(SUPPORT[role])

    supported = args.claim in proven
    print(json.dumps({
        "evidenceRoles": args.evidence_role,
        "provenTypedSupport": sorted(proven),
        "claim": args.claim,
        "claimAdmissible": supported,
        "invariants": {
            "evidenceCountCreatesMissingScope": False,
            "mechanicalSuccessImpliesGoalSuccess": False,
            "projectionMintsOwnerTruth": False,
            "ownerPublicationMintsWorldTruth": False,
            "semanticLiftRequiresExplicitBridge": True,
        },
    }, indent=2, sort_keys=True))
    return 0 if supported else 4


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Research probe for consequential retry admissibility under open-world effects."""

from __future__ import annotations

import argparse
import json


def classify(effect_knowledge: str, operation_standing: str) -> dict:
    if operation_standing != "CURRENT":
        return {
            "retryAdmissibility": "NO",
            "requiredAction": "REVALIDATE_OPERATION_STANDING",
        }

    if effect_knowledge == "UNKNOWN":
        return {
            "retryAdmissibility": "NOT_YET",
            "requiredAction": "RECONCILE_AUTHORITATIVE_EFFECT_STATE",
        }
    if effect_knowledge == "ABSENT":
        return {
            "retryAdmissibility": "YES",
            "requiredAction": "NEW_ATTEMPT_MAY_BE_ADMITTED",
        }
    if effect_knowledge == "PRESENT_AS_INTENDED":
        return {
            "retryAdmissibility": "NO",
            "requiredAction": "ACCEPT_OR_VERIFY_EXISTING_EFFECT",
        }
    if effect_knowledge == "CONFLICTING":
        return {
            "retryAdmissibility": "NO",
            "requiredAction": "RECONCILE_CONFLICT",
        }
    raise ValueError(effect_knowledge)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--effect-knowledge",
        required=True,
        choices=["UNKNOWN", "ABSENT", "PRESENT_AS_INTENDED", "CONFLICTING"],
    )
    parser.add_argument(
        "--operation-standing",
        default="CURRENT",
        choices=["CURRENT", "STALE"],
    )
    args = parser.parse_args()

    result = {
        "effectKnowledge": args.effect_knowledge,
        "operationStanding": args.operation_standing,
        **classify(args.effect_knowledge, args.operation_standing),
        "invariants": {
            "localTimeoutImpliesEffectAbsent": False,
            "localFailureImpliesRetryAdmissible": False,
            "retryRequiresCurrentOperationStanding": True,
        },
    }
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

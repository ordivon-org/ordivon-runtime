#!/usr/bin/env python3
"""Research-only classifier for support reuse admissibility.

It deliberately does not discover semantic equivalence. The caller must provide
whether each required support relation has already been established by the
appropriate owner/contract.
"""

from __future__ import annotations

import argparse
import json


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--same-operation", action="store_true")
    parser.add_argument("--identity-compatible", action="store_true")
    parser.add_argument("--binding-valid", action="store_true")
    parser.add_argument("--input-compatible", action="store_true")
    parser.add_argument("--scope-sufficient", action="store_true")
    parser.add_argument("--evidence-available", action="store_true")
    parser.add_argument("--standing-current", action="store_true")
    args = parser.parse_args()

    checks = {
        "identityCompatible": args.same_operation or args.identity_compatible,
        "bindingValid": args.binding_valid,
        "inputCompatible": args.input_compatible,
        "scopeSufficient": args.scope_sufficient,
        "evidenceAvailable": args.evidence_available,
        "standingCurrent": args.standing_current,
    }

    admissible = all(checks.values())
    result = {
        "checks": checks,
        "admissibleSupport": admissible,
        "freshExecutionNeed": "ZERO_FOR_THIS_OBLIGATION" if admissible else "SUPPORT_GAP_REMAINS",
        "invariants": {
            "commandEqualityAloneIsEnough": False,
            "operationDigestEqualityAloneIsEnoughAcrossDistinctOperations": False,
            "historicalResultAvailabilityAloneIsEnough": False,
        },
    }
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

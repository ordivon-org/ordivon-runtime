#!/usr/bin/env python3
"""Research-only classifier for lineage and bounded causal attribution.

The caller supplies already-observed facts. The probe never turns temporal order or
Runtime terminal success into external causation on its own.
"""

from __future__ import annotations

import argparse
import json


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--same-runtime-operation", action="store_true")
    p.add_argument("--same-higher-obligation", action="store_true")
    p.add_argument("--pre-state-target-absent", action="store_true")
    p.add_argument("--attempt-transport-transition-evidence", action="store_true")
    p.add_argument("--post-state-target-present", action="store_true")
    p.add_argument("--attempt-runtime-success", action="store_true")
    args = p.parse_args()

    bounded_causal = all([
        args.pre_state_target_absent,
        args.attempt_transport_transition_evidence,
        args.post_state_target_present,
    ])

    if bounded_causal:
        attribution = "BOUNDED_CAUSAL_ATTRIBUTION_SUPPORTED"
    elif args.post_state_target_present:
        attribution = "POST_ATTEMPT_ASSOCIATION_ONLY"
    elif args.attempt_runtime_success:
        attribution = "RUNTIME_TERMINAL_FACT_ONLY"
    else:
        attribution = "NO_CAUSAL_ATTRIBUTION"

    print(json.dumps({
        "lineage": {
            "sameHigherLevelObligation": args.same_higher_obligation,
            "sameRuntimeOperation": args.same_runtime_operation,
            "higherObligationEqualityImpliesRuntimeOperationEquality": False,
        },
        "causalEvidence": {
            "preStateTargetAbsent": args.pre_state_target_absent,
            "attemptTransportTransitionEvidence": args.attempt_transport_transition_evidence,
            "postStateTargetPresent": args.post_state_target_present,
            "attemptRuntimeSuccess": args.attempt_runtime_success,
        },
        "attribution": attribution,
        "invariants": {
            "runtimeSuccessAloneProvesExternalEffect": False,
            "postStateAloneProvesAttemptCausation": False,
            "attemptIdentityEqualsEffectIdentity": False,
            "evidenceIdentityEqualsEffectIdentity": False,
            "boundedCausalClaimRequiresBridgeChain": True,
        },
    }, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

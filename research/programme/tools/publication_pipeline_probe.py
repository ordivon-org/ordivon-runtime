#!/usr/bin/env python3
"""Research-only classifier for a layered publication pipeline.

The probe does not perform publication. It classifies already-observed stage facts
without promoting one layer's success into another layer's claim.
"""

from __future__ import annotations

import argparse
import json


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--local-candidate", action="store_true")
    p.add_argument(
        "--delivery-knowledge",
        choices=["UNKNOWN", "ABSENT", "PRESENT"],
        default="UNKNOWN",
    )
    p.add_argument("--owner-current", action="store_true")
    p.add_argument(
        "--atlas-health",
        choices=["UNOBSERVED", "STALE", "CURRENT"],
        default="UNOBSERVED",
    )
    args = p.parse_args()

    claims = {
        "localCandidateExists": args.local_candidate,
        "remoteDeliveryKnownPresent": args.delivery_knowledge == "PRESENT",
        "ownerAuthorityCurrent": args.owner_current,
        "atlasProjectionCurrent": args.atlas_health == "CURRENT",
    }

    if not args.local_candidate:
        state = "NO_LOCAL_CANDIDATE"
    elif args.delivery_knowledge == "UNKNOWN":
        state = "DELIVERY_UNKNOWN"
    elif args.delivery_knowledge == "ABSENT":
        state = "LOCAL_ONLY_NOT_DELIVERED"
    elif not args.owner_current:
        state = "REMOTE_DELIVERED_OWNER_NOT_CURRENT"
    elif args.atlas_health in {"UNOBSERVED", "STALE"}:
        state = "OWNER_PUBLISHED_DOWNSTREAM_STALE_OR_UNOBSERVED"
    else:
        state = "PIPELINE_CONVERGED"

    print(json.dumps({
        "state": state,
        "claims": claims,
        "globalAtomicSuccess": False,
        "invariants": {
            "localImpliesRemote": False,
            "remoteImpliesOwnerCurrent": False,
            "ownerCurrentImpliesAtlasFresh": False,
            "atlasFreshMintsOwnerAuthority": False,
            "stageSuccessesMayBeMeaningfullyOutOfSync": True,
            "semanticLiftRequiresBridge": True,
        },
    }, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

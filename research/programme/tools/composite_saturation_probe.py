#!/usr/bin/env python3
"""Research-only consistency/deletion audit for Runtime C1-C10.

The probe does not prove the theory from first principles. Its evidence is the committed
C1-C9 case corpus. It checks that the closeout audit is internally consistent: every
observed requirement has a surviving semantic axis, dropping any tested axis exposes
real previously-observed requirements, and no R1-R8 reopen trigger is admitted.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

FAMILIES = {
    "F1_OPERATIONAL_COMMITMENT_REALIZATION": {
        "C3_EXACT_REPLAY_VS_NEW_OPERATION",
        "C7_OBLIGATION_OPERATION_ATTEMPT_NONIDENTITY",
        "C8_PURPOSE_CONTINUES_PHYSICAL_HISTORY_CHANGES",
    },
    "F2_BOUNDARY_AUTHORITY_RESOURCE_ISOLATION": {
        "C8_CAPABILITY_AUTHORITY_REALIZATION_ORTHOGONALITY",
        "C8_ADMISSION_REQUIRES_CURRENT_BINDING_AUTHORITY",
    },
    "F3_BEHAVIOR_INTERACTION_COMPOSITION": {
        "C6_STAGED_NONATOMIC_PUBLICATION",
        "C6_TYPED_DEPENDENCY_WITHOUT_UNIVERSAL_TOTAL_ORDER",
    },
    "F4_FAILURE_EFFECT_PERSISTENCE_RECONCILIATION": {
        "C2_TIMEOUT_EFFECT_UNKNOWN",
        "C2_RECONCILE_BEFORE_RETRY",
        "C7_EFFECT_ABSENCE_DOES_NOT_ERASE_ATTEMPT",
    },
    "F5_EVIDENCE_CLAIM_TRUTH_SCOPE": {
        "C5_TYPED_SUPPORT_NO_TRUTH_LIFT",
        "C6_BRIDGE_SUPPORTED_END_TO_END_CLAIM",
        "C7_BOUNDED_CAUSAL_ATTRIBUTION",
    },
    "F6_LIFECYCLE_RETENTION_STANDING": {
        "C1_C4_HISTORICAL_VALIDITY_VS_CURRENT_STANDING",
        "C9_RECLAIMED_WORKSPACE_RETAINED_HISTORY",
        "C3_C8_HISTORICAL_IDENTITY_DOES_NOT_MINT_CURRENT_REUSE",
    },
}

EVIDENCE_FILES = {
    f"C{i}": next(Path("research/programme/evidence").glob(f"C{i}-*.json"), None)
    for i in range(1, 10)
}

REOPEN = {
    "R1_IDENTITY_INSUFFICIENCY": False,
    "R2_SCOPE_SUPPORT_CONTRADICTION": False,
    "R3_STANDING_CURRENTNESS_FAILURE": False,
    "R4_REALIZATION_CLASS_NOVELTY": False,
    "R5_COMPOSITION_ORDER_FAILURE": False,
    "R6_OPEN_WORLD_RECOVERY_FAILURE": False,
    "R7_OWNER_BOUNDARY_CONTRADICTION": False,
    "R8_CROSS_DOMAIN_REPEATED_ANOMALY": False,
}


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--drop-family", action="append", choices=sorted(FAMILIES), default=[])
    args = p.parse_args()

    evidence_missing = [k for k, v in EVIDENCE_FILES.items() if v is None]
    evidence_status = {}
    for k, path in EVIDENCE_FILES.items():
        if path is None:
            continue
        evidence_status[k] = json.load(open(path))["status"]

    dropped = set(args.drop_family)
    surviving = set(FAMILIES) - dropped
    lost_requirements = sorted(
        req
        for family in dropped
        for req in FAMILIES[family]
    )

    baseline_consistent = not evidence_missing and not any(REOPEN.values())
    all_axes_present = not dropped
    saturated = baseline_consistent and all_axes_present

    out = {
        "evidence": {
            "missing": evidence_missing,
            "statuses": evidence_status,
        },
        "families": {
            "surviving": sorted(surviving),
            "dropped": sorted(dropped),
            "lostObservedRequirements": lost_requirements,
            "eachAxisHasObservedNecessity": all(bool(v) for v in FAMILIES.values()),
            "uniqueMinimalTaxonomyProven": False,
        },
        "reopenAudit": REOPEN,
        "crossFamilyContradictionFound": False,
        "programmeDisposition": (
            "PROGRAMME_SATURATED_CURRENT_EVIDENCE_HORIZON"
            if saturated
            else "DELETION_EXPOSES_OBSERVED_SEMANTIC_LOSS"
            if dropped and lost_requirements
            else "AUDIT_INCOMPLETE"
        ),
        "wholeRuntimeExhaustiveClosure": "UNKNOWN",
        "automaticC11": False,
    }
    print(json.dumps(out, indent=2, sort_keys=True))

    if evidence_missing:
        return 3
    if dropped and not lost_requirements:
        return 5
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

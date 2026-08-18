#!/usr/bin/env python3
"""Research-only classifier for lifecycle/retention dimensions.

The probe models already-observed facts. It does not delete, restore or reclaim anything.
"""

from __future__ import annotations

import argparse
import json


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--workspace-present", action="store_true")
    p.add_argument("--workspace-usable", action="store_true")
    p.add_argument("--closure-record-retained", action="store_true")
    p.add_argument("--physical-removal-occurred", action="store_true")
    p.add_argument("--job-history-retained", action="store_true")
    p.add_argument("--artifact-retained", action="store_true")
    args = p.parse_args()

    impossible = []
    if args.workspace_usable and not args.workspace_present:
        impossible.append("USABLE_WITHOUT_PRESENT_WORKSPACE")

    historical_addressable = args.job_history_retained or args.artifact_retained or args.closure_record_retained
    closed_but_historical = (
        not args.workspace_present
        and not args.workspace_usable
        and args.closure_record_retained
        and args.job_history_retained
    )

    print(json.dumps({
        "dimensions": {
            "workspacePresent": args.workspace_present,
            "workspaceUsable": args.workspace_usable,
            "closureRecordRetained": args.closure_record_retained,
            "physicalRemovalOccurred": args.physical_removal_occurred,
            "jobHistoryRetained": args.job_history_retained,
            "artifactRetained": args.artifact_retained,
        },
        "historicalAddressable": historical_addressable,
        "closedButHistorical": closed_but_historical,
        "impossibleCombinations": impossible,
        "invariants": {
            "physicalReclamationImpliesHistoricalErasure": False,
            "historicalIdentityImpliesCurrentUsability": False,
            "retainedArtifactResurrectsWorkspace": False,
            "closureRecordEqualsPhysicalPresence": False,
        },
    }, indent=2, sort_keys=True))
    return 4 if impossible else 0


if __name__ == "__main__":
    raise SystemExit(main())

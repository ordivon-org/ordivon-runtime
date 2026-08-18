#!/usr/bin/env python3
"""Classify typed currentness roles without conflating transport recency and owner authority.

Research-only programme tool. Inputs are already-observed identities/horizons; this
probe does not mint authority or fetch sources.
"""

from __future__ import annotations

import argparse
import json


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--projected-authority-version-ref", required=True)
    p.add_argument("--live-authority-version-ref", required=True)
    p.add_argument("--publication-source-revision", required=True)
    p.add_argument("--local-transport-revision")
    p.add_argument("--remote-transport-revision")
    args = p.parse_args()

    if args.projected_authority_version_ref == args.live_authority_version_ref:
        projection_health = "CURRENT_TO_SOURCE"
        standing = "CURRENT_DECLARED"
    else:
        projection_health = "SOURCE_ADVANCED_STALE"
        standing = "HISTORICAL_NOT_CURRENT"

    transport_relation = "UNOBSERVED"
    if args.local_transport_revision and args.remote_transport_revision:
        transport_relation = (
            "LOCAL_EQUALS_REMOTE"
            if args.local_transport_revision == args.remote_transport_revision
            else "LOCAL_REMOTE_DIVERGED"
        )

    source_local_relation = (
        "UNOBSERVED"
        if not args.local_transport_revision
        else (
            "SOURCE_EQUALS_LOCAL_TRANSPORT"
            if args.publication_source_revision == args.local_transport_revision
            else "SOURCE_DIFFERS_FROM_LOCAL_TRANSPORT"
        )
    )
    source_remote_relation = (
        "UNOBSERVED"
        if not args.remote_transport_revision
        else (
            "SOURCE_EQUALS_REMOTE_TRANSPORT"
            if args.publication_source_revision == args.remote_transport_revision
            else "SOURCE_DIFFERS_FROM_REMOTE_TRANSPORT"
        )
    )

    print(json.dumps({
        "authorityProjection": {
            "projectedAuthorityVersionRef": args.projected_authority_version_ref,
            "liveAuthorityVersionRef": args.live_authority_version_ref,
            "projectionHealth": projection_health,
            "standing": standing,
        },
        "publicationSourceHorizon": args.publication_source_revision,
        "transportObservation": {
            "local": args.local_transport_revision,
            "remote": args.remote_transport_revision,
            "relation": transport_relation,
            "sourceVsLocal": source_local_relation,
            "sourceVsRemote": source_remote_relation,
        },
        "invariants": {
            "localTransportRecencyMintsAuthority": False,
            "remoteTransportRecencyMintsAuthority": False,
            "sourceFenceEqualityMintsCurrentStanding": False,
            "historicalIntegrityImpliesCurrentStanding": False,
        },
    }, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

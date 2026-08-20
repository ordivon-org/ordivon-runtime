#!/usr/bin/env python3
"""Probe owner authority currentness without laundering Git recency into semantic authority.

This is research-programme tooling, not production Runtime code.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
from pathlib import Path


def git(root: Path, *args: str) -> str:
    return subprocess.check_output(["git", "-C", str(root), *args], text=True).strip()


def is_ancestor(root: Path, ancestor: str, descendant: str) -> bool:
    return subprocess.run(
        ["git", "-C", str(root), "merge-base", "--is-ancestor", ancestor, descendant],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    ).returncode == 0


def publication(root: Path, authority_version_ref: str) -> tuple[Path, bytes, dict]:
    digest = authority_version_ref.removeprefix("sha256:")
    path = root / "research" / "authority" / "publications" / f"{digest}.json"
    payload = path.read_bytes()
    return path, payload, json.loads(payload)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", default=".")
    parser.add_argument("--projected-authority-version-ref")
    parser.add_argument("--tip", default="HEAD")
    args = parser.parse_args()

    root = Path(args.root).resolve()
    current = json.loads((root / "research" / "authority" / "CURRENT.json").read_text())
    live_ref = current["currentAuthorityVersionRef"]
    projected_ref = args.projected_authority_version_ref or live_ref

    projected_path, projected_bytes, projected_pub = publication(root, projected_ref)
    projected_digest_valid = (
        "sha256:" + hashlib.sha256(projected_bytes).hexdigest() == projected_ref
    )

    if projected_ref == live_ref:
        projection_health = "CURRENT_TO_SOURCE"
        standing = "CURRENT_DECLARED"
    else:
        # A retained immutable owner publication that no longer equals CURRENT is
        # historical. This is the tested Atlas stale-projection relation.
        projection_health = "SOURCE_ADVANCED_STALE"
        standing = "HISTORICAL_NOT_CURRENT"

    tip = git(root, "rev-parse", args.tip)
    source_revision = projected_pub["source"]["sourceRevision"]
    if source_revision == tip:
        source_horizon_relation = "SOURCE_FENCE_EQUALS_OBSERVED_TIP"
    elif is_ancestor(root, source_revision, tip):
        source_horizon_relation = "SOURCE_FENCE_ANCESTOR_OF_OBSERVED_TIP"
    else:
        source_horizon_relation = "SOURCE_FENCE_DIVERGED_FROM_OBSERVED_TIP"

    result = {
        "authorityRef": current["authorityRef"],
        "projectedAuthorityVersionRef": projected_ref,
        "liveAuthorityVersionRef": live_ref,
        "projectionHealth": projection_health,
        "standing": standing,
        "publication": str(projected_path.relative_to(root)),
        "publicationDigestValid": projected_digest_valid,
        "publicationSourceRevision": source_revision,
        "observedGitTip": tip,
        "sourceHorizonRelation": source_horizon_relation,
        "invariants": {
            "gitRecencyMintsSemanticAuthority": False,
            "historicalIntegrityImpliesCurrentStanding": False,
            "publicationSourceFenceEqualsAuthorityCurrentness": False,
        },
    }

    print(json.dumps(result, indent=2, sort_keys=True))

    if not projected_digest_valid:
        return 2
    if source_horizon_relation == "SOURCE_FENCE_DIVERGED_FROM_OBSERVED_TIP":
        return 3
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

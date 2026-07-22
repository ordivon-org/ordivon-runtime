#!/usr/bin/env python3
"""Validate the common identity envelope for Ordivon evidence files."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path
from typing import Any

SHA40 = re.compile(r"^[0-9a-f]{40}$")
SHA256 = re.compile(r"^sha256:[0-9a-f]{64}$")
REQUIRED = {
    "schemaVersion",
    "phase",
    "evidenceType",
    "generatedAt",
    "sourceRevision",
    "implementationCommit",
    "harnesses",
    "environment",
    "observationsDigest",
    "claimsNotMade",
}


def digest_file(path: Path) -> str:
    return "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()


def fail(path: Path, message: str) -> None:
    raise ValueError(f"{path}: {message}")


def validate(repo_root: Path, path: Path) -> None:
    try:
        evidence: dict[str, Any] = json.loads(path.read_text())
    except Exception as error:
        fail(path, f"cannot parse JSON: {error}")

    missing = sorted(REQUIRED - evidence.keys())
    if missing:
        fail(path, f"missing required envelope fields: {', '.join(missing)}")
    if evidence["schemaVersion"] != 1:
        fail(path, "schemaVersion must be 1")
    for field in ("sourceRevision", "implementationCommit"):
        if not isinstance(evidence[field], str) or not SHA40.fullmatch(evidence[field]):
            fail(path, f"{field} must be a lowercase 40-character Git SHA")
    if not isinstance(evidence["observationsDigest"], str) or not SHA256.fullmatch(
        evidence["observationsDigest"]
    ):
        fail(path, "observationsDigest must be sha256:<64 lowercase hex>")

    harnesses = evidence["harnesses"]
    if not isinstance(harnesses, list) or not harnesses:
        fail(path, "harnesses must be a non-empty array")
    for index, harness in enumerate(harnesses):
        if not isinstance(harness, dict) or set(harness) != {"path", "sha256"}:
            fail(path, f"harnesses[{index}] must contain only path and sha256")
        harness_path = (repo_root / harness["path"]).resolve()
        try:
            harness_path.relative_to(repo_root.resolve())
        except ValueError:
            fail(path, f"harnesses[{index}].path escapes repository")
        if not harness_path.is_file():
            fail(path, f"harness does not exist: {harness['path']}")
        actual = digest_file(harness_path)
        if harness["sha256"] != actual:
            fail(path, f"harness digest mismatch for {harness['path']}")

    claims = evidence["claimsNotMade"]
    if not isinstance(claims, list) or not claims or not all(
        isinstance(item, str) and item.strip() for item in claims
    ):
        fail(path, "claimsNotMade must be a non-empty string array")
    if not isinstance(evidence["environment"], dict):
        fail(path, "environment must be an object")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("paths", nargs="+", type=Path)
    parser.add_argument("--repo-root", type=Path, default=Path.cwd())
    args = parser.parse_args()
    repo_root = args.repo_root.resolve()
    try:
        for path in args.paths:
            validate(repo_root, path.resolve())
    except ValueError as error:
        print(f"EVIDENCE_ENVELOPE_FAIL: {error}", file=sys.stderr)
        return 1
    print(f"EVIDENCE_ENVELOPE_PASS files={len(args.paths)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

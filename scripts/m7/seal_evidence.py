#!/usr/bin/env python3
"""Add the common M7 evidence envelope to one raw evidence document."""

from __future__ import annotations

import argparse
import hashlib
import json
import platform
import subprocess
import time
from pathlib import Path
from typing import Any

REPO = Path(__file__).resolve().parents[2]
ENVELOPE_FIELDS = {
    "evidenceType",
    "implementationCommit",
    "harnesses",
    "environment",
    "observationsDigest",
}


def digest_bytes(data: bytes) -> str:
    return "sha256:" + hashlib.sha256(data).hexdigest()


def digest_file(path: Path) -> str:
    return digest_bytes(path.read_bytes())


def canonical(value: Any) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode()


def host_environment() -> dict[str, Any]:
    return {
        "kernel": platform.release(),
        "architecture": platform.machine(),
        "bootId": Path("/proc/sys/kernel/random/boot_id").read_text().strip(),
        "systemdVersion": subprocess.check_output(["systemctl", "--version"], text=True)
        .splitlines()[0],
        "virtualization": subprocess.run(
            ["systemd-detect-virt"], text=True, capture_output=True, check=False
        ).stdout.strip(),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--evidence-type", required=True)
    parser.add_argument("--implementation-commit", required=True)
    parser.add_argument("--harness", action="append", default=[], type=Path)
    parser.add_argument("--claim", action="append", default=[])
    args = parser.parse_args()

    raw: dict[str, Any] = json.loads(args.input.read_text())
    source = raw.get("sourceRevision")
    if source != args.implementation_commit:
        raise RuntimeError(
            f"sourceRevision {source!r} does not match implementation commit "
            f"{args.implementation_commit!r}"
        )
    harnesses = []
    for item in args.harness:
        resolved = (REPO / item).resolve()
        resolved.relative_to(REPO)
        harnesses.append({"path": str(item), "sha256": digest_file(resolved)})
    if not harnesses:
        raise RuntimeError("at least one harness is required")

    sealed = dict(raw)
    existing_claims = sealed.get("claimsNotMade", [])
    if not isinstance(existing_claims, list):
        raise RuntimeError("claimsNotMade must be an array when present")
    sealed["claimsNotMade"] = [*existing_claims, *args.claim]
    if not sealed["claimsNotMade"]:
        raise RuntimeError("at least one claimsNotMade statement is required")
    sealed["schemaVersion"] = 1
    sealed.setdefault("generatedAt", time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()))
    sealed["evidenceType"] = args.evidence_type
    sealed["implementationCommit"] = args.implementation_commit
    sealed["harnesses"] = harnesses
    sealed["environment"] = raw.get("environment", host_environment())
    observations = {key: value for key, value in sealed.items() if key not in ENVELOPE_FIELDS}
    sealed["observationsDigest"] = digest_bytes(canonical(observations))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(sealed, indent=2) + "\n")
    print(f"M7_EVIDENCE_SEALED {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

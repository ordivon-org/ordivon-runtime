#!/usr/bin/env python3
"""Detect active references to removed Ordivon entry points and stage runtimes."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from dataclasses import asdict, dataclass
from pathlib import Path


PATTERNS = (
    "ordivon-m1-cli",
    "ordivon-m4-http",
    "ordivon-m6-http",
    "ordivon-m7-http",
    "m6-transactional-runtime",
    "m7-runtime-hardening",
    "ORDIVON_M6_",
    "ORDIVON_M7_",
    "src/ordivon_governance_core",
    "src/ordivon_verify",
    "crates/ordivon-kernel",
    "crates/ordivon-ledger",
    "docker-compose.infrastructure.yml",
)

ALLOWED_PREFIXES = (
    "docs/postmortems/",
    "docs/governance/ordivon-m-series-closure-",
    "docs/architecture/post-simplification-acceptance-design.md",
)

ALLOWED_FILES = {
    "docs/operations/compatibility.md",
    "docs/operations/external-consumer-audit-2026-07-22.md",
    "scripts/legacy_reference_scan.py",
}


@dataclass(frozen=True)
class Finding:
    path: str
    line: int
    pattern: str
    text: str


def tracked_files(root: Path) -> list[Path]:
    output = subprocess.run(
        ["git", "ls-files", "-z"], cwd=root, check=True, capture_output=True
    ).stdout
    return [root / item.decode("utf-8") for item in output.split(b"\0") if item]


def allowed(relative: str) -> bool:
    return relative in ALLOWED_FILES or relative.startswith(ALLOWED_PREFIXES)


def scan(root: Path) -> list[Finding]:
    root = root.resolve()
    findings: list[Finding] = []
    for path in tracked_files(root):
        relative = path.relative_to(root).as_posix()
        if allowed(relative) or not path.is_file() or path.is_symlink():
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            continue
        for line_number, line in enumerate(text.splitlines(), start=1):
            for pattern in PATTERNS:
                if pattern in line:
                    findings.append(Finding(relative, line_number, pattern, line.strip()[:240]))
    return findings


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Scan tracked active files for removed runtime references.")
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--json", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    findings = scan(args.root)
    if args.json:
        print(json.dumps([asdict(finding) for finding in findings], indent=2, sort_keys=True))
    else:
        for finding in findings:
            print(f"{finding.path}:{finding.line}: {finding.pattern}: {finding.text}")
        print(f"legacy_reference_findings={len(findings)}")
    return 1 if findings else 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, subprocess.CalledProcessError) as error:
        print(f"legacy reference scan failed: {error}", file=sys.stderr)
        raise SystemExit(2) from error

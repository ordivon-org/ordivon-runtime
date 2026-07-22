#!/usr/bin/env python3
"""Remove only stale Ordivon temporary files and staging directories."""

from __future__ import annotations

import argparse
import shutil
import sys
import time
from pathlib import Path


def is_temporary(path: Path) -> bool:
    name = path.name
    return (name.startswith(".") and ".staging-" in name) or (name.startswith(".") and ".tmp-" in name)


def stale_candidates(root: Path, older_than_seconds: int) -> list[Path]:
    cutoff = time.time() - older_than_seconds
    candidates: list[Path] = []
    for path in root.rglob("*"):
        if path.is_symlink() or not is_temporary(path):
            continue
        try:
            modified = path.stat().st_mtime
        except FileNotFoundError:
            continue
        if modified <= cutoff:
            candidates.append(path)
    return sorted(candidates, key=lambda path: (len(path.parts), str(path)), reverse=True)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="List or remove stale .*.staging-* and .*.tmp-* entries under an explicit root."
    )
    parser.add_argument("--root", required=True, type=Path)
    parser.add_argument("--older-than-hours", type=float, default=24.0)
    parser.add_argument("--apply", action="store_true", help="Delete candidates; default is dry-run")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    root = args.root.expanduser().resolve()
    if not root.is_dir():
        raise ValueError(f"root is not a directory: {root}")
    if args.older_than_hours < 0:
        raise ValueError("older-than-hours must be non-negative")

    candidates = stale_candidates(root, int(args.older_than_hours * 3600))
    for path in candidates:
        relative = path.relative_to(root)
        print(f"{'DELETE' if args.apply else 'WOULD_DELETE'}\t{relative}")
        if not args.apply:
            continue
        if path.is_dir():
            shutil.rmtree(path)
        else:
            path.unlink(missing_ok=True)
    print(f"candidates={len(candidates)} applied={args.apply}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError) as error:
        print(f"cleanup failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error

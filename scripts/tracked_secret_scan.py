#!/usr/bin/env python3
"""Scan the exact tracked Git tree without build artifacts or old history."""

from __future__ import annotations

import argparse
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run Gitleaks against an exported exact Git tree.")
    parser.add_argument("--repo", type=Path, default=Path.cwd())
    parser.add_argument("--allow-dirty", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    repo = args.repo.expanduser().resolve()
    if shutil.which("gitleaks") is None:
        raise ValueError("gitleaks is not installed")
    status = subprocess.run(
        ["git", "status", "--porcelain"], cwd=repo, check=True, text=True, capture_output=True
    ).stdout
    if status and not args.allow_dirty:
        raise ValueError("repository is dirty; exact-head secret scan requires a clean tree")
    with tempfile.TemporaryDirectory(prefix="ordivon-gitleaks-") as temporary:
        export = Path(temporary) / "tree"
        export.mkdir()
        archive = subprocess.Popen(["git", "archive", "--format=tar", "HEAD"], cwd=repo, stdout=subprocess.PIPE)
        assert archive.stdout is not None
        extract = subprocess.run(["tar", "-xf", "-", "-C", str(export)], stdin=archive.stdout)
        archive.stdout.close()
        archive_status = archive.wait()
        if archive_status != 0 or extract.returncode != 0:
            raise subprocess.CalledProcessError(archive_status or extract.returncode, "git archive | tar")
        return subprocess.run(
            ["gitleaks", "dir", "--no-banner", "--redact", str(export)], cwd=repo
        ).returncode


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"tracked secret scan failed: {error}", file=sys.stderr)
        raise SystemExit(2) from error

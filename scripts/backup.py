#!/usr/bin/env python3
"""Create an external, consistent Ordivon Runtime control-root snapshot."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import sqlite3
import sys
import tempfile
from contextlib import closing
from datetime import datetime, timezone
from pathlib import Path


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return f"sha256:{digest.hexdigest()}"


def copy_regular_tree(source: Path, target: Path, excluded: set[Path]) -> list[dict[str, object]]:
    files: list[dict[str, object]] = []
    for path in sorted(source.rglob("*")):
        if path.is_symlink():
            raise ValueError(f"refusing to back up symlink: {path}")
        resolved = path.resolve()
        if resolved in excluded:
            continue
        relative = path.relative_to(source)
        destination = target / relative
        if path.is_dir():
            destination.mkdir(parents=True, exist_ok=True)
            continue
        if not path.is_file():
            raise ValueError(f"refusing unsupported filesystem entry: {path}")
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(path, destination)
        files.append(
            {
                "path": str(Path("control") / relative),
                "bytes": destination.stat().st_size,
                "digest": sha256_file(destination),
            }
        )
    return files


def sqlite_backup(source: Path, destination: Path) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    source_uri = f"file:{source}?mode=ro"
    with closing(sqlite3.connect(source_uri, uri=True)) as source_db:
        with closing(sqlite3.connect(destination)) as destination_db:
            source_db.backup(destination_db)
            result = destination_db.execute("PRAGMA integrity_check").fetchone()
            if result != ("ok",):
                raise sqlite3.DatabaseError(f"backup integrity check failed: {result}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Back up the SQLite Registry and optional control root into a new directory."
    )
    parser.add_argument("--database", required=True, type=Path, help="Registry SQLite file")
    parser.add_argument("--destination", required=True, type=Path, help="New snapshot directory")
    parser.add_argument("--control-root", type=Path, help="Optional control root to copy")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    database = args.database.expanduser().resolve()
    destination = args.destination.expanduser().resolve()
    control_root = args.control_root.expanduser().resolve() if args.control_root else None

    if not database.is_file():
        raise ValueError(f"database is not a regular file: {database}")
    if destination.exists():
        raise ValueError(f"destination already exists: {destination}")
    if control_root and not control_root.is_dir():
        raise ValueError(f"control root is not a directory: {control_root}")
    if control_root and destination.is_relative_to(control_root):
        raise ValueError("destination must not be inside the control root")

    destination.parent.mkdir(parents=True, exist_ok=True)
    staging = Path(tempfile.mkdtemp(prefix=f".{destination.name}.partial-", dir=destination.parent))
    try:
        registry_copy = staging / "registry.sqlite3"
        sqlite_backup(database, registry_copy)
        files: list[dict[str, object]] = [
            {
                "path": "registry.sqlite3",
                "bytes": registry_copy.stat().st_size,
                "digest": sha256_file(registry_copy),
            }
        ]
        if control_root:
            excluded = {
                database,
                Path(f"{database}-wal").resolve(),
                Path(f"{database}-shm").resolve(),
            }
            files.extend(copy_regular_tree(control_root, staging / "control", excluded))

        manifest = {
            "schemaVersion": 1,
            "createdAt": datetime.now(timezone.utc).isoformat(),
            "sourceDatabase": str(database),
            "sourceControlRoot": str(control_root) if control_root else None,
            "files": files,
        }
        (staging / "manifest.json").write_text(
            json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
        os.replace(staging, destination)
    except Exception:
        shutil.rmtree(staging, ignore_errors=True)
        raise

    print(destination)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, sqlite3.Error) as error:
        print(f"backup failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error

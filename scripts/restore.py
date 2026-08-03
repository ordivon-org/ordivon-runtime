#!/usr/bin/env python3
"""Restore a verified Ordivon Runtime snapshot into a new destination."""

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
from pathlib import Path
from typing import Any


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return f"sha256:{digest.hexdigest()}"


def safe_relative_path(value: object) -> Path:
    if not isinstance(value, str) or not value:
        raise ValueError("manifest file path must be a non-empty string")
    path = Path(value)
    if path.is_absolute() or ".." in path.parts:
        raise ValueError(f"manifest path escapes snapshot: {value}")
    return path


def load_and_verify(source: Path) -> tuple[dict[str, Any], list[tuple[Path, dict[str, Any]]]]:
    manifest_path = source / "manifest.json"
    if manifest_path.is_symlink() or not manifest_path.is_file():
        raise ValueError("snapshot manifest.json is missing or not a regular file")
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if not isinstance(manifest, dict) or manifest.get("schemaVersion") != 1:
        raise ValueError("unsupported snapshot manifest schema")
    records = manifest.get("files")
    if not isinstance(records, list) or not records:
        raise ValueError("snapshot manifest contains no files")

    verified: list[tuple[Path, dict[str, Any]]] = []
    seen: set[Path] = set()
    for record in records:
        if not isinstance(record, dict):
            raise ValueError("snapshot file record must be an object")
        relative = safe_relative_path(record.get("path"))
        if relative in seen:
            raise ValueError(f"duplicate snapshot file record: {relative}")
        seen.add(relative)
        path = source / relative
        if path.is_symlink() or not path.is_file():
            raise ValueError(f"snapshot entry is missing or not a regular file: {relative}")
        expected_bytes = record.get("bytes")
        expected_digest = record.get("digest")
        if not isinstance(expected_bytes, int) or expected_bytes < 0:
            raise ValueError(f"invalid byte length for {relative}")
        if path.stat().st_size != expected_bytes:
            raise ValueError(f"byte length mismatch for {relative}")
        if not isinstance(expected_digest, str) or sha256_file(path) != expected_digest:
            raise ValueError(f"digest mismatch for {relative}")
        verified.append((relative, record))

    if Path("registry.sqlite3") not in seen:
        raise ValueError("snapshot does not contain registry.sqlite3")
    return manifest, verified


def verify_sqlite(path: Path) -> None:
    uri = f"file:{path}?mode=ro"
    with closing(sqlite3.connect(uri, uri=True)) as database:
        result = database.execute("PRAGMA integrity_check").fetchone()
        if result != ("ok",):
            raise sqlite3.DatabaseError(f"restored registry integrity check failed: {result}")


def restore_snapshot(source: Path, destination: Path) -> Path:
    source = source.expanduser().resolve()
    destination = destination.expanduser().resolve()
    if not source.is_dir():
        raise ValueError(f"snapshot source is not a directory: {source}")
    if destination.exists():
        raise ValueError(f"destination already exists: {destination}")
    if destination.is_relative_to(source):
        raise ValueError("destination must not be inside the snapshot")

    manifest, files = load_and_verify(source)
    destination.parent.mkdir(parents=True, exist_ok=True)
    staging = Path(tempfile.mkdtemp(prefix=f".{destination.name}.restore-", dir=destination.parent))
    try:
        for relative, _record in files:
            source_file = source / relative
            destination_file = staging / relative
            destination_file.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source_file, destination_file, follow_symlinks=False)
        (staging / "manifest.json").write_text(
            json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
        verify_sqlite(staging / "registry.sqlite3")
        os.replace(staging, destination)
    except Exception:
        shutil.rmtree(staging, ignore_errors=True)
        raise
    return destination


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Restore a verified snapshot into a new directory.")
    parser.add_argument("--snapshot", required=True, type=Path)
    parser.add_argument("--destination", required=True, type=Path)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    print(restore_snapshot(args.snapshot, args.destination))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, json.JSONDecodeError, sqlite3.Error) as error:
        print(f"restore failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error

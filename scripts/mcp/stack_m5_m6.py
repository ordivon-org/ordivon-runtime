#!/usr/bin/env python3
"""Run one child command with independent M5 and M6 local MCP stacks."""

from __future__ import annotations

import argparse
import importlib.util
import os
import subprocess
import sys
from pathlib import Path
from types import ModuleType

REPO_ROOT = Path(__file__).resolve().parents[2]
M5_STATE = Path("/root/.local/share/ordivon-m5-m6-shadow/m5")
M6_STATE = Path("/root/.local/share/ordivon-m5-m6-shadow/m6")


def load_module(name: str, path: Path) -> ModuleType:
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("child", nargs=argparse.REMAINDER)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    child = args.child[1:] if args.child and args.child[0] == "--" else args.child
    if not child:
        raise RuntimeError("a child command is required")
    m5 = load_module("ordivon_stack_m5", Path(__file__).with_name("stack.py"))
    m6 = load_module("ordivon_stack_m6", Path(__file__).with_name("stack_m6.py"))
    m5_manifest = None
    m6_manifest = None
    try:
        m5_manifest = m5.start_stack(M5_STATE, None)
        m6_manifest = m6.start_stack(M6_STATE, None)
        environment = os.environ.copy()
        environment.update(m5.child_environment(m5_manifest))
        environment.update(m6.child_environment(m6_manifest))
        return subprocess.run(child, cwd=REPO_ROOT, env=environment, check=False).returncode
    finally:
        if m6_manifest is not None:
            m6.stop_stack(M6_STATE, tolerate_missing=True)
        if m5_manifest is not None:
            m5.stop_stack(M5_STATE, tolerate_missing=True)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(f"M5_M6_STACK_ERROR: {error}", file=sys.stderr)
        raise SystemExit(1)

#!/usr/bin/env python3
"""Normalize machine-readable tool reports for Agent consumption."""

from __future__ import annotations

import argparse
import json
import sys
import xml.etree.ElementTree as ET
from pathlib import Path
from typing import Any, Iterable

SCHEMA_VERSION = 1


def issue(
    *,
    level: str,
    message: str,
    file: str | None = None,
    line: int | None = None,
    column: int | None = None,
    rule_id: str | None = None,
    test: str | None = None,
) -> dict[str, Any]:
    value: dict[str, Any] = {"level": level, "message": message.strip()}
    for key, item in (
        ("file", file),
        ("line", line),
        ("column", column),
        ("ruleId", rule_id),
        ("test", test),
    ):
        if item is not None:
            value[key] = item
    return value


def result(kind: str, issues: list[dict[str, Any]], summary: dict[str, int]) -> dict[str, Any]:
    failed = summary.get("errors", 0) > 0 or summary.get("failed", 0) > 0
    return {
        "schemaVersion": SCHEMA_VERSION,
        "kind": kind,
        "status": "failed" if failed else "passed",
        "summary": summary,
        "issues": issues,
    }


def normalize_cargo(path: Path) -> dict[str, Any]:
    issues: list[dict[str, Any]] = []
    errors = warnings = 0
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip():
            continue
        try:
            event = json.loads(line)
        except json.JSONDecodeError as error:
            raise ValueError(f"invalid Cargo JSON line {number}: {error}") from error
        if event.get("reason") != "compiler-message":
            continue
        message = event.get("message") or {}
        level = str(message.get("level") or "info")
        if level == "error":
            errors += 1
        elif level == "warning":
            warnings += 1
        else:
            continue
        spans = message.get("spans") or []
        primary = next((span for span in spans if span.get("is_primary")), spans[0] if spans else {})
        issues.append(
            issue(
                level=level,
                message=str(message.get("message") or message.get("rendered") or "compiler diagnostic"),
                file=primary.get("file_name"),
                line=primary.get("line_start"),
                column=primary.get("column_start"),
                rule_id=(message.get("code") or {}).get("code"),
            )
        )
    return result("cargo-json", issues, {"errors": errors, "warnings": warnings})


def testcase_name(case: ET.Element) -> str:
    classname = case.attrib.get("classname")
    name = case.attrib.get("name", "unnamed")
    return f"{classname}::{name}" if classname else name


def normalize_junit(path: Path) -> dict[str, Any]:
    root = ET.parse(path).getroot()
    cases = list(root.iter("testcase"))
    issues: list[dict[str, Any]] = []
    failed = errors = skipped = 0
    for case in cases:
        name = testcase_name(case)
        failure = case.find("failure")
        error = case.find("error")
        skip = case.find("skipped")
        if failure is not None:
            failed += 1
            issues.append(issue(level="error", message=failure.attrib.get("message") or failure.text or "test failed", test=name))
        if error is not None:
            errors += 1
            issues.append(issue(level="error", message=error.attrib.get("message") or error.text or "test error", test=name))
        if skip is not None:
            skipped += 1
    return result(
        "junit",
        issues,
        {"tests": len(cases), "passed": len(cases) - failed - errors - skipped, "failed": failed + errors, "skipped": skipped},
    )


def normalize_sarif(path: Path) -> dict[str, Any]:
    document = json.loads(path.read_text(encoding="utf-8"))
    issues: list[dict[str, Any]] = []
    errors = warnings = notes = 0
    for run in document.get("runs", []):
        for item in run.get("results", []):
            level = str(item.get("level") or "warning")
            if level == "error":
                errors += 1
            elif level == "warning":
                warnings += 1
            else:
                notes += 1
            location = ((item.get("locations") or [{}])[0].get("physicalLocation") or {})
            region = location.get("region") or {}
            artifact = location.get("artifactLocation") or {}
            message = item.get("message") or {}
            issues.append(
                issue(
                    level=level,
                    message=str(message.get("text") or message.get("markdown") or "SARIF result"),
                    file=artifact.get("uri"),
                    line=region.get("startLine"),
                    column=region.get("startColumn"),
                    rule_id=item.get("ruleId"),
                )
            )
    return result("sarif", issues, {"errors": errors, "warnings": warnings, "notes": notes})


def walk_playwright_suites(suites: Iterable[dict[str, Any]]) -> Iterable[dict[str, Any]]:
    for suite in suites:
        yield from suite.get("specs", [])
        yield from walk_playwright_suites(suite.get("suites", []))


def normalize_playwright(path: Path) -> dict[str, Any]:
    document = json.loads(path.read_text(encoding="utf-8"))
    issues: list[dict[str, Any]] = []
    tests = failed = skipped = 0
    for spec in walk_playwright_suites(document.get("suites", [])):
        for test in spec.get("tests", []):
            tests += 1
            status = str(test.get("status") or "")
            if status in {"skipped", "interrupted"}:
                skipped += 1
                continue
            results = test.get("results") or []
            errors = [error for run in results for error in run.get("errors", [])]
            if status not in {"expected", "passed"} or errors:
                failed += 1
                location = spec.get("file") or (spec.get("location") or {}).get("file")
                line = (spec.get("location") or {}).get("line")
                message = "\n".join(str(error.get("message") or error.get("stack") or error) for error in errors) or f"unexpected status: {status}"
                issues.append(issue(level="error", message=message, file=location, line=line, test=spec.get("title")))
    return result("playwright-json", issues, {"tests": tests, "passed": tests - failed - skipped, "failed": failed, "skipped": skipped})


NORMALIZERS = {
    "cargo-json": normalize_cargo,
    "junit": normalize_junit,
    "sarif": normalize_sarif,
    "playwright-json": normalize_playwright,
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kind", required=True, choices=sorted(NORMALIZERS))
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--max-issues", type=int, default=100)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.max_issues < 0 or args.max_issues > 10_000:
        raise ValueError("max-issues must be in 0..=10000")
    report = NORMALIZERS[args.kind](args.input.expanduser().resolve())
    report["issuesTruncated"] = len(report["issues"]) > args.max_issues
    report["issues"] = report["issues"][: args.max_issues]
    print(json.dumps(report, indent=2, sort_keys=True, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, ET.ParseError, json.JSONDecodeError) as error:
        print(f"report normalization failed: {error}", file=sys.stderr)
        raise SystemExit(2) from error

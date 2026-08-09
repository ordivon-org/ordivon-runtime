#!/usr/bin/env python3
"""Validate canonical Runtime documents and generate the Tool reference."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PROJECT = ROOT / ".ordivon/project.yaml"
TOOL_SOURCE = ROOT / "crates/ordivon-runtime-mcp/src/server/tools.rs"
TOOL_REFERENCE = ROOT / "docs/reference/tools.md"

REQUIRED_FRONTMATTER = {
    "schema_version",
    "id",
    "title",
    "type",
    "profile",
    "lifecycle",
    "source_role",
    "visibility",
    "owners",
    "audience",
    "updated",
    "summary",
    "evidence_status",
    "readiness",
    "applies_to",
}
REQUIRED_README_HEADINGS = {
    "Status",
    "What it does",
    "What it does not do",
    "Requirements",
    "Quick start",
    "First Runtime workflow",
    "Responsibility boundary",
    "Security and data",
    "Documentation map",
    "Contributing",
    "License",
}
FRONTMATTER_END = "\n---\n"
LINK_PATTERN = re.compile(r"(?<!!)\[[^\]]+\]\(([^)]+)\)")
TOOL_PATTERN = re.compile(
    r'#\[tool\(\s*name\s*=\s*"([^"]+)",\s*description\s*=\s*"([^"]+)"',
    re.DOTALL,
)
DATE_PATTERN = re.compile(r"^\d{4}-\d{2}-\d{2}$")


class DocumentError(ValueError):
    pass


def parse_frontmatter(path: Path) -> tuple[dict[str, object], str] | None:
    text = path.read_text(encoding="utf-8")
    if not text.startswith("---\n"):
        return None
    end = text.find(FRONTMATTER_END, 4)
    if end < 0:
        raise DocumentError(f"{path.relative_to(ROOT)} has unterminated frontmatter")
    values: dict[str, object] = {}
    active_list: str | None = None
    for line_number, raw in enumerate(text[4:end].splitlines(), start=2):
        if not raw.strip() or raw.lstrip().startswith("#"):
            continue
        if raw.startswith("  - "):
            if active_list is None:
                raise DocumentError(
                    f"{path.relative_to(ROOT)}:{line_number} has a list item without a key"
                )
            current = values.setdefault(active_list, [])
            if not isinstance(current, list):
                raise DocumentError(f"{path.relative_to(ROOT)}:{line_number} mixes scalar and list")
            current.append(raw[4:].strip())
            continue
        match = re.fullmatch(r"([a-z][a-z0-9_]*)\s*:\s*(.*)", raw)
        if not match:
            raise DocumentError(f"{path.relative_to(ROOT)}:{line_number} has unsupported frontmatter")
        key, scalar = match.groups()
        if key in values:
            raise DocumentError(f"{path.relative_to(ROOT)}:{line_number} repeats {key}")
        active_list = key if not scalar else None
        values[key] = scalar if scalar else []
    return values, text[end + len(FRONTMATTER_END) :]


def managed_paths() -> list[Path]:
    lines = PROJECT.read_text(encoding="utf-8").splitlines()
    paths: list[Path] = []
    active = False
    for line in lines:
        if line == "managed_paths:":
            active = True
            continue
        if active and re.match(r"^[a-zA-Z_]", line):
            break
        if active and line.startswith("  - "):
            paths.append(ROOT / line[4:].strip())
    if not paths:
        raise DocumentError(".ordivon/project.yaml defines no managed_paths")
    return paths


def parse_tools() -> list[tuple[str, str]]:
    source = TOOL_SOURCE.read_text(encoding="utf-8")
    tools = [(name, " ".join(description.split())) for name, description in TOOL_PATTERN.findall(source)]
    names = [name for name, _ in tools]
    if len(tools) != 19 or len(names) != len(set(names)):
        raise DocumentError(f"expected 19 unique public Tools, found {len(tools)}")
    return sorted(tools)


def generated_tool_reference() -> str:
    rows = []
    for name, description in parse_tools():
        safe_description = description.replace("|", "\\|")
        rows.append(f"| `{name}` | {safe_description} |")
    return "\n".join(
        [
            "---",
            "schema_version: 1",
            "id: runtime.tools",
            "title: Runtime Tool Reference",
            "type: reference",
            "profile: engineering",
            "lifecycle: active",
            "source_role: generated",
            "visibility: public",
            "owners:",
            "  - ordivon-runtime",
            "audience:",
            "  - user",
            "  - builder",
            "  - agent",
            "updated: 2026-08-04",
            "summary: Generated names and descriptions for the current public Runtime MCP Tool surface.",
            "evidence_status: verified",
            "readiness: READY",
            "applies_to:",
            "  - ordivon-runtime",
            "related:",
            "  - runtime.start",
            "  - runtime.quickstart",
            "  - runtime.model",
            "---",
            "# Runtime Tool Reference",
            "",
            "## Scope",
            "",
            "This reference lists the current generated public MCP Tool names and descriptions. Exact request and result schemas remain machine-owned by discovery.",
            "",
            "## Contract",
            "",
            "The checked-in table must be reproducible from the Tool registry and must change whenever the public generated catalog changes.",
            "",
            "## Errors",
            "",
            "Invalid Tool names, arguments, versions, identities, or bounded-output requests fail through the generated schemas and Runtime error contracts; this page does not redefine them.",
            "",
            "## Compatibility",
            "",
            "Compatibility is bound by protocol lifecycle, Runtime schema, and Tool catalog digest. See the canonical Runtime model and release policy for migration obligations.",
            "",
            "This file is generated from `crates/ordivon-runtime-mcp/src/server/tools.rs`. Do not edit it by hand. Exact argument and result schemas remain machine-owned by `server/discover` and `tools/list`.",
            "",
            "Regenerate and verify:",
            "",
            "```bash",
            "python3 scripts/check_docs.py --write",
            "python3 scripts/check_docs.py",
            "```",
            "",
            "| Tool | Current description |",
            "| --- | --- |",
            *rows,
            "",
            "A Tool description is navigation, not a complete semantic contract. Runtime architecture, effect ambiguity, authority, compatibility, and recovery remain defined by the canonical documents and generated schemas.",
            "",
        ]
    )


def validate_frontmatter() -> list[str]:
    errors: list[str] = []
    ids: dict[str, Path] = {}
    managed = managed_paths()
    for path in managed:
        if not path.is_file():
            errors.append(f"managed path is missing: {path.relative_to(ROOT)}")
            continue
        parsed = parse_frontmatter(path)
        if parsed is None:
            errors.append(f"managed Markdown lacks frontmatter: {path.relative_to(ROOT)}")
            continue
        values, _ = parsed
        missing = sorted(REQUIRED_FRONTMATTER - values.keys())
        if missing:
            errors.append(f"{path.relative_to(ROOT)} lacks frontmatter keys: {', '.join(missing)}")
        identifier = values.get("id")
        if not isinstance(identifier, str) or not identifier:
            errors.append(f"{path.relative_to(ROOT)} has no scalar id")
        elif identifier in ids:
            errors.append(
                f"duplicate document id {identifier}: {ids[identifier].relative_to(ROOT)} and {path.relative_to(ROOT)}"
            )
        else:
            ids[identifier] = path
        updated = values.get("updated")
        if not isinstance(updated, str) or not DATE_PATTERN.fullmatch(updated):
            errors.append(f"{path.relative_to(ROOT)} has invalid updated date")
        if values.get("source_role") not in {"canonical", "generated"}:
            errors.append(f"managed document is not canonical/generated: {path.relative_to(ROOT)}")

    for path in sorted(ROOT.rglob("*.md")):
        if ".git" in path.parts or "target" in path.parts:
            continue
        parsed = parse_frontmatter(path)
        if parsed is None:
            continue
        identifier = parsed[0].get("id")
        if isinstance(identifier, str) and identifier in ids and ids[identifier] != path:
            errors.append(
                f"duplicate document id {identifier}: {ids[identifier].relative_to(ROOT)} and {path.relative_to(ROOT)}"
            )
        elif isinstance(identifier, str):
            ids[identifier] = path
    return errors


def validate_links() -> list[str]:
    errors: list[str] = []
    for path in sorted(ROOT.rglob("*.md")):
        if ".git" in path.parts or "target" in path.parts:
            continue
        text = path.read_text(encoding="utf-8")
        for match in LINK_PATTERN.finditer(text):
            raw_target = match.group(1).strip()
            if raw_target.startswith("<") and raw_target.endswith(">"):
                raw_target = raw_target[1:-1]
            target = raw_target.split(maxsplit=1)[0]
            if not target or target.startswith(("#", "http://", "https://", "mailto:")):
                continue
            relative = target.split("#", 1)[0].split("?", 1)[0]
            resolved = (path.parent / relative).resolve()
            try:
                resolved.relative_to(ROOT)
            except ValueError:
                errors.append(f"{path.relative_to(ROOT)} links outside repository: {target}")
                continue
            if not resolved.exists():
                errors.append(f"{path.relative_to(ROOT)} has broken local link: {target}")
    return errors


def validate_public_contracts() -> list[str]:
    errors: list[str] = []
    readme = (ROOT / "README.md").read_text(encoding="utf-8")
    headings = set(re.findall(r"^## (.+)$", readme, re.MULTILINE))
    missing = sorted(REQUIRED_README_HEADINGS - headings)
    if missing:
        errors.append("README lacks public headings: " + ", ".join(missing))
    security = (ROOT / "SECURITY.md").read_text(encoding="utf-8")
    if "/security/advisories/new" not in security:
        errors.append("SECURITY.md lacks the private advisory route")
    changelog = (ROOT / "CHANGELOG.md").read_text(encoding="utf-8")
    if "## Unreleased" not in changelog:
        errors.append("CHANGELOG.md lacks an Unreleased section")
    project = PROJECT.read_text(encoding="utf-8")
    if "enforcement: strict" not in project:
        errors.append("project manifest is not strict")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true", help="rewrite generated documentation before validation")
    args = parser.parse_args()

    expected_reference = generated_tool_reference()
    if args.write:
        TOOL_REFERENCE.parent.mkdir(parents=True, exist_ok=True)
        TOOL_REFERENCE.write_text(expected_reference, encoding="utf-8")

    errors: list[str] = []
    if not TOOL_REFERENCE.is_file():
        errors.append(f"generated Tool reference is missing: {TOOL_REFERENCE.relative_to(ROOT)}")
    elif TOOL_REFERENCE.read_text(encoding="utf-8") != expected_reference:
        errors.append("generated Tool reference is stale; run python3 scripts/check_docs.py --write")

    for validator in (validate_frontmatter, validate_links, validate_public_contracts):
        try:
            errors.extend(validator())
        except (DocumentError, OSError, UnicodeError) as error:
            errors.append(str(error))

    if errors:
        for error in sorted(set(errors)):
            print(f"docs: {error}", file=sys.stderr)
        return 1
    print("documentation contract: valid")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

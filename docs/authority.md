---
schema_version: 1
id: runtime.authority
title: Runtime Content Authority
type: decision
profile: engineering
lifecycle: active
source_role: canonical
visibility: public
owners:
  - ordivon-runtime
audience:
  - maintainer
  - builder
  - operator
  - agent
updated: 2026-08-04
summary: Decision identifying the documents and machine owners allowed to define current Runtime behavior, public status, operation, data, and release boundaries.
evidence_status: not_applicable
readiness: READY
applies_to:
  - ordivon-runtime
related:
  - runtime.start
  - runtime.quickstart
  - runtime.status
  - runtime.model
  - runtime.effect-kernel
  - runtime.operations
  - runtime.data-privacy
  - runtime.releases
---
# Runtime Content Authority

## Context

Runtime contains public entry material, architecture, operations, recovery notes, compatibility inventories, data policy, release policy, research comparisons, Host-boundary evidence, generated reference, and Agent guidance. These records do not have equal authority, and a phase or stage report must not silently redefine the current Runtime.

## Decision

| Responsibility | Canonical source |
| --- | --- |
| public repository identity and navigation | [`../README.md`](../README.md) |
| supported first-use and installation path | [`quickstart.md`](quickstart.md) |
| stable maturity, support, and known-limit claim | [`status.md`](status.md) |
| current architecture and responsibility boundary | [`runtime.md`](runtime.md) |
| effect-commit concept and admission rule | [`effect-kernel.md`](effect-kernel.md) |
| deployment, health, lifecycle, reclaim, rollback, and operational acceptance | [`operations.md`](operations.md) |
| stored data, sensitivity, retention, export, migration, and deletion | [`data-and-privacy.md`](data-and-privacy.md) |
| versions, change classes, release gates, and deprecation | [`releases.md`](releases.md) |
| current public Tool names and descriptions | generated [`reference/tools.md`](reference/tools.md), derived from server source |

Source code, migrations, generated MCP schemas, tests, live service inspection, systemd/cgroup state, Git, and deployment receipts remain stronger owners for exact fields, transitions, limits, protocol compatibility, binary identity, and current production state.

`AGENTS.md`, recovery guidance, compatibility inventories, comparison protocols, and Host-boundary reports are supporting or evidentiary records unless incorporated by a canonical document. `CHANGELOG.md` records user-visible change but does not override current contracts.

## Consequences

- Canonical documents are listed in `.ordivon/project.yaml` and checked by `scripts/check_docs.py`.
- Document IDs must be unique and local links must resolve.
- Generated reference must exactly match the current server Tool declarations.
- Stage-named documents remain evidence or history and cannot acquire authority from recency alone.
- A new canonical document must declare its responsibility, update this decision, and identify the replaced source when applicable.
- Exact production facts must be queried rather than copied into prose.

## Status

Accepted and active. Reopen when implementation ownership changes, a machine-generated source replaces prose, or two managed documents claim the same Runtime fact.

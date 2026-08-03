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
updated: 2026-08-03
summary: Decision identifying the documents and machine owners allowed to define current Runtime behavior and boundaries.
evidence_status: not_applicable
readiness: READY
applies_to:
  - ordivon-runtime
related:
  - runtime.start
  - runtime.model
  - runtime.effect-kernel
  - runtime.operations
---
# Runtime Content Authority

## Context

Runtime contains architecture, operations, recovery notes, compatibility inventories, research comparisons, Host-boundary evidence, and Agent guidance. These records do not have equal authority, and a phase or stage report must not silently redefine the current Runtime.

## Decision

[`../README.md`](../README.md) is the canonical repository entry. [`runtime.md`](runtime.md) owns the current Runtime architecture and responsibility boundary. [`effect-kernel.md`](effect-kernel.md) owns the effect-commit concept and admission rule. [`operations.md`](operations.md) owns deployment, health, lifecycle, reclaim, rollback, and operational acceptance.

Source code, migrations, generated MCP schemas, tests, live service inspection, and deployment receipts remain stronger owners for exact fields, transitions, limits, compatibility, and current production state. `AGENTS.md`, recovery guidance, compatibility inventories, comparison protocols, and Host-boundary reports are supporting or evidentiary records unless incorporated by a canonical document.

## Consequences

Only the named canonical documents enter strict content management in this adoption step. Stage-named documents remain evidence or history and cannot acquire authority from recency alone. A new canonical document must declare its responsibility, update this decision, and identify the replaced source when applicable.

## Status

Accepted and active. Reopen when implementation ownership changes, a machine-generated source replaces prose, or two managed documents claim the same Runtime fact.

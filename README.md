---
schema_version: 1
id: runtime.start
title: Ordivon Runtime
type: start
profile: organization
lifecycle: active
source_role: canonical
visibility: public
owners:
  - ordivon-runtime
audience:
  - builder
  - operator
  - agent
updated: 2026-08-04
summary: Canonical entry to the Runtime execution, evidence, reconciliation, recovery, and authority boundary.
evidence_status: not_applicable
readiness: READY
applies_to:
  - ordivon-runtime
related:
  - runtime.model
  - runtime.effect-kernel
  - runtime.operations
  - runtime.authority
---
# Ordivon Runtime

## Purpose

Ordivon Runtime is the owner-trusted effect execution and recovery subsystem of **Ordivon**. It is not the complete Ordivon system.

```text
participant purpose or Host-admitted work
→ cognition and coordination
→ Ordivon Runtime
→ durable execution
→ observable result
→ recovery or continuation
```

The Runtime owns the deterministic commitment boundary between Agent decisions and reality: Git Workspaces, stable operation identity, Jobs and Attempts, durable structured Patch receipts, at-most-once physical dispatch, process-tree ownership, bounded evidence, cancellation, reconciliation, and recovery. Higher-level cognition, planning, memory, coordination, and product interfaces belong to Ordivon or other components above the Runtime. The kernel contract is defined in [`docs/effect-kernel.md`](docs/effect-kernel.md).

## Current boundary

Runtime owns physical execution commitment and recoverable evidence under the installed service user's authority. It does not own Agent planning, semantic task completion, product policy, hostile multi-tenant isolation, or external-world authority beyond the operation contracts it can enforce.

An authenticated participant or Host uses the canonical `trusted_local` profile by default and receives the installed service user's local authority. `contained_local` is an explicit no-fallback reduction profile: it removes inherited credentials and capabilities, blocks network access, hides host state directories, and exposes only the Workspace, Attempt bundle, declared caches, Runner, and Git metadata required for source commitment. It is not a strong multi-tenant or hostile-code boundary. Run high-consequence untrusted workloads behind a disposable VM or container owned outside Runtime.

## Repository selection

| Change concerns | Use | Do not put here |
| --- | --- | --- |
| Workspace, Job, Attempt, process tree, Artifact, physical cancellation, or execution recovery | `ordivon-runtime` | Task meaning, Agent Run policy, or domain completion |
| durable Task continuity, Journal/CAS, commitment admission, verification records, or Task outcomes | `ordivon-host` | Provider loops, Harness Run semantics, or physical process truth |
| Assignment, Agent Run, Provider adapter, model–Tool loop, Tool-step checkpoint, or Run recovery | `ordivon-harness` | a second Task database, Runtime supervision, or domain-world authority |

## Start here

For repository work, start with [`AGENTS.md`](AGENTS.md). Current Runtime authority is [`docs/runtime.md`](docs/runtime.md), [`docs/effect-kernel.md`](docs/effect-kernel.md), [`docs/operations.md`](docs/operations.md), and [`docs/authority.md`](docs/authority.md). Supporting operational or compatibility guidance lives in [`docs/recovery.md`](docs/recovery.md), [`docs/agent-ux.md`](docs/agent-ux.md), and [`docs/compatibility.md`](docs/compatibility.md). [`docs/effect-comparison.md`](docs/effect-comparison.md) is a retained research protocol, and [`docs/host-boundary-stage2.md`](docs/host-boundary-stage2.md) is historical correlation evidence rather than a second current architecture.

## Identity

- Project family: **Ordivon**
- Component: **Ordivon Runtime**
- Repository: `ordivon-runtime`
- Service binary: `ordivon-runtime`
- MCP adapter: `ordivon-runtime-mcp`
- Runtime core crate: `ordivon-runtime-core`

The persisted `/var/lib/ordivon` namespace and `ORDIVON_*` environment variables remain stable because they belong to the Ordivon project family rather than one executable name.

## License

Apache-2.0. See [`LICENSE`](LICENSE).

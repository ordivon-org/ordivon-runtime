---
schema_version: 1
id: runtime.status
title: Runtime Status
type: reference
profile: organization
lifecycle: active
source_role: canonical
visibility: public
owners:
  - ordivon-runtime
audience:
  - user
  - builder
  - operator
  - agent
updated: 2026-08-04
summary: Stable maturity claim, support boundary, known limits, and live-state verification path for Ordivon Runtime.
evidence_status: verified
readiness: READY
applies_to:
  - ordivon-runtime
related:
  - runtime.start
  - runtime.quickstart
  - runtime.model
  - runtime.operations
  - runtime.releases
---
# Runtime Status

## Scope

This document states the stable public maturity, supported environment, known limits, and verification route for the current Runtime repository.

## Contract

“Operational” means the owner-trusted local execution and recovery paths are implemented and verified; “pre-1.0” means public interfaces may still change only through explicit compatibility, migration, rollback, or cutover evidence.

## Errors

This page is not live state. It must not be used to infer active Jobs, installed release-artifact digests and modes, current storage size, service health, or semantic Task completion.

## Compatibility

Current `main` and the receipt-bound previous production binary are the supported code boundaries. Protocol, Tool catalog, Runtime schema, and Registry migrations remain separately identified.

## Maturity

Ordivon Runtime is **operational for owner-trusted local engineering work** and **pre-1.0 as a public product interface**.

Operational means the repository contains a deployed service path, durable Registry and Artifact state, real systemd/cgroup supervision, reconciliation and repair procedures, receipted deployment and rollback, backup/restore, bounded health reporting, lifecycle management, and repeatable real-system acceptance.

Pre-1.0 means public Tool arguments, persisted schema, operational packaging, and installation assumptions may still change. Changes must nevertheless preserve explicit compatibility, migration, rollback, or major-cutover evidence; pre-1.0 does not permit silent state loss.

## Supported environment

The canonical deployment currently supports:

- Linux;
- systemd as PID 1;
- cgroup v2;
- an owner-trusted root or equivalently privileged service account;
- a loopback-bound MCP origin protected by a Bearer token;
- local Git repositories and executables already trusted by the operator.

Windows, macOS, non-systemd Linux, hostile multi-tenant execution, controlled egress, and disposable remote sandboxes are not currently supported Runtime authority profiles.

## Supported version

Only current `main` and the exact receipt-bound previous production binary are supported. There are no LTS or backport branches.

Compatibility is not inferred from version numbers alone. Runtime separately tracks:

- public MCP protocol lifecycle;
- Tool catalog digest;
- Runtime request schema;
- Registry migrations and checksums;
- additive compatibility storage;
- previous-binary rollback;
- deployment receipt and binary digests.

See [`compatibility.md`](compatibility.md) and [`releases.md`](releases.md).

## Current capabilities

| Area | Status |
| --- | --- |
| Git Workspace create/read/write/patch/diff/close | operational |
| durable Job and Attempt admission | operational |
| systemd/cgroup process-tree ownership | operational |
| bounded output and Artifact retention | operational |
| observation, cancellation, reconciliation | operational |
| doctor, repair, backup, restore | operational |
| receipted deploy and rollback | operational |
| Workspace lifecycle and cache reclamation | operational |
| `contained_local` authority reduction | experimental but verified for the declared boundary |
| hostile multi-tenant sandboxing | not provided |
| semantic Task completion | outside Runtime |
| external-world effect verification | structured adapter dependent; not generic |

## Known limits

- `trusted_local` inherits the installed service user's local authority.
- `contained_local` reduces ambient authority but does not provide kernel-grade hostile-code isolation.
- source commitment is checked before spawn but is not an immutable filesystem snapshot.
- ignored files, wall-clock time, network state, and ambient external services are not included in source-state commitment.
- arbitrary command execution remains effect-opaque.
- a successful process exit is not proof of semantic completion or external-world success.
- real systemd/cgroup integration tests cannot run on ordinary hosted CI and require the explicit local acceptance path.
- public installation and versioning remain pre-1.0.

## Live state is machine-owned

This document does not copy current production counts, binary digests, active Jobs, or storage sizes. Query them from the installed system:

```bash
scripts/ordivon-runtime-status --health --json
scripts/ordivon-runtime-status --diagnose --json
ordivon-runtime-doctor inspect --database <registry> --pretty
```

A repository document may describe the support contract. Only the live service, configuration, Git state, Registry, systemd/cgroup state, deployment receipts, and digest-bound files can describe the exact current instance.

## Reopen conditions

Revisit this status when:

- a non-systemd supervisor becomes supported;
- the service stops requiring privileged trusted-local authority;
- a hostile-code isolation owner enters Runtime;
- the public interface reaches a declared 1.0 stability contract;
- persisted-state ownership or rollback policy changes;
- real-system acceptance no longer covers the production execution path.

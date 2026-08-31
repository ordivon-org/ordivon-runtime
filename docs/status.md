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
updated: 2026-08-15
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

macOS, non-systemd Linux, hostile multi-tenant execution, controlled egress, and disposable remote sandboxes are not currently supported Runtime authority profiles. `executionTarget=windows_native` is an experimental opt-in backend for configured WSL/Windows nodes, orthogonal to the existing authority profiles; an instance without explicit Windows launcher and WSL-distribution provider facts rejects Windows admission.

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
| structured Runtime self-release (`release.apply` / `release.get`) | implemented; operator-configured, exact-replay-safe admission binds a deterministic release effect to the ordinary Job/Attempt lifecycle, while `release.get` reads deployment receipt truth without redispatch |
| Workspace lifecycle and cache reclamation | operational |
| `contained_local` authority reduction | experimental but verified for the declared boundary |
| Immutable input materialization and bound execution | implemented and real-system verified: `workspace.execBound` keeps the reduced-authority contract (`local_linux` contained-local; configured `windows_native` trusted-local limited), while `workspace.execBoundTrusted` is an explicit local-Linux-only higher-authority sibling that keeps trusted-local host/network authority but presents the same Job-owned digest-bound input set read-only at `/run/ordivon/inputs`; named authority roots remain operator configuration and input presence never implies provider/domain permission |
| stable trusted-local Cargo target presentation | implemented after cross-Workspace sccache falsification; all trusted-local Jobs see `CARGO_TARGET_DIR=/proc/self/fd/198` while each Workspace retains its own physical build-target backing, so compiler-visible path identity is stable without sharing mutable target bytes |
| `windows_native` execution target and Windows Job Object ownership | experimental opt-in backend; real Runtime Job/Attempt success, frozen Windows baseline environment, limited/elevated ordinary execution, limited-only immutable-input presentation, replay, timeout/cancel, evidence, and descendant cleanup verified on WSL/Windows |
| Runtime-owned execution-provider commitment | implemented and physically falsified on Linux Runner and Windows launcher drift; new Jobs bind provider contract/digest and fail before dispatch on mismatch; current provider-bound Linux Runner-start evidence also binds the running `/proc/self/exe` digest and Core cross-checks live systemd MainPID image identity |
| explicit Host Dependency commitments | implemented for trusted-local Linux; P2 bound Agent-declared path+SHA-256 prerequisites into operation-v6, P3 added Attempt-lifetime host-path/topology witnesses, and P4 physically demonstrated that a same-authority target can intentionally reinterpret the same pathname inside another mount namespace without changing Runtime's host view. `runtime.describe` and terminal evidence now expose continuity scope `runtime_host_namespace_path_witness`; this is not target namespace isolation, target-byte-consumption proof, immutability, or complete environment closure |
| local-Linux target executable runtime continuity | implemented as a path/topology witness established before the Runner's final executable hash and retained through each step; runtime path replacement/write/delete fails `EXECUTABLE_RUNTIME_DRIFT` while shebang `__file__` and ELF `/proc/self/exe` pathname semantics remain unchanged |
| structured Workspace Patch effect | operational as the second materially different structured effect; durable pre-mutation intent/before-after digests, exact replay, and `workspace.patch.get` reconciliation exist without Job/Runner machinery |
| `runtime.describe` Agent affordance projection | implemented; projection-only discovery exposes current ceilings, executable roots, named input authorities, target/profile support, provider identity, and current Windows authority availability without selecting or dispatching work |
| hostile multi-tenant sandboxing | not provided |
| semantic Task completion | outside Runtime |
| external-world effect verification | structured adapter dependent; not generic |

## Known limits

- `trusted_local` inherits the installed service user's local authority.
- `contained_local` reduces ambient authority but does not provide kernel-grade hostile-code isolation.
- source commitment is checked before spawn but is not an immutable filesystem snapshot.
- ignored files, wall-clock time, network state, and ambient external services are not included in source-state commitment.
- execution-provider commitment covers Runtime's own Linux Runner or Windows launcher. P3 strengthens Linux Runner start identity with actual-image evidence and adds Attempt-lifetime filesystem path-drift witnesses for declared Host Dependencies and target executables, but these are witnesses rather than immutable snapshots. P4 physically proved that `trusted_local` target code can create a private mount view in which a committed pathname resolves different bytes while the Runtime host path remains unchanged; Host Dependency continuity therefore explicitly reports scope `runtime_host_namespace_path_witness`. Runtime still does not claim automatic shared-library/toolchain closure, same-authority mount/root-namespace immutability, target-byte-consumption proof, kernel semantics, network responses, or an arbitrary target's external provider contract.
- arbitrary command execution remains effect-opaque. Runtime self-release and Workspace Patch are two dedicated structured effect contracts with different physical mechanisms; their shared recovery vocabulary does not make arbitrary commands or external APIs idempotent/reconcilable by analogy.
- a successful process exit is not proof of semantic completion or external-world success.
- real systemd/cgroup integration tests cannot run on ordinary hosted CI and require the explicit local acceptance path.
- the R-W5 Windows target still uses systemd as the outer WSL launcher supervisor, and `systemd-run` environment expansion is explicitly disabled so Agent `$...` arguments remain literal. Recovery semantics are now physically closed for Runtime reconstruction, launcher death, and WSL distro termination. Runtime reconstruction while the launcher lives reattaches the same Attempt. Natural Windows execution whose launcher unit/process lineage is gone with no result is a deterministic `failed` execution because `KILL_ON_JOB_CLOSE` means the native tree cannot still be running; exact replay never redispatches it. WSL2 kernel `boot_id` is not treated as a distro-instance identity because two real terminate/restart experiments kept it unchanged.
- Windows admission remains `trusted_local` and single-command at the Runtime profile layer. `windowsAuthority=limited` is default and produces a Primary, non-elevated, no-higher-than-Medium token with Administrators not enabled. `windowsAuthority=elevated` is explicit, operation-identity-bound selection of an already-elevated provider token and is accepted only when effective evidence proves Primary/elevated/High/Admin-enabled authority for the same SID. The bounded Windows baseline environment is frozen at admission for either class. Every active Windows Attempt automatically owns a launcher-scoped `SystemRequired` Power Request with the Attempt ID in its reason; terminal result publication follows lease release, while process-handle teardown covers cancel/crash cleanup. Long Paths admission evidence remains unimplemented. Immutable-input Windows admission is implemented for configured `windows_native` `workspace.execBound` under `windowsAuthority=limited`; elevated input-bound admission fails closed.
- the launcher's active-process and committed-memory limits are Windows-native controls and are not claimed to be exact semantic equivalents of cgroup `TasksMax` and `MemoryMax`.
- public installation and versioning remain pre-1.0.

## Live state is machine-owned

This document does not copy current production counts, binary digests, active Jobs, or storage sizes. Query them from the installed system:

```bash
scripts/ordivon-runtime-status --health --json
scripts/ordivon-runtime-status --dashboard
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

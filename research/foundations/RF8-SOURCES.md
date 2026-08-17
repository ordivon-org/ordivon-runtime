---
schema_version: 1
id: runtime.foundations.rf8.sources
title: RF8 Sources — Resource, Capacity, Constraint and Scarcity
type: reference
profile: research
lifecycle: completed
source_role: canonical
visibility: public
owners:
  - ordivon-runtime
audience:
  - researcher
  - builder
  - agent
updated: 2026-08-17
summary: Primary/canonical sources used by RF8 to separate limits, protections, reservations, pressure, overcommit, GPU allocation and current Runtime admission/resource-control semantics.
evidence_status: verified
readiness: READY
related:
  - runtime.foundations.rf8
---
# RF8 Sources — Resource, Capacity, Constraint and Scarcity

## S1 — Linux cgroup v2 resource model

Linux kernel documentation, **Control Group v2**.

Canonical source: kernel.org / docs.kernel.org.

RF8 use:

- cgroups distribute system resources hierarchically;
- limits are upper consumption bounds and may be overcommitted, so sum of child limits may
  exceed parent capacity;
- protections are distinct from limits and can provide hard or best-effort protected
  service;
- `cpu.max` expresses maximum CPU bandwidth over a period rather than exclusive CPU
  ownership;
- memory and PID controllers have their own state/accounting semantics;
- directly supports `ConfiguredLimit != ReservedCapacity`,
  `CPUQuota != ExclusiveCPUReservation` and the multi-axis resource-control model.

## S2 — Linux Pressure Stall Information

Linux kernel documentation, **PSI — Pressure Stall Information**.

Canonical source: kernel.org.

RF8 use:

- PSI measures time workloads are stalled because CPU, memory or I/O resources are
  contended;
- distinguishes partial (`some`) from full workload stalls;
- motivates `Pressure != Utilization` and the idea that scarcity impact is about lost
  progress rather than raw resource occupancy;
- documents dynamic management use cases such as load shedding and migration without
  implying that PSI itself is a scheduler/resource manager.

## S3 — systemd resource-control ownership

systemd documentation, **systemd.resource-control** and related unit/session resource
control references.

Canonical source: freedesktop.org.

RF8 use:

- `MemoryMax=`, `TasksMax=` and CPU controls are OS/unit resource-control mechanisms;
- reinforces Runtime's design of delegating physical accounting/enforcement to systemd/
  cgroup rather than implementing a second kernel-level resource controller.

## S4 — NVIDIA CUDA memory allocation/pools

NVIDIA CUDA Programming Guide and CUDA Driver/Runtime API documentation, current 2026
editions.

Canonical source: docs.nvidia.com.

RF8 use:

- GPU/device memory allocation can fail when requested memory cannot be provided;
- stream-ordered allocators and memory pools retain/reuse allocations and can change
  allocator-visible availability independently of simple physical-free-memory arithmetic;
- managed-memory systems may support oversubscription/migration on suitable hardware;
- supports `DeviceExistence != AvailableCapacity`, `Allocation != Capacity` and
  `Overcommit != InvalidConfiguration` under explicit memory-management semantics.

## S5 — Current Ordivon Runtime as empirical source

RF8 audits current Core, Registry, systemd/Windows provider code, canonical docs and tests.

### ExecutionBudget

Current `ExecutionBudget` contains:

```text
memoryMaxBytes
tasksMax
cpuQuotaPercent
```

These values are committed into the physical Execution Plan/operation identity and mapped
on Linux to systemd `MemoryMax=`, `TasksMax=` and `CPUQuota=`. Windows uses Job-native
memory, active-process and CPU-rate controls while explicitly refusing exact cross-platform
accounting equivalence.

RF8 classifies these as per-Attempt consumption ceilings, not reservations.

### Durable admission capacity

`SubmitRequest.global_limit` is checked inside the SQLite admission transaction against
`concurrency_reservations` in `active` or `held_orphaned` state. A successful admission
creates a durable reservation before physical dispatch.

Current tests prove simultaneous admissions cannot overbook the last global slot.

### Workspace capacity

Current Registry separately enforces one active/held execution per Workspace. RF8 treats
this as a cardinality-one coordination/admission resource rather than hardware capacity.

### Reservation lifetime

Reservations have states:

```text
active
held_orphaned
released
```

and are released/converged with terminal/reconciliation state transitions. Orphaned work
can intentionally hold capacity until safe release is justified.

### Immediate admission model

Current docs explicitly state capacity admission is immediate rather than queued. The
configured production global limit is currently eight and the per-Workspace limit one,
but RF8 treats numeric production settings as operational policy rather than foundation
constants.

### Current non-scheduler claim

Canonical docs explicitly state physical budgets constrain consumption and are not a
scheduler, priority system, sandbox or approval policy.

## Source discipline

Later rounds should preserve:

```text
limit != reservation
capacity != availability
quota/entitlement != physical reservation
usage != reservation
pressure != utilization
global_limit remains logical Runtime admission capacity, not host capacity
ExecutionBudget remains committed consumption ceilings, not shared-capacity reservation
resource accounting/enforcement should remain owned by OS/provider when possible
scheduler/queue/fairness semantics require separate justification
```

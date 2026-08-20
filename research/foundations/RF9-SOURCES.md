---
schema_version: 1
id: runtime.foundations.rf9.sources
title: RF9 Sources — Scheduling, Coordination, Admission and Backpressure
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
summary: Primary/canonical sources used by RF9 to separate admission, queueing, scheduling, fairness/deadline policy, flow/backpressure and current Runtime dispatch/refusal semantics.
evidence_status: verified
readiness: READY
related:
  - runtime.foundations.rf9
---
# RF9 Sources — Scheduling, Coordination, Admission and Backpressure

## S1 — Linux CFS / EEVDF scheduler documentation

Linux kernel documentation, **CFS Scheduler** and **EEVDF Scheduler**.

Canonical source: kernel.org/docs.kernel.org.

RF9 use:

- CFS models an explicit fair-share objective using virtual runtime and selects runnable
  tasks according to scheduler accounting;
- EEVDF selects eligible tasks using lag and virtual deadlines while retaining fair-share
  semantics and improving latency sensitivity;
- demonstrates that scheduling encodes objective/accounting state rather than being a
  neutral “pick next” mechanism;
- supports the distinction between kernel runnable-task scheduling and hypothetical
  Runtime Operation scheduling.

## S2 — Linux CFS bandwidth control

Linux kernel documentation, **CFS Bandwidth Control**.

RF9 use:

- CPU bandwidth quota and the task-selection scheduler are distinct mechanisms;
- aggregate over-subscription can be allowed for work-conserving behavior;
- supports `ResourceLimit != SchedulingPolicy` and the requirement to name the resource
  domain when saying work-conserving.

## S3 — Classical deadline scheduling

C. L. Liu and J. W. Layland, **Scheduling Algorithms for Multiprogramming in a Hard-Real-
Time Environment**, Journal of the ACM 20(1), 1973.

Canonical publication: ACM/JACM; university-hosted archival copies are widely available.

RF9 use:

- rate/deadline scheduling is meaningful only under assumptions about execution demand,
  period/deadline and processor feasibility;
- supports `DeadlineField != SchedulabilityProof` and `SchedulingPolicy != CapacityCreation`;
- RF9 uses this as a model contrast, not as a proposal to add EDF to Runtime.

## S4 — HTTP/2 flow control

IETF **RFC 9113 — HTTP/2**, section 5.2 and WINDOW_UPDATE semantics.

Canonical source: IETF Datatracker/RFC Editor.

RF9 use:

- flow control protects endpoints under resource constraints by bounding outstanding data;
- it is hop-by-hop and receiver-controlled;
- demonstrates that backpressure can throttle upstream production without an application-
  level waiting-work queue;
- supports `Backpressure != Queue`.

## S5 — QUIC flow control

IETF **RFC 9000 — QUIC: A UDP-Based Multiplexed and Secure Transport**, section 4.

Canonical source: IETF Datatracker/RFC Editor.

RF9 use:

- receiver flow-control credit can block a sender;
- credit sizing trades receiver resource commitment against throughput and control overhead;
- stream cancellation still requires accounting of consumed credit;
- supports bounded outstanding responsibility and `CapacityCredit != SchedulingPriority`.

## S6 — Transport flow control versus congestion control

IETF **RFC 8095 — Services Provided by IETF Transport Protocols and Congestion Control
Mechanisms**.

RF9 use:

- receiver flow control regulates sender outstanding data according to endpoint capacity;
- congestion control addresses network/path capacity and is a separate mechanism;
- supports `Backpressure/FlowControl != CongestionControl` and scoped truth ownership.

## S7 — Current Ordivon Runtime as empirical source

RF9 audits current Core, Registry, error schema, docs and tests.

### Immediate capacity refusal

New admissions count durable `active`/`held_orphaned` reservations within one SQLite
transaction. When global or Workspace capacity is exhausted, Runtime returns
`CONCURRENCY_LIMIT` with scope, active count, limit, bounded holder Job/Workspace identities,
`retryable=true` and a retry hint. No new Job/Attempt/reservation is created.

RF9 classifies this as pre-commit admission refusal and explicit upstream backpressure.

### Ordinary admission/dispatch

Ordinary new Jobs are durably admitted and reserved under the Workspace lifecycle critical
section, then the lock is released and Runtime immediately calls the physical dispatch path.
Dispatch intent is separately committed as Attempt `starting` / `dispatch_issued` before the
provider boundary is crossed.

### Specialized Runtime Release deferral

`release.apply` deliberately admits an Accepted, reserved Job without immediate dispatch so
its durable acceptance is visible before a self-replacing deployment can sever the caller's
MCP connection. Normal reconciliation later dispatches that already-admitted Attempt.

RF9 classifies this as `AdmittedDispatchDeferral`, not a capacity-waiting queue.

### Accepted cancellation and reconciliation

Current Registry can cancel an Accepted Attempt before dispatch and release its reservation.
Reconciliation can discover an Accepted Attempt and converge it through the normal dispatch
path. These mechanisms operate on already-admitted work and do not create waiting-item
scheduler semantics.

### Queue absence

Current Registry has no durable capacity-waiting queue/item table, priority/fairness/deadline
fields or scheduler selection state. Canonical docs explicitly state capacity admission is
immediate rather than queued.

## Source discipline

Later rounds should preserve:

```text
eligibility != admission != scheduling != dispatch
not-running != queued
admitted dispatch deferral != capacity-waiting queue
capacity refusal before Job creation != queued acceptance
retry hint != reservation or future-start promise
FIFO/priority/fairness/deadline are explicit scheduling policies
kernel CPU scheduling remains outside Runtime Operation scheduling
backpressure can be refusal/credit/blocking without queueing
queue introduction creates durable waiting-work ownership/recovery semantics
```

---
schema_version: 1
id: runtime.foundations.rf8.continuation
title: RF8 Continuation — Enter RF9
type: continuation
profile: research
lifecycle: active
source_role: canonical
visibility: public
owners:
  - ordivon-runtime
audience:
  - researcher
  - agent
updated: 2026-08-17
summary: Cross-conversation continuation checkpoint after RF8, preserving resource/capacity/constraint distinctions and the exact RF9 frontier on Scheduling, Coordination, Admission and Backpressure.
evidence_status: verified
readiness: READY
related:
  - runtime.foundations.rf8
  - runtime.foundations.rf8.sources
---
# RF8 Continuation — Enter RF9

## Completed

RF8 — Resource, Capacity, Constraint and Scarcity.

Primary record:

`research/foundations/RF8-RESOURCE-CAPACITY-CONSTRAINT-SCARCITY.md`

Source map:

`research/foundations/RF8-SOURCES.md`

## Durable results to carry forward

1. `Resource != Capacity != Availability != Allocation != Reservation`.
2. Resource capacity may be physical, contractual or provider-defined and must name scope,
   units and temporal model.
3. Availability is consumer/time-relative current usability, not guaranteed by a previous
   observation.
4. Entitlement/quota/limit are policy/accounting/ceiling relations and do not imply
   physical reservation.
5. Linux cgroup limit overcommit directly proves configured per-consumer limits are not
   reserved capacity.
6. Current `ExecutionBudget` is a committed per-Attempt consumption-ceiling vector:
   memoryMaxBytes, tasksMax, cpuQuotaPercent.
7. `MemoryMax != MemoryReservation`, `TasksMax != ComputeCapacityReservation`, and
   `CPUQuota != ExclusiveCPUReservation`.
8. Windows native budget controls implement corresponding resource dimensions with
   target-specific accounting rather than exact Linux equivalence.
9. timeout/output bounds are mechanical constraints but differ from physical resource
   capacity/accounting.
10. Current `global_limit` is a configured discrete Runtime admission-capacity ceiling,
    not machine CPU/RAM capacity.
11. Current `concurrency_reservations` are genuine durable slot reservations committed
    before physical dispatch and reconstructed from Registry after service restart.
12. `held_orphaned` deliberately preserves capacity under uncertainty; reservation tracks
    safety commitment rather than measured utilization.
13. Current per-Workspace one-active-execution limit is a cardinality-one coordination/
    lease resource, not hardware capacity.
14. Scarcity can be physical, contractual or policy-created; contention can exist before
    hard exhaustion.
15. Pressure is measured loss/stall from scarcity/contention and is distinct from raw
    utilization; Linux PSI is a mature example.
16. CPU, memory, disk, network, locks, GPU memory, API rate tokens and time represent
    materially different resource classes.
17. Overcommit can be valid; safety depends on reclaim/enforcement/pressure/failure
    semantics rather than aggregate limits alone.
18. Admission capacity exhaustion is pre-commit and distinct from post-admission resource
    exhaustion/limit-hit behavior.
19. Accounting, limiting, reservation/admission, scheduling and complete resource
    management are separate responsibilities.
20. Current Runtime constrains selected physical dimensions and reserves logical admission
    slots but is neither an internal scheduler nor a general multi-resource manager.
21. Pressure-aware admission, vector resource packing, priorities and fairness remain
    unjustified until current fixed immediate capacity is concretely falsified.

## RF8 compact grammar

```text
R                           resource
Cap(R,scope,t)              supported capacity
Avail(R,C,t)                current usable capacity for consumer C
Alloc(R,C,q)                actual allocation/use
Resv(R,C,q,lifetime)        held capacity claim
Ent(C,R,q)                  entitlement
Quota(C,R,scope,interval)   policy/accounting allowance
Limit(C,R,q/rate)           upper bound
Budget(O,{R_i:q_i})         committed consumption envelope
Use(C,R,t)                  observed usage
Contend({C_i},R,t)          resource competition/interference
Scarce(R,scope,t)           demand conflicts with available service
Pressure(R,scope,t)         measured stall/degradation from scarcity
Overcommit                  aggregate potential claims > simultaneous capacity
```

## Current implementation interpretation

```text
global_limit                   Runtime-global admission capacity policy
Workspace limit                one-slot coordination/admission capacity
concurrency_reservation        durable discrete slot reservation
active                         currently held reservation
held_orphaned                  conservatively held reservation under uncertainty
released                       relinquished reservation
MemoryMax                      memory consumption ceiling
TasksMax                       task-count ceiling
CPUQuota                       CPU service-rate ceiling
Windows Job resource controls  target-native enforcement of same dimensions
timeout                        temporal execution bound
stdout/stderr limits           retained-output observation bounds
systemd/cgroup                 physical accounting/enforcement owner
capacity-holder projection     reservation accounting/evidence
```

## RF9 exact frontier

Enter:

# RF9 — Scheduling, Coordination, Admission and Backpressure

Core questions:

```text
What is admission versus scheduling versus dispatch?
When resource capacity is unavailable, should work be rejected, queued, delayed, blocked,
backpressured or delegated upward?
What is a queue and who owns waiting-work identity/lifetime/cancellation?
What is scheduling policy versus coordination invariant?
What are priority, fairness, starvation, aging and deadline scheduling?
What is backpressure: refusal, bounded queue, rate feedback, flow control or all of these?
What is load shedding and how does it differ from capacity rejection?
What is work conservation and when is leaving capacity idle correct?
What does one-Workspace-at-a-time protect: resource capacity or serialization invariant?
What is dispatch ordering after RF5 partial/serialization orders?
What is a scheduler's truth owner for capacity and eligibility?
When should Runtime remain queue-less and force an upper Agent/Host to choose strategy?
```

Mandatory falsifiers:

```text
two Jobs compete for last slot with no queue
FIFO queue causes urgent Job to wait behind long batch work
priority scheduler starves low-priority work
aging fixes starvation but changes policy
shortest-job-first improves mean latency but needs duration prediction
EDF meets deadlines under feasible load but behaves badly under overload
one Workspace serializes despite abundant CPU
capacity becomes free after caller already received rejection
bounded queue fills and applies backpressure
unbounded queue converts overload into latency/memory failure
load shedding rejects low-value work under pressure
external provider rate-limit reset creates temporal admission capacity
work-conserving scheduler versus reservation kept idle for safety/recovery
cancel queued work before dispatch
Runtime restart with queued-but-never-admitted work
```

Do not enter RF10 Recovery, Retry and Exactly-Once Semantics until RF9 separates admission,
waiting/queue ownership, scheduling, dispatch and backpressure well enough that retry is not
mistaken for queueing or scheduler re-dispatch.

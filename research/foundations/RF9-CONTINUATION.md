---
schema_version: 1
id: runtime.foundations.rf9.continuation
title: RF9 Continuation — Enter RF10
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
summary: Cross-conversation continuation checkpoint after RF9, preserving admission/queue/scheduling/backpressure distinctions and the exact RF10 frontier on Failure, Recovery, Retry, Idempotency and Exactly-Once.
evidence_status: verified
readiness: READY
related:
  - runtime.foundations.rf9
  - runtime.foundations.rf9.sources
---
# RF9 Continuation — Enter RF10

## Completed

RF9 — Scheduling, Coordination, Admission and Backpressure.

Primary record:

`research/foundations/RF9-SCHEDULING-COORDINATION-ADMISSION-BACKPRESSURE.md`

Source map:

`research/foundations/RF9-SOURCES.md`

## Durable results to carry forward

1. `Eligibility != Admission != Queueing != Scheduling != Dispatch`.
2. Admission commits ownership/identity/capacity obligations; dispatch later crosses the
   physical execution-provider boundary.
3. Queue means owned waiting-work state with capacity, ordering, lifetime, cancellation,
   persistence and recovery semantics; `not running yet` is insufficient.
4. Current Runtime has no capacity-waiting queue: a full global/Workspace capacity boundary
   rejects the new request before a Job/Attempt/reservation exists.
5. `CONCURRENCY_LIMIT` is pre-commit admission refusal plus scoped capacity/holder evidence;
   it is not a queue receipt.
6. `retryAfterMs` is a retry hint, not a reservation, priority guarantee or scheduled wakeup.
7. Upper-layer retry/re-admission is distinct from Runtime queueing.
8. Current Runtime does support specialized **admitted dispatch deferral**: Runtime Release
   persists an Accepted reserved Job, then reconciliation later performs at-most-once
   dispatch. This is not a capacity-waiting scheduler queue.
9. Ordinary newly admitted Jobs are reserved then immediately sent toward dispatch after
   releasing the lifecycle critical section.
10. Scheduling chooses service among eligible owned contenders; admission control and
    physical dispatch are distinct.
11. One-active-execution-per-Workspace is a coordination/serialization invariant, not a
    hardware scheduler policy.
12. FIFO is a policy; arrival order is not urgency/value/duration truth.
13. SJF/SRT-like scheduling imports service-time prediction and prediction error.
14. Priority can starve; aging/fairness are explicit equity/value policies.
15. Linux CFS/EEVDF demonstrates fair-share/latency are explicit scheduler objectives with
    dedicated accounting; kernel CPU scheduling remains outside Runtime Operation policy.
16. Deadline scheduling needs demand/feasibility assumptions; deadlines do not create
    capacity.
17. Unbounded queues hide overload as growing latency/storage/staleness; bounded queues only
    move the overload signal boundary.
18. Backpressure can be explicit refusal, credit or blocking and does not require queueing.
19. Flow control/backpressure and network congestion control are distinct scoped mechanisms.
20. Capacity rejection is not value-aware load shedding; generic Runtime lacks a cross-domain
    value owner.
21. A Runtime queue would create new semantics for waiting-item identity, persistence,
    cancellation, expiry, revalidation, fairness, restart recovery and dispatch integration.
22. Current queue-less refusal preserves strategic ownership at Host/Agent/domain level and
    remains preferred until concrete workload evidence proves it inadequate.

## RF9 compact grammar

```text
Elig(W,t)                              current candidate eligibility
Admit(W)                               accept ownership/commit obligations
Q                                     owned waiting-work state
Sched(Candidates,Resources,Policy)     select service ordering
Dispatch(W)                            cross physical provider boundary
Coord(I)                               preserve cross-actor invariant I
BP(downstream→upstream,constraint)     propagate capacity constraint upstream
Shed(W,Policy)                         value-aware reject/drop/degrade under overload
Overload                               offered work exceeds sustainable service
WorkConserving(R,Elig)                 don't idle R while compatible eligible work waits,
                                       subject to higher invariants
```

## Current implementation interpretation

```text
Registry capacity transaction        admission arbitration
concurrency reservation              durable admission ownership/capacity claim
CONCURRENCY_LIMIT                    pre-commit refusal/backpressure
retryAfterMs                         retry hint only
capacity holder IDs                  actionable blocking evidence
Workspace one-active rule            coordination/serialization invariant
ordinary new Job                     admit → immediate dispatch attempt
Runtime Release Accepted state       specialized admitted dispatch deferral
mark_dispatch_issued                 durable at-most-once dispatch intent
reconcile Accepted                   dispatch/recovery convergence, not scheduler
cancel Accepted                      admitted-before-dispatch cancellation
no queue table/priority fields       no capacity-waiting scheduler contract
```

## RF10 exact frontier

Enter:

# RF10 — Failure, Recovery, Retry, Idempotency and Exactly-Once

Core questions:

```text
What is failure: inability to realize, observed error, violated postcondition, unknown
outcome, or several classes?
What is crash versus omission versus timeout versus rejection versus partial effect?
What is recovery versus retry versus resume versus reconciliation versus compensation?
What exactly does idempotency mean: same request identity, same operation commitment, same
external effect, or same final state?
What is at-most-once, at-least-once and exactly-once relative to admission, dispatch,
execution and Effect?
Can exactly-once physical execution be guaranteed across crash/network partitions?
What does exactly-once effect require from the Effect Owner?
When is retry safe, unsafe or merely unknown?
How do deduplication, idempotency keys and receipts relate?
What is ambiguity and what evidence closes it?
What does recovery mean for accepted-before-dispatch, starting, running, orphaned and lost?
What should survive process crash, daemon crash, OS reboot or provider outage?
What is compensation and why is it not rollback?
```

Mandatory falsifiers:

```text
response lost after admission commit
response lost after dispatch commit but before provider launch observation
provider effect committed but response lost
retry with same idempotency key
retry with different idempotency key
HTTP timeout after external POST
process exits zero after effect already partially failed
Runtime daemon crashes while Runner continues
machine reboots after opaque external effect
at-most-once dispatch leaves unknown outcome rather than duplicate launch
provider supports idempotency key but key TTL expires
transactional outbox/inbox pattern
exactly-once Kafka-style processing claim versus external side effect
compensating transaction after irreversible effect
resume checkpointed computation versus restart from beginning
```

Do not enter RF11 Observation, Evidence and Truth Surfaces until RF10 separates failure,
uncertainty, retry, recovery, reconciliation and effect idempotency well enough that evidence
is not confused with a retry policy.

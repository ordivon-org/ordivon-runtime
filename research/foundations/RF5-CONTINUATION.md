---
schema_version: 1
id: runtime.foundations.rf5.continuation
title: RF5 Continuation — Enter RF6
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
summary: Cross-conversation continuation checkpoint after RF5, preserving temporal/order/causal distinctions and the exact RF6 frontier on Authority, Capability, Permission and Control.
evidence_status: verified
readiness: READY
related:
  - runtime.foundations.rf5
  - runtime.foundations.rf5.sources
---
# RF5 Continuation — Enter RF6

## Completed

RF5 — Time, Ordering, Concurrency and Causality.

Primary record:

`research/foundations/RF5-TIME-ORDERING-CONCURRENCY-CAUSALITY.md`

Source map:

`research/foundations/RF5-SOURCES.md`

## Durable results to carry forward

1. Runtime has no universal single Time coordinate. Keep physical time, wall/realtime
   clock, monotonic duration, logical order, synchronization order and causal order
   distinct.
2. Wall timestamps support persistent/civil/evidence correlation but are not universal
   duration proofs.
3. Monotonic clocks are the preferred basis for local elapsed waits/deadlines; current
   Rust `Instant` use fits this role.
4. `OccurrenceTime != ObservationTime != RecordCommitTime`.
5. Lamport happened-before is a partial process/message/synchronization order; it is not
   equivalent to wall-clock order or complete physical causation.
6. Concurrency means incomparability under a declared partial order, not merely close or
   overlapping timestamps.
7. Scalar logical clocks/total orders can extend causal order while imposing arbitrary
   order among events that were concurrent under the partial relation.
8. Current `event_sequence` is a per-Job Registry append/serialization total order, not a
   Runtime-global chronology or external causal order.
9. `event_sequence` and `observed_at_ms` answer different questions and can disagree
   without inconsistency.
10. Serializable transactions can physically overlap while being equivalent to some
    serial history; serialization order is not physical start order.
11. Linearizability additionally preserves real-time precedence for non-overlapping
    operations.
12. Current same-key admission races correctly converge through SQLite transactional
    identity/admission semantics rather than arrival-time comparison.
13. Temporal precedence, synchronization, lineage and causality are distinct relations.
14. Chandy–Lamport consistent snapshots show distributed global state need not be one
    simultaneous wall-clock read.
15. External Effect/receipt order belongs to effect-owner/protocol semantics unless a
    stronger cross-clock relation is established.
16. Current persisted latency metrics are wall/evidence-timeline diagnostics rather than
    exact monotonic/causal duration truth.
17. Preserve partial/scoped order by default; add wider total order only for an explicit
    coordination consumer.

## RF5 compact grammar

```text
C_wall      persistent/civil time coordinate
C_mono      local elapsed/deadline coordinate
≺_reg       Registry/storage serialization order
≺_proc      per-process/program order
send→recv   communication/synchronization edge
→           happened-before partial order
||          concurrency = incomparability under declared order
≺_ser       serialization/sequential-equivalence order
≺_lin       linearization order respecting required real-time precedence
≺_commit    domain/effect-owner commit order
⇒_cause     broader causal relation, not reducible to timestamp/order alone
```

## Current implementation interpretation

```text
created/started/finished/observed_at_ms  durable wall/evidence coordinates
Rust Instant                             local monotonic control/deadline time
event_sequence                           per-Job Registry total order
SQLite transaction/row version           concurrency/serialization arbitration
Attempt state guards                     lifecycle-order constraints
systemd/Windows evidence                 synchronization/realization evidence
provider receipt                          domain-owned order/commit evidence
inspection latency                        wall/evidence diagnostic projection
```

## RF6 exact frontier

Enter:

# RF6 — Authority, Capability, Permission and Control

Core questions:

```text
What is capability: ability, possession of a token/reference, enforceable authority, or
several concepts?
What is authority and who/what grants it?
What is permission versus capability: can an actor be able but forbidden, or permitted
but unable?
What is authentication versus authorization versus delegation?
What is principal identity relative to actor/intender/executor roles from RF2?
What does control mean after RF2/RF5: ability to select intervention, exclusive control,
shared control, policy control or physical possession?
How should delegation change authority without transferring all semantic authorship?
What is ambient authority versus explicitly passed capability?
What is least authority/least privilege in a recoverable Runtime?
What does revocation mean for already committed Operations and ongoing Attempts?
Which authority is checked at proposal/admission/dispatch/effect boundaries?
What is policy: normative rule, admission predicate, risk constraint, or mechanism?
```

Mandatory falsifiers:

```text
root process technically able but policy-forbidden action
unprivileged caller permitted by policy but lacking OS capability
bearer capability token handed to another agent
Unix file descriptor versus pathname ambient lookup
Linux capability set versus application authorization
sudo/elevation request denied or granted
Windows limited versus elevated authority
expired/revoked token after Operation admission
principal A delegates one narrow operation to Agent B
same command under different principals
automated controller acts within pre-authorized policy
Host authorizes Goal but Runtime rejects concrete proposal
Runtime admits operation but external provider rejects domain authorization
malicious target inherits excessive ambient authority
read capability versus write/control capability
shared resource controlled concurrently by multiple principals
```

Do not enter RF7 Boundary/Environment/Context/Dependency until RF6 distinguishes
ability/capability, identity/authentication, permission/policy, authorization/delegation and
control sufficiently that a boundary is not mistaken for an authority model.

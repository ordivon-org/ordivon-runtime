---
schema_version: 1
id: runtime.foundations.rf4.continuation
title: RF4 Continuation — Enter RF5
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
summary: Cross-conversation continuation checkpoint after RF4, preserving layered identity/replay semantics and the exact RF5 frontier on Time, Ordering, Concurrency and Causality.
evidence_status: verified
readiness: READY
related:
  - runtime.foundations.rf4
  - runtime.foundations.rf4.sources
---
# RF4 Continuation — Enter RF5

## Completed

RF4 — Identity, Sameness, Replay and History.

Primary record:

`research/foundations/RF4-IDENTITY-SAMENESS-REPLAY-HISTORY.md`

Source map:

`research/foundations/RF4-SOURCES.md`

## Durable results to carry forward

1. Identity must name an entity kind, namespace/scope and lifetime.
2. `Identity != equality of all state/content/properties`; entities can change while
   identity persists.
3. Keep token/entity identity, type identity, content identity, structural equality,
   semantic equivalence and observational equivalence separate.
4. Content-addressing is excellent for immutable objects/materialized evidence but not a
   universal mutable-resource identity model.
5. UUID/hash/name/locator values do not themselves define identity semantics.
6. Request identity is best modeled as receiver-scoped continuity key + request semantic
   fingerprint.
7. Current `(principal, clientRequestId)` is the request-continuity namespace and current
   request/proposal digest is the canonical request-semantic fingerprint.
8. Canonicalization defines the equivalence relation before hashing; digest equality only
   has meaning under that contract.
9. Replay is **historical identity reattachment**: same request returns/continues the
   historical committed interpretation rather than interpreting current world/policy.
10. Current `jobId` and `operationDigest` have different roles: durable committed entity
    handle versus committed operational/world/provider fingerprint.
11. Attempt identity is one physical realization generation below Operation identity.
12. PID/process/unit identifiers are narrower physical-substrate identities and do not
    replace Attempt identity.
13. Launch token binds one Attempt realization lineage to its parent Operation; it is not
    either entity's identity.
14. External Effect, resource and receipt identities belong to their own owner/contracts
    and require explicit relation to Runtime Operation/Attempt identities.
15. Identity lifetime/tombstones are necessary for late retries, delete/recreate and
    response-loss recovery.
16. `Replay != Retry != ReExecution != ReObservation != Reconciliation`.
17. Retryability/transport retry does not itself permit creation of another committed
    Operation; commit/effect state governs that decision.
18. Deterministic recomputation/equal result is equivalence, not the same historical
    execution occurrence.
19. History is multi-projection: entity-state, evidence/event, lineage, source/version,
    causal and semantic Task histories are distinct.
20. Superseding evidence/state correction should preserve prior history rather than
    rewriting it.

## RF4 compact identity grammar

```text
Entity kind K
  + Identity contract I_K(scope, lifetime, persistence/distinction laws)
        ↓
Identifier / handle id_K
  + optional Fingerprint fp_K
  + purpose-relative Equivalence ~_Q
        ↓
Lineage relations among distinct identities

Request → Operation → Attempt → Effect → Receipt/Resource
```

Replay uses historical request identity; a new intended operation requires a new request
identity even when payload/wording is equal.

## Current implementation interpretation

```text
principal                request-key namespace participant
clientRequestId           caller request-continuity token
request/proposal digest   canonical request-semantic fingerprint
jobId                     durable committed-operation entity handle
operationDigest           committed operation/world/provider fingerprint
attemptId                 one realization-generation identity
launchToken               Attempt↔Operation lineage binding
unit/PID/start identities physical execution-substrate identities
Artifact ID/digest        evidence entity/content identities
release effectId          structured external deployment-effect identity
Workspace ID              mutable working-world entity identity
sourceStateDigest          world/content-state fingerprint, not Workspace identity
```

## RF5 exact frontier

Enter:

# RF5 — Time, Ordering, Concurrency and Causality

Core questions:

```text
What is time for Runtime: physical duration, wall clock, monotonic clock, logical order,
causal order or protocol sequence?
What does before/after mean when entities and histories are now distinct?
Which orders are total, partial or merely observer-imposed?
What makes two actions/executions concurrent rather than simply close in timestamp?
What is synchronization and what information does it add?
What is causality versus temporal precedence, lineage or correlation?
How do invocation/response, admission/dispatch/start/effect/receipt orders differ?
What is a consistent cut/global snapshot after RF1/RF4?
How should races, linearizability, serializability and happens-before differ?
What does Runtime's per-Job event_sequence actually order and not order?
How do deadlines/timeouts relate to clocks and physical execution?
How do retry/replay races interact with identity and commit boundaries?
```

Mandatory falsifiers:

```text
two independent operations with overlapping wall-clock intervals
same timestamp but superdense/microstep order
clock skew between Runtime and external provider
monotonic duration versus wall-clock adjustment
message send/receive establishing happens-before
two concurrent requests with same clientRequestId
linearizable concurrent object
serializable database transactions
non-serializable external effects
Chandy-Lamport distributed snapshot
process starts before log/observation arrives
external effect commits before local terminal result
receipt observed long after effect
cancel races with natural completion
provider response races with timeout
partial-order event history versus Registry total event_sequence
```

Do not enter RF6 Authority/Capability/Permission/Control until RF5 has separated temporal,
logical, causal and serialization relations well enough that authority decisions are not
mistaken for ordering mechanisms.

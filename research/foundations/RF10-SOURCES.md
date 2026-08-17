---
schema_version: 1
id: runtime.foundations.rf10.sources
title: RF10 Sources — Failure, Recovery, Retry, Idempotency and Exactly-Once
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
summary: Primary/canonical sources used by RF10 to separate idempotency, retries, RPC duplicate suppression, exactly-once scope, transaction-domain guarantees and current Runtime recovery/effect semantics.
evidence_status: verified
readiness: READY
related:
  - runtime.foundations.rf10
---
# RF10 Sources — Failure, Recovery, Retry, Idempotency and Exactly-Once

## S1 — HTTP idempotency and retry semantics

IETF / RFC Editor, **RFC 9110 — HTTP Semantics**, section 9.2.2 Idempotent Methods.

Canonical source: `https://www.rfc-editor.org/rfc/rfc9110.html`.

RF10 use:

- idempotency is defined by the intended server effect of repeated identical requests,
  not by absence of mutation;
- PUT, DELETE and safe methods are idempotent while still allowing implementation-side
  logging/history side effects;
- idempotent methods may be retried after communication failure because repeating them has
  the same intended effect even if the first request succeeded;
- non-idempotent requests should not be automatically retried unless the client has stronger
  knowledge that retry is safe or can prove the original was not applied;
- supports `Idempotent != ReadOnly`, `Timeout/Disconnect != NonOccurrence` and the principle
  that retry safety is semantic/effect-specific rather than exception-specific.

## S2 — Classic RPC duplicate suppression

Andrew D. Birrell and Bruce Jay Nelson, **Implementing Remote Procedure Calls**, ACM
Transactions on Computer Systems 2(1), 1984.

Canonical archival publication page: Microsoft Research,
`https://www.microsoft.com/en-us/research/publication/implementing-remote-procedure-calls/`.

RF10 use:

- classic RPC design uses stable call identities and server-side duplicate suppression so a
  retransmitted request after a lost acknowledgement need not execute twice;
- demonstrates that exactly-once-like call behavior depends on identity/state retained by
  the callee, not reliable transport alone;
- supports `ReliableTransport != ExactlyOnceEffect` and durable deduplication-state
  requirements.

## S3 — Apache Kafka idempotent/transactional producer semantics

Apache Kafka official **Design** and `KafkaProducer` documentation.

Canonical sources:

- `https://kafka.apache.org/43/design/design/`
- `https://kafka.apache.org/43/javadoc/org/apache/kafka/clients/producer/KafkaProducer.html`

RF10 use:

- idempotent producer prevents duplicate Kafka log entries from producer retries using
  producer identity/sequence mechanisms;
- transactional producer coordinates writes across Kafka partitions and can atomically
  update consumed offsets with produced records;
- demonstrates strong exactly-once processing semantics inside an integrated transaction/
  storage domain rather than an unbounded open world;
- supports `ExactlyOnceWithinDomain != ExactlyOnceAcrossOpenWorld`.

## S4 — Kafka Streams exactly-once scope

Apache Kafka official **Core Concepts / Exactly Once Processing** documentation.

Canonical source: `https://kafka.apache.org/` Streams documentation.

RF10 use:

- Kafka Streams exactly-once processing integrates source offsets, state-store updates and
  Kafka output writes atomically;
- the guarantee is possible because those facts are coordinated inside Kafka's own storage/
  transaction domain;
- provides a concrete counterexample to unscoped “exactly once everywhere” language.

## S5 — Current Ordivon Runtime as empirical source

RF10 audits current Core, Registry, recovery/repair code, effect-kernel docs and tests.

### Idempotent request admission

Current Registry binds `(principal, clientRequestId)` to one Job and verifies a request
identity digest on replay. Exact replay returns the same Job; changed meaning under the same
key produces `IDEMPOTENCY_CONFLICT`.

### At-most-once Attempt dispatch

`mark_dispatch_issued` atomically changes an Accepted Attempt to Starting and records
`dispatch_issued` with reason `AT_MOST_ONCE_BOUNDARY_COMMITTED` before Runtime crosses the
physical provider boundary. Ambiguous dispatch is never speculatively repeated.

### Explicit uncertainty

Current Attempt/Job terminal model includes `lost` and `orphaned`; orphaned reservations
remain held. Reconciliation can later use stronger identity-bound Runner evidence to
supersede an orphaned conclusion, preserving both terminal-evidence artifacts.

### Recovery without retry

Startup/maintenance/targeted reconciliation compares Registry state with Runner,
supervisor/process/cgroup and target-specific evidence. Canonical docs explicitly say the
maintenance loop does not redispatch ambiguous work or retry failed commands.

### Doctor/repair

Runtime Doctor proposes evidence-backed repairs when canonical Runner truth exists and
requires manual review when it does not. Repair uses state fingerprints, snapshots,
row-version checks, explicit manual-case selection and atomic batch application; stale or
corrupt evidence fails closed.

### Structured Runtime Release Effect

Release admission binds deterministic effect identity, request digest, exact Commit,
candidate-manifest digest and canonical receipt location into Job side truth. Exact replay
reattaches before current-world checks; `release.get` reads the deployment receipt without
redispatch and reports `reconciliation_required` when receipt truth is unavailable,
inconsistent or recovery/rollback failed.

### Structured Workspace Patch Effect

Workspace Patch stores durable pre-mutation intent, complete before/after digest plan and
states `prepared`, `committed`, `unknown`. Exact replay returns the committed operation;
`workspace.patch.get` reconciles physical files against before/after state and preserves
mixed state as `unknown` rather than repeating the mutation.

### Opaque arbitrary execution

Current effect-kernel classifies arbitrary `workspace.exec` as implicitly `OPAQUE` because
Runtime cannot prove its internal/external Effect identity, count or reconciliation
contract. Caller assertions cannot upgrade that class.

## Source discipline

Later rounds should preserve:

```text
failure != uncertainty
timeout/disconnect != non-occurrence
replay/reattachment != retry execution
request idempotency != external-effect idempotency
at-most-once permits zero
exactly-once claims require explicit boundary/domain/owner
transport retry safety depends on effect semantics
idempotency keys require durable owner-side binding/retention
arbitrary workspace.exec remains OPAQUE
structured effect guarantees remain adapter/domain scoped
reconciliation prefers evidence/query over blind repetition
```

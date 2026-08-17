---
schema_version: 1
id: runtime.foundations.rf12.sources
title: RF12 Sources — Composition, Transactions and Cross-Operation Atomicity
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
summary: Primary/canonical sources used by RF12 for local atomic commit, distributed transaction commit, Saga compensation, ordering/isolation distinctions and current Runtime transaction/handoff semantics.
evidence_status: verified
readiness: READY
related:
  - runtime.foundations.rf12
---
# RF12 Sources — Composition, Transactions and Cross-Operation Atomicity

## S1 — SQLite atomic commit

SQLite official, **Atomic Commit In SQLite**.

Canonical source: `https://www.sqlite.org/atomiccommit.html`.

RF12 use:

- atomic commit means all database changes within one SQLite transaction occur or none do;
- SQLite provides the appearance of instantaneous all-or-none commit even though physical
  storage writes are serialized;
- crash/power-loss recovery is part of the atomic-commit contract;
- supports current Registry-local transaction analysis and
  `LocalAtomicity != CrossOwnerAtomicity`.

## S2 — PostgreSQL transaction semantics

PostgreSQL official documentation, **BEGIN**, **COMMIT**, **ROLLBACK**, **SET TRANSACTION**,
and glossary entries for transaction/ACID.

Canonical sources:

- `https://www.postgresql.org/docs/current/sql-begin.html`
- `https://www.postgresql.org/docs/current/sql-commit.html`
- `https://www.postgresql.org/docs/current/sql-rollback.html`
- `https://www.postgresql.org/docs/current/sql-set-transaction.html`
- `https://www.postgresql.org/docs/current/glossary.html`

RF12 use:

- one transaction block groups statements into a single transaction until COMMIT/ROLLBACK;
- COMMIT makes transaction changes durable/visible under the database contract;
- ROLLBACK discards transaction-owned updates;
- isolation level is a separate transaction characteristic from atomicity;
- supports the distinction among atomicity, durability, isolation and semantic correctness.

## S3 — Gray & Lamport distributed transaction commit

Jim Gray and Leslie Lamport, **Consensus on Transaction Commit**, Microsoft Research
MSR-TR-2003-96; ACM TODS 31(1), 2006.

Canonical author/publication source:
`https://www.microsoft.com/en-us/research/publication/consensus-on-transaction-commit/`.

RF12 use:

- distributed transaction commit is agreement on commit versus abort across resource
  managers/participants;
- classic Two-Phase Commit can block when its transaction manager/coordinator fails;
- Paxos Commit replicates the commit decision and tolerates coordinator faults;
- 2PC is presented as the zero-fault-tolerance special case of Paxos Commit;
- supports `AtomicSafety != NonBlockingLiveness` and
  `ReplicatedCoordinator != CrossOwnerTransactionalParticipation`.

## S4 — Saga model

Hector Garcia-Molina and Kenneth Salem, **SAGAS**, Princeton University TR-070-87, 1987.

Canonical Princeton source:
`https://www.cs.princeton.edu/research/techreps/598`.

RF12 use:

- a long-lived transaction may be decomposed into a sequence of ordinary transactions that
  can interleave with others;
- either all subtransactions complete or compensating transactions amend partial execution;
- demonstrates that Saga semantics intentionally permit independent commits/intermediate
  visibility rather than preserving one global ACID transaction;
- supports `Saga != LongACIDTransaction` and
  `SagaCompensation != TransactionRollback`.

## S5 — Multi-transaction activities

Garcia-Molina, Salem, Gawlick, Klein and Kleissner,
**Coordinating Multi-Transaction Activities**, Princeton TR-247-90.

Canonical source: `https://www.cs.princeton.edu/research/techreps/774`.

RF12 use:

- related activities can require explicit communication, compensation, dynamic binding and
  checkpointing across multiple transactions;
- supports the broader distinction between semantic workflow/activity coordination and one
  database transaction.

## S6 — Current Ordivon Runtime as empirical source

RF12 audits current Core, Registry, Runner, structured Patch/Release contracts and Agent UX
prose.

### Admission local atomicity

Current Registry submission uses one immediate SQLite transaction to bind a new Job,
Attempt, idempotency key, concurrency reservation, Runtime-owned side truth and events. The
commit happens before physical dispatch. Commit errors are reconciled through durable
readback/rollback classification instead of assuming failure means non-commit.

### Durable handoff to dispatch

A newly committed Accepted Job may exist before any physical systemd/Windows launch. Runtime
then persists `dispatch_issued` before crossing the physical provider boundary and refuses
speculative redispatch after ambiguity. This is durable handoff + at-most-once dispatch,
not a transaction spanning SQLite and the supervisor.

### Terminal/repair transactions

Terminal commit atomically updates the Attempt terminal record, Artifact registrations,
conditions, reservation resolution, Job resolution/current-attempt state and events inside
Registry. Administrative repair can apply a selected multi-case batch atomically and rolls
back the entire Registry batch on late conflict/invariant failure.

### execPlan orchestration

Runner executes ordered steps sequentially. Steps stop on first failure by default and may
continue only with explicit `continueOnError`. Progress records completed/failed step IDs and
indices. No Runtime contract rolls back already executed process or external Effects from
prior steps.

### Workspace Patch

Patch durably records its exact before/after file plan before physical mutation and later
reconciles physical state as prepared/committed/unknown. Mixed before/after state remains
unknown rather than being represented as a successful all-or-none transaction.

### Runtime Release

Release admission atomically binds deterministic effect side truth with an Accepted Job but
deliberately does not dispatch before returning. Release receipt states include deployed,
not_committed, rolled_back and reconciliation_required. Domain rollback/recovery does not
erase the Runtime Job/Attempt/evidence history.

### Tool commitment evidence

The Agent-facing error envelope distinguishes `commitState=not_started|not_committed|
committed|unknown`. This is Runtime-operation commitment evidence and must not be inflated
into an external Effect commit claim.

## Source discipline

Later rounds should preserve:

```text
sequence != transaction
composition != atomicity
local atomicity != cross-owner atomicity
admission transaction != physical dispatch transaction
durable handoff != distributed transaction
one process != one transactional Effect
execPlan = orchestration, not ACID
stop-on-error != rollback
preflight != prepare vote
consensus cannot manufacture transactional participants
Saga compensation != rollback
outbox-like local intent commit != atomic external delivery
Runtime commitState != external Effect state
atomic primitive composition does not imply compound atomicity
not_committed != rolled_back
shared Job/clientRequest identity != shared atomic commit
AtomicityScope <= TransactionParticipantScope
```

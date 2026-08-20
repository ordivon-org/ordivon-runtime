---
schema_version: 1
id: runtime.foundations.rf16.sources
title: RF16 Sources — Cross-Domain Falsification and Runtime Foundations v1
type: reference
profile: research
lifecycle: completed
source_role: canonical
visibility: public
owners: [ordivon-runtime]
audience: [researcher, builder, agent]
updated: 2026-08-17
summary: RF16 source discipline: RF0-RF15 canonical records, their primary external sources, and current Runtime code/contracts used to falsify and freeze Runtime Foundations v1.
evidence_status: verified
readiness: READY
related:
  - runtime.foundations.rf16
  - runtime.foundations.v1
---
# RF16 Sources — Cross-Domain Falsification and Runtime Foundations v1

RF16 intentionally adds no new external theory dependency. Its task is synthesis/falsification
of the already sourced RF0-RF15 theory against current Runtime implementation.

## Canonical research corpus

Primary inputs:

```text
RF0  — Runtime problem-space/root-model reconstruction
RF1  — State, Change, Event, Behavior and Dynamics
RF2  — Action, Intent, Proposal, Command and Operation
RF3  — Execution, Realization, Effect and Consequence
RF4  — Identity, Sameness and Replay
RF5  — Time, Order, Causality and Concurrency
RF6  — Authority and Delegation
RF7  — Environment, Dependency and Closure
RF8  — Resource, Capacity, Scheduling and Backpressure
RF9  — Lifecycle, Cancellation and Termination
RF10 — Failure, Recovery, Retry, Idempotency and Exactly-once
RF11 — Observation, Evidence, Proof and Truth Surfaces
RF12 — Composition, Transactions and Cross-Operation Atomicity
RF13 — Isolation, Containment and Trust
RF14 — Determinism, Nondeterminism and Agent Action
RF15 — External World and Open-System Effects
```

Each round's `*-SOURCES.md` remains authoritative for its external primary literature.

## Current Runtime empirical sources

RF16 revalidated current code/contracts around:

- `ExecutionProposal -> RuntimeExecutionPlan`;
- request identity and `operationDigest`;
- `RuntimeJobRecord` and `AttemptRecord`;
- source-state and provider commitments;
- immutable inputs and Host Dependencies;
- Linux systemd/cgroup and Windows native Job Object realization substrates;
- Runtime authority profiles;
- durable Registry admission/terminal transactions;
- `Lost`, `Orphaned`, delivery/recovery projections;
- Artifact/evidence validation;
- structured Runtime Release and Workspace Patch receipts/reconciliation;
- permanent generic `semanticCompletionEvaluated=false` boundary.

## RF16 source rule

RF16 does not replace RF0-RF15. `RUNTIME-FOUNDATIONS-V1.md` is the frozen compression; the
individual RF records remain the derivation/evidence corpus and must be reopened only through
a stated FoundationReopenCondition.

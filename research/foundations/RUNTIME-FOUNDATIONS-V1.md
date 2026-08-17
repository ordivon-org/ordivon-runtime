---
schema_version: 1
id: runtime.foundations.v1
title: Runtime Foundations v1
type: foundation
profile: research
lifecycle: frozen
source_role: canonical
visibility: public
owners:
  - ordivon-runtime
audience:
  - reader
  - researcher
  - builder
  - agent
updated: 2026-08-17
summary: Frozen Runtime Foundations v1 distilled from RF0-RF16.
evidence_status: verified
readiness: FROZEN
related:
  - runtime.foundations.rf16
---
# Runtime Foundations v1

## Definition

Ordivon Runtime is a bounded operational commitment, realization and evidence system.

It accepts candidate Proposals, commits admitted Operations, realizes them through governed
Attempts, binds material authority/provider/dependency semantics, preserves identity-bound
evidence, and reconciles uncertainty conservatively.

It may correlate or explicitly own structured Effects where the effect contract is truly under
its authority, but it does not generically own external Reality or semantic Goals.

## Root grammar

```text
Goal / Semantic Objective             external to generic Runtime
  ↓
Proposal
  ↓ admission / resolution / authority binding
Committed Operation
  ↓
Attempt / Realization
  ↔ Resource / Dependency
  ↔ Authority / Provider / Isolation Contract
  → Effect(s), possibly zero/many/partial/opaque
  → Observation / Evidence
  → scoped Claim / Projection
  → Reconciliation when knowledge diverges or becomes ambiguous
```

## Frozen root distinctions

```text
Proposal != Operation
Operation != Attempt
Attempt != Process
Operation != Effect
Effect != Observation
Observation != Reality
Evidence != Claim
Job != Operation ontology
Workspace != security boundary
Process exit != semantic completion
Replay != re-execution
Reconciliation != retry
Compensation != rollback
Isolation != authority reduction
Determinism != hermeticity
Request acceptance != external effect completion
Provider effect success != semantic Goal success
```

## Frozen cross-round laws

### Scope Conservation

```text
ClaimScope <= ActualSupportScope
```

Corollaries:

```text
ClaimScope <= ProvenEvidenceScope
AtomicityScope <= TransactionParticipantScope
IsolationClaimScope <= EnforcedBoundaryScope
DeterminismClaimScope <= ControlledStateBoundaryScope
ExternalEffectClaimScope <= ProviderOwnedEvidenceAndControlScope
```

### Identity Conservation

Keep request, Operation, Attempt, process/provider realization, Effect, resource and receipt
identities distinct unless an owning contract proves their mapping.

### Epistemic Separation

```text
Reality != Observation != Evidence != Projection
```

Unknown, stale, partial and superseded knowledge must remain explicit.

### Authority Locality

Claims/actions remain bounded by actual delegated/enforced authority and truth ownership.

### Open-World Non-Closure

Generic execution may remain influenced by undeclared/uncontrolled world dimensions unless a
specific contract closes them.

### Recovery by Truth Convergence

After ambiguity, reconcile surviving authoritative evidence before repeating consequential
work.

### Non-Upgrading Composition

Sequencing/combining weaker mechanisms does not automatically create stronger guarantees.

## Foundation versus implementation

Foundational:

```text
Proposal
Operation
Attempt/Realization
Authority
Resource/Dependency
Effect
Observation/Evidence
Claim/Projection
Reconciliation
```

Current implementation, replaceable beneath the grammar:

```text
RuntimeJobRecord
Workspace/Git worktree
SQLite Registry
systemd/cgroup
Windows Job Object
Runner/launcher
trusted_local/contained_local names
/run/ordivon/inputs
/proc/self/fd cache presentation
current MCP schemas
```

## Runtime-owned truth

Runtime may own facts about:

```text
admission and Operation identity
Attempt/realization identity it creates
Registry state and Runtime transactions
its dispatch/control protocol
validated Artifacts/evidence
configured provider/authority commitments
structured Effects whose resource/receipt semantics it genuinely owns
```

Runtime must not generically claim ownership of:

```text
user Goal correctness
Agent intent/reasoning semantics
complete world/environment state
arbitrary external Effect state
provider truth merely because Runtime called it
human semantic outcome
cross-owner atomicity
universal deterministic replay
universal hostile-code isolation
```

## FoundationReopenConditions

Reopen only on a concrete falsifier involving one of:

1. Operation cannot represent a required Runtime responsibility without severe distortion.
2. Attempt/Realization cannot represent a required physical realization continuity model.
3. Scope Conservation is directly falsified.
4. Reality/evidence separation becomes invalid under an explicit new truth model.
5. Runtime intentionally becomes owner of generic semantic Goals.
6. Threat model changes to hostile multi-tenancy/arbitrary untrusted-code ownership.
7. Runtime actually owns a shared multi-provider transaction protocol.
8. Deterministic physical replay becomes a first-class product requirement.
9. A new context/resource model reveals a missing root category rather than only replacing
   Workspace/Git.
10. Runtime becomes authoritative owner of a formerly external Effect domain.
11. A new execution substrate cannot map into provider+authority+realization+evidence.
12. Indefinitely reactive controllers become first-class Runtime primitives and bounded
    Operation granularity fails.

Substrate replacement alone is not a Foundation reopening condition.

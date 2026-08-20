---
schema_version: 1
id: runtime.foundations.rf3.continuation
title: RF3 Continuation — Enter RF4
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
summary: Cross-conversation continuation checkpoint after RF3, preserving execution/realization/effect boundaries and the exact RF4 frontier on Identity, Sameness, Replay and History.
evidence_status: verified
readiness: READY
related:
  - runtime.foundations.rf3
  - runtime.foundations.rf3.sources
---
# RF3 Continuation — Enter RF4

## Completed

RF3 — Execution, Realization, Effect and Consequence.

Primary record:

`research/foundations/RF3-EXECUTION-REALIZATION-EFFECT-CONSEQUENCE.md`

Source map:

`research/foundations/RF3-SOURCES.md`

## Durable results to carry forward

1. `CommittedOperation != Dispatch != Execution != Realization != Effect != Consequence
   != Outcome`.
2. Execution is provisionally **contract-governed realization behavior inside a named
   execution envelope**, not process existence alone.
3. Multiple nested execution envelopes may coexist; `execution started` must name scope
   where ambiguity matters.
4. Environment/setup realization can precede target execution.
5. Current `DISPATCH_ISSUED` is an at-most-once dispatch commitment/issue boundary
   durably recorded before provider submission; Runner/target start requires separate
   evidence.
6. Realization is a relation from abstract/committed operation/specification to concrete
   behavior/structure and is broader than Job/Runner execution.
7. Realization lineage preserves identity-bound evidence of participation without
   asserting exclusive causal ownership of all consequences.
8. An external Effect is a contract-relevant external-world change relative to a declared
   effect projection and effect owner.
9. Preserve `RequestedEffect`, `ActualEffect`, `IncidentalEffect`, `Consequence` and
   `SemanticOutcome` as distinct roles.
10. `EffectOccurrence != EffectConditionSatisfied / OperationSemanticsSatisfied`; an
    idempotent replay can succeed without a fresh mutation.
11. Provider acceptance, transport success/failure, response receipt and local execution
    terminality do not by themselves determine Effect occurrence/completion.
12. Receipt is identity-bound scoped evidence, not the Effect itself; effect-owner receipt
    can outrank generic process evidence for the effect claim it owns.
13. Effects can occur before local execution later fails, or asynchronously after the
    client-side execution terminates.
14. Atomic visibility, durability and finality are separate properties.
15. `CancelExecution != RollbackEffect`; timeout is not a universal Effect failure verdict.
16. Runtime self-release and Workspace Patch jointly prove that structured Effect
    contracts may share identity/precondition/receipt/reconciliation grammar while using
    different physical realization mechanisms.
17. Workspace Patch proves `EffectOccurrence != ReceiptPublicationTime` and that
    reconciliation may inspect world state rather than replay a mixed/uncertain mutation.
18. Current Ordivon Runtime's strongest generic ownership is committed-operation
    realization lineage + execution evidence; external Effect truth exists only where a
    structured effect/domain contract gives an authoritative basis.

## RF3 compact grammar

```text
Committed Operation O
  + Execution Contract C_exec
        ↓
Dispatch Commitment
        ↓
Executor submission / acceptance / start evidence
        ↓
Execution Behavior τ_exec
        ↓
Realization Lineage L
   ┌────┴────────────────┐
   ↓                     ↓
execution evidence   Effect Owner + Effect Contract C_eff
                          ↓
                     External Effect E
                          ↓
                  Receipt / state evidence ρ
                          ↓
                    Consequences K...
                          ↓
                    Semantic Outcome Y
```

Timing need not be a simple total chain: Effect can occur before local terminalization,
receipt can arrive after Effect, and asynchronous provider processing can continue after
client execution ends.

## Current implementation interpretation

```text
Job                         committed Operation record/handle
Attempt                     one Runtime-managed realization generation
bundle/input materialization environment/spec realization before target execution
DISPATCH_ISSUED             durable at-most-once dispatch commitment/issue boundary
systemd/Windows submission  executor/provider handoff
runner-start/windows-start   realization-lineage start evidence
bind_running                Runtime projection that start identity is bound
process tree / Windows Job   owned execution substrate
result.json                  execution-result evidence
terminal evidence            execution/ownership evidence synthesis
executionDisposition         execution-layer terminal classification
deliveryDisposition          certainty/convergence of Runtime physical result
self-release receipt         deployment Effect-owner evidence
workspace.patch receipt      direct filesystem Effect commitment/reconciliation
Host Task outcome            semantic Outcome outside generic Runtime truth
```

## RF4 exact frontier

Enter:

# RF4 — Identity, Sameness, Replay and History

Core questions:

```text
What is identity, and identity of what?
What distinguishes type identity, request identity, Operation identity, Attempt identity,
Effect identity, resource identity and evidence identity?
When are two descriptions/snapshots the same entity versus merely equivalent?
Is sameness intrinsic, relation-relative, observer-relative or contract-relative?
What survives when state changes while entity identity persists?
What survives when implementation/provider changes while an old Operation must remain replayable?
What is replay versus retry versus re-execution versus re-observation versus reconciliation?
Does replay mean reproduce behavior, reproduce result, reattach to historical identity, or re-enact effects?
When may one Operation have multiple Attempts without becoming a different Operation?
How should idempotency-key scope/lifetime relate to effect/resource identity?
What is history: ordered evidence, entity lineage, event log, causal history, or several distinct projections?
When should an old request return historical truth rather than reinterpret current Reality?
```

Mandatory falsifiers:

```text
same command text after Workspace/source drift
same clientRequestId with changed parameters
same proposal under different principal/authority/provider
exact request replay after Workspace was changed or closed
same bytes/content at two distinct resource identities
resource created, then deleted, then a very late idempotent retry arrives
retry with same provider effect token after response loss
two textually identical market orders intentionally submitted as separate operations
one committed Operation with two explicitly justified Attempts
same higher Operation after process restart/new PID
Effect receipt recovered after response loss without repeating Effect
Human says “do it again” with same semantic wording but a new intention/request
pure deterministic computation repeated and producing identical output
mutable resource renamed/aliased while identity persists or changes
Runtime/provider upgrade after old Operation admission while exact historical replay remains required
```

Do not enter RF5 Time/Ordering/Concurrency/Causality until RF4 has separated identity,
equivalence, history, replay, retry and re-execution well enough that temporal ordering is
not defined over ambiguous entities.

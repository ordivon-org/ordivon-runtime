---
schema_version: 1
id: runtime.foundations.rf14.continuation
title: RF14 Continuation — Enter RF15
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
summary: Cross-conversation checkpoint after RF14, preserving determinism/reproducibility/replay/hermeticity distinctions and the exact RF15 frontier on External World and Open-System Effects.
evidence_status: verified
readiness: READY
related:
  - runtime.foundations.rf14
  - runtime.foundations.rf14.sources
---
# RF14 Continuation — Enter RF15

## Completed

RF14 — Determinism, Nondeterminism and Agent Action.

Primary record:

`research/foundations/RF14-DETERMINISM-NONDETERMINISM-AGENT-ACTION.md`

Source map:

`research/foundations/RF14-SOURCES.md`

## Durable results to carry forward

1. Same Operation identity does not imply same result or same physical realization.
2. Determinism is relative to a modeled state/input/semantic boundary.
3. Repeatability, reproducibility, request replay, re-execution and result reproduction are
   distinct.
4. Reproducibility requires a declared source/environment/instruction contract and output
   equivalence relation.
5. Hermeticity is dependency closure/isolation from undeclared ambient influence and is
   distinct from deterministic computation.
6. Dependency closure/sandboxing strongly helps reproducibility but residual nondeterminism
   such as timestamps/randomness can remain.
7. Current exact replay returns the original durable Job/history and deliberately does not
   physically re-execute.
8. A new physical re-execution is a new historical action and may observe a changed world.
9. Current Operation identity binds important explicit proposal/source/environment/executable/
   provider/input semantics but is not a complete world-state key.
10. `sourceStateDigest` deliberately excludes ignored caches/build outputs; source identity
    is not complete execution-state identity.
11. Ignored state may remain causally relevant if target code reads it.
12. Executable/provider commitments close selected realization machinery but not automatic
    transitive toolchain/kernel/CPU/external-service state.
13. Host Dependencies and immutable inputs close selected dependency classes at increasing
    strength without becoming universal environment closure.
14. Environment construction/fixing reduces ambient variation; environment map remains only
    one execution-state dimension.
15. Wall clock remains open target-observable input; Runtime timestamps/deadlines do not
    virtualize target time.
16. Kernel RNG is a deliberate nondeterministic input and can vary under otherwise equivalent
    execution conditions.
17. Fixed seeds control one random stream/state dimension but not concurrency/scheduling,
    platform or other randomness sources.
18. Internal scheduling/races can make identical explicit inputs realize different histories
    or outputs.
19. `execPlan` fixes ordered control structure, not internal/process/result determinism.
20. Filesystem enumeration/metadata/path and cache/persistent state can all be material hidden
    inputs depending on workload.
21. Network/external service responses remain open-world inputs absent version/snapshot/
    recording contracts.
22. Recording a response can turn one live input into immutable historical input, but
    universal deterministic replay is costly and effect-sensitive.
23. Agent/model policy introduces higher-layer nondeterminism; Runtime freezes the submitted
    mechanical proposal rather than the reasoning process that selected it.
24. Runtime should often preserve actual nondeterministic realization evidence rather than
    eliminate all variation.
25. Determinism/reproducibility are orthogonal to semantic correctness, reliability and
    auditability.
26. Nondeterminism may be intentional/useful; control only dimensions material to the
    consumer contract.
27. Freezing more state has compatibility/freshness/storage/complexity cost.
28. A consumer-specific determinism budget identifies which dependencies must be fixed,
    recorded or virtualized.
29. Host currently owns reproduction/falsification experiments across multiple Runtime
    Operations; Runtime supplies exact mechanical evidence.
30. Governing law: `DeterminismClaimScope <= ControlledStateBoundaryScope`.

## RF14 compact grammar

```text
F_C(S,I) -> O              deterministic mapping under C
F_C(S,I) -> {O1...On}      nondeterministic mapping under C
Repeat_C(E)                repeated-trial equivalence
Reproduce_C(E,~Q)          independent reproduction under declared contract
Hermetic_C(E)              no undeclared ambient dependency influence
RequestReplay              retrieve/re-observe same historical Operation
ReExecution                new physical realization/historical action
ResultReproduction         new realization intended to match prior result
DeterminismBudget          state dimensions required to control for claim
```

## Current implementation interpretation

```text
clientRequestId replay          historical continuity, no rerun
sourceStateDigest               scoped source identity
ignored cache/build state       outside source identity
executable digest               primary executable bytes only
provider commitment             Runtime-owned realization machinery identity
explicit env/fixed projections  reduced ambient variation
Host Dependencies               selected mutable dependency commitment/witness
immutable inputs                exact named input-byte closure
trusted/contained caches        persistent execution-world state
wall clock                      open input
kernel RNG                      open nondeterministic input
thread/process scheduling       open internal nondeterminism
kernel/CPU/toolchain closure    not generic Runtime contract
network/external service        open world
execPlan                        ordered control, nondeterministic constituent execution
Release effect ID               deterministic identity, variable outcome
Patch plan                      deterministic expected transition, variable realization
Agent/model policy              higher-layer nondeterminism outside Runtime authority
```

## RF15 exact frontier

Enter:

# RF15 — External World and Open-System Effects

Core questions:

```text
What makes a system open rather than closed?
What is an external Effect versus local state transition?
Where exactly does Runtime's authority end and domain/provider authority begin?
What is a command/request versus an observed external Effect?
How can an Effect be identified when the external world changes independently?
What is a receipt, query, callback, webhook or state observation in an open system?
What does reconciliation mean when neither Runtime nor Host owns the external state?
How do partial fills, eventual consistency, rate limits and asynchronous completion change
Effect semantics?
What does causation mean when external actors can also modify the same resource?
Can an external Effect be idempotent, reversible, compensatable or exactly-once under a
provider contract?
What is freshness/staleness for external truth?
What does Runtime know after network timeout/connection reset/provider 500?
How should financial, cloud, deployment, email and human-world actions differ?
What is effect authority versus observation authority versus semantic authority?
```

Mandatory falsifiers:

```text
HTTP POST accepted then response lost
payment provider idempotency key
market order partial fill over time
cancel order racing with fill
cloud create returns operation handle and completes asynchronously
email API accepts message but downstream delivery/bounce occurs later
GitHub API mutation plus later human edit
DNS/network timeout after server commit
provider read API is eventually consistent
external resource changed by another actor between observations
webhook arrives before local polling sees state
receipt says accepted but final semantic outcome differs
provider retires idempotency key/history before Runtime replay horizon
human action triggered by a message with no machine receipt
```

Do not enter RF16 cross-domain falsification until RF15 can state what Runtime can prove,
reconcile and control across truly external owners without collapsing request, provider
acceptance, external transition and semantic outcome.

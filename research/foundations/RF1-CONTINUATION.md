---
schema_version: 1
id: runtime.foundations.rf1.continuation
title: RF1 Continuation — Enter RF2
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
summary: Cross-conversation continuation checkpoint after RF1, preserving the state/dynamics grammar and the exact RF2 frontier on Action, Intent, Proposal, Command and Operation.
evidence_status: verified
readiness: READY
related:
  - runtime.foundations.rf1
  - runtime.foundations.rf1.sources
---
# RF1 Continuation — Enter RF2

## Completed

RF1 — State, Change, Event, Transition, Behavior and Dynamics.

Primary record:

`research/foundations/RF1-STATE-CHANGE-EVENT-BEHAVIOR-DYNAMICS.md`

Source map:

`research/foundations/RF1-SOURCES.md`

## Durable results to carry forward

1. `State != Reality`. A model state is boundary/question-relative information adequate for selected future-evolution reasoning under a declared model.
2. State can be nonunique, nonminimal, hidden and history-derived.
3. `State != Observation != Estimate/Record`.
4. A flat operational state enum can be a useful lossy control projection over multiple underlying dimensions.
5. Change is projection-relative; `no modeled change != no world change`.
6. TLA-style stuttering is a key refinement intuition: wider/lower-level activity may occur while selected higher-level variables remain unchanged.
7. Continuous dynamics falsifies a universal discrete-transition ontology.
8. Event is a modeled distinguished occurrence/boundary in behavior, not a guaranteed canonical atom of Reality.
9. `Event != Transition != Event Record`; timestamp does not define event identity.
10. Transition/step granularity and atomicity are abstraction-relative.
11. Behavior/trajectory is more general than current state, finite lifecycle or event sequence.
12. Dynamics constrains possible behaviors; one behavior/trajectory is one realization/sample of those possibilities.
13. Dynamics can be deterministic, nondeterministic, stochastic, continuous, discrete or hybrid.
14. System/environment/input partition is boundary-relative and assumptions about environment belong in the model contract.
15. Transition relation and temporal order do not by themselves prove causation.
16. Current Ordivon `AttemptState` is an operational projection, not the Runtime Foundations state ontology.

## RF1 compact dynamic grammar

```text
World history H_<=t
  ↓ boundary/question-relative state abstraction σ_(B,Q)
State s_t
  + Inputs / Environment
  + Dynamics D
  ↓
Admissible behavior space
  ↓
One behavior / trajectory τ
  ├─ optional event extraction E(τ)
  └─ observation O(τ) → estimate / record ŝ
```

A discrete transition relation `T ⊆ S×S` is one optional representation inside this grammar, not the universal substrate.

## Current implementation interpretation

Carry this mapping without using it as ontology:

```text
AttemptState                 operational lifecycle/control projection
JobProjection dimensions     partially decomposed state projections
job_events                   ordered Runtime evidence/history records
systemd/cgroup                physical realization/process-tree observations
Result/Artifacts              evidence projections
external receipts             domain/effect-owner evidence
```

## RF2 exact frontier

Enter:

# RF2 — Action, Intent, Proposal, Command and Operation

Core questions:

```text
What distinguishes spontaneous/allowed system evolution from Action?
Does Action require an Agent, controller, policy, authorship or intent?
Can an automatic feedback controller act without phenomenal/reflective intent?
What is Intent versus Goal versus Commitment?
What is a Proposal before it becomes admissible/selected?
What is a Command: representation, speech act, control input or authority-bearing request?
What is an Operation, and why is it not merely command text or one transition?
Where does identity enter before physical realization?
How do delegated, joint and automated actions challenge authorship?
What survives across process execution, API calls, robotics, finance and human delegation?
```

Mandatory falsifiers:

```text
spontaneous physical evolution
random/nondeterministic program branch
thermostat feedback
cron/timer-triggered automation
human explicit command
Agent Tool proposal rejected before admission
same command text under different authority/context
remote API request accepted but not effected
robot command followed by plant dynamics
financial order instruction with partial fills
multi-step operation with one user intention
human delegation through an Agent/tool chain
malicious/accidental effect without corresponding intent
```

Do not enter RF3 Execution/Realization/Effect until RF2 separates `Action`, `Intent`, `Proposal`, `Command`, `Operation`, `Control Input` and `Commitment` well enough that execution does not inherit their ambiguity.

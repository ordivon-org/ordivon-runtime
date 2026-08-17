---
schema_version: 1
id: runtime.foundations.rf2.continuation
title: RF2 Continuation — Enter RF3
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
summary: Cross-conversation continuation checkpoint after RF2, preserving the action/proposal/operation grammar and the exact RF3 frontier on Execution, Realization, Effect and Consequence.
evidence_status: verified
readiness: READY
related:
  - runtime.foundations.rf2
  - runtime.foundations.rf2.sources
---
# RF2 Continuation — Enter RF3

## Completed

RF2 — Action, Intent, Proposal, Command and Operation.

Primary record:

`research/foundations/RF2-ACTION-INTENT-PROPOSAL-COMMAND-OPERATION.md`

Source map:

`research/foundations/RF2-SOURCES.md`

## Durable results to carry forward

1. `WorldChange/TransitionOccurrence/Cause != Action`.
2. Action is provisionally an attribution over selected behavior/intervention relative to
   an actor/controller/action model.
3. Preserve `Control Action`, `Intentional Action` and `Operational Action/Operation` as
   non-equivalent projections.
4. Selection can be automatic, policy-based or stochastic; `Selection != Deliberation`.
5. `Goal != Intent`. Goal names a target; Intent is a planning/practical commitment that
   constrains subsequent selection/planning.
6. Intent does not guarantee action and is not required for all engineering control
   action.
7. Intentional/planning commitment and Runtime operational commitment are distinct.
8. Proposal is a communicated candidate operational specification and may be rejected
   before any committed Operation exists.
9. Command is an encoded directive/request interpreted by a receiver; `Command !=
   Action != Operation != Effect`.
10. Planning Operator/Plan and physical Execution/Outcome remain distinct.
11. Decompose `Operation` when identity matters into:

```text
Operation Type / Contract
Operation Request / Proposal Instance
Committed Operation Instance
```

12. `Operation != Transition != Effect != Attempt`.
13. Operation identity is richer than command text and may bind principal, world/context,
    preconditions, environment, authority, provider and dependencies.
14. Delegated action requires role separation: Goal Owner, Intender/Planner, Proposer,
    Principal, Admitter, Controller/Dispatcher, Executor/Realizer, Effect Owner and
    Observer/Verifier.
15. Admission validates/commits an operational contract; it does not prove psychological
    intent, Goal wisdom, semantic approval or external effect success.
16. Current `ExecutionProposal → RuntimeExecutionPlan → Job/operationDigest` is strongly
    supported as a proposal/request → operational commitment pipeline.

## RF2 compact grammar

```text
Goal G
  ↕
Intent I / Policy π
  ↓ selection
Control Action / Candidate Course
  ↓ representation
Proposal / Command
  ↓ interpretation + validation/admission
Operation Request Instance
  ↓ stable system commitment
Committed Operation Instance
  ↓
RF3: Execution / Realization / Effect / Consequence
```

Not every system traverses every layer.

## Current implementation interpretation

```text
ExecutionProposal       candidate operational specification
clientRequestId          stable request/proposal-instance continuity identity
proposal/request digest authored/delegated request identity
RuntimeExecutionPlan    concrete resolved realization specification
operationDigest          committed Operation identity digest
Job                      durable operational commitment record/handle
Attempt                  physical realization generation, studied later
principal                operational authenticated role, not sole Actor ontology
```

## RF3 exact frontier

Enter:

# RF3 — Execution, Realization, Effect and Consequence

Core questions:

```text
What is Execution if Command and Operation have already been separated?
Is Execution a behavior, a relation between specification and behavior, or both?
What is Realization: implementation, enactment, instantiation, causal lineage?
What does it mean for one committed Operation to cross a dispatch/realization boundary?
When does an execution begin if setup/materialization precedes target process start?
What is an Effect relative to RF1 state projections and RF2 intended/operational semantics?
What distinguishes intended/requested effect from actual consequence and incidental side effect?
Can one execution realize zero, one or many effects?
What evidence links a Runtime-owned realization lineage to an external domain effect?
Where does Runtime ownership end when an external provider/physical world owns the transition?
What does execution success mean independently of effect success and semantic outcome?
```

Mandatory falsifiers:

```text
pure deterministic in-memory computation
process starts but target code never enters main
process exits zero after no relevant domain change
process crashes after external effect committed
HTTP request transmitted but not accepted
provider accepts request but effect occurs asynchronously later
financial order partial fills
filesystem transaction with atomic rename
robot long-running motion with continuous control
message send with multiple recipients
idempotent API retry with stable identity
opaque command with unknown side effects
Runtime self-release where provider receipt outranks generic process state
workspace.patch direct effect with no Job/Runner
```

Do not enter RF4 Identity/Sameness/Replay until RF3 separates `Execution`, `Realization`,
`Dispatch`, `Effect`, `Consequence`, `Receipt` and `Outcome` well enough that identity is
not defined over an ambiguous object.

---
schema_version: 1
id: runtime.foundations.rf2.sources
title: RF2 Sources — Action, Intent, Proposal, Command and Operation
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
summary: Primary/canonical sources used by RF2 to separate control action, intention/planning, planning operators, commands, requests, long-running action protocols and Runtime operational commitment.
evidence_status: verified
readiness: READY
related:
  - runtime.foundations.rf2
---
# RF2 Sources — Action, Intent, Proposal, Command and Operation

RF2 deliberately combines control, AI planning/agent theory, protocol/OS semantics and
robot execution. No one field is treated as the complete ontology of Action.

## S1 — Åström & Murray: feedback/control and selected system input

Karl J. Åström and Richard M. Murray, **Feedback Systems: An Introduction for
Scientists and Engineers**, second-edition author-hosted material, Caltech Control and
Dynamical Systems.

Canonical source: Caltech FBS.

RF2 use:

- feedback/control shapes system behavior by generating control inputs from state/output
  and reference information;
- control action can be defined mechanically without importing human-like intention;
- separates controller policy/input from plant trajectory and disturbances.

## S2 — Fikes & Nilsson: planning operators and goal-directed model transformation

Richard E. Fikes and Nils J. Nilsson, **STRIPS: A New Approach to the Application of
Theorem Proving to Problem Solving**, IJCAI / Artificial Intelligence, 1971.

Canonical proceedings: IJCAI-71; canonical journal record: Artificial Intelligence.

RF2 use:

- planner searches a space of world models;
- operators characterize applicable modeled transformations;
- plans compose operators toward a goal condition;
- provides the key separation between a planning operator/model and later physical
  execution.

## S3 — Fikes: monitored execution of robot plans

Richard E. Fikes, **Monitored Execution of Robot Plans Produced by STRIPS**, 1971/1972.

Canonical public record: NASA Technical Reports Server, document 19730026650.

RF2 use:

- STRIPS creates a plan while PLANEX executes the actions;
- execution monitors whether desired results were achieved;
- failure can lead to reexecution or replanning;
- directly falsifies `Plan/Operator = physical execution/result`.

## S4 — Bratman: intentions as elements of partial plans

Michael E. Bratman, **Intention, Plans, and Practical Reason**, Harvard University Press
1987, reissued by CSLI Publications 1999.

Canonical author/publisher record: Stanford CSLI Publications / Stanford Philosophy.

RF2 use:

- intentions have a planning role distinct from mere desire/expectation;
- partial plans organize activity over time and serve as inputs/constraints to further
  practical reasoning;
- intended and expected effects are distinguishable;
- supports `Goal/Desire != Intent != Action/Consequence` without requiring Runtime to
  model human psychology.

## S5 — Rao & Georgeff: computational BDI agents

Anand S. Rao and Michael P. Georgeff, **BDI Agents: From Theory to Practice**, First
International Conference on Multiagent Systems, 1995, AAAI proceedings.

Canonical source: AAAI.

RF2 use:

- computational agent architectures explicitly distinguish beliefs, desires and
  intentions;
- demonstrates that `intention` already has a technical/computational planning role
  beyond ordinary-language wish or goal;
- used as a competing agent model, not universal Runtime ontology.

## S6 — POSIX Shell Command Language

The Open Group / IEEE, **POSIX.1-2024 Shell Command Language**.

Canonical source: The Open Group Base Specifications Issue 8 / IEEE Std 1003.1-2024.

RF2 use:

- the shell is a command-language interpreter;
- input is tokenized and parsed into simple/compound commands before utility execution;
- supports `command representation != process/execution`.

## S7 — POSIX exec family

The Open Group / IEEE, **exec family — execute a file**, POSIX.1-2024.

Canonical source: The Open Group Base Specifications Issue 8 / IEEE Std 1003.1-2024.

RF2 use:

- successful exec replaces the current process image with a new process image;
- process image/environment/inheritance are runtime facts distinct from shell command
  syntax;
- reinforces `Command != Process != Operation identity`.

## S8 — RFC 9110: HTTP request semantics

R. Fielding, M. Nottingham, J. Reschke (eds.), **RFC 9110 — HTTP Semantics**, IETF,
2022.

Canonical source: RFC Editor.

RF2 use:

- request messages communicate client intentions toward identified resources;
- request method semantics describe the purpose/requested successful result;
- server interprets the request and returns a response from which the client evaluates
  whether its requested intention was carried out;
- safe/idempotent method properties concern requested/intended semantics, not every
  incidental implementation side effect;
- strongly supports `Request/Command != Effect` and contract-scoped effect semantics.

## S9 — ROS 2 Actions

Open Robotics, **ROS 2 Actions** and ROS 2 interface documentation.

Canonical source: docs.ros.org.

RF2 use:

- a long-running Action protocol separates Goal request, acceptance/rejection, unique
  Goal identity, feedback, cancellation/preemption and result;
- demonstrates an operational pattern in which request, admission, running behavior and
  result are first-class distinct stages;
- comparison only: ROS `Action` is a protocol-specific term, not proof of Runtime
  foundations vocabulary.

## S10 — Current Ordivon Runtime as empirical source

RF2 audits current source and canonical docs at the research head:

- `ExecutionProposal` is explicitly Agent-authored and allows only proven mechanical
  limit resolution to be delegated;
- `TaskRunProposal` pairs `clientRequestId`, principal and `ExecutionProposal`;
- `RuntimeExecutionPlan` is the concrete plan after Runtime resolution;
- request/proposal identity digests are distinct from `operation_digest`;
- Registry Job stores both request and Operation identities;
- `docs/effect-kernel.md` explicitly states Proposal is not commitment and maps current
  Operation to Job + committed Execution Plan/bundle;
- exact replay looks up the existing request/Job before current-world reinterpretation.

These are local pressure-tested data, not authority for universal terminology.

## Source discipline

Later rounds should preserve:

```text
control action != intentional action
planning operator != realized behavior
command/request semantics != effect occurrence
protocol term "Action" != universal Action ontology
psychological/computational intention != Runtime operational commitment
local Runtime object names remain empirical specializations
```

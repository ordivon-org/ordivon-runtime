---
schema_version: 1
id: runtime.foundations.rf2
title: RF2 — Action, Intent, Proposal, Command and Operation
type: report
profile: research
lifecycle: completed
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
summary: RF2 separates system evolution from control action, intentional action, proposal, command, request, operation and commitment; it derives a role- and boundary-aware action grammar for automated, delegated and Agent-mediated systems and reclassifies current Ordivon ExecutionProposal/Job semantics without treating them as ontology.
evidence_status: verified
readiness: READY
applies_to:
  - RUNTIME-FOUNDATIONS-001
  - RF2
related:
  - runtime.foundations.start
  - runtime.foundations.rf1
  - runtime.foundations.rf2.sources
  - runtime.foundations.rf2.continuation
---
# RF2 — Action, Intent, Proposal, Command and Operation

## 0. Status and research question

RF2 enters the first layer where a possible world evolution can become **selected,
attributed, requested or operationally committed**.

RF1 established:

```text
State + Environment/Input + Dynamics
→ admissible behavior space
→ one realized/modelled trajectory
```

Nothing in that grammar yet implies an Actor, Intent, Command, Operation or Runtime.
A stone falls, a capacitor discharges and an uncontrolled process crashes without
requiring an action model.

RF2 therefore asks:

> **What distinguishes mere system evolution from action? What roles do control,
> selection, intention, proposal, command, request, operation and commitment play, and
> which of those roles can Runtime truthfully own without pretending to know an Agent's
> internal reasons or the external world's final effect?**

RF2 remains foundation research. It does not authorize renaming `ExecutionProposal`,
changing `clientRequestId`, changing Operation identity, adding Intent fields, adding a
generic Action type, or moving Host/Harness semantics into Runtime.

---

# 1. Starting deletion — Action is not any state transition

The broadest candidate is:

```text
Action = a transition that happens
```

This fails immediately.

Examples:

```text
rock falls under gravity
radioactive atom decays
capacitor discharges
weather changes
market moves without our participation
process is killed by kernel OOM
```

All can produce modeled transitions. None requires the concept `Action` for a useful
explanation.

Therefore:

```text
TransitionOccurrence != Action
WorldChange != Action
```

RF2 needs an additional **attribution/selection structure**.

---

# 2. Second deletion — Action is not every caused transition

A stronger candidate is:

```text
Action = a change caused by another system
```

This is still too broad.

A falling branch can knock a switch. A software bug can corrupt a file. An electrical
spike can trigger a relay. These are causal interactions but calling all of them actions
collapses causal occurrence into agency/control attribution.

Thus:

```text
Cause != Action
```

RF1 already postponed full causality. RF2 adds that causal participation is at most one
ingredient of action attribution.

---

# 3. Third deletion — Action is not necessarily intentional action

If RF2 instead defines:

```text
Action = intentional behavior by a reflective agent
```

it excludes ordinary engineering cases that are routinely and usefully called actions:

```text
thermostat switches heater
feedback controller changes actuator input
cron starts a maintenance command
a watchdog terminates a process
an automated trading policy submits a request
ROS action server executes an accepted goal
```

These systems can select interventions according to policies/contracts without having
human-like phenomenal intention.

RF2 therefore refuses to make **intentional action** the universal parent category.

The correct move is to separate several projections of `Action`.

---

# 4. Action is an attribution, not a raw physical primitive

RF2's strongest current reconstruction is:

> **An action is behavior or an intervention attributed to an actor/controller under a
> declared action model because that actor/controller's selection mechanism is treated
> as relevant to why that behavior occurred among alternatives.**

Compactly:

```text
ActionAttribution =
  Actor/Controller
  + alternative interventions/behaviors
  + selection relation/policy
  + selected intervention/behavior
  + causal/realization linkage
  + declared description/boundary
```

This is deliberately a **relational attribution** rather than a new substance inside
Reality.

Consequences:

```text
the same physical trajectory may support several action descriptions
one action description may abstract over many physical events
not every causal contributor is the actor
not every actor role requires human-like intention
```

RF2 does not yet claim a complete causal theory of action.

---

# 5. Three Action projections must remain separate

## 5.1 Control Action

A controller selects or generates an input/intervention according to a policy, state,
reference signal or rule:

```text
u_t = π(state/observation/reference/history)
```

The selected `u_t` can be called a **control action** relative to that control model.

Examples:

```text
thermostat heater ON
PID actuator value
scheduler sends SIGTERM
policy selects API call
```

No reflective intention is required.

## 5.2 Intentional Action

An action is intentional when its occurrence is appropriately connected to an
agent-level intention/plan/reason structure under the relevant action description.

RF2 does not settle philosophy of mind. It retains only the practical distinction:

```text
intentional action
!=
all control action
```

and:

```text
intended consequence
!=
all actual consequence
```

## 5.3 Operational Action / Operation

A system can define a bounded unit that can be proposed, validated, identified,
admitted, executed, observed and/or reconciled.

This is not automatically a psychological action. It is an **operational abstraction**.

Current Ordivon Runtime primarily lives here.

---

# 6. Selection does not imply deliberation

The word `selected` can sound cognitive. RF2 uses it more generally.

A policy can select among alternatives by:

```text
explicit human decision
Agent planning
feedback law
rule table
state machine
randomized policy
stored schedule
precommitted automation
```

Therefore:

```text
Selection != Deliberation
Selection != Conscious Choice
Selection != Intention
```

A thermostat meaningfully selects an actuator value under a control model, while it is
usually misleading to attribute human-like intention to it.

---

# 7. Random choice is a useful falsifier

Suppose a program chooses randomly between two permitted branches.

At the dynamics level:

```text
both branches are possible
```

At the controller/action level, two modeling choices are legitimate:

```text
randomizer is part of controller
→ selected branch can count as control action
```

or:

```text
randomness is internal dynamics below action boundary
→ higher model sees no distinct action selection
```

Thus `Action` depends partly on the declared controller/action boundary.

This reinforces:

```text
Action description != intrinsic transition label
```

---

# 8. Goal is not Intent

A **Goal** is provisionally:

> a represented desired/target property, condition, outcome or region of behavior/state
> space.

Examples:

```text
file contains expected text
robot reaches waypoint
account exposure is below threshold
service is healthy
```

But a goal can exist without a current commitment to act:

```text
wish
preference
aspiration
candidate objective
infeasible target
externally imposed target
```

So:

```text
Goal != Intent
```

A goal can also become true without the agent acting—for example because the environment
changes independently.

Therefore:

```text
GoalSatisfied != ActionOccurred
```

---

# 9. Intent is not merely desire or goal representation

Bratman's planning theory is useful because it treats intention as having a distinct
functional role in **settling practical questions, organizing future activity and
constraining further planning/means-end reasoning**.

RF2 therefore uses a deliberately weak operational research definition:

> **Intent is a controller/agent-side practical commitment or settled orientation toward
> pursuing/maintaining a course, outcome or action in a way that constrains subsequent
> selection/planning.**

This preserves:

```text
Goal        what condition/outcome is targeted
Intent      a practical commitment/orientation toward acting/pursuing
Plan        structured means/course for carrying it out
Action      attributed selected behavior/intervention
```

RF2 does not require Runtime to model or verify Intent.

---

# 10. Intent is not guaranteed to produce action

An intention can be:

```text
reconsidered
blocked
forgotten
made infeasible
superseded
rejected by authority
prevented by resource failure
```

Therefore:

```text
Intent != ActionOccurrence
```

Likewise an action may occur with little or no contemporaneous explicit intention:

```text
habit
automatic policy
pre-scheduled automation
reflex-like control
previously delegated execution
```

Hence:

```text
ActionOccurrence != proof of current Intent
```

---

# 11. Intention commitment and Runtime commitment are different kinds of commitment

`Commitment` is overloaded across Human/Agent and Runtime layers.

RF2 separates:

## Planning / intentional commitment

A practical stance that stabilizes future reasoning/action selection.

## Operational commitment

A system accepts a particular operation instance/identity/contract such that it will no
longer silently reinterpret it as a different operation.

Current Ordivon Runtime's durable Job admission is of the second kind.

Thus:

```text
IntentionalCommitment != RuntimeOperationalCommitment
```

Runtime can commit an operation without proving the principal psychologically intended
it in any philosophical sense.

---

# 12. Proposal is a representation of a candidate, not the action itself

RF2 defines **Proposal** as:

> **A communicated candidate specification for a possible action/operation, presented to
> another system/authority for evaluation, selection, admission or transformation.**

Properties:

```text
proposal can be rejected
proposal can be invalid
proposal can be revised
proposal can omit delegated/mechanical details
proposal can exist without physical dispatch
proposal can exist without durable operation identity
```

Therefore:

```text
Proposal != Commitment
Proposal != ActionOccurrence
Proposal != OperationCommitment
```

This strongly supports the current `ExecutionProposal` name.

---

# 13. Current `ExecutionProposal` is already close to the RF2 distinction

Current source says:

```text
Agent-authored execution proposal.
Action fields are concrete;
only proven mechanical execution limits may be omitted and resolved by Runtime.
```

Its authored fields include:

```text
workspace
executable
args
cwd
env
optional limits
steps
budget
profile/target/authority selections
foreign references
host dependencies
```

RF2 interpretation:

- this is a **candidate operational specification**;
- it is not yet a Runtime Operation;
- omission of a mechanical limit is delegated resolution, not hidden intent;
- the proposal records authored operational choices, not the user's semantic Goal.

No implementation change is required by this interpretation.

---

# 14. Proposal identity and Operation identity are different

Current Runtime already distinguishes:

```text
request/proposal identity
```

from:

```text
committed operation digest
```

On new admission, Runtime can resolve delegated mechanical values, capture source/world
and provider commitments, and then form a concrete Execution Plan and Operation identity.

This leads RF2 to a strong staged model:

```text
Proposal/Request Instance
        ↓ validation / policy resolution / world binding
Admitted Operation Instance
        ↓ later RF3 realization
Attempt / Dispatch / Behavior
```

The proposal can be identical across a retrying client while exact replay returns the
same already-admitted Operation instead of reinterpreting the current world.

---

# 15. Command is not Proposal, Operation or Action

`Command` is another overloaded term. It can mean:

```text
shell syntax
imperative message
API request
control directive
operator instruction
executable + argv shorthand
```

RF2 retains a broad but precise role:

> **A command is an encoded directive/request intended to be interpreted by a receiver
> according to some command/protocol semantics.**

A command therefore belongs primarily to the **representation/communication layer**.

It can fail before action:

```text
parse error
unknown command
unauthorized
invalid precondition
resource unavailable
receiver absent
admission rejected
```

Thus:

```text
Command != Action
Command != AcceptedOperation
Command != Effect
```

---

# 16. POSIX makes Command/Process separation explicit

The POSIX shell is a **command language interpreter**. It reads, tokenizes and parses
input into simple/compound commands.

Separately, `execve`/the exec family replace a process image with a new executable image.

Therefore even at the operating-system boundary:

```text
shell command text
→ parsing/expansion/interpreter semantics
→ executable invocation
→ process-image replacement/execution behavior
```

These are different stages.

A command string is not a process, and a process image is not the command string that
caused it to be selected.

---

# 17. HTTP makes Request/Intent/Effect separation explicit

HTTP semantics are especially useful because the protocol says a request communicates
the client's intention toward a target resource, while the server interprets the method
semantics and returns a response that helps the client determine whether the requested
intention was carried out.

This directly preserves:

```text
Request != Effect
Request semantics != server implementation
Response != necessarily complete external-world consequence
```

HTTP `safe` and `idempotent` properties are also defined around **requested/intended
semantics**, not every incidental implementation side effect such as logging.

This is an important warning for Runtime:

> Operation semantics must identify which consequences belong to the operation contract
> rather than treating every causal consequence in the universe as part of one Effect.

---

# 18. Command authority is not contained in syntax by default

The same command text can be:

```text
permitted under one principal
forbidden under another
interpreted against another resource
executed in another directory
given another environment
resolved to different executable/dependencies
```

Therefore:

```text
SameCommandText != SameOperationalAction
```

This strongly supports current Runtime operation identity binding facts beyond
`executable + argv`, including principal, Workspace/source state, environment, provider
and declared dependencies.

RF6 will study authority formally. RF2 only establishes that authority/context cannot be
recovered from command syntax alone.

---

# 19. Control Input is not Command

In control theory:

```text
u(t)
```

is an input that influences the plant/system dynamics.

The input can be produced by a controller without any symbolic command language.

Examples:

```text
voltage
throttle position
motor torque setpoint
binary heater signal
```

Conversely a textual command may never reach the plant.

Therefore:

```text
ControlInput != Command
```

A command may eventually be transformed into one or more control inputs through
multiple layers.

---

# 20. Planning Operator is not realized Action

STRIPS provides a historically important separation.

A planning `operator` describes applicability/preconditions and how a world model would
change. A planner composes operators to reach a goal model.

Fikes' PLANEX work then separately executes the resulting robot plan, monitors results,
reexecutes portions when results are not achieved and replans when the plan becomes
ineffective.

This yields:

```text
PlanningOperator
!=
Plan
!=
Execution
!=
ObservedOutcome
```

That distinction is directly relevant to Agent tool calls: a model-generated plan or
tool-call candidate is not physical execution merely because its planning semantics say
what should happen.

---

# 21. What is an Operation?

`Operation` is too overloaded for one flat definition. RF2 introduces three levels.

## 21.1 Operation Type / Contract

A reusable semantic shape:

```text
PATCH file
PUT resource
execute program
submit order
move robot to waypoint
```

It defines some combination of:

```text
parameters
preconditions
requested semantics
possible results/errors
control/lifecycle contract
```

## 21.2 Operation Request / Proposal Instance

A particular request to instantiate that type with concrete parameters/context.

Example:

```text
PUT /resource X with body B under precondition ETag=K
```

or current Runtime `ExecutionProposal` with one `clientRequestId`.

## 21.3 Committed Operation Instance

A system-owned identity-bound accepted instance whose enforceable semantics and bound
facts will no longer be silently reinterpreted.

Current Runtime's strongest mapping is:

```text
Committed Operation
≈ Job + committed Execution Plan + operationDigest + bound side truth
```

This is the sense RF2 recommends when discussing Runtime's **Operation identity**.

---

# 22. Why Operation is not merely Command text

An Operation can include facts absent from command syntax:

```text
principal
request identity
resource/world preconditions
source revision/digest
environment
budget
provider identity
authority class
exact input bindings
reconciliation semantics
```

Conversely, one compound command string can contain several semantic operations.

Therefore:

```text
Operation != CommandText
```

and:

```text
OperationIdentity != hash(command string)
```

This is one of RF2's strongest confirmations of the existing Runtime direction.

---

# 23. Operation is not one Transition

A single operation may generate a long trajectory:

```text
accepted
start
partial progress
multiple writes
remote request
partial fill
cleanup
terminal evidence
```

A transaction-like operation can be modeled as one high-level atomic transition while
its realization contains many lower transitions.

A robot navigation operation may include continuous motion plus many control cycles.

Therefore:

```text
Operation != Transition
Operation != Event
```

`Operation` is an attribution/contractual unit over a behavior, not necessarily one state
jump.

---

# 24. Operation is not Effect

An operation may:

```text
perform zero relevant external effects
perform one effect
perform multiple effects
partially affect the world
have unknown external effect status
```

One process/operation can also create incidental effects outside its intended contract.

Thus:

```text
Operation != Effect
```

RF3 will study Execution/Realization/Effect in detail.

---

# 25. Operation is not Attempt

Current Runtime already separates them:

```text
Job / Operation identity
        ↓
Attempt / one dispatch generation
```

RF2 gives the conceptual reason:

> The Operation defines **what bounded operational commitment this is**; an Attempt
> defines **one physical realization generation**.

A future explicit retry can therefore remain the same higher Operation while creating a
new Attempt only if later effect/retry semantics justify that relation.

RF2 does not authorize multi-Attempt retry implementation.

---

# 26. One Intent can generate many Operations

Example:

```text
Intent: deploy release safely
```

may generate:

```text
build candidate
verify manifest
apply release
check health
rollback if needed
```

Similarly:

```text
Intent: reduce portfolio exposure
```

can produce multiple order operations.

Therefore:

```text
Intent != Operation
```

and cardinality can be:

```text
1 Intent → N Operations
```

---

# 27. One Operation can serve many higher intentions/goals

The same operation:

```text
read file X
```

might support:

```text
debugging
security audit
research
build verification
user explanation
```

Runtime cannot infer which higher semantic goal is the true one from the physical
operation specification alone.

Thus Runtime must not promote operation success into goal success.

---

# 28. Delegation splits the actor role

Agent systems make `who acted?` fundamentally ambiguous unless roles are named.

A single chain can be:

```text
Human/Host        owns Goal / higher commitment
Harness Agent     selects a candidate course
Agent Tool Call   proposes an operation
Runtime           admits/commits operational identity
Runtime/OS        dispatches realization machinery
Target program    performs domain requests
Remote provider   commits domain effect
```

Calling one participant simply `the actor` erases important distinctions.

RF2 therefore introduces a role grammar.

---

# 29. RF2 action-role grammar

Not every operation has every role, and one entity can fill several roles.

## Goal Owner

Whose objective/outcome is being pursued?

## Intender / Planning Agent

Whose planning/intention structure settles or constrains the course?

## Proposer

Who emits the candidate operation/request?

## Principal

Whose authenticated/recognized identity and authority are attached to the request?

## Admitter / Authorizer

Who decides whether this candidate may become an operational commitment?

## Controller / Dispatcher

Who selects when/how an admitted realization proceeds?

## Executor / Realizer

What mechanism physically carries the behavior?

## Effect Owner / Domain Provider

Which external system owns the authoritative semantics/receipt of the target effect?

## Observer / Verifier

Who/what supplies evidence and evaluates resulting claims?

This is not a mandatory Runtime schema. It prevents attribution errors.

---

# 30. Delegated action is not transferred authorship in one simple sense

Suppose a human says:

```text
"deploy this release"
```

an Agent chooses exact commands, Runtime commits them and a deployment tool mutates the
host.

Different true statements can coexist:

```text
Human intended/authorized a deployment goal.
Agent proposed the concrete operation.
Runtime committed/dispatched the operational instance.
Deployer performed the filesystem/service mutation.
```

There is no need to force one universal answer to `who acted?`.

Action attribution is **role- and description-relative**.

This will be important later for Security, Finance and Human but RF2 remains Runtime
foundations only.

---

# 31. Joint and shared action falsify single-actor ontology

Human collaboration and multi-Agent systems show that coordinated action can arise from
multiple interlocking plans/intentions and role assignments rather than one centralized
mind/controller.

RF2 therefore rejects:

```text
Every Action has exactly one indivisible Actor.
```

At the operational layer, however, one Runtime commitment can still require one
canonical principal/identity for accountability even when the higher activity is joint.

This separates:

```text
social/joint agency structure
```

from:

```text
operational principal identity
```

---

# 32. Accepted request is not executed action

ROS 2 Actions make this separation very explicit:

```text
client sends Goal
server may accept/reject Goal
accepted Goal gets identity/handle
server provides progress feedback
Goal may be cancelled/preempted
server eventually returns result
```

This is almost a textbook falsifier of:

```text
Request = Action = Result
```

Instead:

```text
GoalRequest
→ Acceptance
→ Long-running procedure/behavior
→ Feedback
→ Result
```

The resemblance to Runtime Proposal → Job → Attempt → Evidence is structurally
interesting, but RF2 does not infer identical semantics.

---

# 33. Admission is not psychological endorsement

When Runtime admits an operation, it says roughly:

```text
this exact operational request is valid under current Runtime contract and may become
one durable realization commitment
```

It does not say:

```text
the user's goal is wise
the Agent intended the right thing
the command is morally endorsed
the external effect is guaranteed
the operation will satisfy the semantic Task
```

Thus:

```text
Admission != Intent Validation
Admission != Goal Validation
Admission != Semantic Approval
```

---

# 34. Rejected Proposal is a decisive falsifier

Current Runtime can reject new admission because of:

```text
invalid request
stale world/source precondition
provider drift
unsupported target/profile
capacity limit
deployment admission fence
workspace conflict
```

In these cases a proposal/request existed, but no new committed Runtime Operation needs
to exist.

Therefore:

```text
ProposalExistence != OperationExistence
```

This is one of the cleanest local distinctions in RF2.

---

# 35. Same proposal text under changed context may not be the same Operation

Suppose the same executable/args are proposed twice but:

```text
principal differs
Workspace differs
source state differs
authority differs
provider differs
declared dependency differs
```

The operational reality is materially different.

Therefore same human-readable command/proposal surface does not imply same Operation.

Current Runtime captures this by separating request/proposal identity, concrete
Execution Plan and operation digest/world bindings.

---

# 36. Timers and cron show that current intention is not required at dispatch time

A human or Agent can establish automation at t0:

```text
run maintenance at 02:00
```

At t1 the scheduler triggers work with no contemporaneous deliberation.

Possible attribution:

```text
earlier human/agent intention established policy
scheduler performs automated control action at t1
Runtime/OS realizes the operation
```

Therefore:

```text
current deliberation != necessary condition for automated action
```

and intent/action timing may differ.

RF5 will later study temporal identity/order in depth.

---

# 37. Accidents show that Effect can exceed Action description

A target command intended to update one file may accidentally delete another.

We can have:

```text
intended operation: update A
actual behavior: update A + delete B
```

The deletion is a real consequence but may not belong to the intended action description
or contract.

Therefore:

```text
ActualConsequence != IntendedEffect
```

and:

```text
Action description != complete causal consequence set
```

This is why structured Effect contracts must declare which external projection they
own.

---

# 38. Malicious behavior shows Intent attribution can belong to a different principal

If target code is malicious, the Runtime principal may have intended benign work while
the program's internal controller produces hostile effects.

RF2 therefore forbids:

```text
Principal intent = target-program internal intent/policy
```

and:

```text
Runtime admission = endorsement of every internal subaction
```

A process tree can contain behavior whose detailed action semantics Runtime does not
own.

This reinforces arbitrary `workspace.exec` effect opacity.

---

# 39. Financial order is a strong Operation/Effect falsifier

A single order instruction can be:

```text
rejected
accepted but unfilled
partially filled many times
fully filled
cancelled after partial fill
expired
```

Thus:

```text
OrderInstruction != Acceptance != Fill != PositionEffect
```

One Operation can induce multiple domain events/effects over time.

Runtime may transport/realize an order request, but Finance/venue semantics own the
order/fill/position truth.

---

# 40. Robot command is a strong Command/Trajectory falsifier

A command such as:

```text
move to waypoint W
```

is not the robot trajectory.

The realized behavior depends on:

```text
controller
plant dynamics
sensor feedback
obstacles
disturbances
cancellation
```

So:

```text
Command != ControlTrajectory
```

and one high-level operation can correspond to thousands of low-level control actions.

---

# 41. Cross-domain falsifier matrix

| Case | Premature collapse attacked | Surviving distinction |
|---|---|---|
| stone falls | transition = action | mere dynamics can evolve without action attribution |
| branch accidentally hits switch | cause = action | causation alone is insufficient |
| thermostat toggles heater | action requires reflective intention | control action can exist without human-like intention |
| randomized policy chooses branch | selection = deliberation | selection can be stochastic/policy-based |
| goal becomes true by environment change | goal satisfaction = action success | target condition can occur without agent action |
| intention blocked before acting | intent = action | intention can exist without action occurrence |
| cron executes later | current intent required for action | earlier policy/commitment can drive later automated action |
| rejected Agent Tool proposal | proposal = operation | proposal can terminate before operational commitment |
| shell text parse error | command = execution | command representation can fail before realization |
| same shell text under another principal | command text = operation identity | context/authority changes operational identity |
| POSIX exec replaces process image | command = process | command interpretation and process realization differ |
| HTTP POST rejected | request = effect | request communicates requested semantics; server may not carry it out |
| HTTP safe method logs request | intended effect = every side effect | contract/intended semantics differ from incidental causal consequences |
| STRIPS operator in plan | planning operator = real action | modeled operator/plan and monitored execution are separate |
| ROS goal rejected | goal request = accepted action | acceptance is a separate boundary |
| ROS accepted long-running goal | acceptance = completion | accepted operation can have feedback/cancel/result lifecycle |
| robot waypoint command | command = physical trajectory | high-level directive expands into feedback-controlled behavior |
| market order partial fills | operation = one transition/effect | one operation can create many domain transitions |
| accidental file deletion | action = intended consequence set | actual consequences can exceed intended/contracted effect |
| malicious child behavior | principal intent = all target subactions | delegated execution can contain differently attributed behavior |
| human → Agent → Runtime → provider | exactly one actor owns action | action roles can be distributed across participants |

---

# 42. RF2 anti-laws

RF2-L1: `WorldChange != Action`.

RF2-L2: `TransitionOccurrence != Action`.

RF2-L3: `Cause != Action`.

RF2-L4: `Action != necessarily IntentionalAction`.

RF2-L5: `ControlAction != IntentionalAction`.

RF2-L6: `Selection != Deliberation`.

RF2-L7: `Selection != ConsciousChoice`.

RF2-L8: `Goal != Intent`.

RF2-L9: `GoalSatisfied != ActionOccurred`.

RF2-L10: `Intent != Desire/Goal representation`.

RF2-L11: `Intent != ActionOccurrence`.

RF2-L12: `ActionOccurrence != proof of current Intent`.

RF2-L13: `IntentionalCommitment != RuntimeOperationalCommitment`.

RF2-L14: `Proposal != Commitment`.

RF2-L15: `Proposal != ActionOccurrence`.

RF2-L16: `ProposalExistence != CommittedOperationExistence`.

RF2-L17: `Command != Action`.

RF2-L18: `Command != Operation`.

RF2-L19: `Command != Effect`.

RF2-L20: `ControlInput != Command`.

RF2-L21: `PlanningOperator != ExecutedAction`.

RF2-L22: `Plan != Execution`.

RF2-L23: `OperationType != OperationRequestInstance`.

RF2-L24: `OperationRequest != CommittedOperation`.

RF2-L25: `Operation != Transition`.

RF2-L26: `Operation != Event`.

RF2-L27: `Operation != Effect`.

RF2-L28: `Operation != Attempt`.

RF2-L29: `OperationIdentity != hash(CommandText)`.

RF2-L30: `SameCommandText != SameOperationalAction`.

RF2-L31: `OneIntent != OneOperation`.

RF2-L32: `OperationSuccess != GoalSuccess`.

RF2-L33: `Admission != IntentValidation`.

RF2-L34: `Admission != GoalValidation`.

RF2-L35: `Admission != SemanticApproval`.

RF2-L36: `Principal != necessarily sole Actor`.

RF2-L37: `Executor != necessarily Intender`.

RF2-L38: `RuntimeAdmission != endorsement of target internal actions`.

RF2-L39: `ActualConsequence != IntendedEffect`.

RF2-L40: `ActionDescription != complete causal consequence set`.

---

# 43. Minimum RF2 grammar

RF2 retains the following conceptual roles.

## 43.1 Goal `G`

A represented target condition/property/outcome.

## 43.2 Intent `I`

A controller/agent-side practical commitment/orientation that constrains future
selection/planning.

## 43.3 Policy / Selection rule `π`

A mapping/relation that chooses or generates interventions/proposals from
state/observation/history/goals.

## 43.4 Control action `a_c`

A selected intervention/input at a control boundary.

## 43.5 Proposal `P`

A communicated candidate operational specification presented for admission/selection.

## 43.6 Command `C`

An encoded directive/request interpreted under a command/protocol semantics.

## 43.7 Operation Type `O_type`

Reusable operation contract/schema.

## 43.8 Operation Request Instance `O_req`

Concrete requested instantiation of an operation type.

## 43.9 Operational Commitment `O_commit`

Identity-bound accepted operation instance whose enforceable facts are stable for the
owning system.

## 43.10 Action-role relation `R_actor`

Mapping among Goal Owner, Intender/Planner, Proposer, Principal, Admitter, Controller,
Executor, Effect Owner and Observer.

These are conceptual coordinates, not a required Runtime object graph.

---

# 44. RF2 dynamic reconstruction

RF1's behavior grammar can now be extended without jumping to Execution/Effect:

```text
World / History / Observation
          │
          ▼
   Goal / Intent / Policy           [optional agent/controller layer]
          │
    selection/planning
          │
          ▼
  Candidate Control Action
     or Proposal/Command
          │
      interpretation
          │
          ▼
 Operation Request Instance
          │
 validation / authority / preconditions / policy resolution
          │
          ▼
  Operational Commitment
          │
          └──── RF3 begins here: Execution / Realization / Effect / Consequence
```

Not every system traverses every box:

- thermostat can jump from policy to control action;
- shell can go from command representation toward execution without durable operation
  commitment;
- current Ordivon inserts an explicit proposal/admission/operation commitment boundary;
- structured remote APIs may add their own acceptance identities.

---

# 45. Current Ordivon Runtime reclassified by RF2

| Current mechanism | RF2 role |
|---|---|
| Agent Tool call draft | candidate proposal before Runtime authority |
| `ExecutionProposal` | concrete candidate operational specification |
| `clientRequestId` | stable request/proposal-instance reattachment identity |
| proposal identity digest | authored/delegated request identity boundary |
| Runtime validation/policy resolution | proposal → admissible concrete plan transformation |
| `RuntimeExecutionPlan` | concrete committed realization specification after resolution |
| source/provider/input/dependency bindings | operational precondition/world context |
| `operationDigest` | committed Operation identity digest |
| Job | durable operational commitment record/handle |
| Attempt | later physical realization generation, not Operation identity |
| executable + args | one command/invocation component inside operation contract |
| principal | authenticated operational identity/authority role, not complete Actor ontology |
| Host Task | higher semantic Goal/commitment layer outside Runtime |

This mapping explains current code without promoting it to a universal Runtime ontology.

---

# 46. What RF2 strongly supports in current design

## 46.1 Keep Proposal distinct from admitted Job

The distinction is now foundation-level rather than merely idempotency plumbing.

## 46.2 Keep `clientRequestId` at request/proposal continuity level

It should not be silently reinterpreted as current world state or as the physical Attempt.

## 46.3 Keep Operation identity richer than command text

Principal, context, source state, environment, provider, dependencies and concrete plan
can materially change what operational commitment means.

## 46.4 Keep Job and Attempt separate

Operation identity and one physical realization generation are different conceptual
roles.

## 46.5 Keep Runtime ignorant of semantic Goal/Intent by default

Runtime can execute a concrete operational proposal without needing to model Human/Agent
psychology or Host Task semantics.

## 46.6 Keep arbitrary execution effect-opaque

Command/process semantics do not provide enough information to infer intended external
effect contracts.

---

# 47. What RF2 reopens

## 47.1 `Action fields are concrete` is engineering wording, not foundations ontology

Current `ExecutionProposal` documentation uses `Action fields` to mean caller-authored
operational facts. RF2 does not treat this wording as proof that these fields define an
Action in the broader foundation sense.

## 47.2 Is `Operation` best reserved for committed instances?

RF2 recommends distinguishing operation type, request/proposal instance and committed
operation. Future documentation may eventually benefit from sharper terminology, but no
rename is authorized now.

## 47.3 Where exactly should operational commitment begin?

Current durable Job admission is a strong boundary. RF4/RF6/RF8 will test identity,
authority and evidence before foundations can declare it minimal/universal.

## 47.4 How much action attribution belongs in Runtime evidence?

Runtime currently names principal, request and physical lineage. It should resist
claiming psychological authorship or external-domain action ownership without stronger
contracts.

---

# 48. Boundary with Harness, Host and domain owners after RF2

## Harness

Harness can own Agent cognition, planning and proposal generation.

```text
Goal/context → Agent reasoning → candidate proposal
```

Runtime does not need to know whether that proposal came from a conscious-like intention,
a heuristic, a deterministic policy or a cached plan.

## Host

Host can own durable higher-level Task commitment and outcome semantics.

```text
Host semantic commitment != Runtime operational commitment
```

## Runtime

Runtime owns the admission/identity/physical-realization contract of concrete operations
within its declared authority.

## Domain owner / external provider

Owns target effect semantics and authoritative domain receipts/state.

This creates a cleaner ladder:

```text
Goal / semantic commitment             Host / user / domain
Intent / planning / proposal selection Harness / Agent
Operational proposal + commitment      Runtime
Physical realization                   Runtime + OS/provider machinery
External effect truth                  domain/effect owner
Semantic outcome evaluation            Host/domain/user
```

---

# 49. Research constitution added by RF2

Later Runtime rounds should obey:

1. Never infer Action merely from state change.
2. When using `Action`, name whether the claim is control, intentional, operational or
   domain action when ambiguity matters.
3. Do not require Runtime to prove internal Intent.
4. Keep Goal, Intent, Proposal, Command, Operation and Effect separate.
5. Treat Proposal as a candidate representation that may die before admission.
6. Treat Command as protocol/language representation, not proof of accepted operation.
7. Separate operation type, request instance and committed operation where identity or
   replay matters.
8. Do not derive Operation identity from command text alone.
9. Keep principal/authorship/executor/effect-owner roles separate in delegation chains.
10. Do not call admission semantic endorsement.
11. Do not call one operational commitment one physical transition by default.
12. Let RF3 own Execution/Realization/Effect rather than importing them backward.

---

# 50. RF2 result — conceptual compression

The strongest RF2 compression is:

```text
Dynamics defines what may happen.
A controller/agent can select among interventions or courses.
That selection can be attributed as an Action under a declared action model.

Intent is one possible planning/commitment structure behind selection,
but not every control action requires human-like intent.

Proposal and Command are representations/requests of possible operational action.
They can be rejected and therefore are not the action/effect itself.

Operation is a system-recognized bounded work/action contract.
For recoverable Runtime semantics, distinguish:
  operation type
  operation request/proposal instance
  committed operation instance

Current Ordivon Runtime primarily owns the transition:
  Proposal/Request → identity-bound Operational Commitment
not the user's Intent and not the external domain's final Effect truth.
```

In compact form:

```text
Goal G
  ↕
Intent I / Policy π
  ↓ selection
Control Action / Candidate Course
  ↓ representation
Proposal / Command
  ↓ interpretation + admission
Operation Request
  ↓ operational commitment
Committed Operation
  ↓
[RF3: Execution / Realization / Effect / Consequence]
```

---

# 51. RF2 closeout

RF2 closes with sixteen durable results:

1. **Action is not raw transition or causation; it is an attribution over selected
   behavior/intervention relative to an actor/controller model.**
2. `Control Action`, `Intentional Action` and `Operational Action/Operation` must not be
   collapsed.
3. Selection can be policy-based, automatic or stochastic and does not imply reflective
   deliberation.
4. `Goal != Intent`; goals specify targets while intentions constrain future practical
   selection/planning.
5. Intent is not required for every engineering control action and does not guarantee an
   action occurs.
6. Intentional/planning commitment and Runtime operational commitment are different.
7. **Proposal is a communicated candidate specification, not a physical or committed
   action.**
8. **Command is an encoded directive/request, not execution, operation commitment or
   effect.**
9. Planning operators/plans remain separate from monitored physical execution.
10. Operation should be decomposed into `Operation Type`, `Operation Request/Proposal
    Instance`, and `Committed Operation Instance` when replay/identity matters.
11. Operation identity is richer than command text/context-free parameters.
12. `Operation != Transition != Effect != Attempt`.
13. Delegated action requires role separation among goal owner, intender/planner,
    proposer, principal, admitter, controller, executor, effect owner and observer.
14. Admission validates an operational contract; it does not prove intent, goal wisdom or
    semantic success.
15. Current Ordivon `ExecutionProposal → RuntimeExecutionPlan → Job/operationDigest`
    structure is substantially supported by RF2.
16. Runtime's cleanest foundation ownership after RF2 is **proposal/request to stable
    operational commitment**, with physical realization deliberately deferred to RF3.

The next unavoidable frontier is now:

> **What exactly are Execution, Realization, Effect and Consequence? What does it mean
> for a committed Operation to cross from specification/commitment into physical
> behavior, and where does Runtime's causal/evidence responsibility end when an open
> world produces consequences beyond its direct control?**

That is RF3.

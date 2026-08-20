---
schema_version: 1
id: runtime.foundations.rf1
title: RF1 — State, Change, Event, Transition, Behavior and Dynamics
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
summary: RF1 reconstructs state, change, event, transition, behavior and dynamics below the current Runtime state machine; it separates model state from Reality and observation, shows that events and atomic transitions are abstraction-relative, unifies discrete, continuous, concurrent and hybrid evolution through behavior/trajectory, and derives a state-dynamics grammar needed before Action and Operation can be studied.
evidence_status: verified
readiness: READY
applies_to:
  - RUNTIME-FOUNDATIONS-001
  - RF1
related:
  - runtime.foundations.start
  - runtime.foundations.rf0
  - runtime.foundations.rf1.sources
  - runtime.foundations.rf1.continuation
---
# RF1 — State, Change, Event, Transition, Behavior and Dynamics

## 0. Status and research question

RF1 is the second Runtime Foundations round. RF0 established that `Process` is a
realization substrate/view rather than a safe foundation root and that `State
Transition` is too broad to define Runtime. RF1 therefore moves *below* action,
execution and effect.

RF1 asks:

> **What are state, change, event, transition, behavior and dynamics before we assume
> that a Runtime, Agent or process causes them? Which of these are properties of
> Reality, which are modeling projections, and which distinctions must survive across
> discrete software, concurrent systems, continuous physical systems and hybrid
> cyber-physical systems?**

RF1 is explanatory research only. It does not authorize a Runtime state-machine
refactor, new Registry dimensions, event sourcing, a hybrid-system execution backend,
or public Tool changes.

---

# 1. Inherited RF0 constraints

RF1 carries forward:

```text
Process != Runtime primitive
State Transition != Runtime definition
Intentional/specification truth != ontic realization truth != epistemic evidence truth
Observation != Reality
Process exit != global termination != external finality != semantic completion
Recovery != Retry
Effect is relative to a declared world/projection boundary
```

RF1 must not smuggle `Action`, `Operation`, `Execution` or `Effect` into the definition
of change. Those are later rounds.

---

# 2. Why the ordinary word `state` is dangerously overloaded

In ordinary engineering discussion, `state` may mean any of the following:

```text
physical condition of a system
values of selected model variables
row in a database
finite-state-machine mode
current process lifecycle phase
estimated hidden condition
cached observation
business/domain status
protocol phase
historical summary
```

These are useful but non-equivalent.

A Runtime document can truthfully say an Attempt is `running` while:

- the target process is blocked on I/O;
- no bytes are changing in the Workspace;
- a remote API effect has already committed;
- Runtime has not yet observed that effect;
- the operator's semantic Task remains incomplete.

The word `state` therefore cannot be allowed to imply one exhaustive Reality snapshot.

---

# 3. Competing models of State

RF1 keeps several models alive long enough to test their scope.

## S0 — State as complete instantaneous Reality

```text
State_t = everything true about the system at time t
```

### Strength

Intuitively simple. If such a complete state and complete physical law were available,
future evolution could in principle be discussed without worrying about omitted facts.

### Failure

For practical systems the boundary is undefined or enormous. Which facts belong to
`the system` rather than environment? Do kernel caches, wall clock, remote services,
operator attention, network packets, electromagnetic state and every dependency belong
inside? More importantly, models routinely answer useful questions with much smaller
representations.

**Disposition:** reject as an engineering foundation definition. `Complete Reality
state` may be a philosophical idealization, not the meaning of Runtime state.

## S1 — State as values of declared variables

```text
s = {x1=v1, x2=v2, ...}
```

### Strength

This is the standard formal-method and state-space representation. TLA actions relate
an unprimed state to a primed state; control models describe evolution of a selected
state vector.

### Failure

The variable set is chosen. Different coordinates, abstractions or boundaries can
represent the same physical system. An inadequate variable set may fail to summarize
history and therefore fail to support prediction.

**Disposition:** essential representation form; not a claim that the variables are the
system's unique intrinsic state.

## S2 — State as future-relevant history compression

A control-theoretic formulation says that state variables summarize past history for
the purpose of predicting future evolution under the model.

Provisional RF1 form:

```text
history H_<=t
   ↓ state abstraction σ
state s_t
   + admissible future input/environment
   ↓ dynamics
possible future behavior
```

### Strength

Explains why velocity belongs in the state of many mechanical models while the entire
past trajectory need not. It also explains why a delayed or history-dependent system
may need an enlarged state carrying memory.

### Failure

`Future-relevant` depends on the question, fidelity, admissible inputs and model. There
may be multiple equally useful state representations; a stochastic system may require a
conditional distribution rather than one determined future.

**Disposition:** strongest portable functional account in RF1, provided its scope is
explicit.

## S3 — State as equivalence class of histories

RF1 derives a stronger research interpretation from S2:

Two histories may be treated as the same modeled state when, for the questions and
admissible future interventions under consideration, their future behavior spaces are
indistinguishable enough for the model's purpose.

```text
H1 ~ H2
iff
relevant future behavior from H1 and H2 is equivalent under the chosen model/query
```

This connects naturally to abstraction, refinement and bisimulation traditions.

### Strength

Explains why state coordinates need not be unique and why a higher-level state may merge
many lower-level states.

### Failure

The equivalence criterion is purpose-dependent. Exact future equivalence may be too
strong; approximate or observational equivalence may be the useful relation.

**Disposition:** retain as a powerful derived interpretation, not a universal exact
mathematical definition.

## S4 — State as observed/estimated condition

```text
measurements/history → observer → estimated state ŝ
```

### Strength

Matches real systems where important state variables are hidden and must be inferred.
Control theory's observability distinction makes this explicit.

### Failure

An estimate is epistemic. The fact that Runtime *records* `running` or an observer
estimates a velocity does not make the record identical to the underlying condition.

**Disposition:** preserve, but call it estimate/projection/record rather than collapsing
it into ontic state.

---

# 4. RF1 provisional account of State

The strongest surviving account is:

> **A model state is a boundary- and question-relative representation of a system's
> condition that retains the information needed by a declared dynamics/model to reason
> about the future properties or behaviors of interest.**

Important consequences:

```text
State is not necessarily minimal.
State is not necessarily unique.
State is not necessarily directly observable.
State can contain remembered/history-derived information.
State is relative to a system/environment boundary.
State is relative to model fidelity and question.
State can be transformed into different coordinates without changing modeled dynamics.
A coarser state may intentionally erase distinctions visible in a finer model.
```

A useful notation is:

```text
W_<=t        = relevant world history up to t
B            = declared system/environment boundary
Q            = questions/properties of interest
σ_(B,Q)      = state abstraction/history compressor
s_t          = σ_(B,Q)(W_<=t)
```

This notation does **not** claim Runtime computes such a function explicitly. It keeps
the dependency of state on boundary and purpose visible.

---

# 5. State is not Observation

A decisive distinction is:

```text
state s_t
observation o_t
state estimate ŝ_t
```

These can all differ.

Control theory makes this ordinary: some states cannot be measured directly, and an
observer estimates state from input/output history. In Runtime terms:

```text
systemd/cgroup reality
Registry records
Runner evidence
MCP projection
```

are related but are not one object.

Therefore:

```text
recorded state != complete physical state
latest observation != current physical state by definition
estimated state != necessarily true state
```

This is not skepticism for its own sake. It is the condition that makes reconciliation
meaningful.

---

# 6. State is not one flat dimension

Current `AttemptState` gives a useful local falsifier. It contains:

```text
Accepted / Starting / Running / Stopping   realization/lifecycle phase
Recovering                                 convergence/recovery activity
Succeeded / Failed / TimedOut / Cancelled  terminal execution classification
Lost / Orphaned                            epistemic/ownership uncertainty class
```

These labels work as one operational finite-state machine because Runtime needs a
single guarded transition surface. But they are not one ontological dimension.

Current `JobProjection` already partially decomposes the world into separate fields:

```text
desiredState
attemptState
terminationIntent
executionTerminal
executionDisposition
deliveryDisposition
recoveryRequired
semanticCompletionEvaluated
```

RF1 interpretation:

> A useful operational state machine may be a **lossy control projection** over several
> underlying dimensions. Its utility does not prove that those dimensions are naturally
> one enum.

This is a foundation explanation, not a refactor request.

---

# 7. What is Change?

The naive definition is:

```text
change ⇔ s_t1 != s_t2
```

This works only after a state projection and time/index scheme are chosen.

RF1 separates three notions.

## 7.1 Ontic change

Some aspect of Reality differs/evolves.

RF1 does not attempt a complete metaphysics of physical change.

## 7.2 Modeled change

A declared state projection varies over an interval or between modeled points.

```text
s(t) varies
```

or in a discrete model:

```text
s_k != s_(k+1)
```

## 7.3 Epistemic change

The observer's information or estimate changes.

```text
belief/record/estimate at t1 != belief/record/estimate at t2
```

Epistemic change can occur without new target-world change—for example when delayed
evidence arrives about an effect that already happened.

Conversely, target-world change can occur while Runtime's modeled projection remains
unchanged.

---

# 8. Stuttering — unchanged modeled state does not mean unchanged Reality

TLA's stuttering concept is a direct RF1 falsifier. A stuttering step leaves all
**relevant variables** unchanged while allowing changes outside the modeled system.
Specifications support stuttering so that refinement does not depend on arbitrary
sampling rates or on every change in the wider universe appearing in the subsystem's
state.

RF1 generalization:

```text
No modeled change
!=
No world change
```

If Runtime's projection stays:

```text
AttemptState = Running
```

for 20 seconds, enormous physical activity may occur inside that interval:

```text
CPU instructions
file reads
network packets
child-process work
remote state changes
```

The state label is intentionally coarse.

This gives a strong rule:

> **A state projection reports distinctions chosen as relevant; it must not be read as
> a claim that unrepresented Reality is static.**

---

# 9. Continuous change falsifies transition-only ontology

For a continuous dynamical system, the natural representation may be:

```text
x_dot = f(x, u, t)
```

and a behavior is a trajectory:

```text
x : time → state space
```

There need not be a naturally privileged sequence of discrete `next states`.

One may sample the trajectory:

```text
x(t0), x(t1), x(t2), ...
```

or define threshold events, but the sampling/event partition is a modeling choice.

Therefore:

```text
continuous evolution != infinitely many ontologically primitive Runtime transitions
```

A general Runtime foundation must be able to discuss behavior without first assuming
a finite/discrete state machine.

---

# 10. What is an Event?

`Event` is one of the most overloaded words in systems engineering.

It may mean:

```text
something happened in Reality
a message occurrence
a log record
a transition label
a threshold crossing
a signal sample
a command invocation
a completion notification
```

RF1 keeps a deliberately weak definition:

> **An event is a modeled distinguished occurrence or boundary in a behavior, selected
> because some observer/model treats that occurrence as relevant.**

This means events can correspond to real discontinuities or messages, but eventhood is
not assumed to be a universal atom of Reality.

Examples:

```text
continuous temperature passes 100°C      → threshold event in one model
same temperature trajectory               → no event boundary in another model
packet arrives                             → event in network model
packet bytes gradually propagate physically→ continuous electromagnetic behavior below it
Runtime emits DISPATCH_ISSUED              → operational event record
external provider commits an order         → separate domain occurrence
```

---

# 11. Event != log record

A `job_events` row is evidence that Runtime recorded a modeled operational occurrence.
It is not the physical occurrence itself.

```text
physical occurrence
    ↓ observation/commit
Runtime event record
```

The record can be:

- delayed;
- absent after a crash;
- reconstructed from stronger evidence;
- corrected by later reconciliation;
- ordered by Registry event sequence even when external-world causality is more
  complicated.

Therefore:

```text
EventRecord != EventOccurrence
```

and even `EventOccurrence` remains relative to the model boundary.

---

# 12. Event != Transition

Different formalisms relate them differently:

```text
event triggers transition
event labels transition
transition emits event
event is derived from a trajectory crossing a condition
```

None of these identities is universal.

TLA is revealing because it does not need `event` as the primitive relation. A `step`
is a pair of consecutive states, and an action describes a state-transition relation.

RF1 therefore separates:

```text
Event       = distinguished occurrence in behavior
Step        = pair of adjacent modeled states in a discrete behavior representation
Transition  = allowed relation/evolution from one modeled condition to another
```

A transition can be modeled without naming an event. An event can be observed without
changing the state variables currently under consideration.

---

# 13. Time stamps do not uniquely identify events

Hybrid-system operational semantics exposes an important edge case: multiple ordered
discrete events can occur at the same real-time coordinate. Berkeley work uses
**superdense time**—roughly `(real time, microstep index)`—so signals can have multiple
ordered values at one physical time point.

Therefore:

```text
time(event1) = time(event2)
```

need not imply:

```text
event1 = event2
```

or:

```text
no order exists
```

Conversely, two timestamped records with different clock values do not by themselves
establish causal order.

RF5 will study time/order/causality in depth. RF1 only freezes that `timestamp` is not
the ontology of event identity.

---

# 14. What is a Transition?

For discrete models, the cleanest RF1 definition is relational:

```text
T ⊆ S × S
```

or with labels/inputs:

```text
T ⊆ S × U × S
```

A transition relation specifies which state pairs/evolutions are permitted by the
model under relevant inputs/conditions.

Important:

```text
Transition relation != actual realized transition
```

The relation defines possibilities. One particular behavior realizes a sequence or
structure of permitted transitions.

For nondeterministic systems:

```text
same state + same coarse input
→ several allowed successors
```

so dynamics is naturally a relation or set-valued map rather than one function.

---

# 15. Atomicity is abstraction-relative

A transition may be called `atomic` when intermediate states are not observable or
relevant at the declared abstraction boundary.

It need not be physically instantaneous or internally indivisible.

Examples:

```text
higher-level transaction commit
   ↕ implemented by many log/page/lock steps

Runtime Job admission
   ↕ implemented by SQLite writes and validation

process creation
   ↕ implemented by many kernel operations

filesystem rename
   ↕ one namespace-level atomicity contract, not atomicity of every related external fact
```

Lamport's refinement/reduction work formalizes the general ability to map many
lower-level steps to one higher-level step while preserving the higher specification.
Stuttering is necessary because lower-level steps may not change higher-level state.

Therefore:

```text
Atomic(model) != physically indivisible Reality event
```

and:

```text
one macro-transition may refine into many micro-transitions
```

---

# 16. Transition granularity is not unique

Suppose a command writes a file:

```text
open
write chunk 1
write chunk 2
fsync
rename
close
```

Possible models include:

```text
one high-level transition: old file → new file
```

or:

```text
many low-level transitions over descriptor/buffer/inode/name states
```

Which is right depends on the property being studied.

If crash consistency matters, the coarse one-step model may erase the exact failure
window of interest. If only final consumer-visible file identity matters, the coarse
model may be ideal.

RF1 principle:

> **Transition granularity must be justified by the questions and observability of the
> boundary, not by code indentation or implementation call count.**

---

# 17. Behavior / Trajectory is more general than Event Sequence

RF1 uses `behavior` as the broad family term for a system's modeled evolution.
A single realized member is a trajectory/run/history depending on the formalism.

Different useful representations include:

```text
discrete:   s0, s1, s2, ...
continuous: x(t)
hybrid:     continuous flows + discrete jumps/mode changes
concurrent: interleaved state sequence or partial-order/event structure
stochastic: sampled trajectory under probabilistic dynamics
```

There is no foundation requirement that all behaviors be finite.

A long-running service may have a valid nonterminal behavior indefinitely. A reactive
controller may be defined by continued interaction rather than a final state.

Therefore:

```text
Behavior != finite Job lifecycle
```

Current Ordivon Runtime intentionally specializes in bounded operation-like work; that
is scope, not the definition of behavior.

---

# 18. One State does not characterize one Behavior

A state predicate describes a condition at one modeled point. Many important system
properties are properties of trajectories:

```text
never enters forbidden condition
request is eventually answered
value eventually converges
infinitely often receives service
always responds within a deadline
```

Safety/liveness traditions make this distinction explicit.

Consequences for Runtime Foundations:

```text
CurrentState alone may be insufficient to state correctness.
History/event evidence may be necessary.
Terminal status alone may be insufficient to explain trajectory.
```

This does **not** imply Runtime should store every possible event forever. It means
state and behavior properties are different semantic kinds.

---

# 19. What is Dynamics?

RF1 defines dynamics as:

> **The declared laws, relations, constraints or distributions that determine which
> state evolutions/behaviors are possible from given conditions, inputs and environment
> assumptions.**

Dynamics is not one mathematical type.

## Deterministic discrete

```text
s_(k+1) = F(s_k, u_k)
```

## Nondeterministic discrete

```text
s_(k+1) ∈ F(s_k, u_k)
```

or a transition relation.

## Stochastic

```text
P(s_(k+1) | s_k, u_k)
```

or a more general stochastic process/kernel.

## Continuous

```text
x_dot = f(x, u, t)
```

or a differential inclusion for set-valued dynamics.

## Hybrid

```text
continuous flow within mode
+
discrete jumps/mode switches
```

The hybrid-systems literature exists precisely because real engineered systems can mix
continuous physical evolution and discrete software/control structure.

---

# 20. Dynamics != Behavior

This distinction is fundamental:

```text
Dynamics = space/rules of possible evolution
Behavior  = one allowed evolution
```

For a deterministic closed model, one initial condition may select one trajectory.
For nondeterministic, stochastic or environmentally driven systems, one initial modeled
state can admit many futures.

Therefore:

```text
same state != same future trajectory
```

unless the model's dynamics and future inputs make that guarantee.

This is another reason `same operation identity` cannot imply `same outcome identity`.

---

# 21. Environment and inputs are part of the dynamics contract

Abadi/Lamport's assumption/guarantee framing gives RF1 an important discipline:
system behavior is often specified relative to assumptions about the environment.

The same physical world can be modeled at different boundaries:

```text
larger autonomous system:
  controller + plant + disturbance generator inside boundary

open subsystem:
  controller state inside
  plant/environment represented as inputs
```

Thus:

```text
internal state vs input vs environment
```

is boundary-relative.

This directly explains why current Runtime `sourceStateDigest` can be a strong source
commitment while intentionally excluding:

```text
wall clock
network responses
ambient external services
some ignored files
undeclared dependencies
```

Those exclusions define the current commitment boundary; they are not evidence that
Runtime believes the excluded world does not exist.

---

# 22. Autonomous, driven, reactive and controlled are different

RF1 needs a vocabulary before later Action/Authority rounds.

## Autonomous evolution

Dynamics evolves from internal state without an externally modeled input:

```text
x_dot = f(x)
```

This is a model-relative description; the larger Reality may still contain causes not
represented explicitly.

## Driven/open system

Environment/input participates in evolution:

```text
x_dot = f(x, u)
```

## Reactive system

Behavior is substantially defined by ongoing responses to interactions rather than by
computing one terminal answer.

## Controlled system

Some input is selected with the purpose of shaping reachable behavior/state. Control
adds a policy/intervention relation; RF1 does not yet equate that with agency or intent.

## Hybrid system

Discrete modes/events and continuous dynamics interact.

These categories can overlap.

---

# 23. Dynamics does not by itself establish causation

A transition relation says:

```text
these evolutions are allowed/related by the model
```

It does not automatically say:

```text
A uniquely caused B
```

Likewise:

```text
A happened before B
```

is not enough for causal attribution.

A control input occurring before a state change may be a causal intervention under a
well-supported dynamical model, but cause attribution depends on boundary,
counterfactual assumptions, confounding/environment and the claimed level of
explanation.

RF1 therefore postpones full causality to later rounds and freezes only:

```text
Transition relation != causal proof
Temporal order != causal proof
Runtime lineage participation != exclusive causal ownership
```

---

# 24. Event extraction from continuous behavior

Consider temperature `T(t)` rising smoothly.

One model can define:

```text
Event E = first t where T(t) >= 100°C
```

Another model can ignore that threshold entirely.

Nothing physically discontinuous must happen at exactly 100°C merely because the
software model creates an event there.

Conversely, some physical systems do have discontinuities or mode changes for which
discrete-event semantics is highly useful.

RF1 result:

> **Events can be discovered/observed in Reality, but the decision that a particular
> occurrence is one event with one identity/boundary is still model- and scale-sensitive.**

This avoids both extremes:

```text
"events are purely fictional"
"Reality comes pre-partitioned into one canonical event log"
```

---

# 25. Hybrid systems defeat the discrete/continuous false dichotomy

Cyber-physical and hybrid systems combine:

```text
differential/continuous flow
mode/state-machine changes
discrete messages/events
concurrency
sensor observation
actuator intervention
```

Berkeley hybrid-system work explicitly combines continuous state trajectories with
discrete transitions and shows that execution semantics may require richer time models
such as superdense time.

Therefore a Runtime foundation that assumes either:

```text
Everything is a discrete state machine.
```

or:

```text
Everything is one smooth continuous dynamical system.
```

would be unnecessarily narrow.

The correct foundation level is **behavior/evolution with multiple possible models of
computation and dynamics**.

---

# 26. Concurrency also defeats one canonical event sequence

A concurrent system can often be represented by an interleaving sequence of atomic
steps, but that sequence can serialize operations that were independent.

Alternative models preserve partial-order/concurrency structure.

RF1 does not choose one universal concurrency ontology. It retains:

```text
behavior representation may be a sequence
behavior representation may preserve partial order
one physical concurrent evolution may admit multiple observational serializations
```

Hence Registry `event_sequence` is a canonical **Runtime record order**, not a claim
that every external occurrence in Reality was intrinsically totally ordered by that
sequence.

RF5 will return to this.

---

# 27. Change of Model State can happen without Target-World Change

An observation-only or reconciliation system gives the inverse falsifier of stuttering.

Suppose an external deployment committed at t1. Runtime lost transport and still records
unknown. At t2 Runtime reads a durable provider receipt and updates:

```text
unknown → committed
```

The external deployment need not change at t2.

What changed is Runtime's epistemic/control state.

Thus:

```text
StateRecordChange != target-world effect
```

This is a critical foundation for reconciliation.

---

# 28. Cross-domain falsifier matrix

| Case | What it attacks | Surviving distinction |
|---|---|---|
| pure deterministic calculation | dynamics must be stochastic/open | deterministic dynamics remains one valid special case |
| Dijkstra nondeterministic guarded commands | state determines unique next state | one state can permit multiple successors/final states |
| long-running server | behavior requires terminal state | valid behavior may be indefinite/reactive |
| TLA stuttering | unchanged subsystem state means unchanged Reality | external/lower-level change can occur with unchanged projected state |
| refinement from many low-level steps to one high-level step | transition granularity is intrinsic | atomicity/granularity is abstraction-relative |
| unobservable plant state estimated from sensors | state = observation | hidden state, output and estimate differ |
| continuous pendulum/vehicle trajectory | evolution requires discrete events | continuous dynamics can evolve without privileged event boundaries |
| threshold crossing at 100°C | event is physical atom | event can be extracted from continuous trajectory by model semantics |
| hybrid controller | discrete and continuous models are mutually exclusive | one behavior can combine flows, jumps and modes |
| two microsteps at same wall-clock time | timestamp identifies event/order | superdense/microstep ordering may be needed |
| concurrent independent operations | behavior is one intrinsic total order | interleaving is one representation; independence can survive as partial order |
| distributed system snapshot | separately observed local states form current global state | coherent global-state observation requires explicit protocol/semantics |
| database transaction | atomic transition is physically one step | higher atomicity can hide many implementation steps |
| filesystem patch response loss | state update equals effect evidence | durable intent/receipt/reconciliation may be separate projections |
| remote API commits before timeout | caller state drives provider state | local epistemic state can lag external state |
| Runtime later learns old result | state change means world changed now | epistemic update can occur without new target effect |
| robot drive command | command = transition | command is input/specification; plant follows continuous dynamics |
| financial order with partial fills | one operation = one state jump | external behavior can contain multiple domain transitions/effects |
| human delegation | all relevant state is directly observable | hidden cognitive/social state and uncertain observation are ordinary |
| same Runtime `Running` label during heavy activity | no state transition = no behavior | coarse state may stutter while rich behavior continues |

---

# 29. RF1 anti-laws

RF1-L1: `State != Reality`.

RF1-L2: `State != complete instantaneous universe snapshot`.

RF1-L3: `State representation != unique intrinsic coordinates`.

RF1-L4: `State != necessarily minimal`.

RF1-L5: `State != necessarily observable`.

RF1-L6: `State != Observation`.

RF1-L7: `State != State Estimate`.

RF1-L8: `State Record != target-world state`.

RF1-L9: `One flat enum != proof of one ontological dimension`.

RF1-L10: `Modeled change != every world change`.

RF1-L11: `No modeled change != no Reality change`.

RF1-L12: `Epistemic state change != target-world effect`.

RF1-L13: `Event != log record`.

RF1-L14: `Event != Transition`.

RF1-L15: `Event != necessarily physical discontinuity`.

RF1-L16: `Timestamp != event identity`.

RF1-L17: `Same timestamp != same event`.

RF1-L18: `Transition relation != realized transition`.

RF1-L19: `Transition != Effect`.

RF1-L20: `Transition relation != causal proof`.

RF1-L21: `Atomic transition != physically instantaneous/indivisible change`.

RF1-L22: `Transition granularity != intrinsic implementation call granularity`.

RF1-L23: `Discrete next-state relation != universal dynamics form`.

RF1-L24: `Behavior != State`.

RF1-L25: `Behavior != finite lifecycle`.

RF1-L26: `Behavior != necessarily event sequence`.

RF1-L27: `Dynamics != Behavior`.

RF1-L28: `Dynamics != deterministic function by definition`.

RF1-L29: `Same modeled state != same realized future` without dynamics/input conditions.

RF1-L30: `Input/Environment boundary != intrinsic universal partition`.

RF1-L31: `Reactive != necessarily terminating`.

RF1-L32: `Controlled != autonomous`.

RF1-L33: `Hybrid behavior != reducible by default to only discrete or only continuous ontology`.

RF1-L34: `Registry event order != universal causal order`.

RF1-L35: `Current Runtime AttemptState != Runtime Foundations state ontology`.

---

# 30. Minimum RF1 grammar

RF1 now retains the following concepts for later rounds.

## 30.1 World / system boundary `B`

What is inside the system, what is environment, and what is ignored?

## 30.2 History `H_<=t`

The prior evolution relevant to forming current state.

## 30.3 State abstraction `σ`

A representation/history compression adequate for selected future reasoning.

```text
s_t = σ_(B,Q)(H_<=t)
```

## 30.4 Observation `o`

Information acquired about world/system behavior.

## 30.5 State estimate / recorded projection `ŝ`

Observer-owned belief/record about state.

## 30.6 Input/environment `u/e`

Exogenous or separately modeled influences at the chosen boundary.

## 30.7 Dynamics `D`

Rules/constraints/distribution of admissible evolution.

## 30.8 Behavior/trajectory `τ`

One modeled evolution satisfying `D` under initial conditions and environment/input.

## 30.9 Event extractor `E(τ)`

Optional model that marks distinguished occurrences in a behavior.

## 30.10 Discrete transition relation `T`

Optional relation used when a discrete state-step representation is appropriate.

## 30.11 Property `P`

A claim over states or whole behaviors. State predicates and temporal/trajectory
properties must remain distinguishable.

---

# 31. A more general dynamic reconstruction

RF0 used a mostly linear realization diagram. RF1 replaces the lower half with a more
general behavior model:

```text
                World / Environment
                       │
            boundary + observation
                       │
                       ▼
History H_<=t ──σ──→ State s_t
                       │
             inputs/environment u
                       │
                       ▼
                 Dynamics D
                       │
          ┌────────────┴─────────────┐
          │ possible behavior space │
          └────────────┬─────────────┘
                       │
                realized trajectory τ
                       │
           ┌───────────┴────────────┐
           │                        │
           ▼                        ▼
     event extraction          observation
      E(τ) optional                 │
           │                        ▼
           │                  estimate/record ŝ
           └──────────────┬─────────┘
                          ▼
                  later interpretation
```

The crucial shift is:

```text
state-machine transitions
```

are now one possible projection of `τ`, not the universal substrate beneath Reality.

---

# 32. A behavior-space view of a system

A useful RF1 abstraction is:

```text
SystemModel = set/constraint/distribution of admissible behaviors
```

rather than:

```text
System = current enum state
```

TLA explicitly models a system specification through allowed behaviors; control theory
models trajectories generated by dynamical laws; hybrid systems combine discrete and
continuous evolution.

This lets RF1 unify them without claiming their mathematical formalisms are identical.

For Runtime research, this suggests a later distinction:

```text
Operation specification   constrains desired/admissible realization behavior
Runtime machinery         constrains and owns some physical realization lineage
Actual world trajectory   is one concrete evolution
Evidence                  is a partial projection of that trajectory
```

The exact terms `Operation` and `Realization` are intentionally deferred to RF2/RF3.

---

# 33. When can Runtime say it realized a transition?

RF1 cannot yet give a full causal definition because Action/Authority/Execution are
unresolved. It can establish a conservative boundary.

Runtime should not claim:

```text
"state changed, therefore Runtime caused it"
```

or:

```text
"Runtime issued a command, therefore the target state changed"
```

A future claim of Runtime realization will need at least:

```text
identified specification/operation
+ admitted authority/preconditions
+ identity-bound realization lineage
+ evidence connecting that lineage to relevant behavior
+ explicit target-world/effect boundary when claiming external transition
```

RF1 therefore leaves `cause` and `action` unresolved while preserving the evidence
requirements already suggested by current Runtime practice.

---

# 34. Current Runtime design re-audit under RF1

## 34.1 What remains strongly supported

### Separate execution and delivery certainty

Because ontic realization and epistemic knowledge differ, `executionDisposition` and
`deliveryDisposition` should not collapse conceptually.

### Explicit `lost` / `orphaned`

These encode operational knowledge/ownership conditions rather than pretending every
physical state is known.

### Event history as evidence, not sole truth owner

`job_events` is useful for reconstruction/latency/history while stronger owners remain
Registry rows, systemd/cgroups, Result/Artifact bytes and external receipts.

### Reconciliation can change Runtime state without repeating external effect

This follows directly from epistemic/control state being separate from target-world
state.

### Source-state commitment is a scoped projection

The digest need not include the universe. Its correctness claim must name exactly what
world slice it binds.

## 34.2 What RF1 prevents us from overclaiming

### `AttemptState` is not the ontology of execution

It is an operational projection.

### Job event sequence is not universal physical causality

It is a Runtime-owned total record order.

### State-machine transition tests do not by themselves prove physical effect semantics

They prove Registry/control-plane transition law.

### Terminal state is not a universal Runtime requirement

It is appropriate for current bounded operation scope, not all reactive runtimes.

---

# 35. Research constitution added by RF1

Later Runtime rounds should obey these rules.

1. Every use of `state` should identify whose state/projection it is when ambiguity
   matters.
2. Do not use `current state` to imply complete current Reality.
3. Distinguish target-world state from Runtime-recorded/estimated state.
4. Do not invent an event merely because a log row exists.
5. Do not call a transition atomic without naming the observation/abstraction boundary.
6. Do not assume discrete step semantics for continuous/reactive domains.
7. When state is insufficient because behavior/history matters, say so rather than
   stuffing every fact into a bigger enum by default.
8. Treat dynamics as a constraint on possible behavior, not as the realized behavior
   itself.
9. Environment assumptions are part of the model contract.
10. Delay causal language until the evidence and intervention model justify it.

---

# 36. RF1 result — the key conceptual compression

The strongest RF1 compression is:

```text
Reality does not arrive as one canonical state machine.

A model chooses:
  boundary
  state representation
  observations
  inputs/environment
  dynamics
  time/granularity

Those choices define a behavior space.
A concrete evolution is one trajectory in that space.
Events and discrete transitions are optional projections/segmentations of the trajectory.
Evidence gives an observer partial knowledge of that evolution.
```

In compact notation:

```text
H_<=t --σ_(B,Q)--> s_t
(s_t, u/e, D) --> admissible future trajectories
τ = one realized/modelled behavior
E(τ) = optional event projection
T = optional discrete transition relation
O(τ) --> observations/estimates, not Reality itself
```

This is the lower dynamic grammar that RF0 was missing.

---

# 37. RF1 closeout

RF1 closes with fourteen durable results:

1. **State is a model projection/history compression, not complete Reality.**
2. The strongest portable state criterion is future-relevance under a declared
   boundary, question, dynamics and admissible environment/input.
3. State representations can be nonunique, nonminimal and hidden/unobservable.
4. Target state, observation, estimate and recorded operational state must remain
   distinct.
5. A flat operational enum may intentionally compress multiple underlying dimensions.
6. Change is projection-relative; stuttering proves that unchanged modeled state does
   not imply unchanged wider Reality.
7. Continuous evolution shows that discrete transitions are not universal ontic atoms.
8. Events are distinguished modeled occurrences, not a guaranteed canonical partition
   of Reality.
9. Event, transition, event record and timestamp are distinct concepts.
10. Atomicity and transition granularity are abstraction-relative.
11. **Behavior/trajectory is more general than state sequence or event sequence.**
12. **Dynamics describes possible evolution; behavior is one realized evolution.**
13. Open, reactive, concurrent and hybrid systems require explicit environment,
    boundary, time/granularity and possibly nonterminal behavior.
14. Current Ordivon Runtime's state/evidence separation is substantially supported,
    while its finite Attempt state machine remains an engineering specialization.

The next unavoidable frontier is now safe to approach:

> **What are Action, Intent, Proposal, Command and Operation, and how does an Agent- or
> controller-selected possibility differ from a merely allowed transition in the
> system's dynamics?**

That is RF2.

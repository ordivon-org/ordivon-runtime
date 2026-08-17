---
schema_version: 1
id: runtime.foundations.rf0
title: RF0 — Runtime Phenomenon Space and Existing-System Decomposition
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
summary: RF0 brackets the existing Ordivon Runtime implementation, reconstructs the lower phenomenon-space of state, event, behavior, action realization, observation, commitment, termination and uncertainty, compares rival Runtime models, falsifies premature process/execution/state-machine reductions, and derives the minimum grammar needed for later Runtime Foundations rounds.
evidence_status: verified
readiness: READY
applies_to:
  - RUNTIME-FOUNDATIONS-001
  - RF0
related:
  - runtime.start
  - runtime.model
  - runtime.effect-kernel
  - runtime.foundations.rf0.sources
  - runtime.foundations.rf0.continuation
---
# RF0 — Runtime Phenomenon Space and Existing-System Decomposition

## 0. Status and research question

RF0 is the first Runtime Foundations round. It is a **foundation research record**,
not authorization to refactor Runtime Core, rename public Tools, add a generic Effect
framework, replace systemd/cgroups, or generalize Ordivon Runtime into a workflow
engine.

The current Runtime is already an operational, heavily dogfooded execution and
recovery system. RF0 deliberately treats that maturity as **evidence to explain**, not
as ontology to inherit.

RF0 asks:

> **What lower phenomenon-space must exist before terms such as runtime, execution,
> process, operation, effect, completion, recovery and reconciliation can even be
> meaningful, and which parts of the current Ordivon Runtime are general structure
> versus one implementation of that structure?**

The round starts by temporarily bracketing the name `Runtime` and the current object
model. It then works upward from world change, state, event, behavior, observation,
intervention and uncertainty.

---

# 1. Why RF0 is needed

Ordivon Runtime was built primarily by pressure from real engineering failures:
response loss, duplicate dispatch risk, mutable source state, process-tree leakage,
crash recovery, ambiguous completion, provider drift, external input mutation,
Windows execution, concurrency limits and self-replacement.

That process produced strong local invariants:

```text
Proposal != durable commitment
Job != Attempt
Attempt dispatch is at-most-once
process exit != semantic completion
unknown != failed
replay != retry
execution evidence != external-effect receipt
```

Those distinctions are valuable. But they were discovered while solving one class of
owner-trusted Agent execution problems. A foundation account must still ask whether:

- `Process` is fundamental or merely one implementation view;
- `State` is a property of Reality or a modeling projection;
- `Execution` is the root phenomenon or one form of realization;
- `Effect` belongs to Runtime or to the external domain;
- `Commitment` is universal to runtimes or specific to durable effectful runtimes;
- `Termination` means local process death, global quiescence, semantic completion, or
  something else;
- `Recovery` means retry, reconstruction, convergence, compensation, or restoration;
- one exact global current state is even available to the observer;
- Runtime should be defined by what it **does to Reality**, what it **knows about
  Reality**, or both.

RF0 therefore copies the Game / Media / Finance / Human foundation method:

```text
surface term
→ overloaded-term separation
→ competing root models
→ cross-domain triangulation
→ edge-case falsification
→ anti-collapse laws
→ minimum surviving grammar
→ reconnect to existing implementation
→ expose next unavoidable boundary
```

---

# 2. First deletion — Runtime is not synonymous with a process runner

A process runner is an important realization mechanism, but `process` does not survive
as a foundation primitive.

Lamport's *Processes are in the Eye of the Beholder* shows why: the same system
behavior may be decomposed into different numbers of processes. The process boundary
can therefore be a useful **view of state and behavior** without being the unique
ontology of the system.

The current Ordivon Runtime itself already provides a physical example:

```text
local_linux
  Runtime Attempt
    → systemd unit
    → cgroup
    → Runner
    → target process tree

windows_native
  Runtime Attempt
    → WSL systemd supervisor
    → Windows launcher
    → Windows Job Object
    → native process tree
```

The upper Runtime identity can remain one Job/Attempt while the lower process
realization changes substantially.

Therefore:

```text
Runtime != Process
Operation != Process
Attempt != PID
process decomposition != semantic decomposition
```

A process is one carrier of activity under a particular substrate.

---

# 3. Second deletion — Runtime is not synonymous with computation

Pure computation is too narrow.

A Runtime operation may:

- calculate a digest;
- compile source;
- mutate files;
- start a service;
- deploy software;
- invoke a network API;
- submit a financial instruction;
- drive a robot or device;
- emit a message that changes another actor's later behavior.

Some of these primarily transform information inside a computational substrate. Others
matter because they alter an external world that the process does not own.

Conversely, not every computation needs the properties that motivated Ordivon
Runtime. A deterministic pure function can often be recomputed after interruption with
little semantic concern.

Therefore:

```text
Runtime phenomenon > computation alone
computation != effect
computation completion != external consequence completion
```

`Execution` remains a useful term, but RF0 does not accept it as the unexplained root.

---

# 4. Third deletion — State transition is necessary as a model, insufficient as a discriminator

A very general model is:

```text
S_t --transition--> S_(t+1)
```

State-machine traditions, Dijkstra's program semantics, Harel's Statecharts and
Lamport's work all show the power of describing behavior by states and transitions.
Statecharts additionally demonstrate that realistic reactive systems require hierarchy,
concurrency and communication beyond a flat state diagram.

But `state transition` is too broad to define Runtime. Weather, corrosion, biological
metabolism, market prices and an unattended radioactive decay can all be represented
as changing state.

RF0 therefore distinguishes:

```text
World change                any difference in the modeled world
Event                       a modeled occurrence/boundary in a behavior
Transition                  relation between modeled states
Action                      intervention attributable to some acting/controller relation
Realization                 physical/operational carrying-out of a selected specification or action
Effect                      consequence in some declared external state/projection
Runtime                     still unresolved; must explain a special action-realization relation
```

A state-transition model is a **representation grammar**, not proof that Reality is
literally a finite state machine.

---

# 5. Fourth deletion — Runtime is not a scheduler or resource allocator

Operating systems, cluster runtimes and MapReduce-style systems often schedule work,
allocate machines and recover failed tasks. Resource management is important because
realization always consumes finite substrate.

But scheduling is not necessary to the narrow Runtime phenomenon. A single-operation
runtime with one execution slot can still establish identity, authority, realization,
evidence and recovery. Conversely a scheduler can order work without owning the
meaning or truth of its external effects.

Therefore:

```text
Runtime != Scheduler
capacity != priority
resource budget != operation meaning
admission != queueing
```

Ordivon's current choice of immediate admission failure rather than a hidden queue is
an implementation/policy result, not a foundation law.

---

# 6. Fifth deletion — Runtime is not just an execution environment

Language runtimes and container runtimes highlight another model: prepare an
environment in which a program can execute.

The OCI Runtime Specification is especially instructive. It separates configuration,
container identity, states such as `creating/created/running/stopped`, and lifecycle
operations such as create/start/kill/delete. It also freezes configuration semantics at
specific lifecycle boundaries.

This captures a real Runtime family:

```text
Specification
→ realized environment
→ started activity
→ lifecycle observation/control
→ cleanup
```

But it remains insufficient for Ordivon's problem. Container lifecycle does not by
itself answer whether an opaque target already placed an order, sent a message, or
committed a deployment before the client lost contact.

Therefore:

```text
execution environment is one Runtime function
process lifecycle is one Runtime projection
neither proves open-world effect semantics
```

---

# 7. Competing RF0 root models

RF0 keeps nine rival models alive long enough to expose their useful scope and failure.

## M0 — Process execution model

```text
Runtime = start / supervise / stop processes
```

### Strength

- concrete and mechanically enforceable;
- aligns with OS process supervisors and container runtimes;
- explains resource limits, cancellation and descendant cleanup.

### Failure

Process decomposition is not unique; one operation can span multiple substrates; a
process can succeed while the desired external action fails, or fail after the external
action already occurred.

**Disposition:** realization substrate, not root ontology.

## M1 — State-transition model

```text
Runtime = system that realizes valid state transitions
```

### Strength

- highly general;
- supports invariants, preconditions, safety/liveness and formal reasoning;
- naturally handles concurrency and reactive behavior when enriched.

### Failure

Too broad. It includes spontaneous natural change and says nothing about who selected
the transition, which authority permits it, or how occurrence is known.

**Disposition:** foundational representation grammar, insufficient discriminator.

## M2 — Computation / behavior realization model

```text
specification/program → realized behavior
```

### Strength

- captures language, VM, container and OS runtime traditions;
- separates description from physical execution;
- explains environment and substrate dependence.

### Failure

`behavior` can continue indefinitely and may have consequences outside the execution
substrate. It also does not explain durable identity or ambiguity after interruption.

**Disposition:** strongest common runtime-family ancestor so far.

## M3 — Controller / actuator model

```text
controller intent → actuator/runtime → world intervention → feedback
```

### Strength

- connects runtime to control, robotics and Agent action;
- makes world interaction and feedback explicit;
- prevents command text from being mistaken for physical consequence.

### Failure

Not every runtime is a closed-loop controller. Pure batch computation can be realized
without an application-level target state or feedback policy.

**Disposition:** powerful for Agent/world effects; not universal runtime definition.

## M4 — Durable operation commitment model

```text
proposal → durable commitment → realization → evidence
```

### Strength

- explains Ordivon's response-loss and duplicate-effect problems;
- separates candidate action from irreversible physical dispatch;
- supports exact replay and recovery across control-plane interruption.

### Failure

Many conventional runtimes do not need durable admission before execution. A local
interpreter can begin evaluating immediately. Durability is therefore not necessary to
`runtime` in the broad sense.

**Disposition:** likely central to **Ordivon Runtime**, not universal runtime ancestry.

## M5 — Transaction / commit model

```text
precondition → operation → commit/abort → durable state
```

### Strength

- transaction processing gives mature language for atomic commitment, durability,
  recovery and concurrency;
- Gray/Lamport show that distributed commit is a distinct agreement problem, not a
  synonym for local process completion.

### Failure

Arbitrary open-world effects are not generally transactional. A sent email, market
order, physical motion or third-party API may have no rollback or atomic commit owner.

**Disposition:** essential comparison; reject transaction totalization.

## M6 — Linearizable operation model

```text
invocation ─────── response
        \ some abstract effect point /
```

### Strength

- Herlihy/Wing separate invocation and response from an abstract point at which an
  operation can be reasoned about as taking effect;
- useful for shared concurrent objects and for thinking about `commit point`.

### Failure

Not every operation is linearizable, atomic or closed-world. Long external effects can
have multiple visible intermediate consequences and no single authoritative point.

**Disposition:** strong local correctness model; not universal effect ontology.

## M7 — Evidence / reconciliation model

```text
world + durable record + observations → converged operational claim
```

### Strength

- explains restart recovery, ambiguous dispatch and late evidence;
- captures the fact that control-plane knowledge can lag physical reality;
- Dijkstra-Scholten and Chandy-Lamport show that termination/global-state facts may
  require additional observation protocols rather than local inactivity alone.

### Failure

Observation/reconciliation can describe what happened but does not itself cause the
original action.

**Disposition:** irreducible epistemic half of Ordivon Runtime, not the whole runtime.

## M8 — Authority-bounded realization model

```text
principal + capability/authority + specification
→ admissible realization
```

### Strength

- explains why the same command under different identities or privileges is a
  materially different operational possibility;
- maps naturally to limited/elevated, contained/trusted and external resource access.

### Failure

Authority determines what may/can be done, not what actually occurred or whether the
result is semantically useful.

**Disposition:** necessary coordinate for effectful runtime; insufficient alone.

---

# 8. The first surviving reconstruction

No single rival model is sufficient. The strongest surviving RF0 structure is a
**relation between a specification/selection and a realized behavior in a bounded
world, plus an evidence channel about that realization**.

Provisional hypothesis H-RF0:

> **A runtime is a realization boundary: a system/subsystem that takes a selected
> executable specification or operation under some authority and environment,
> participates in producing a concrete behavior in Reality, and exposes enough
> lifecycle/control/observation structure for another participant to relate that
> behavior back to the selection.**

For **Ordivon Runtime**, the hypothesis is narrower and stronger:

> **Ordivon Runtime is a durable, authority-bounded realization-and-convergence layer
> for Agent-selected local operations: it turns a candidate operation into an
> identity-bound physical commitment, owns the execution substrate it can actually
> supervise, preserves evidence and uncertainty across interruption, and converges
> operational claims without pretending to own semantic or external-domain truth.**

Neither sentence is frozen as the final Runtime definition. RF1+ must attack them.

---

# 9. Three planes that RF0 must not collapse

A major result of RF0 is that current Runtime concepts live on at least three different
planes.

## 9.1 Intentional / specification plane

What some participant proposes, selects or wants realized:

```text
proposal
command
plan
requested authority
precondition
budget
operation identity
```

This plane may contain meaning that physical execution cannot verify.

## 9.2 Ontic / realization plane

What physically happens:

```text
bytes exist
process starts
syscall occurs
message leaves
file changes
remote service changes
process tree terminates
```

This plane does not automatically carry the user's semantic interpretation.

## 9.3 Epistemic / evidence plane

What Runtime or another observer can justifiably claim:

```text
observed start
receipt exists
output retained
process tree absent
state query says committed
unknown
orphaned
lost
```

The crucial anti-collapse is:

```text
intended(X) != occurred(X) != proven(X)
```

Many historical Runtime bugs can be restated as accidentally equating two of these
planes.

A fourth plane — **normative/domain evaluation** such as `correct`, `useful`, `safe`,
`profitable`, `task complete` — normally belongs above Runtime.

---

# 10. State, event, behavior and trajectory

RF0 adopts these as research handles, not database schemas.

## State

A chosen projection of relevant properties at a modeled instant/boundary.

Important limitation:

```text
state is observer/model relative
```

A complete physical world state is neither required nor usually available.

## Event

A modeled occurrence that distinguishes one part of a behavior from another.

Examples:

```text
request admitted
runner bound
child started
file renamed
response received
receipt committed
cgroup became empty
```

An event is not automatically an effect. Logging `started` is itself an event; the
external operation may still have produced no target-domain effect.

## Transition

A relation between modeled states.

A transition can be atomic in the model while its implementation contains many
physical events.

## Behavior / trajectory

A temporally/causally structured sequence or partial order of states/events.

This is more fundamental than process decomposition for RF0 because the same behavior
can be realized by different process structures.

---

# 11. Time and causality appear before scheduling

Lamport's event-ordering result is foundational here. In a distributed system,
`happened-before` gives a partial order; not every pair of events has an intrinsic
causal order. A total timestamp ordering can be imposed for coordination, but that is
not the same thing as discovering a universal physical sequence.

RF0 therefore rejects:

```text
wall-clock order == causal order
response order == effect order
log line order == world order
```

For Runtime Foundations, later time/concurrency work must distinguish at least:

```text
physical time
logical/causal order
admission order
dispatch order
observation order
commit/effect order
completion order
```

These can differ.

---

# 12. Termination is not one thing

Dijkstra-Scholten termination detection is a decisive falsifier of naive local
completion. A distributed computation can have locally idle nodes while messages or
engaged descendants still make the whole computation active. Chandy-Lamport likewise
treat termination and deadlock as global/stable-state detection problems.

RF0 therefore separates:

```text
step termination
process exit
process-tree quiescence
runtime-attempt terminalization
distributed computation termination
external effect finality
semantic task completion
```

Current Ordivon Runtime intentionally owns only some of these.

A process exit code is useful evidence about one realization. It is not a universal
completion predicate.

---

# 13. Nondeterminism is native, not an error condition

Dijkstra's guarded-command work makes non-determinacy explicit: the same initial state
and valid program structure need not force one unique activity or even one unique final
state, while correctness can still constrain the set of permissible outcomes.

This matters directly for Agent Runtime research:

```text
same high-level goal
→ multiple valid proposals

same proposal under nondeterministic environment
→ multiple possible trajectories

same trajectory prefix
→ multiple possible futures
```

Therefore:

```text
deterministic reproduction is not prerequisite for correctness
operation identity does not imply outcome identity
retry may generate a different trajectory even when request text matches
```

The runtime problem is partly to constrain and evidence nondeterministic realization,
not to pretend nondeterminism is absent.

---

# 14. Observation changes the Runtime problem

A runtime is not only an actuator. Once failures, asynchronous execution or external
systems exist, another problem appears:

> **How does the controller know what Reality currently justifies claiming?**

This is why `task.get` and `task.observe` being different operations is not merely API
polish.

RF0 distinguishes:

```text
projection-only observation
active probing
reconciliation
control intervention
```

These differ in whether they may alter Runtime state or physical work.

Chandy-Lamport's global snapshot work is a useful warning: an observer cannot casually
assume that independently read local states form one coherent global instant.

Therefore:

```text
observation != global truth
multiple true local observations != one atomic global snapshot
reconciliation != passive read
```

---

# 15. Commitment is a boundary, not a synonym for action

Current Ordivon Runtime has a strong distinction:

```text
proposal
→ durable admission
→ dispatch
```

RF0 generalizes this cautiously.

A **commitment** is a boundary after which some participant/system has taken on a
stable obligation, reservation, identity or irreversible choice relative to a declared
contract. Different systems have different commitment points:

```text
transaction log commit
queue acceptance
container create
runtime Job admission
broker order acceptance
filesystem rename
human promise
```

These are not equivalent mechanisms.

The durable-admission boundary is therefore not a universal law of all runtimes, but
it is a likely primitive for **recoverable effectful Agent runtimes** because model
output can otherwise be regenerated differently after interruption.

---

# 16. Effect is relational and projection-dependent

`Effect` is often used as if it were a primitive object. RF0 rejects that simplification.

An event has an effect only relative to some chosen state/projection:

```text
process wrote bytes
→ filesystem effect

HTTP request accepted
→ provider state effect

market order filled
→ portfolio / venue-position effect

message delivered
→ communication-state effect
```

The same physical trajectory can be:

```text
no effect on projection A
material effect on projection B
unknown effect on projection C
```

Therefore:

```text
effect requires a declared world/projection boundary
Runtime cannot infer arbitrary domain effect from process evidence
```

This strongly supports the current refusal to let callers label arbitrary
`workspace.exec` as `READ_ONLY`, `IDEMPOTENT` or `RECONCILABLE` without an enforceable
adapter contract.

---

# 17. Failure is multidimensional

RF0 rejects a single `success/failure` axis.

A Runtime-related failure can occur at different layers:

```text
proposal invalid
precondition stale
authority unavailable
admission rejected
resource capacity unavailable
dispatch uncertain
provider drift
process start failure
runtime drift during execution
timeout
cancellation
process nonzero exit
missing result
orphaned physical activity
external effect rejected
external effect ambiguous
semantic objective unsatisfied
```

These are not interchangeable.

At minimum, later foundations must preserve separate dimensions for:

```text
admission status
realization/execution status
delivery/observation certainty
physical ownership/quiescence
external-effect status
semantic status
```

Current Runtime already exposes several of these independently; RF0 now explains why
that separation is structurally justified.

---

# 18. Recovery is not retry

One of the strongest RF0 anti-laws is:

```text
Recovery != Retry
```

Recovery may mean:

```text
reattach to existing execution
reconstruct durable state
re-observe physical owner
reconcile evidence
finish cleanup
release stale reservation
restore from backup
compensate a committed effect
form a new explicit attempt
```

Retry is only one possible strategy and is safe only when effect semantics justify it.
AWS's public idempotency guidance gives the standard distributed-systems dilemma: a
network timeout can leave the caller unable to know whether a mutating request already
took effect, and blind retry can duplicate the effect unless stable request identity or
reconciliation exists.

Current Ordivon's `clientRequestId + existing Job lookup + no speculative redispatch`
therefore appears to be a principled response to a general ambiguity class, not an
arbitrary implementation habit.

---

# 19. Cross-domain falsifier matrix

| Case | Premature model falsified | Surviving distinction |
|---|---|---|
| pure SHA-256 computation | Runtime = external effect system | realization can matter without open-world mutation |
| Python interpreter executing pure function | Runtime requires durable admission | broad runtimes can realize behavior without durable commitment |
| OCI container created but not started | create = execute | environment realization != target activity start |
| process exits 0 after HTTP request timed out | exit success = external success | process result != provider effect status |
| market order accepted before client disconnect | no response = no effect | delivery uncertainty != occurrence |
| retry creates two resources | same request text = same effect | stable effect identity/idempotency matters |
| file transaction atomically renames final path | all effects are gradual | some effect owners provide strong commit points |
| email sends to two recipients before crash | operation = one atomic effect | one command can create multiple irreversible effects |
| child survives parent PID | PID = execution ownership | process tree / ownership relation matters |
| distributed computation has idle node + message in flight | local inactivity = termination | global termination is relational/state property |
| same system modeled as 2 or N processes | process = fundamental ontology | process is a decomposition/view |
| two concurrent events have no causal relation | timestamp sequence = intrinsic order | causal order can be partial |
| same command under limited vs elevated token | command text = operation identity | authority changes feasible realization |
| same executable with changed shared library | executable hash = complete environment identity | behavior depends on environment/dependencies |
| same host path, private mount namespace changes target view | host observation = target consumption truth | observer scope must be explicit |
| long-running service | Runtime implies natural finite completion | some runtime behaviors are open-ended/reactive |
| cancellation arrives after effect already committed | cancel intent = undo | control intent does not reverse past world effects |
| process crashes after durable deployment receipt | failed process = failed deployment | effect receipt may outrank generic execution outcome |
| task objective unsatisfied despite all commands exit 0 | execution success = task success | semantic completion belongs elsewhere |

---

# 20. RF0 anti-laws

RF0-L1: `Runtime != Process Runner`.

RF0-L2: `Process != fundamental behavior ontology`.

RF0-L3: `Runtime != Scheduler`.

RF0-L4: `Runtime != Workflow Engine`.

RF0-L5: `Runtime != State Machine`; state machines are models of behavior.

RF0-L6: `State transition != action`; spontaneous changes also transition state.

RF0-L7: `Command != Operation`.

RF0-L8: `Proposal != Commitment`.

RF0-L9: `Commitment != Dispatch`.

RF0-L10: `Dispatch != proven start`.

RF0-L11: `Start != completion`.

RF0-L12: `Process exit != process-tree quiescence`.

RF0-L13: `Process-tree quiescence != distributed/global termination`.

RF0-L14: `Execution completion != external-effect finality`.

RF0-L15: `Execution success != semantic completion`.

RF0-L16: `Event != Effect`.

RF0-L17: `Effect != Receipt`.

RF0-L18: `Receipt != semantic correctness`.

RF0-L19: `Observation != Reality`.

RF0-L20: `Local observations != atomic global state`.

RF0-L21: `Wall-clock order != causal order`.

RF0-L22: `Timeout != failure`.

RF0-L23: `Transport loss != execution loss`.

RF0-L24: `Unknown != failed`.

RF0-L25: `Recovery != retry`.

RF0-L26: `Replay != retry`.

RF0-L27: `Same command text != same operation`.

RF0-L28: `Same operation identity != deterministic outcome`.

RF0-L29: `Authority != effect`.

RF0-L30: `Isolation != authority elimination`.

RF0-L31: `Resource budget != scheduling policy`.

RF0-L32: `Nondeterminism != incorrectness`.

RF0-L33: `A model commit point != necessarily one physical instant`.

RF0-L34: `External effect semantics cannot be truthfully inferred from arbitrary process behavior by caller assertion`.

RF0-L35: `Ordivon Runtime Foundations != permission to generalize Runtime Core before repeated falsification`.

---

# 21. Minimum RF0 research grammar

RF0 does not freeze a universal schema. It retains the following coordinates because
deleting any one produces recurring category errors.

## 21.1 World / boundary

What portion of Reality is relevant to this claim?

```text
Workspace bytes
host OS
process tree
remote API
account state
physical device
human/social world
```

## 21.2 State projection

Which properties of that world are being modeled?

## 21.3 Participant / principal

Who proposes, controls, observes or is accountable?

## 21.4 Specification / proposal

What possible behavior/change is represented before realization?

## 21.5 Authority / capability

What permits or physically enables realization?

## 21.6 Preconditions / environment / dependencies

Which world facts must hold for the intended realization contract?

## 21.7 Identity

What makes this the same request/operation/attempt/effect rather than another one?

## 21.8 Commitment boundary

When does a possibility become a durable or contractually stable operational fact?
Not every runtime requires one; effectful recoverable runtimes often do.

## 21.9 Realization substrate

What physical mechanism carries behavior?

```text
process
thread
VM
container
systemd unit
Windows Job
remote provider
robot actuator
```

## 21.10 Behavior / trajectory

What states/events actually unfold?

## 21.11 Effect projection

Which consequences matter outside the realization substrate?

## 21.12 Observation / evidence

What facts are available to justify claims about realization/effect?

## 21.13 Control

What interventions remain possible?

```text
start
cancel
kill
pause
resume
compensate
```

## 21.14 Termination / finality criterion

What does `done` mean for this layer?

## 21.15 Uncertainty / epistemic status

What remains unknown or ambiguous?

## 21.16 Recovery / convergence relation

How can durable representation, physical reality and external effect evidence be
brought back into a justified relation after interruption?

---

# 22. Provisional dynamic grammar

The smallest current skeleton is:

```text
World_t + Environment_t + Authority_t
                 │
                 ▼
       Proposal / Specification
                 │
          validation/admission
                 │
                 ▼
        [optional Commitment]
                 │
                 ▼
        Realization / Dispatch
                 │
                 ▼
     Behavior / Physical Trajectory
        │                    │
        │                    └──→ External Effects
        ▼
  Observation / Evidence
        │
        ▼
  Operational Claim / State
        │
   interruption/drift?
        │
        ▼
 Reconciliation / Recovery
        │
        ▼
World_(t+n) + justified operational knowledge
```

Important: arrows do **not** imply one linear total order. Real systems can have
concurrency, partial order, overlapping effects, asynchronous evidence and observations
that arrive after later physical events.

---

# 23. Existing Ordivon Runtime reclassified by RF0

The current system becomes easier to understand when mapped onto the grammar instead
of treated as the grammar itself.

| Current object/mechanism | RF0 role |
|---|---|
| Workspace | bounded mutable source-world realization/projection |
| `sourceStateDigest` | precondition/world commitment evidence |
| `clientRequestId` | proposal/request identity coordinate |
| Job | durable operation commitment record |
| Attempt | one physical realization/dispatch generation |
| Execution Plan | committed realization specification |
| Runner bundle | realization materialization |
| systemd unit / cgroup | Linux physical ownership substrate |
| Windows Job Object | Windows physical ownership substrate |
| execution profile / Windows authority | authority/environment realization coordinates |
| immutable inputs | explicit dependency/world binding |
| hostDependencies | partial declared environment commitment |
| Result | execution observation/evidence |
| Artifact | retained digest-bound evidence bytes |
| terminal evidence | identity-bound realization evidence projection |
| `task.get` | passive operational projection |
| `task.observe` | observation + targeted reconciliation/control-plane convergence |
| `task.cancel` | persisted control intervention + physical stop attempt |
| `workspace.patch` receipt | structured filesystem-effect commitment/evidence |
| release receipt | structured deployment-effect evidence |
| `lost` / `orphaned` | preserved epistemic/ownership uncertainty |
| Host semantic completion | explicitly outside Runtime's operational truth plane |

This mapping suggests that much of the current design is not accidental. But RF0 does
**not** prove every current object is minimal or ideally named.

---

# 24. What current design now appears strongly justified

RF0 increases confidence in several existing decisions.

## 24.1 Job and Attempt should remain distinct

They separate durable operation identity from one physical realization generation.
This matches the broader distinction between specification/commitment and trajectory.

## 24.2 Unknown states should remain explicit

Distributed observation and external effects make binary success/failure projection
inherently unsafe in some cases.

## 24.3 Projection-only reads should remain distinguishable from reconciliation

Observation can be passive or state-advancing; hiding that distinction creates causal
confusion.

## 24.4 Arbitrary execution should remain effect-opaque

No process supervisor can infer a complete external-world effect contract from command
text or exit status.

## 24.5 Runtime should not own semantic completion

Semantic success belongs to the goal/domain evaluation plane, not the physical
realization/evidence plane.

## 24.6 Strong truth should remain delegated to substrate owners

Git, SQLite, systemd/cgroups, provider receipts and domain APIs each know different
facts. Runtime should compose these without pretending one database becomes Reality.

---

# 25. What RF0 reopens

RF0 also identifies assumptions that must no longer be accepted merely because the
current implementation uses them.

## 25.1 Is `Operation` really the right root identity?

Maybe later foundations will separate `Action`, `Operation`, `Commitment` and `Effect`
more sharply.

## 25.2 Is one Job → one Attempt still the right ordinary shape?

The ontology permits multiple realization generations, but effect semantics constrain
when they are safe. This needs later study, not immediate implementation.

## 25.3 Is bounded finite Job execution fundamental?

No. Reactive/continuous runtimes challenge that assumption. Ordivon may intentionally
remain an operation runtime, but the limitation should be understood as scope, not
ontology.

## 25.4 Is Workspace a Runtime foundation or an engineering specialization?

Workspace is extremely useful for software engineering work, but non-code operations
may have no Git source world. It is likely a specialization of a broader bound input /
working-world concept.

## 25.5 Is `Effect Kernel` the root or one strengthened branch?

RF0 suggests `Effect Commit Kernel` is probably a specialized solution for durable
Agent action under ambiguity, downstream of more general realization foundations.

## 25.6 Is reconciliation an execution concern or a knowledge concern?

Current Runtime necessarily does both. Later rounds should determine whether
reconciliation is best understood as epistemic convergence, control convergence, or a
family containing both.

---

# 26. Boundary with adjacent Ordivon systems

## Harness

Harness owns cognition/model-tool behavior and Agent Run semantics. RF0 says more
precisely:

```text
Harness selects/proposes behaviors.
Runtime realizes selected operational behaviors.
```

The boundary is not `AI vs shell`; it is primarily **selection/cognition versus
physical operational realization/evidence**.

## Host

Host owns durable Task continuity, commitments and semantic verification records. RF0
exposes an overloaded word: both Host and Runtime may have `commitment`, but at
different levels.

```text
Host commitment       semantic/task/world-facing commitment
Runtime commitment    physical operation/admission commitment
```

Later foundations must prevent these from collapsing.

## World / domain systems

World/domain owners know external reality semantics. Runtime can carry actions and
retain receipts, but it cannot universally decide what a trade, deployment, message or
human-world outcome means.

## Security

Security owns broader authority and trust questions. Runtime consumes concrete
execution authority/isolation contracts and must truthfully state their scope.

---

# 27. RF0 result — the key conceptual shift

Before RF0, a plausible compressed picture was:

```text
Runtime = Agent Effect Commit Kernel
```

After RF0, that is too early in the derivation.

The stronger current hierarchy is:

```text
Reality / World
  ↓
State projections + events + behavior
  ↓
Selected / specified possible behavior
  ↓
Authority + environment + preconditions
  ↓
Realization boundary
  ├─ lifecycle / execution substrate
  ├─ control / ownership
  └─ observation / evidence
  ↓
Effects in external projections
  ↓
Uncertainty / finality / convergence
  ↓
Durable effectful Agent realization
  ↓
Agent Effect Commit Kernel     [Ordivon's specialized strengthened branch]
```

This explains why the existing Effect Kernel is valuable without making it the
ontology of every Runtime.

---

# 28. RF0 closeout

RF0 closes with eleven durable research results:

1. `Process` is a realization view/substrate, not a safe foundation root.
2. `State transition` is a powerful modeling grammar but too broad to define Runtime.
3. The broad Runtime family is best approached through **specification/selection →
   realization → lifecycle/observation** rather than process taxonomy.
4. Ordivon Runtime is a strengthened branch for **durable Agent-selected operational
   realization under interruption and effect ambiguity**.
5. Intentional/specification truth, ontic realization truth and epistemic/evidence truth
   must remain separate.
6. Time/order is multi-dimensional; causal order need not be total.
7. Termination/finality is layer-relative; local process exit is only one criterion.
8. Nondeterminism is native to execution and does not imply incorrectness.
9. External effect is projection-relative and cannot be inferred generically from
   arbitrary process execution.
10. Failure and recovery are multidimensional; `Recovery != Retry` is foundational.
11. Existing `Job / Attempt / Evidence / Reconciliation` structure is substantially
    supported by broader foundations, but `Workspace`, finite Jobs and the Effect Kernel
    remain specializations to be re-tested.

The next unavoidable question is no longer “what features should Runtime have?” It is:

> **What exactly are State, Change, Event, Transition, Behavior and Dynamics, and what
> does it mean for an action/runtime to cause or realize one transition rather than
> merely observe a changing world?**

That is RF1.

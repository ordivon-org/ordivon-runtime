---
schema_version: 1
id: runtime.foundations.rf3
title: RF3 — Execution, Realization, Effect and Consequence
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
summary: RF3 separates dispatch, execution, realization, effect, consequence, receipt and outcome. It defines execution as contract-governed realization behavior, realization as a specification-to-concrete-behavior relation, effect as contract-relevant external-world change relative to an owned projection, and consequence as the wider downstream causal envelope. It reclassifies current Runtime Job/Attempt/Runner/receipt boundaries without treating process exit as effect truth.
evidence_status: verified
readiness: READY
applies_to:
  - RUNTIME-FOUNDATIONS-001
  - RF3
related:
  - runtime.foundations.start
  - runtime.foundations.rf2
  - runtime.foundations.rf3.sources
  - runtime.foundations.rf3.continuation
---
# RF3 — Execution, Realization, Effect and Consequence

## 0. Status and research question

RF2 ended at an identity-bound **Committed Operation** and deliberately stopped before
physical realization.

RF3 asks:

> **What does it mean for a committed Operation to become concrete behavior in Reality?
> Where do dispatch, execution and realization begin and end? What counts as an external
> Effect rather than a wider Consequence? Which participant can prove each claim, and why
> can process success not substitute for domain-effect truth?**

RF3 is explanatory research only. It does not authorize changing the `Attempt` state
machine, renaming `DISPATCH_ISSUED`, adding a generic Effect framework, moving domain
receipts into the generic Runner, or changing production retry behavior.

---

# 1. Inherited RF0–RF2 constraints

Carry forward:

```text
State != Reality
Observation != Reality
Event != Transition
Behavior != finite lifecycle
Dynamics != Behavior
WorldChange != Action
Goal != Intent
Proposal != Commitment
Command != Operation
OperationRequest != CommittedOperation
Operation != Transition
Operation != Effect
Operation != Attempt
```

RF3 must not define `Execution` by smuggling these collapsed concepts back together.

---

# 2. The naive Runtime chain fails

A common implicit model is:

```text
command issued
→ process starts
→ process runs
→ process exits 0
→ operation succeeded
→ effect happened
→ task succeeded
```

Every arrow can fail.

Examples:

```text
process image starts but initialization fails
process exits zero after pure computation and no external mutation
process crashes after a remote transaction already committed
HTTP server accepts a request but performs it asynchronously later
one financial order produces many partial fills
one robot command produces a continuous trajectory of actuator effects
one operation produces intended and unintended consequences
workspace.patch commits a structured effect without a Job/Runner Attempt
```

RF3 therefore starts with:

```text
Dispatch != Start
Start != Execution completion
Execution completion != Effect occurrence
Effect occurrence != Effect finality
Effect finality != Semantic outcome
```

---

# 3. Competing models of Execution

RF3 keeps several meanings visible.

## E0 — Execution as process existence

```text
Execution ⇔ process exists/runs
```

### Strength

Simple and mechanically observable for ordinary OS workloads.

### Failure

Pure in-process operations, direct filesystem transactions and remote provider actions do
not line up with one target process lifetime. A process can exist before the intended
program meaningfully begins, and external effects can outlive the process.

**Disposition:** useful substrate projection, rejected as foundation definition.

## E1 — Execution as successful `exec`/program entry

POSIX `exec` replaces the current process image with a new image. This gives a precise OS
boundary for one class of executable realization.

### Failure

Runtime setup can occur before `exec`; managed runtimes/interpreters can execute many
logical programs inside one process image; direct structured effects can be committed
without creating a separate target process.

**Disposition:** strong OS primitive, not universal Runtime execution ontology.

## E2 — Execution as transition sequence under operational semantics

Programming-language operational semantics models execution as a sequence/tree of
configuration transitions permitted by semantic rules.

### Strength

General for program behavior and independent of one OS process representation.

### Failure

A Runtime Operation can include environment materialization, provider interaction and
external physical behavior whose semantics are not captured by one programming-language
transition system.

**Disposition:** foundational ancestry for program execution, not sufficient for open
Runtime realization.

## E3 — Execution as lifecycle between start and terminal result

```text
start evidence → running behavior → terminal evidence
```

### Strength

Matches practical Runtime lifecycle/accounting.

### Failure

The beginning depends on which start boundary is chosen; external effects may precede or
follow terminalization; long-running/reactive work may have no normal terminal point.

**Disposition:** valid operational projection when a contract names the boundaries.

## E4 — Execution as contract-governed realization behavior

RF3's strongest portable candidate:

> **Execution is the behavior performed by an execution mechanism while interpreting or
> enacting an executable specification under a declared execution contract, from a named
> execution boundary until a named execution termination criterion.**

Compactly:

```text
Exec_C(spec, environment, authority)
→ τ_exec + execution evidence
```

where `C` names the execution semantics/boundaries.

**Disposition:** retained as RF3's primary account.

---

# 4. Execution is behavior plus contract, not merely activity

Not every CPU instruction caused by an operation belongs to the same useful execution
claim.

A declared execution contract answers:

```text
what specification is being enacted?
which executor/provider owns the execution?
what environment/authority is assumed?
what counts as execution start?
what behavior is inside the execution envelope?
what counts as terminal execution?
what evidence is authoritative?
```

Thus:

```text
Execution = contract-governed behavior projection
```

rather than:

```text
Execution = any physical activity causally downstream of a request
```

---

# 5. There can be several nested execution envelopes

One Ordivon Attempt can contain multiple legitimate execution scopes:

```text
Runtime dispatch execution
  └─ systemd transient unit / Windows launcher execution
      └─ Runner execution
          └─ target executable execution
              └─ interpreter/runtime execution
                  └─ remote request execution at provider
```

Each has different start/termination/evidence semantics.

Therefore:

```text
"Execution started"
```

is incomplete when more than one envelope matters.

RF3 recommends naming the scope:

```text
Attempt dispatch issued
Runner started
Target process started
Provider accepted request
Domain operation processing started
```

---

# 6. OCI proves that environment realization can precede target execution

OCI explicitly separates container `create` from `start`:

```text
create
→ runtime environment/resources created
→ user-specified program not yet run

start
→ start hooks
→ user-specified program run
```

The `created` state can therefore mean that substantial physical realization has already
occurred while the target program has not executed.

RF3 conclusion:

```text
EnvironmentRealization != TargetExecution
```

and:

```text
SetupStarted != UserProgramStarted
```

This directly generalizes to Ordivon input materialization, bundle construction and
provider preparation.

---

# 7. POSIX `exec` proves process-image realization is one boundary, not the whole action

A successful POSIX `exec` replaces the current process image and does not return to the
old image.

That gives a strong answer to:

```text
Did this process cross the exec image-replacement boundary?
```

It does not answer:

```text
Did application initialization succeed?
Did the intended domain operation happen?
Did external state commit?
Was the semantic goal achieved?
```

Hence:

```text
SuccessfulExec != SuccessfulOperationEffect
```

---

# 8. What is Dispatch?

RF3 defines **Dispatch** conservatively as:

> **A control-plane act that submits/releases one committed realization generation toward
> an executor/provider under a declared dispatch contract.**

Dispatch has at least three potentially distinct boundaries:

```text
Dispatch commitment/intention
Executor submission
Executor acceptance/start evidence
```

A system can make one boundary durable before another is physically observable.

Therefore:

```text
DispatchCommitted != ExecutorAccepted != ExecutorStarted
```

---

# 9. Current Ordivon `DISPATCH_ISSUED` is an at-most-once dispatch boundary, not start proof

Current source performs:

```text
Attempt Accepted + bundle committed
→ Registry transaction changes Attempt to Starting
→ records condition/event DISPATCH_ISSUED
   reason = AT_MOST_ONCE_BOUNDARY_COMMITTED
→ commits Registry transaction
→ only then invokes systemd_run / windows_systemd_run
```

Later, Runner/process identity is separately reconciled and `bind_running` moves the
Attempt into `Running`.

RF3 interpretation:

> `DISPATCH_ISSUED` is best understood as the durable **dispatch commitment/issue
> boundary whose purpose is preventing speculative duplicate physical dispatch**, not as
> proof that the Runner or target has started.

This is an explanatory interpretation. RF3 does not authorize renaming the event.

---

# 10. Dispatch uncertainty is not execution uncertainty of the same kind

After dispatch commitment but before start evidence, Runtime can know:

```text
we must not blindly dispatch this Attempt again
```

while not knowing:

```text
whether the executor actually started the Runner
```

Thus epistemic states can differ by boundary:

```text
Dispatch commitment: known
Executor acceptance: unknown
Runner start: unknown
Target effect: unknown
```

This explains why `Starting`, `lost`, `orphaned` and reconciliation are not merely
process-state bookkeeping.

---

# 11. What is Realization?

`Realization` is broader than execution.

RF3 provisional definition:

> **Realization is the relation by which an abstract/specification-level commitment,
> structure or operation is instantiated/enacted as concrete behavior or world
> structure under an implementation contract.**

Notation:

```text
O_commit  --Realize_R-->  τ_concrete
```

where `R` specifies what counts as an acceptable realization.

Realization is therefore a **relation between levels**, not just an event.

---

# 12. Realization has at least four useful senses

## 12.1 Representation realization

Turning an abstract specification into a concrete executable plan/materialization.

```text
proposal → concrete Execution Plan
```

## 12.2 Environment realization

Constructing the environment/resources required for execution.

```text
container namespaces
input staging
Windows native presentation
cgroup/unit
```

## 12.3 Behavioral realization

The executor actually produces behavior conforming to the execution specification.

```text
specification → concrete trajectory
```

## 12.4 Effect realization

Concrete behavior reaches an external effect owner/world and brings about a
contract-relevant effect.

These levels can fail independently.

---

# 13. Execution and Realization overlap but are not identical

Execution is one prominent mechanism of behavioral realization.

But RF3 rejects:

```text
Realization = Job/Runner process execution
```

because current Runtime itself has a counterexample:

```text
workspace.patch
```

A durable structured filesystem Effect is:

```text
prepared operation receipt
→ direct validated filesystem mutation
→ committed/unknown receipt
```

with no Job/Attempt/Runner lifecycle.

Some server code obviously executes in the physical implementation, but the **Runtime
operation is not modeled as a Job execution**.

Therefore:

```text
StructuredEffectRealization != necessarily RuntimeJobExecution
```

---

# 14. Realization lineage is more important than one process identity

For open-system action, Runtime often cannot prove every internal causal step. What it
can preserve is an identity-bound lineage:

```text
Committed Operation
→ Attempt
→ dispatch boundary
→ Runner/provider identity
→ target process/request identity
→ result/effect receipt when available
```

RF3 calls this a **realization lineage**.

A realization-lineage claim says:

> these observed pieces belong to the concrete enactment of this exact committed
> operation under the declared contracts.

It does **not** by itself prove every downstream consequence was caused exclusively by
that lineage.

---

# 15. Realization != causal exclusivity

If a Runtime-launched target updates a shared service while other actors also act, the
final state can reflect multiple causal contributors.

Thus:

```text
Runtime participated in realization of operation O
```

must not silently expand into:

```text
Runtime exclusively caused all resulting world state
```

RF3 retains:

```text
RealizationLineage != ExclusiveCausalOwnership
```

Full causality remains broader than this round.

---

# 16. What is an Effect?

RF0 said Effect is projection-relative. RF3 sharpens that.

> **An external Effect is a contract-relevant change in a declared external-world
> projection that is attributed to an operation's realization under the effect owner's
> semantics.**

Notation:

```text
W_ext --effect(O, contract)--> W'_ext
```

where `W_ext` is not the whole universe but the effect contract's declared projection.

Examples:

```text
filesystem path now names new bytes
provider records deployment generation G
order gains one fill
message appears in recipient mailbox
resource is created/deleted
```

---

# 17. Effect is narrower than Consequence

One Effect can have many consequences:

```text
Deploy version V                     target Effect
  ↓
service restarts                     direct consequence
  ↓
cache warms                          indirect consequence
  ↓
latency changes                      further consequence
  ↓
user changes behavior                wider consequence
```

RF3 defines **Consequence** as:

> **A downstream change/event that stands in the relevant causal envelope of an action,
> realization or effect, whether or not the operation contract treats it as part of its
> owned effect semantics.**

Consequences are potentially open-ended.

Therefore:

```text
Effect ⊂ possible consequences
```

conceptually, not necessarily as a literal set in every model.

---

# 18. Intended/requested Effect is not Actual Effect

RF2 already separated requested semantics from reality.

RF3 now distinguishes:

```text
RequestedEffect    what the operation contract asks to change
ActualEffect       contract-relevant external change that actually occurs
IncidentalEffect   externally visible change caused by implementation but outside the
                   requested contract
Consequence        broader downstream causal change
```

Example:

```text
HTTP request intends to update resource X
server also writes an access log
```

The log mutation is real, but need not be part of the requested Effect semantics.

Thus:

```text
RequestedEffect != ActualEffect != CompleteConsequenceSet
```

---

# 19. Effect occurrence is not the same as target-condition satisfaction

An idempotent operation gives a subtle falsifier.

Suppose desired external condition is already true:

```text
resource state = V
```

A repeated idempotent request may succeed without producing a **new mutation**.

Therefore distinguish:

```text
EffectOccurrence        new contract-relevant external mutation/change
EffectConditionSatisfied target effect condition already/now holds
OperationSemanticsSatisfied provider says this request has the promised semantic result
```

These can differ.

This prevents RF3 from defining every successful mutating operation as requiring one
fresh state delta.

---

# 20. Read-only operation success does not require external mutation

A pure read can execute successfully and produce a Result/response while guaranteeing:

```text
no contract-relevant external mutation
```

Therefore:

```text
ExecutionSuccess != ExternalMutation
```

This is the correct place for the Effect Kernel's `READ_ONLY` category: it is an effect
contract about absent external mutation, not a claim that no computation or observation
occurred.

---

# 21. HTTP 202 proves Acceptance != Effect completion

HTTP provides a canonical open-system example:

```text
request
→ server acceptance
→ asynchronous processing
→ possible later action/effect
```

`202 Accepted` explicitly means the request has been accepted for processing while
processing is not complete and might ultimately never be acted upon.

Thus:

```text
ProviderAcceptance != EffectOccurrence
ProviderAcceptance != EffectCompletion
```

An accepted request is an operational fact, not final effect proof.

---

# 22. HTTP 201/204 prove effect semantics are protocol/domain-owned

HTTP can also express stronger application/resource-level states:

```text
201 Created
```

signals that the request was fulfilled and created resource(s), while:

```text
204 No Content
```

can indicate a request was successfully fulfilled/applied without response content.

RF3 conclusion:

> Whether a request counts as fulfilled/applied is defined by the **effect owner/protocol
> semantics**, not inferred by the caller from TCP write success or local process exit.

---

# 23. Transport transmission is not provider acceptance

A client can write bytes into a transport and then lose the connection.

Possible realities include:

```text
request never reached server
server received partial request
server accepted full request but response was lost
server committed effect and response was lost
```

The local send call cannot decide among these after the fact.

AWS's idempotent-API guidance describes exactly this ambiguity: after a network timeout,
a client may not know whether a resource was created, and blind retry can create a
duplicate unless the provider exposes stable request identity/idempotency semantics.

Therefore:

```text
TransportSuccess != ProviderAcceptance
TransportFailure != ProviderRejection
ResponseLoss != EffectAbsence
```

---

# 24. Effect identity belongs with the effect owner

If a remote service accepts a stable idempotency token and binds it to one semantic
request, that provider can deduplicate retries because it owns:

```text
request identity
resource state
commit semantics
response/receipt semantics
```

A local Runtime that merely launches the client process does not possess those facts.

RF3 therefore strengthens a current design law:

> **An external effect contract must be owned by a participant that can enforce or
> authoritatively observe the target effect semantics.**

Caller labels on arbitrary execution cannot create that ownership.

---

# 25. End-to-end reasoning supports keeping domain-effect truth at capable endpoints

The end-to-end argument is not a theorem that all functionality belongs at applications,
but it gives a durable design heuristic: functions requiring application-specific
knowledge often cannot be completely implemented by lower layers that lack that
knowledge.

RF3 applies this narrowly:

```text
Runtime can own process/realization evidence.
Domain provider can own domain-effect semantics/receipts.
Host/domain can own semantic outcome.
```

Runtime should not duplicate a weaker approximation of knowledge already owned by the
effect endpoint.

---

# 26. What is a Receipt?

RF3 defines a **Receipt** as:

> **Identity-bound evidence emitted or maintained by an owner/authority that attests to
> some operation/effect state under a declared contract.**

Examples:

```text
Runtime terminal evidence
Workspace Patch committed receipt
deployment receipt
provider order ID/fill record
HTTP representation/status monitor
```

A receipt is epistemic evidence about an occurrence/state.

Thus:

```text
Receipt != Effect
Receipt != Consequence
Receipt != SemanticOutcome
```

---

# 27. Receipt authority is scoped

A receipt can prove only what its issuer is authoritative for.

Example:

```text
systemd/Runner evidence
```

can strongly support:

```text
this process tree/runner executed and terminated under this identity
```

but not:

```text
remote broker filled the order
```

Similarly a broker fill receipt can prove a domain fill without proving the local
process had a clean shutdown.

Therefore:

```text
ReceiptStrength = issuer authority + contract + identity binding + evidence integrity
```

not merely file presence.

---

# 28. Evidence can arrive after the Effect

A remote effect can commit at `t1` while Runtime remains uncertain.

At `t2`, a reconciliation query obtains authoritative receipt/state and Runtime changes:

```text
unknown → committed
```

No new external Effect need occur at `t2`.

This preserves RF1's:

```text
EpistemicChange != TargetWorldChange
```

and gives it operational meaning in RF3.

---

# 29. Effect can occur before local execution terminalizes

Example:

```text
process sends order
broker accepts/fills
process later crashes before writing result.json
```

Then:

```text
ExternalEffect occurred
LocalExecution failed/lost
```

Therefore:

```text
ExecutionFailure != EffectAbsence
```

and:

```text
ProcessCrash != EffectRollback
```

unless an effect contract explicitly supplies rollback/transaction semantics.

---

# 30. Local execution can succeed with no target Effect

Examples:

```text
pure SHA-256 computation
validate configuration
dry-run command
read-only query
program exits 0 after deciding no mutation is needed
```

Hence:

```text
ExecutionSuccess != EffectOccurrence
```

This is not an edge case; it is structurally normal.

---

# 31. Effect can occur after local request-handling execution appears complete

HTTP `202` demonstrates a provider may accept work for later processing. A local client
can receive the acceptance response and terminate before the domain Effect occurs.

Thus:

```text
ClientExecutionTerminal < EffectOccurrence
```

is valid.

For asynchronous systems, `operation completed` must name which operation layer is meant.

---

# 32. One execution can realize many Effects

A process can:

```text
write 100 files
send 10 messages
submit several orders
update multiple resources
```

Therefore:

```text
1 Execution → N Effects
```

is ordinary.

The converse is also possible at a higher abstraction: one logical distributed Effect
can be realized by many processes/executions participating in a transaction/protocol.

---

# 33. One Operation can have many Effect events while remaining one domain operation

Financial orders are a canonical example:

```text
Order Operation
→ accepted
→ fill #1
→ fill #2
→ fill #3
→ cancel remainder
```

The order's domain identity is one while its effect trajectory contains multiple fills
and position/cash consequences.

Thus:

```text
OperationIdentity != EffectEventIdentity
```

RF4 will study identity formally.

---

# 34. Transaction commit is an effect/finality protocol, not process exit

Distributed transaction commit explicitly decides whether a transaction commits or
aborts through a participant protocol. Gray/Lamport show transaction commit can be
formulated as consensus problems over participant commit/abort choices.

This gives RF3 a clean anti-collapse:

```text
TransactionCommit != CoordinatorProcessExit
```

A process can disappear while the protocol's durable state determines or later recovers
the commit decision.

---

# 35. Linearizability shows an abstract Effect point need not be one physical instruction

For a linearizable concurrent object, an operation is reasoned about as if it takes
effect atomically at some point between invocation and response, even though the
implementation can execute many lower-level concurrent steps.

RF3 uses this as an abstraction lesson:

```text
abstract effect/linearization point
!=
claim of one physically indivisible implementation event
```

This is consistent with RF1's abstraction-relative atomicity.

---

# 36. Atomic Effect != Durable Effect

POSIX gives an especially useful filesystem falsifier.

Directory operations such as rename are required to be atomic/serializable with respect
to specified filesystem effects, but POSIX rationale separately notes that directory
operations are not necessarily durable across crash unless synchronization requirements
are met.

Therefore:

```text
AtomicVisibility != Durability
```

and:

```text
EffectOccurredInLiveSystem != EffectGuaranteedAfterCrash
```

Finality/durability must be stated separately.

---

# 37. `rename` demonstrates projection-scoped Effect atomicity

For replacement rename, other threads must continue to see the target name referring to
either the old or new entry rather than an absent in-between state under the specified
contract.

That is a strong filesystem namespace Effect property.

It does not imply atomicity of:

```text
all underlying device writes
all application consequences
all replicated copies
all cache state
```

Thus:

```text
Atomicity must name its projection and failure model.
```

---

# 38. What is Consequence?

`Consequence` deliberately remains broader and weaker than `Effect`.

RF3 definition:

> **A consequence is a later state/event/behavior difference that is relevantly caused or
> enabled by an action, realization or effect, without requiring that the originating
> operation contract owns or guarantees it.**

Consequence can be:

```text
direct / indirect
intended / unintended
foreseen / unforeseen
local / remote
immediate / delayed
reversible / irreversible
```

RF3 does not claim Runtime can enumerate the full causal cone.

---

# 39. Consequences are open-world and potentially unbounded

A deployment changes software bytes. That can later change service responses, user
behavior, market behavior and other systems.

No useful Runtime operation identity can bind every future consequence.

Therefore:

```text
OperationContract != CompleteFutureCausalCone
```

This is why Runtime needs explicit effect projections rather than a universal
`sideEffects` list.

---

# 40. Outcome is a separate semantic/normative layer

RF3 uses **Outcome** for evaluation relative to a goal, task, policy or domain criterion.

Examples:

```text
execution succeeded but benchmark regressed
patch applied but bug remains
order filled but strategy lost money
deployment committed but service SLO worsened
```

Thus:

```text
ExecutionResult != EffectReceipt != SemanticOutcome
```

This strongly preserves current Runtime's
`semanticCompletionEvaluated=false` boundary.

---

# 41. Execution success needs a declared execution contract

`Succeeded` cannot mean everything succeeded.

For current ordinary Runtime execution it means approximately:

```text
this Attempt's physical execution contract reached its successful terminal condition
under Runtime's evidence rules
```

It does not mean:

```text
all external effects happened
no unintended side effects happened
all consequences are acceptable
the Host Task is complete
```

RF3 therefore recommends reading `executionDisposition=succeeded` literally as an
execution-layer claim.

---

# 42. Realization success is still not effect success

Runtime may prove:

```text
correct committed Runner image started
correct target executable path identity was witnessed
process tree was bounded/terminated
result evidence is identity-bound
```

while having no authoritative knowledge of a remote API's effect.

Therefore:

```text
RealizationIntegrity != DomainEffectSuccess
```

The former is still valuable: it rules out many ways the wrong machinery could have
realized the operation.

---

# 43. Current self-release is a strong Effect-owner example

Runtime self-release is structured because Runtime owns enough of the target domain:

```text
candidate identity
install/Registry/environment authority
deployer identity
deterministic release effect identity
canonical deployment receipt
release.get reconciliation
```

Its key RF3 feature is:

> the deployment receipt can be stronger evidence of external release occurrence than
> generic Job/process state, including across Runtime self-replacement.

Thus:

```text
DomainReceipt > GenericProcessExit
```

for the specific effect claim the receipt owns.

---

# 44. `workspace.patch` is the decisive no-Job Effect realization counterexample

Current Workspace Patch:

```text
stable request identity
→ durable prepared operation + exact before/after plan
→ direct filesystem mutation
→ committed receipt
```

If receipt publication is lost, Runtime can inspect actual files and recover `committed`.
If files are in a mixed state, it marks the operation `unknown` and refuses replay.

This proves:

```text
StructuredEffect contract
!=
Job/Runner mechanism
```

and:

```text
Effect reconciliation can inspect world state rather than repeat realization.
```

---

# 45. Workspace Patch also separates Effect from Receipt publication

The physical file Effect can already be present while the durable Patch record has been
artificially/lostly left `prepared`.

Reconciliation can later discover:

```text
all files match committed after-state
→ receipt can converge to committed
```

or:

```text
mixed files
→ unknown
```

Therefore:

```text
EffectOccurrence != ReceiptPublicationTime
```

and response/receipt loss does not justify replay.

---

# 46. Arbitrary `workspace.exec` remains Effect-opaque for a deeper reason

The target executable can internally:

```text
read
write
open sockets
submit external operations
fork children
change namespaces
invoke other programs
communicate with humans/services
```

Runtime can own the realization/process lineage but cannot infer the target domain's
effect projection, identity, commit point, receipt or compensation semantics.

Thus opaque execution is not merely a missing enum.

It is an **epistemic and authority boundary**.

---

# 47. A caller-declared Effect class cannot solve missing ownership

Suppose an Agent says:

```text
effectClass = IDEMPOTENT
```

for an arbitrary command.

Unless the effect owner actually accepts a stable deduplication identity and Runtime can
bind/reconcile it, the label cannot make retries safe.

Hence:

```text
EffectClaim + no enforceable owner contract = annotation, not guarantee
```

RF3 strongly supports the existing refusal to accept caller-defined effect classes for
opaque execution.

---

# 48. Effect finality is not one universal concept

Possible finality dimensions include:

```text
visible to concurrent observers
durably persisted across crash
committed by transaction protocol
accepted by remote provider
irreversible after a market fill
settled after external confirmation
semantically accepted by Host/domain
```

Therefore:

```text
EffectFinal != one boolean without a contract
```

RF3 keeps `finality criterion` explicit for RF8/RF9/RF10.

---

# 49. Cancellation is a control intervention, not retroactive Effect erasure

If cancellation occurs after a process has already produced an external Effect:

```text
stop remaining process tree
```

cannot generally undo:

```text
message already sent
order already filled
remote resource already created
```

Therefore:

```text
CancelExecution != RollbackEffect
```

and:

```text
ProcessTreeTerminated != ExternalWorldRestored
```

Compensation/rollback requires domain semantics and belongs later.

---

# 50. Timeout is not a universal execution/effect verdict

A timeout can mean:

```text
caller stopped waiting
Runtime deadline requested termination
provider request timed out
remote operation continues asynchronously
```

Thus:

```text
Timeout != EffectFailure
```

A timed-out local Attempt can coexist with a committed remote Effect.

This is why timeout belongs to the execution/control projection rather than domain truth
unless an effect contract says otherwise.

---

# 51. Cross-domain falsifier matrix

| Case | Collapse attacked | Surviving distinction |
|---|---|---|
| pure SHA-256 | execution success = external effect | valid execution can have no external mutation |
| OCI create before start | setup = target execution | environment realization can precede user program |
| successful POSIX exec then init failure | exec = operation success | process-image boundary is narrower than semantic execution |
| dispatch intent before systemd run | dispatch record = process started | at-most-once dispatch commitment precedes start evidence |
| Runner starts, target never reaches useful work | Runner start = target realization success | nested execution envelopes differ |
| process exits 0 after dry run | zero exit = domain effect | execution result may have no effect |
| process crashes after remote commit | execution failure = effect absence | effect can precede failed terminal execution |
| HTTP bytes sent, response lost | transport failure = no effect | provider acceptance/effect is ambiguous |
| HTTP 202 | acceptance = completion | accepted work can remain incomplete/unrealized |
| HTTP 201/204 | caller process defines effect | protocol/effect owner defines fulfillment/applied semantics |
| idempotent retry | every successful request creates new mutation | semantic satisfaction can occur without new effect occurrence |
| financial partial fills | one operation = one effect event | one operation can produce many domain effects |
| transaction commit | coordinator exit = commit | commit is protocol/durable agreement semantics |
| linearizable object | abstract effect point = physical single step | effect atomicity can be abstraction-level |
| POSIX rename | atomic visibility = durability | atomic/serializable effect and crash durability differ |
| robot waypoint | one command/execution = one discrete effect | continuous control trajectory produces extended physical effects |
| multi-recipient message | one process = one effect | one execution can generate multiple external effects |
| self-release | generic terminal result = release truth | deployment receipt is stronger effect-specific evidence |
| workspace.patch | structured effect requires Job/Runner | direct effect transaction can own identity/receipt/reconciliation |
| patch receipt loss | missing receipt = missing effect | reconciliation can prove committed world state later |
| patch mixed state | retry = recovery | mixed effect state becomes unknown, not replayed |
| cancellation after send/fill | stop = rollback | execution control cannot erase committed external effects |

---

# 52. RF3 anti-laws

RF3-L1: `CommittedOperation != Dispatch`.

RF3-L2: `DispatchCommitment != ExecutorSubmission`.

RF3-L3: `ExecutorSubmission != ExecutorAcceptance`.

RF3-L4: `ExecutorAcceptance != RunnerStart`.

RF3-L5: `RunnerStart != TargetStart`.

RF3-L6: `TargetStart != ExecutionCompletion`.

RF3-L7: `Dispatch != EffectOccurrence`.

RF3-L8: `Execution != ProcessExistence`.

RF3-L9: `Execution != POSIX exec alone`.

RF3-L10: `EnvironmentRealization != TargetExecution`.

RF3-L11: `SetupStarted != UserProgramStarted`.

RF3-L12: `Execution != Effect`.

RF3-L13: `ExecutionSuccess != EffectOccurrence`.

RF3-L14: `ExecutionFailure != EffectAbsence`.

RF3-L15: `ProcessExit != EffectFinality`.

RF3-L16: `ProcessCrash != EffectRollback`.

RF3-L17: `Realization != Job/Runner mechanism`.

RF3-L18: `RealizationLineage != ExclusiveCausalOwnership`.

RF3-L19: `RealizationIntegrity != DomainEffectSuccess`.

RF3-L20: `Effect != Consequence`.

RF3-L21: `RequestedEffect != ActualEffect`.

RF3-L22: `ActualEffect != CompleteConsequenceSet`.

RF3-L23: `EffectOccurrence != EffectConditionSatisfied`.

RF3-L24: `OperationSemanticsSatisfied != necessarily NewMutation`.

RF3-L25: `ProviderAcceptance != EffectCompletion`.

RF3-L26: `TransportSuccess != ProviderAcceptance`.

RF3-L27: `TransportFailure != ProviderRejection`.

RF3-L28: `ResponseLoss != EffectAbsence`.

RF3-L29: `Receipt != Effect`.

RF3-L30: `Receipt != Consequence`.

RF3-L31: `ReceiptPublicationTime != EffectOccurrenceTime`.

RF3-L32: `GenericProcessEvidence != DomainEffectReceipt`.

RF3-L33: `OperationIdentity != EffectEventIdentity`.

RF3-L34: `TransactionCommit != CoordinatorProcessExit`.

RF3-L35: `AbstractAtomicEffect != PhysicallyIndivisibleInstruction`.

RF3-L36: `AtomicVisibility != Durability`.

RF3-L37: `LiveEffectOccurrence != CrashDurability`.

RF3-L38: `CancelExecution != RollbackEffect`.

RF3-L39: `ProcessTreeTerminated != ExternalWorldRestored`.

RF3-L40: `Timeout != EffectFailure`.

RF3-L41: `ExecutionResult != EffectReceipt`.

RF3-L42: `EffectReceipt != SemanticOutcome`.

RF3-L43: `OperationContract != CompleteFutureCausalCone`.

RF3-L44: `CallerEffectLabel != EnforceableEffectContract`.

RF3-L45: `StructuredEffect != one shared physical dispatcher`.

---

# 53. Minimum RF3 grammar

RF3 retains the following conceptual coordinates.

## 53.1 Committed Operation `O`

Identity-bound operational commitment inherited from RF2.

## 53.2 Execution Contract `C_exec`

Defines executor, execution scope, start/terminal boundaries, environment/authority and
execution evidence semantics.

## 53.3 Dispatch Commitment `D_commit`

Durable boundary after which the same realization generation cannot be blindly issued
again.

## 53.4 Executor Submission / Acceptance `D_submit`, `D_accept`

Physical/protocol handoff stages toward the execution provider.

## 53.5 Execution Behavior `τ_exec`

Concrete behavior within a named execution envelope.

## 53.6 Realization Relation `R`

Relates abstract/committed operation/specification to concrete behavior/structure.

```text
O --R--> τ
```

## 53.7 Realization Lineage `L`

Identity-bound observed chain connecting committed operation to execution/provider/world
interactions.

## 53.8 Effect Contract `C_eff`

Defines external-world projection, requested semantics, effect identity/preconditions,
owner, evidence/receipt, finality and ambiguity/reconciliation behavior.

## 53.9 External Effect `E`

Contract-relevant world-projection change attributed to realization under `C_eff`.

## 53.10 Effect Condition / Semantic Satisfaction `S_eff`

Whether the promised external condition/semantic result holds, possibly without a new
mutation on replay.

## 53.11 Receipt `ρ`

Identity-bound effect/execution evidence emitted by an authority under a declared
contract.

## 53.12 Consequence `K`

Broader downstream causal changes/events outside or beyond the owned Effect projection.

## 53.13 Outcome `Y`

Goal/domain/task-relative evaluation of resulting reality.

---

# 54. RF3 reconstructed ladder

```text
Goal / Intent / Policy                         RF2 upper layers
        ↓
Proposal / Command
        ↓
Operation Request
        ↓ admission
Committed Operation O
        │
        │ execution contract C_exec
        ▼
Dispatch Commitment D_commit
        ↓
Executor Submission / Acceptance
        ↓
Execution Behavior τ_exec
        │
        ├──── realization lineage L ──────────────┐
        │                                         │
        ▼                                         ▼
local execution result/evidence            external Effect Owner
                                                  │
                                      Effect Contract C_eff
                                                  │
                                                  ▼
                                           External Effect E
                                                  │
                                      Receipt / state evidence ρ
                                                  │
                                                  ▼
                                           Consequences K...
                                                  │
                                                  ▼
                                         Semantic Outcome Y
```

Important:

```text
execution and effect timing can overlap/reorder
receipt may arrive after effect
one execution may create many effects
one distributed effect may involve many executions
consequence has no useful universal closed boundary
```

---

# 55. Current Ordivon Runtime mapping under RF3

| Current mechanism | RF3 interpretation |
|---|---|
| `Job` | committed Operation record/handle |
| `Attempt` | one Runtime-managed physical realization generation |
| bundle/input materialization | environment/specification realization before target execution |
| `DISPATCH_ISSUED` | durable at-most-once dispatch commitment/issue boundary |
| `systemd_run` / Windows launcher submission | executor/provider submission |
| `runner-start.json` / Windows start evidence | realization-lineage start evidence for Runtime provider/child |
| `bind_running` | Runtime operational projection that start identity is bound |
| process tree/cgroup/Windows Job | owned execution substrate/lineage |
| `result.json` | Runner execution result evidence |
| terminal evidence | identity-bound execution/ownership/evidence synthesis |
| `executionDisposition` | execution-layer terminal classification |
| `deliveryDisposition` | Runtime certainty about committed physical execution result |
| `processTreeDisposition` | process-tree ownership/termination evidence projection |
| Runtime self-release receipt | effect-owner-specific deployment receipt |
| `workspace.patch` prepared/committed/unknown | direct structured Effect commitment/reconciliation contract |
| Host Task outcome | semantic Outcome above Runtime execution truth |

---

# 56. What RF3 strongly supports in current design

## 56.1 Persist dispatch commitment before physical submission

The current order gives at-most-once Attempt dispatch a durable epistemic/control boundary
instead of interpreting transport/process uncertainty as permission to resend.

## 56.2 Keep Runner start evidence separate from dispatch intent

They answer different questions.

## 56.3 Keep execution disposition separate from delivery certainty

An execution terminal classification and Runtime's confidence that the terminal result is
mechanically converged are distinct.

## 56.4 Keep generic execution Effect-opaque

Runtime owns realization evidence; arbitrary external-domain semantics remain unknowable
without a structured effect contract.

## 56.5 Prefer domain-specific receipt over generic process evidence for effect claims

Self-release already proves this architecture.

## 56.6 Permit structured Effects with different realization mechanisms

Workspace Patch proves that a correct effect contract does not require the common
Job/Attempt/Runner machinery.

## 56.7 Preserve unknown rather than replay mixed effects

Patch reconciliation and opaque Attempt recovery both fit the same deeper law:

```text
uncertain effect state → observe/reconcile first
```

---

# 57. What RF3 reopens

## 57.1 The phrase `physical execution`

Current docs use it effectively, but RF3 shows there are several nested physical
execution scopes. Later docs may benefit from explicitly naming `Runtime Attempt
execution`, `Runner execution` or `target execution` when ambiguity matters.

## 57.2 The name `DISPATCH_ISSUED`

Its current implementation is intentionally a durable at-most-once boundary before the
provider call. RF3 explains it but does not decide whether future naming should emphasize
`dispatch_commitment` versus `issued`.

## 57.3 Where execution start is measured

Current latency tracks admission → dispatch → Runner bound. That is mechanically useful.
It must not be promoted into one universal definition of operation execution start.

## 57.4 Whether Effect should include no-op semantic success

RF3 chooses a narrow external-change meaning for `Effect` and separately adds
`EffectConditionSatisfied / OperationSemanticsSatisfied` so idempotent replays need not
invent a fresh mutation.

This distinction should be attacked again in RF14 cross-world work.

---

# 58. Conditions for a defensible Runtime external-Effect claim

RF3 does not require every operation to meet these conditions. It says an external Effect
claim should not be stronger than its contract.

A strong claim generally needs:

```text
1. stable Operation/effect identity
2. declared external-world projection
3. enforceable preconditions/authority
4. identity-bound realization lineage or effect-owner request identity
5. effect-owner semantics for what counts as occurrence/satisfaction
6. authoritative receipt/state query or equivalent evidence
7. explicit ambiguity/finality criteria
8. reconciliation behavior that does not invent/repeat effects
```

Missing pieces reduce the strength of the claim rather than being filled by inference.

---

# 59. Research constitution added by RF3

Later rounds should obey:

1. Never use `dispatch` as proof of start unless the contract defines them together.
2. Name execution scope when multiple nested envelopes matter.
3. Treat setup/materialization as realization work without automatically calling it target
   execution.
4. Treat Realization as a specification-to-concrete-behavior relation, not a synonym for
   process execution.
5. Define Effect only relative to a declared external projection and effect owner.
6. Separate requested effect, actual effect, effect-condition satisfaction, incidental
   effect and wider consequence.
7. Prefer effect-owner receipts/state over generic process evidence for domain claims.
8. Treat receipt as evidence, not occurrence itself.
9. State atomicity, durability and finality separately.
10. Do not infer Effect absence from timeout/process failure.
11. Do not infer Effect success from zero exit/process success.
12. Cancellation stops owned realization; it does not imply rollback.
13. Keep semantic Outcome above execution/effect truth unless a domain contract explicitly
    owns it.
14. Structured Effects may share a contract grammar without sharing a dispatcher/framework.

---

# 60. RF3 result — conceptual compression

RF3 reduces the old ambiguous chain:

```text
run command → effect happened
```

into:

```text
Committed Operation
  ↓
Execution Contract
  ↓
Dispatch Commitment
  ↓
Executor handoff/start evidence
  ↓
Execution Behavior
  ↓
Realization Lineage
  ├─ local execution evidence
  └─ external effect-owner interaction
         ↓
      Effect Contract
         ↓
      External Effect / Effect-condition satisfaction
         ↓
      Receipt / state evidence
         ↓
      wider Consequences
         ↓
      Semantic Outcome
```

The key compression is:

> **Execution is contract-governed realization behavior. Realization relates a committed
> specification/operation to concrete behavior. Effect is a contract-relevant external
> world change owned by a declared effect semantics. Consequence is the wider downstream
> causal envelope. Receipt is evidence, and Outcome is a higher semantic evaluation.**

---

# 61. RF3 closeout

RF3 closes with eighteen durable results:

1. **Dispatch, start, execution, realization, effect, consequence and outcome are distinct
   semantic layers.**
2. Execution is best modeled as contract-governed behavior under a named execution
   envelope rather than process existence alone.
3. Multiple nested execution envelopes can coexist; `execution started` must name scope
   when ambiguity matters.
4. OCI/POSIX show environment realization, target start and process-image execution are
   separate boundaries.
5. Current `DISPATCH_ISSUED` is a durable at-most-once dispatch commitment/issue boundary
   before provider submission, not start proof.
6. Realization is a specification/commitment-to-concrete-behavior relation broader than
   Job/Runner execution.
7. Realization lineage preserves identity-bound participation evidence without claiming
   exclusive causal ownership.
8. **Effect is a contract-relevant external-world change relative to a declared projection
   and effect owner.**
9. Requested Effect, Actual Effect, Incidental Effect and wider Consequence remain
   distinct.
10. Effect occurrence is distinct from effect-condition/operation-semantic satisfaction,
    allowing idempotent no-op success without inventing a new mutation.
11. Provider acceptance, execution completion and effect occurrence can be temporally and
    semantically separate.
12. Transport success/failure and response loss do not determine effect occurrence.
13. Receipt is identity-bound scoped evidence, not the Effect itself.
14. Effect can precede local execution failure or occur asynchronously after client
    execution terminates.
15. Atomic visibility, durability and finality are different effect properties.
16. Cancellation/timeout are execution-control facts and do not imply external rollback or
    effect failure.
17. Self-release and Workspace Patch jointly confirm that effect-specific receipts and
    reconciliation outrank generic process inference and that structured effects need not
    share one physical execution mechanism.
18. Current Ordivon Runtime is best understood as owning **committed-operation realization
    lineage and execution evidence**, with external effect truth only where a structured
    effect contract gives Runtime/domain authority to know it.

The next frontier is now sufficiently defined:

> **What makes two requests, Operations, Attempts, effects or histories the same? What is
> replay, what is retry, what persists across time, and which identities must remain
> stable when the world changes?**

That is RF4 — Identity, Sameness, Replay and History.

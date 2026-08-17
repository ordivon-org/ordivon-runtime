---
schema_version: 1
id: runtime.foundations.rf12
title: RF12 — Composition, Transactions and Cross-Operation Atomicity
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
summary: RF12 separates composition, sequencing, compound operations, transactions, atomicity, isolation, durability, distributed commit, orchestration, Saga/compensation and outbox-style handoff. It classifies current Runtime Registry mutations as owner-local transactions, admission+dispatch as durable intent followed by non-transactional physical realization, execPlan as ordered orchestration rather than transactionality, Workspace Patch as a structured local multi-file effect with reconciled all-before/all-after/unknown semantics, and Runtime Release rollback as domain recovery rather than rollback of the whole open-world operation. It establishes that atomicity is fact/effect-owner scoped and cannot be extended across SQLite, process launch and arbitrary external effects without explicit participant protocols.
evidence_status: verified
readiness: READY
applies_to:
  - RUNTIME-FOUNDATIONS-001
  - RF12
related:
  - runtime.foundations.start
  - runtime.foundations.rf11
  - runtime.foundations.rf12.sources
  - runtime.foundations.rf12.continuation
---
# RF12 — Composition, Transactions and Cross-Operation Atomicity

## 0. Status and research question

RF11 established that every truth/proof claim is scoped to fact owners and evidence
contracts. RF12 now asks:

> **When several state changes, process steps or external Effects are treated as one larger
> activity, what does “compose” mean, when can they truthfully be called one transaction,
> which owner provides atomic commit, where can partial commit appear, and when should the
> model instead be orchestration, durable handoff, Saga or compensation?**

RF12 is explanatory research only. It does not authorize distributed transactions,
a generic Saga engine, an outbox service, multi-Job transactions, cross-provider atomicity,
new EffectAdapter abstractions, automatic compensation or production API changes.

---

# 1. First deletion — sequence is not transaction

These two structures look similar syntactically:

```text
A
B
C
```

but may mean very different things:

```text
ordered execution
workflow
pipeline
compound command
local transaction
nested transaction
Saga
distributed transaction
```

Therefore:

```text
Sequence != Transaction
```

---

# 2. Composition

RF12 defines **Composition** broadly:

> **The construction of a larger activity/behavior from multiple sub-actions,
> sub-operations, state transitions or Effects with explicit relations among them.**

Relations may include:

```text
order
dependency
all-or-nothing commit
parallelism
retry
compensation
conditional branching
resource sharing
```

Composition itself says nothing about atomicity.

---

# 3. Compound Operation

A **Compound Operation** is one logical requested activity whose realization contains
multiple constituent actions/effects.

Example:

```text
Build → test → package
```

can be one logical Operation even though each step executes separately.

But:

```text
CompoundOperation != Transaction
```

unless the contract provides transaction semantics.

---

# 4. Transaction

RF12 uses **Transaction** only when a named owner/domain supplies a commit/abort contract for
a set of state changes/effects.

Minimum transaction questions:

```text
What state/effect set participates?
Who owns commit authority?
What is the commit point?
What is visible before/after commit?
What happens on abort?
What survives crash?
What failure model is assumed?
```

---

# 5. Atomicity

**Atomicity** means, relative to a declared transactional state/effect set:

```text
all committed
or
none committed
```

with no externally valid committed intermediate subset under the transaction contract.

PostgreSQL documentation describes transactions as groups of commands acting as one atomic
unit, and SQLite's atomic-commit documentation similarly defines one transaction's database
changes as all occurring or none occurring.

---

# 6. Atomicity is owner-scoped

If SQLite atomically commits:

```text
Job row
Attempt row
reservation row
idempotency row
```

that does not imply:

```text
systemd unit launched atomically with SQLite commit
external API Effect committed atomically with SQLite commit
```

Therefore:

```text
LocalAtomicity != CrossOwnerAtomicity
```

---

# 7. Atomicity is not “everything succeeded”

A transaction can atomically commit a state representing failure.

Example:

```text
Attempt state = failed
reservation = released
terminal evidence registered
Job resolution = failed
```

can be committed atomically.

Thus:

```text
AtomicCommit != SuccessfulBusinessOutcome
```

---

# 8. Atomicity is not durability

Atomicity answers all-or-none.

Durability answers whether committed state survives failures.

Therefore:

```text
Atomicity != Durability
```

though ACID systems often provide both.

---

# 9. Atomicity is not isolation

Isolation constrains how concurrent transactions observe/interleave with one another.

Atomicity constrains the transaction's commit set.

Therefore:

```text
Atomicity != Isolation
```

RF13 will later study isolation/containment separately.

---

# 10. Atomicity is not consistency correctness

A transaction can atomically commit an application-level invalid value if invariants fail to
reject it.

Thus:

```text
Atomicity != SemanticConsistency
```

Database “consistency” in ACID still depends on invariant-preserving transaction logic.

---

# 11. Commit

A **Commit** is the owner-defined transition after which the transaction's intended state
changes become authoritative/durable under its contract.

A commit point is meaningful only relative to the owner/domain.

---

# 12. Abort

An **Abort** terminates the transaction without committing its participating state changes.

Within a true local transaction, abort/rollback can discard provisional writes that were not
yet committed.

---

# 13. Rollback — strict meaning

RF10 already reserved **Rollback** for restoring transaction-owned provisional state to the
pre-transaction condition under a real rollback contract.

Therefore:

```text
Rollback != Compensation
```

and:

```text
Rollback != Later New Effect
```

---

# 14. Savepoint / subtransaction

A savepoint is an internal partial rollback boundary inside one transaction domain.

It does not create an externally committed subtransaction.

Therefore:

```text
Savepoint != IndependentCommit
```

---

# 15. Nested composition must distinguish commit visibility

If child action C commits independently before parent P, it is not an ordinary closed nested
transaction whose effects remain provisional under P.

This distinction matters for long-running workflows.

---

# 16. One owner makes atomicity tractable

When one storage engine owns:

```text
all participating records
commit protocol
recovery log
visibility rules
```

it can provide strong atomicity without involving open-world external actors.

This is why SQLite Registry transactions are strong.

---

# 17. Current admission is a true owner-local transaction

Current `Registry::submit_preallocated` / `submit` begins an immediate SQLite transaction and
commits together:

```text
Job
Attempt
idempotency identity
concurrency reservation
execution-provider side truth
Host dependency side truth where applicable
Runtime Release side truth where applicable
Job events
```

before returning a newly admitted Operation.

---

# 18. Admission transaction commit point

For Runtime-owned durable admission facts:

```text
SQLite COMMIT
```

is the commit point.

After successful commit:

```text
one durable Operation identity exists
one Attempt exists
one reservation exists
replay identity is bound
```

---

# 19. Admission atomicity is valuable because partial Registry identity would be dangerous

Without one transaction, crashes could produce impossible states such as:

```text
Job without Attempt
Attempt without reservation
reservation without Job
idempotency key without target Job
release binding without release Job
```

Current local transaction prevents these committed intermediate subsets.

---

# 20. Admission transaction does not include physical dispatch

Current code explicitly commits durable admission first and later marks/executes dispatch.

Runtime Release admission even intentionally returns an Accepted Job before any self-replacing
release is dispatched.

Therefore:

```text
AdmissionTransactionScope ends before PhysicalDispatch
```

---

# 21. SQLite commit then crash before systemd launch is not atomicity violation

Timeline:

```text
T1: SQLite admission COMMIT
T2: Runtime crashes
T3: no systemd launch yet
```

The Registry transaction is fully committed.

The physical realization simply has not happened yet.

This is a designed durable-handoff state.

---

# 22. Durable handoff

RF12 defines **Durable Handoff**:

> **One owner atomically commits durable intent/identity sufficient for another later phase
> to safely realize or reconcile the requested action, without claiming the later phase was
> part of the same transaction.**

Current admission → dispatch is exactly this pattern.

---

# 23. Durable handoff is not distributed transaction

The downstream launcher/systemd does not vote prepare/commit inside the SQLite transaction.

Therefore:

```text
DurableIntent + LaterDispatch != DistributedTransaction
```

---

# 24. `dispatch_issued` creates another local commitment frontier

Current Runtime records dispatch intent before crossing the physical provider boundary.

This produces:

```text
Registry fact: dispatch may now have crossed
```

but still does not atomically commit systemd/native process creation with SQLite.

---

# 25. The dispatch frontier intentionally admits ambiguity

After `dispatch_issued` commit:

```text
Runtime crash
```

can leave:

```text
launch happened
or
launch did not happen
```

unless substrate evidence later closes the question.

This is RF10's at-most-once ambiguity frontier.

---

# 26. Cross-owner atomicity would require the owners to participate

To atomically bind:

```text
Registry state
+
systemd launch
```

one would need a protocol in which both domains expose compatible prepare/commit/recovery
semantics or are brought under one stronger owner.

Generic SQLite + systemd do not provide that transaction.

---

# 27. Process execution creates a still wider boundary

Even if process launch could be atomically coordinated, arbitrary target code may call:

```text
filesystem
network
cloud APIs
exchange APIs
email APIs
other processes
```

Each creates additional owners.

Thus:

```text
AtomicLaunch != AtomicOpenWorldExecution
```

---

# 28. One process is not one transaction

A process can perform many independently committed Effects.

Therefore:

```text
OneProcessExecution != OneTransactionalEffect
```

---

# 29. `workspace.execPlan` is ordered orchestration

Current Runner executes steps sequentially, records per-step progress/results, stops at first
failure by default, and may continue after failure only with explicit `continueOnError`.

This provides:

```text
ordering
mechanical control flow
progress evidence
```

not transactionality.

---

# 30. Earlier execPlan step Effects remain after later failure

Example:

```text
step1: create cloud resource → success
step2: write external record → success
step3: test something → failure
```

Runtime marks overall execution failed, but step1/2 Effects do not roll back automatically.

Therefore:

```text
StopOnError != RollbackPreviousSteps
```

---

# 31. `continueOnError` makes orchestration nature explicit

Allowing later steps to run after one step fails is perfectly coherent for a workflow.

It would be unusual as a simple ACID transaction model because intermediate operations may
have independently committed side effects.

---

# 32. `execPlan` result is execution composition evidence

Fields such as:

```text
completedSteps
totalSteps
failedStepId
failedStepIndex
per-step result
```

allow callers to know how much orchestration ran.

They are not a rollback log for external Effects.

---

# 33. Compound execution should preserve partiality

If step 3 fails after steps 1–2 succeed, the correct representation is:

```text
compound execution failed at step3
steps1-2 mechanically completed
external Effects of each step remain domain-specific/opaque
```

not:

```text
nothing happened
```

---

# 34. Partial commit

RF12 defines **Partial Commit** across a compound open-world activity as:

> some constituent effect owners have durably committed their effects while others have not.

This is possible whenever no single atomic commit protocol spans all owners.

---

# 35. Partial commit is not necessarily corruption

A Saga deliberately permits intermediate subtransactions to commit independently.

What matters is whether the activity contract defines recovery/compensation for that state.

Therefore:

```text
PartialCommit != AutomaticallyInvalidState
```

---

# 36. Distributed transaction

A **Distributed Transaction** is a transaction whose participating state/effect changes are
owned by multiple resource managers/participants and coordinated under a shared commit
protocol.

The important property is participation in one commit decision—not merely “multiple
machines were involved.”

---

# 37. Two-Phase Commit

Classic 2PC separates:

```text
Phase 1: prepare/vote
Phase 2: global commit/abort decision
```

Participants make promises that constrain what they may do after voting prepared.

---

# 38. Prepare is stronger than “I think I can do it”

A prepared participant must retain sufficient durable state/resources to honor a later
global commit decision despite relevant failures.

Therefore:

```text
PreflightCheck != PrepareVote
```

---

# 39. Current Runtime validation is not 2PC prepare

Checks such as:

```text
source digest valid
host dependency valid
capacity available
provider configured
```

are admission/precondition checks.

They do not make external effect owners promise a later atomic commit.

---

# 40. 2PC coordinates atomic decision but has failure costs

Gray and Lamport describe the distributed transaction commit problem as deciding whether all
participants commit or abort and note that classic 2PC can block when the transaction
manager/coordinator fails.

This illustrates:

```text
DistributedAtomicity requires protocol/state/resource cost.
```

---

# 41. Paxos Commit shows commit decision itself can be replicated

Gray and Lamport present Paxos Commit as a fault-tolerant commit protocol and characterize
2PC as a special zero-fault-tolerance case in that framework.

RF12 lesson:

> consensus can strengthen the availability of the **commit decision**, but the transaction
> still requires actual participating resource managers with prepare/commit semantics.

---

# 42. Consensus does not make arbitrary APIs transactional

Putting Runtime's coordinator state in Raft/Paxos would not make:

```text
OKX order API
GitHub API
email send
local filesystem
```

join one atomic transaction.

Therefore:

```text
ReplicatedCoordinator != CrossOwnerTransactionalParticipation
```

---

# 43. Transactional participants must expose commit obligations

For participant P, a shared atomic transaction generally needs a protocol that can answer:

```text
prepare T?
commit T
abort T
recover decision for T
```

or a semantically equivalent mechanism.

Generic process execution lacks this interface.

---

# 44. Lock/resource retention is part of distributed commit cost

Prepared transactions often hold resources/locks until the final decision.

Long-running/open-world workflows make this expensive or infeasible.

This is one reason Saga-like models exist.

---

# 45. Saga

Garcia-Molina and Salem define a Saga as a long-lived transaction decomposed into a sequence
of ordinary transactions that may interleave with others; either all component transactions
complete or compensating transactions are run to amend partial execution.

RF12 adopts the key distinction:

```text
Saga = sequence of independently committed subtransactions + compensation policy
```

not one global ACID transaction.

---

# 46. Saga compensation is not rollback

Once subtransaction T1 committed and became externally visible, later C1 compensation is a
new transaction/effect.

Therefore:

```text
SagaCompensation != TransactionRollback
```

RF10's compensation distinction is preserved.

---

# 47. Compensation may be semantically imperfect

Examples:

```text
send email → cannot unsend
book seat → cancel later but another user observed it
market order → opposite trade does not erase price/slippage/tax history
```

Thus:

```text
Compensation != RestoreExactPriorWorld
```

---

# 48. Compensation can fail

A Saga requires recovery logic for:

```text
forward action fails
compensation fails
compensation becomes ambiguous
```

Therefore Saga is not “easy rollback across services.”

---

# 49. Saga ordering semantics vary

A Saga may compensate completed steps in reverse order or use domain-specific dependencies.

No generic Runtime compensation order should be inferred from execPlan order.

---

# 50. Orchestration

RF12 defines **Orchestration** as explicit coordination of action order/conditions without
implying a shared atomic commit protocol.

Examples:

```text
A then B then C
if A succeeds run B
retry C after observation
on failure call compensation D
```

---

# 51. Orchestration is often the correct open-world model

When actions touch independent APIs without transaction participation, orchestration with
identity, receipts, reconciliation and compensation is more truthful than claiming
transactionality.

---

# 52. Workflow is broader than transaction

A durable workflow may last hours/days and include:

```text
human approval
external API
wait condition
retry
compensation
```

It is a state machine over activities, not necessarily a transaction.

---

# 53. Runtime currently does not need to become a Saga/workflow engine

Current Runtime's responsibility is physical execution/effect adapters.

Host can own semantic workflow composition until concrete repeated workloads prove a Runtime
primitive is required.

This preserves thin-core architecture.

---

# 54. Transactional outbox pattern

The outbox idea moves a critical boundary into one local transaction:

```text
business state update
+
durable message/effect intent record
```

are committed atomically in one database.

A separate relay performs external delivery later.

---

# 55. Outbox does not atomically include external delivery

A relay can crash:

```text
after publishing
before recording publish completion
```

so duplicate delivery remains possible and downstream idempotency/deduplication is needed.

Thus:

```text
TransactionalOutbox != AtomicDatabasePlusBrokerDelivery
```

---

# 56. Current Runtime admission resembles the durable-intent half of outbox thinking

Runtime atomically commits:

```text
Operation identity
Attempt
reservation
dispatch eligibility/side truth
```

then crosses into physical execution later.

This is not literally a message outbox implementation, but the architectural lesson is
similar:

> atomically commit what one owner truly controls, then make cross-owner delivery explicit.

---

# 57. `dispatch_issued` resembles a handoff fence, not a distributed commit

Its purpose is to prevent duplicate launch after ambiguity.

It does not make systemd a transactional participant.

---

# 58. Current commit-state error semantics align with RF12

Tool errors distinguish:

```text
not_started
not_committed
committed
unknown
```

These are meaningful because caller behavior depends on where transaction/commit boundaries
were crossed.

---

# 59. `commitState=committed` says Runtime commitment exists, not external Effect committed

This must remain scope-qualified.

Therefore:

```text
RuntimeCommitState != ExternalEffectCommitState
```

---

# 60. `commitState=unknown` is a transaction-outcome ambiguity

If SQLite COMMIT result itself cannot be resolved by rollback/readback, Runtime preserves
unknown and requires reconciliation before a new operation identity.

This is correct transaction semantics.

---

# 61. COMMIT response loss does not undo commit

RF10 established this generally.

RF12 places it inside transaction grammar:

```text
commit decision may be durable
client observation of commit may be lost
```

Therefore:

```text
CommitKnowledge != CommitReality
```

---

# 62. Current terminal commit is another true Registry-local transaction

Terminal commit atomically updates:

```text
Attempt terminal state/result
Artifacts
conditions
reservation release/hold outcome
Job resolution/current attempt
Job events
```

so Runtime does not expose a partially terminal Registry model.

---

# 63. Admin repair batch is explicitly atomic within Registry

Current repair applies selected cases inside one SQLite transaction and aborts the whole
batch on a late conflict/invariant violation.

This is a legitimate multi-case local transaction.

---

# 64. Local batch atomicity does not repair external world atomically

If repair marks a historical Attempt Lost/Failed, that administrative transaction does not
undo external effects that may already have occurred.

Thus:

```text
RegistryRepairAtomicity != ExternalWorldRollback
```

---

# 65. Workspace mutation can have its own local atomicity

`workspace.mutate` is documented as a digest-guarded atomic batch for its owned mutation
mechanism.

This local contract is stronger than arbitrary exec, but it lacks durable replay identity.

---

# 66. Response-loss safety and atomicity are different

An operation may be physically atomic but hard to safely retry after response loss if it
lacks durable identity/receipt.

Therefore:

```text
AtomicEffect != IdempotentReplay
```

---

# 67. Workspace Patch is a stronger structured multi-file effect

Patch records a durable before/after plan before mutation and can reconcile physical files
into:

```text
all before
all after
mixed/unexpected
```

This gives a domain-specific notion of correlated multi-file effect state.

---

# 68. Multi-file does not automatically mean one filesystem transaction

Ordinary filesystem APIs do not provide a universal atomic transaction spanning arbitrary
multiple file replacements.

Patch's contract therefore uses planning + physical mutation + digest-based reconciliation,
not a fictional global filesystem transaction.

---

# 69. Patch `unknown` is the honest representation of partial physical commit

If some files match before and others after:

```text
state = unknown
```

rather than pretending all-or-none succeeded.

This directly demonstrates:

```text
StructuredCompoundEffect != GuaranteedAtomicPhysicalTransaction
```

---

# 70. Patch can still provide a stronger logical contract than opaque exec

Because Runtime owns:

```text
complete file set
before digests
after digests
mutation method
reconciliation
```

it can prevent/recover more failure classes than arbitrary process execution.

---

# 71. Atomic rename is not multi-file transaction

A single rename may be atomic under filesystem semantics for one pathname transition.

That does not imply a sequence of several renames forms one all-or-none transaction.

Therefore:

```text
AtomicPrimitive × N != AtomicCompoundOperation
```

without another protocol.

---

# 72. Runtime Release is a structured orchestration/recovery contract

Release includes:

```text
admission
candidate/source binding
physical deploy
probe/verification
possible rollback
receipt
```

This is richer than opaque execution.

---

# 73. Release rollback is domain rollback/recovery, not rollback of the whole Runtime history

If deployment installs candidate bytes then restores previous installation:

```text
Effect disposition = rolled_back
```

but the Runtime Job, Attempt, receipts and historical deployment attempt remain real.

Therefore:

```text
ReleaseRolledBack != OperationNeverHappened
```

---

# 74. Release rollback may be closer to compensation depending on physical mechanism

If the installer actually restores a previous filesystem/service state, it can legitimately
call that domain transition rollback.

But from the larger open-world Operation perspective it is still a later recovery action
with historical effects/observations.

Thus rollback terminology remains scope-specific.

---

# 75. `rolled_back` and `not_committed` must remain distinct

```text
not_committed
```

means target release effect never crossed the release commit contract.

```text
rolled_back
```

means a release state was crossed/attempted and then previous state was restored under the
release contract.

These histories differ.

---

# 76. Cross-owner financial example

Suppose an Agent wants:

```text
1. submit exchange order
2. update local ledger
```

If the exchange API and SQLite ledger do not join one shared distributed transaction, then:

```text
order committed + ledger update failed
```

and:

```text
ledger update committed + order failed
```

are both possible depending on order.

---

# 77. Reordering cannot eliminate the dual-write problem

Choosing:

```text
ledger first → exchange second
```

or:

```text
exchange first → ledger second
```

only changes which partial-commit failure is possible.

Thus:

```text
Ordering != Atomicity
```

---

# 78. Local transaction + external idempotency can improve recovery without becoming one transaction

A finance adapter might use:

```text
local durable order intent/effect ID
exchange idempotency/client order ID
exchange query/receipt
local reconciliation
```

This can create strong exactly-once-like business behavior.

But unless both owners participate in one commit protocol:

```text
CrossOwnerAtomicity is still not literal ACID atomicity.
```

---

# 79. Semantic invariant may be eventually restored rather than atomically preserved

Example invariant:

```text
local ledger reflects exchange orders
```

can be temporarily false after partial commit and later restored by reconciliation.

This is **convergent consistency**, not atomic commit.

---

# 80. Compensation can preserve a higher-level invariant without erasing history

If order succeeds but business validation later fails:

```text
submit offsetting/cancel action
```

may restore some economic invariant.

It does not undo market exposure history.

---

# 81. Compensation eligibility is domain-specific

Some Effects support:

```text
true inverse
approximate inverse
cancel-before-finalization
no inverse
```

Runtime cannot infer this from process exit codes.

---

# 82. Open-world effects often have irreversible observations

Even if physical state is restored, other parties may have observed the intermediate state.

Therefore global historical rollback is generally impossible.

---

# 83. Atomicity must name observer visibility too

Within a database transaction, intermediate writes may be hidden from other sessions.

Across external APIs, an intermediate effect may become globally visible immediately.

Thus:

```text
AllOrNothingFinalState != NoIntermediateExternalObservation
```

---

# 84. Isolation/visibility matters for composition but is separate

Two activities can each be atomic locally but interleave in ways that violate a higher-level
business invariant.

RF12 records this dependency but leaves detailed isolation models to RF13.

---

# 85. Serializability

Serializability asks whether concurrent transaction executions are equivalent to some serial
order under the transactional data model.

It is not the same as atomicity.

Therefore:

```text
Serializable != Atomic
```

though database transactions often provide both properties in combination.

---

# 86. Linearizability

Linearizability concerns concurrent operations appearing to take effect atomically at a
point between invocation and response while respecting real-time order.

This is an operation/history consistency property, not a synonym for ACID transaction
atomicity.

---

# 87. One linearizable operation can contain a local transaction

For example a service endpoint may expose one logical linearizable state transition backed
by a database transaction.

But external side effects can break that abstraction unless included in the operation's
contract.

---

# 88. Multi-operation transaction requires a transaction identity

A true transaction spanning operations needs stable identity such as:

```text
transactionId
participants
state
prepare votes
commit decision
recovery records
```

Current Runtime Job IDs are operation identities, not generic distributed transaction IDs.

---

# 89. Reusing Job identity cannot manufacture cross-Operation transactionality

Putting several effects under one Job only correlates them.

Therefore:

```text
SharedJobId != SharedAtomicCommit
```

---

# 90. Shared clientRequestId likewise does not create a transaction

Idempotency identity prevents duplicate logical requests within one contract.

It does not atomically group unrelated effects.

---

# 91. Correlation is weaker than composition; composition is weaker than transaction

RF12 hierarchy:

```text
correlated
< ordered/composed
< transactionally committed
```

These should not be collapsed.

---

# 92. One transaction can contain many records without many Operations

Conversely, one Runtime admission transaction writes many Registry records but represents
one Operation admission.

Therefore database statement count is not Operation count.

---

# 93. Transaction boundary should follow invariant ownership

A useful heuristic:

> If a correctness invariant requires several state changes to be simultaneously committed
> and one owner can enforce that invariant transactionally, place them in one local
> transaction.

Current Job/Attempt/reservation admission is a good example.

---

# 94. Do not stretch local transaction boundaries across foreign owners by naming

Calling a code block:

```text
transaction { call API; update DB; }
```

does not make the API participate in the database transaction.

This is a semantic anti-pattern.

---

# 95. Process crash is not transaction abort for external effects

If a process:

```text
POSTs external mutation
then crashes before local result write
```

external Effect may be committed.

Therefore:

```text
ProcessAbort != ExternalTransactionAbort
```

---

# 96. OS process supervision cannot supply business rollback

systemd/Windows Job can terminate child execution trees.

They cannot rewind external API state.

Thus:

```text
ContainmentTermination != TransactionRollback
```

---

# 97. Cancellation is similarly non-transactional

RF6/RF10 already established:

```text
CancelExecution != RollbackEffect
```

RF12 places cancellation as a control transition over future execution, not a transaction
abort across already committed external Effects.

---

# 98. Timeout is not abort either

Timeout stops/waits based on control policy; an external Effect may already be committed.

Therefore:

```text
Timeout != TransactionAbort
```

---

# 99. Recovery protocol is part of transaction semantics

A transaction guarantee is incomplete if, after crash, the system cannot determine whether
commit or abort won.

Current Registry commit paths explicitly read back/reconcile COMMIT failure outcomes.

---

# 100. Transaction logs/receipts are not optional diagnostics

Durable commit/recovery evidence is part of preserving the transaction decision after
failure.

This matches RF11 evidence ownership.

---

# 101. Distributed commit also needs durable decision recovery

2PC/Paxos Commit are not merely message sequences; participants/coordinators must recover
prepared/commit/abort decisions after failures.

Therefore:

```text
ProtocolMessagesWithoutDurableState != TransactionCommitProtocol
```

---

# 102. Blocking and availability are different from atomicity safety

2PC may preserve atomicity while blocking under coordinator failure.

Thus:

```text
AtomicSafety != NonBlockingLiveness
```

RF10's safety/liveness distinction recurs at transaction scale.

---

# 103. Paxos Commit improves commit availability, not semantic rollback

Replicating transaction-manager decision state can reduce blocking/fault sensitivity.

It does not create compensating inverses for irreversible external effects.

---

# 104. Long-lived transactions pressure isolation/resources

Holding locks/resources for hours or days can be unacceptable.

Saga research arose partly from this tension: decompose long-lived work into independently
committed transactions and compensate on failure.

---

# 105. Saga gives up isolation/atomic visibility of the whole activity

Other transactions may observe/interleave between Saga subtransactions.

Therefore:

```text
Saga != LongACIDTransaction
```

---

# 106. Saga correctness is semantic

A Saga is only sound if domain designers define compensation and acceptable intermediate
states.

Runtime cannot mechanically infer this.

---

# 107. Host is the more natural current owner of semantic composition

Host can reason over:

```text
Task objective
multiple Runtime Jobs
external receipts
human/domain decisions
compensation requirements
```

while Runtime remains execution-authoritative.

This matches current responsibility separation.

---

# 108. Runtime should expose enough partial-progress evidence for Host orchestration

Current fields such as:

```text
Job/Attempt state
step results
Artifacts
Effect receipt/disposition
unknown/reconciliation-required
```

are exactly what a higher-level orchestrator needs to decide next actions safely.

---

# 109. Runtime should not hide partial effects behind one success boolean

A compound plan may require domain reconciliation even when local process state is terminal.

RF11's multi-axis outcome model therefore remains necessary for composition.

---

# 110. Transaction boundaries and authority boundaries align

General law:

```text
AtomicityScope cannot exceed the set of participants whose commit behavior is controlled by
one transaction protocol.
```

This is RF12's central boundary theorem.

---

# 111. Stronger atomicity can be created by moving state under one owner

Examples:

```text
instead of DB update + immediate message publish:
  DB update + outbox row in one DB transaction

instead of two independent files:
  create immutable bundle then atomic pointer swap
```

Architectural restructuring can shrink the cross-owner problem.

---

# 112. But ownership consolidation has costs

Moving more state under one owner may increase:

```text
coupling
latency
storage burden
availability dependencies
authority concentration
```

so “make everything one database transaction” is not always desirable.

---

# 113. Atomic reference swap is a useful composition technique

If many immutable objects are prepared first and a single pointer/reference controls
visibility, one atomic pointer update can make a logically larger state transition appear
atomic to readers under the pointer contract.

---

# 114. Preparation outside commit scope can be safe when invisible

Example:

```text
build candidate bundle
validate it
then atomically switch active pointer
```

Only the final visibility-changing pointer belongs to atomic commit; preparatory work can be
nontransactional if it remains unreachable/inert.

---

# 115. This is different from externally visible pre-commit Effects

If “preparation” sends an email or places an order, it is not merely invisible preparation.

Thus:

```text
Preparation != SideEffectfulEarlyCommit
```

---

# 116. Two independent external APIs with no prepare/commit cannot be made atomic by Runtime

For APIs A and B:

```text
A.commit()
B.commit()
```

some crash point always exists after one commits and before the other unless:

```text
both expose a shared transaction protocol
or one action is not actually visible/committed yet
```

---

# 117. Exactly-once and atomicity answer different questions

Exactly-once:

```text
How many times did logical Effect E occur?
```

Atomicity:

```text
Did the participating effect set commit all-or-none?
```

Therefore:

```text
ExactlyOnce != Atomicity
```

---

# 118. A transaction can be exactly-once committed yet semantically wrong

One atomic transfer of the wrong amount is still one atomic transaction.

Again:

```text
TransactionalCorrectness != IntentCorrectness
```

---

# 119. An idempotent Saga step still does not make the Saga atomic

Idempotency improves retry safety for one step.

It does not hide intermediate states or create a global commit point.

---

# 120. Reconciliation can complete knowledge without completing atomicity

After partial commit, reconciliation may learn:

```text
A committed
B did not
```

That is epistemic convergence.

The system may still need compensation/forward recovery.

---

# 121. Forward recovery

**Forward recovery** completes or adapts the remaining work from a partially committed state
rather than undoing already committed effects.

This may be preferable when compensation is impossible or expensive.

---

# 122. Backward recovery

**Backward recovery** attempts to restore a prior state through true rollback where still
inside transaction scope or compensation where already externally committed.

These mechanisms must not be conflated.

---

# 123. Partial commit should be first-class when possible

A higher-level orchestrator benefits from knowing:

```text
which sub-actions committed
which are unknown
which failed
which compensations committed
```

rather than receiving one coarse “failed.”

---

# 124. Current Runtime already exposes some of this mechanically

`execPlan` preserves per-step results; structured Effects expose dedicated dispositions;
terminal evidence and Artifacts preserve history.

This is sufficient for current thin-core composition without a universal transaction layer.

---

# 125. Transaction groups across multiple Runtime Jobs are currently undefined

Runtime has no current contract for:

```text
begin Runtime transaction
admit Job A
admit Job B
commit both Jobs
```

and should not be described as providing one.

---

# 126. Multi-Job atomic admission might be possible locally but needs a consumer

SQLite could theoretically atomically insert several Job/Attempt/reservation sets.

But that would introduce new semantics:

```text
shared transaction identity
capacity reservation across Jobs
cancellation semantics
partial dispatch
resolution
replay
```

No current evidence justifies it.

---

# 127. Atomic admission of multiple Jobs would still not atomically execute them

Even if local multi-Job admission were atomic:

```text
dispatch A
crash
dispatch B not yet
```

remains possible.

So the benefit must be stated narrowly.

---

# 128. Resource reservation can participate in local transaction

Current admission atomically reserves capacity with Job identity.

This guarantees no committed admitted Job exists without its reservation accounting.

That is a real compositional invariant.

---

# 129. Reservation is not lock on external world

It limits Runtime local capacity.

It cannot reserve:

```text
exchange liquidity
cloud API capacity
external file state
human attention
```

unless those domains expose separate mechanisms.

---

# 130. ACID vocabulary should remain domain-specific

It is useful to say:

```text
Registry transaction is atomic/durable under SQLite contract.
```

It is misleading to say:

```text
Runtime Operation is ACID.
```

because the Operation may span opaque external Effects.

---

# 131. Cross-domain falsifier matrix

| Case | Collapse attacked | Surviving distinction |
|---|---|---|
| Job+Attempt+reservation one SQLite transaction | many rows = many independent commits | one owner can atomically bind related records |
| SQLite commit then crash before dispatch | admission transaction includes execution | durable handoff leaves valid Accepted state |
| `dispatch_issued` then crash | intent commit = launch commit | cross-owner ambiguity remains |
| systemd unit launched then process calls API | launch atomicity = business atomicity | process can create independent effects |
| execPlan step1 succeeds, step2 fails | stop-on-error = rollback | prior step Effects survive |
| `continueOnError=true` | plan = transaction | explicit workflow sequencing |
| terminal Registry commit | Job failed = transaction failed | failure state can be atomically committed |
| admin repair atomic batch | Registry repair = world rollback | only Registry facts change atomically |
| workspace.mutate atomic batch | atomic = safe replay | no durable request identity means response-loss ambiguity still matters |
| Patch mixed before/after files | structured effect = physical transaction | reconciliation can yield unknown partial state |
| single atomic rename × 3 | atomic primitives compose automatically | compound atomicity needs protocol |
| Release rolled back | whole operation never happened | history/evidence remains; rollback is domain scoped |
| Release not_committed vs rolled_back | both = failure | different commit histories |
| exchange order + SQLite ledger | sequence = atomic | dual-write partial commit exists |
| exchange idempotency key + local intent | idempotency = transaction | retry safety improves without cross-owner ACID |
| two APIs no prepare protocol | coordinator code = transaction | no shared commit participation |
| 2PC coordinator failure | atomicity = liveness | can preserve safety while blocking |
| Paxos Commit | consensus = arbitrary API transaction | consensus protects decision availability only for participants |
| Saga | compensation = rollback | independent commits remain visible/history-preserving |
| email compensation | every Effect invertible | some effects lack true inverse |
| outbox DB+message | outbox = atomic delivery | local state+intent atomic, delivery later/possibly duplicate |
| multi-Job atomic admission | admission atomicity = execution atomicity | dispatch remains separate |

---

# 132. RF12 anti-laws

RF12-L1: `Sequence != Transaction`.

RF12-L2: `Composition != Atomicity`.

RF12-L3: `CompoundOperation != Transaction`.

RF12-L4: `LocalAtomicity != CrossOwnerAtomicity`.

RF12-L5: `AtomicCommit != SuccessfulBusinessOutcome`.

RF12-L6: `Atomicity != Durability`.

RF12-L7: `Atomicity != Isolation`.

RF12-L8: `Atomicity != SemanticConsistency`.

RF12-L9: `Rollback != Compensation`.

RF12-L10: `Savepoint != IndependentCommit`.

RF12-L11: `AdmissionTransaction != PhysicalDispatchTransaction`.

RF12-L12: `DurableHandoff != DistributedTransaction`.

RF12-L13: `AtomicLaunch != AtomicOpenWorldExecution`.

RF12-L14: `OneProcessExecution != OneTransactionalEffect`.

RF12-L15: `ExecPlan != Transaction`.

RF12-L16: `StopOnError != RollbackPreviousSteps`.

RF12-L17: `PartialCommit != AutomaticallyInvalidState`.

RF12-L18: `PreflightCheck != PrepareVote`.

RF12-L19: `ReplicatedCoordinator != CrossOwnerTransactionalParticipation`.

RF12-L20: `Saga != LongACIDTransaction`.

RF12-L21: `SagaCompensation != TransactionRollback`.

RF12-L22: `Compensation != RestoreExactPriorWorld`.

RF12-L23: `Orchestration != Transaction`.

RF12-L24: `TransactionalOutbox != AtomicDatabasePlusExternalDelivery`.

RF12-L25: `RuntimeCommitState != ExternalEffectCommitState`.

RF12-L26: `CommitKnowledge != CommitReality`.

RF12-L27: `RegistryRepairAtomicity != ExternalWorldRollback`.

RF12-L28: `AtomicEffect != IdempotentReplay`.

RF12-L29: `StructuredCompoundEffect != GuaranteedAtomicPhysicalTransaction`.

RF12-L30: `AtomicPrimitive × N != AtomicCompoundOperation`.

RF12-L31: `ReleaseRolledBack != OperationNeverHappened`.

RF12-L32: `Ordering != Atomicity`.

RF12-L33: `AllOrNothingFinalState != NoIntermediateExternalObservation`.

RF12-L34: `Serializable != Atomic`.

RF12-L35: `SharedJobId != SharedAtomicCommit`.

RF12-L36: `ProcessAbort != ExternalTransactionAbort`.

RF12-L37: `ContainmentTermination != TransactionRollback`.

RF12-L38: `Timeout != TransactionAbort`.

RF12-L39: `AtomicSafety != NonBlockingLiveness`.

RF12-L40: `ExactlyOnce != Atomicity`.

---

# 133. Minimum RF12 grammar

## 133.1 Composition `Compose(A1...An,R)`

Actions combined under relation set R.

## 133.2 Compound Operation `Op{A1...An}`

One logical requested activity realized by multiple actions.

## 133.3 Transaction `Txn(owner,participants,stateSet)`

Owner/protocol-controlled commit/abort unit over participating state/effect set.

## 133.4 Atomicity `Atomic_C(S)`

Under contract C, participating commit set S becomes all committed or none committed.

## 133.5 Commit `Commit_O(T)`

Owner O makes transaction T's intended state authoritative/durable.

## 133.6 Abort `Abort_O(T)`

Owner O terminates T without committing its provisional participating state.

## 133.7 Prepare `Prepare_P(T)`

Participant P durably promises it can honor a later global commit/abort decision.

## 133.8 Durable Handoff `Handoff(O1→O2,intent)`

O1 atomically commits intent/identity; O2 later realizes it outside O1's transaction.

## 133.9 Partial Commit `Partial({E1...En})`

Some constituent effect owners committed while others did not/are unknown.

## 133.10 Saga `Saga(T1...Tn,C1...Cn)`

Sequence of independently committed subtransactions plus compensating actions/policy.

## 133.11 Orchestration `Orch(A1...An,control)`

Coordination/order/branch/retry semantics without implied shared atomic commit.

## 133.12 Compensation `Comp(E)`

New domain Effect intended to offset a prior committed Effect.

## 133.13 Forward Recovery

Continue from partial commit toward acceptable final state.

## 133.14 Backward Recovery

Restore transaction-owned provisional state by rollback or committed state by domain-specific
compensation.

---

# 134. Current Runtime composition/transaction map

| Current mechanism | RF12 interpretation |
|---|---|
| SQLite admission transaction | true Registry-local atomic transaction |
| Job+Attempt+reservation+idempotency | one local atomic commitment set |
| execution-provider/Host-dependency/release side truth in admission | additional Registry-owned facts inside same local transaction |
| Accepted Job after commit | durable handoff state before physical realization |
| `dispatch_issued` | local at-most-once handoff commitment, not systemd transaction |
| systemd/Windows launch | separate physical owner boundary |
| arbitrary process execution | open-world compound Effect, OPAQUE |
| `workspace.execPlan` | ordered orchestration inside one Attempt, not transaction |
| step stop on failure | control-flow policy, not rollback |
| `continueOnError` | explicit workflow/orchestration semantics |
| terminal commit | true Registry-local atomic terminal-state transaction |
| admin repair batch | atomic Registry repair transaction |
| `workspace.mutate` | local atomic mutation primitive, weaker replay semantics |
| Workspace Patch | structured multi-file correlated Effect + reconciliation; can be unknown |
| Runtime Release | structured deployment orchestration/effect contract with receipt/recovery |
| Release `not_committed` | release effect commit not crossed |
| Release `rolled_back` | domain recovery restored prior release state after later action |
| `commitState` error field | Runtime operation commitment evidence only |
| Host Task/Workflow | natural current owner of semantic multi-Operation orchestration |

---

# 135. What RF12 strongly supports in current design

## 135.1 Registry-local transactions for Runtime-owned invariants

Job/Attempt/reservation/idempotency and terminal transitions belong in one SQLite transaction
because Runtime owns all these facts and their invariants.

## 135.2 Admission before dispatch

The durable-handoff design is more truthful and recoverable than pretending SQLite and
systemd share one transaction.

## 135.3 At-most-once dispatch intent as a handoff fence

`dispatch_issued` solves duplicate-risk/recovery without overclaiming distributed atomicity.

## 135.4 execPlan as orchestration

Per-step order/progress/failure semantics are useful precisely because Runtime does not hide
partial execution behind fake rollback.

## 135.5 Structured Effects remain domain-specific

Patch and Release gain stronger local effect semantics only where Runtime owns identity,
preconditions, commit/recovery and receipt/state proof.

## 135.6 Host remains semantic composition owner

Cross-Job/business workflow/Saga-like reasoning should remain above Runtime until a concrete
repeated consumer proves a lower primitive necessary.

---

# 136. What RF12 reopens

## 136.1 “transaction” wording should always state owner scope

Future docs should say `Registry transaction`, `Workspace mutation atomicity`, `Release
rollback`, etc., rather than implying whole-operation ACID semantics.

## 136.2 Structured adapters may eventually need explicit effect-state machines

Finance/cloud adapters could model:

```text
intent
submitted
committed
cancelled/compensated
unknown
```

with provider identity/receipt/reconciliation. This remains adapter-specific and is not
implemented by RF12.

## 136.3 Transactional-outbox-like handoff may be useful for some future consumers

If a repeated consumer needs local state + external delivery intent, co-locating durable
intent with local state under one owner may reduce dual-write ambiguity. No generic outbox
is justified yet.

## 136.4 Multi-Job atomic admission remains unproven

It is mechanically possible inside SQLite but has unclear product semantics and does not
provide execution atomicity.

---

# 137. Research constitution added by RF12

1. Never call an ordered sequence a transaction without a commit/abort owner contract.
2. Scope atomicity to the participating state/effect set and owner/protocol.
3. Keep atomicity, durability, isolation, consistency and semantic success distinct.
4. Treat current SQLite Registry transactions as strong local transactions only.
5. Do not extend Registry atomicity across systemd/Windows launch or arbitrary target
   Effects.
6. Preserve Accepted-after-commit as a valid durable-handoff state.
7. Treat `dispatch_issued` as an at-most-once handoff commitment, not distributed commit.
8. Keep process execution and execPlan effect-opaque with respect to arbitrary external
   Effects.
9. Treat execPlan stop/continue behavior as orchestration, never rollback semantics.
10. Preserve partial-step evidence when a compound execution fails.
11. Require actual participant prepare/commit/recovery semantics before claiming distributed
    transactionality.
12. Consensus/replicated coordinators improve commit-decision availability only for
    participating transactional resource managers.
13. Treat Saga compensation as new effects over already committed subtransactions, not
    rollback.
14. Do not assume every Effect is compensatable or exactly invertible.
15. Prefer orchestration + identity + receipts + reconciliation when independent external
    systems cannot join one transaction.
16. Use transactional-outbox-style thinking only to atomically commit local state + durable
    intent; never claim the later external delivery is in the same local transaction.
17. Keep `commitState` scoped to Runtime commitment, not external Effect commitment.
18. Keep Registry repair atomicity separate from external-world correction.
19. Do not infer compound atomicity from individually atomic filesystem/process primitives.
20. Preserve `not_committed`, `rolled_back`, `unknown` and partial histories as distinct.
21. Align transaction boundaries with invariant ownership whenever possible.
22. Do not use a shared Job/clientRequestId merely as a label for atomic grouping.
23. Treat forward recovery, true rollback and compensation as distinct recovery strategies.
24. Keep Host as current semantic multi-Operation composition owner unless real consumers
    falsify this boundary.
25. Do not enter RF13 until isolation/containment can be studied independently from
    transactionality.

---

# 138. RF12 result — conceptual compression

```text
Composition says several actions belong together.
Ordering says when they run.
A Compound Operation says they realize one logical request.
A Transaction says one owner/protocol controls all-or-none commit of a named participating
state/effect set.
A Durable Handoff atomically commits intent in one owner, then realizes work later across an
owner boundary.
A Distributed Transaction requires actual participants in one prepare/commit/recovery
protocol.
A Saga permits independently committed subtransactions and uses compensating actions when
partial execution must be amended.
Orchestration coordinates actions without pretending they share one atomic commit.
```

Strongest Runtime-specific compression:

```text
Registry facts:
  strong local SQLite transactions

Registry → physical dispatch:
  durable handoff + at-most-once dispatch fence
  NOT one transaction

execPlan:
  ordered orchestration
  NOT rollback/ACID

arbitrary target external Effects:
  open-world / OPAQUE

Workspace Patch / Runtime Release:
  stronger adapter-specific compound-effect contracts
  but still only within their owned domains

semantic multi-Operation workflow:
  Host/domain owned
```

The central theorem is:

```text
AtomicityScope <= TransactionParticipantScope
```

or, in RF11 language:

```text
ClaimedAtomicityScope must not exceed the effect owners actually joined by the commit
protocol.
```

---

# 139. RF12 closeout

RF12 closes with twenty-nine durable results:

1. **Composition, sequence, Compound Operation and Transaction are distinct.**
2. A Transaction requires a named owner/domain and commit/abort contract over a declared
   participating state/effect set.
3. Atomicity is all-or-none commit under that contract; it is distinct from durability,
   isolation, consistency and semantic success.
4. **Atomicity is owner-scoped: local SQLite atomicity does not extend across systemd,
   Windows process launch or arbitrary external APIs.**
5. Current admission is a genuine Registry-local transaction atomically binding Job,
   Attempt, reservation, idempotency and relevant Runtime-owned side truth.
6. SQLite admission commit is therefore the canonical Runtime admission commit point.
7. Admission deliberately excludes physical dispatch; `SQLite commit → crash before launch`
   is a valid durable Accepted state, not transaction corruption.
8. This pattern is best modeled as **durable handoff**, not distributed transaction.
9. `dispatch_issued` is another Registry-local at-most-once handoff frontier and intentionally
   leaves cross-owner launch ambiguity after failure.
10. A process launch is not an atomic open-world execution; one process may commit many
    independent external Effects.
11. **Current `workspace.execPlan` is ordered orchestration, not transactionality.** Earlier
    successful step Effects remain after later failure; `continueOnError` reinforces this.
12. Partial commit across open-world constituent Effects is legitimate and must be modeled
    explicitly rather than rewritten as “nothing happened.”
13. Distributed transactionality requires actual resource-manager participation in a common
    prepare/commit/abort/recovery protocol.
14. Preflight/admission validation is not a 2PC prepare vote.
15. Classic 2PC demonstrates strong atomic commit can trade liveness/availability and block
    under coordinator failure; transaction safety is not nonblocking progress.
16. Paxos Commit shows replication/consensus can strengthen commit-decision fault tolerance,
    but cannot make arbitrary external APIs transactional participants.
17. **Saga is independent subtransaction commits + compensation**, not one long ACID
    transaction; compensation is a new Effect and may be imperfect or fail.
18. Orchestration is the truthful default for independent open-world systems without shared
    transactional participation.
19. Transactional-outbox thinking atomically commits local state plus delivery intent but
    external delivery remains later and may duplicate; it is not DB+broker atomicity.
20. Current Runtime admission follows the same broader discipline: atomically persist what
    Runtime owns, then cross physical boundaries explicitly.
21. `commitState` must remain scoped to Runtime durable commitment and never be read as
    external Effect commitment.
22. Current terminal commit and admin repair batch are legitimate Registry-local atomic
    transactions; neither rolls back external Reality.
23. Workspace mutation/patch can provide stronger local compound-effect semantics where
    Runtime owns complete state/preconditions/reconciliation, but multi-file physical
    mutation may still converge to `unknown` rather than all-or-none.
24. **Atomic primitives do not automatically compose into an atomic compound operation.**
25. Runtime Release `not_committed` and `rolled_back` represent different histories; release
    rollback is domain-scoped recovery and does not erase the Runtime Operation history.
26. Exchange-order + local-ledger dual writes illustrate why ordering alone cannot provide
    cross-owner atomicity; idempotency/receipts/reconciliation improve safety but are not
    literal shared ACID commit.
27. Shared Job ID, clientRequestId or coordinator state does not manufacture transactionality;
    transaction scope follows actual participant ownership/protocol.
28. Current thin-core boundary remains sound: Runtime should expose mechanical partial
    progress/effect evidence while Host/domain owns semantic cross-Operation composition and
    compensation policy.
29. **The governing RF12 law is `AtomicityScope <= TransactionParticipantScope`: never claim
    atomicity beyond the effect owners that actually participate in the commit protocol.**

The next frontier is:

> **When multiple executions share machines, files, credentials, namespaces and mutable
> dependencies, what does isolation actually mean? What is containment versus authority,
> non-interference versus secrecy, and which execution boundaries protect against accidental
> interference versus hostile actors?**

That is RF13 — Isolation, Containment and Trust.
